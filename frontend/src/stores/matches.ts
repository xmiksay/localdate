import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import * as socialApi from '@/api/social'
import type { MatchSummary, Message, WsEvent } from '@/api/types'
import { useAuthStore } from './auth'
import { useNearbyStore } from './nearby'

export const PAGE_SIZE = 50
const TMP_PREFIX = 'tmp-'
export const isPendingMessage = (m: Message) => m.id.startsWith(TMP_PREFIX)

/** Chronological (oldest first), unique by id. */
function mergeMessages(existing: Message[], incoming: Message[]): Message[] {
  const byId = new Map<string, Message>()
  for (const m of [...existing, ...incoming]) byId.set(m.id, m)
  return [...byId.values()].sort((a, b) => a.created_at.localeCompare(b.created_at))
}

export const useMatchesStore = defineStore('matches', () => {
  const matches = ref<MatchSummary[]>([])
  const messages = ref<Record<string, Message[]>>({})
  const hasMore = ref<Record<string, boolean>>({})
  const unread = ref<Record<string, boolean>>({})
  /** The chat currently on screen never counts incoming messages as unread. */
  const activeMatchId = ref<string | null>(null)
  const loaded = ref(false)
  let tmpSeq = 0

  const hasUnread = computed(() => Object.values(unread.value).some(Boolean))
  const findMatch = (id: string) => matches.value.find((m) => m.match_id === id)

  function sortMatches() {
    const activity = (m: MatchSummary) => m.last_message?.created_at ?? m.created_at
    matches.value = [...matches.value].sort((a, b) => activity(b).localeCompare(activity(a)))
  }

  function upsertMatch(m: MatchSummary) {
    const rest = matches.value.filter((x) => x.match_id !== m.match_id)
    matches.value = [...rest, m]
    sortMatches()
  }

  async function loadMatches() {
    matches.value = await socialApi.getMatches()
    sortMatches()
    loaded.value = true
  }

  function markRead(matchId: string) {
    unread.value = { ...unread.value, [matchId]: false }
  }

  function setActive(matchId: string | null) {
    activeMatchId.value = matchId
    if (matchId) markRead(matchId)
  }

  async function loadMessages(matchId: string) {
    const page = await socialApi.getMessages(matchId, undefined, PAGE_SIZE)
    messages.value = {
      ...messages.value,
      [matchId]: mergeMessages(messages.value[matchId] ?? [], page),
    }
    hasMore.value = { ...hasMore.value, [matchId]: page.length >= PAGE_SIZE }
  }

  async function loadOlder(matchId: string) {
    const oldest = messages.value[matchId]?.find((m) => !isPendingMessage(m))
    if (!oldest) return loadMessages(matchId)
    const page = await socialApi.getMessages(matchId, oldest.id, PAGE_SIZE)
    messages.value = {
      ...messages.value,
      [matchId]: mergeMessages(messages.value[matchId] ?? [], page),
    }
    hasMore.value = { ...hasMore.value, [matchId]: page.length >= PAGE_SIZE }
  }

  function touchLastMessage(msg: Message) {
    const m = findMatch(msg.match_id)
    if (!m) return
    const prev = m.last_message
    if (!prev || prev.created_at <= msg.created_at) upsertMatch({ ...m, last_message: msg })
  }

  async function send(matchId: string, body: string) {
    const me = useAuthStore().user?.id ?? ''
    const tmp: Message = {
      id: `${TMP_PREFIX}${++tmpSeq}`,
      match_id: matchId,
      sender_id: me,
      body,
      created_at: new Date().toISOString(),
    }
    messages.value = { ...messages.value, [matchId]: [...(messages.value[matchId] ?? []), tmp] }
    try {
      const real = await socialApi.sendMessage(matchId, body)
      // The WS echo may already have delivered `real`; merging by id keeps one copy.
      messages.value = {
        ...messages.value,
        [matchId]: mergeMessages(
          (messages.value[matchId] ?? []).filter((m) => m.id !== tmp.id),
          [real],
        ),
      }
      touchLastMessage(real)
    } catch (e) {
      messages.value = {
        ...messages.value,
        [matchId]: (messages.value[matchId] ?? []).filter((m) => m.id !== tmp.id),
      }
      throw e
    }
  }

  function applyMessage(msg: Message) {
    const list = messages.value[msg.match_id]
    // Histories not opened yet are fetched fresh on open; only keep ones we already hold.
    if (list) messages.value = { ...messages.value, [msg.match_id]: mergeMessages(list, [msg]) }
    if (!findMatch(msg.match_id)) {
      void loadMatches()
    } else {
      touchLastMessage(msg)
    }
    const mine = msg.sender_id === useAuthStore().user?.id
    if (!mine && activeMatchId.value !== msg.match_id) {
      unread.value = { ...unread.value, [msg.match_id]: true }
    }
  }

  function applyEvent(ev: WsEvent) {
    if (ev.type === 'message') applyMessage(ev.message)
    else if (ev.type === 'match') upsertMatch(ev.match)
    else if (ev.type === 'wave')
      void useNearbyStore()
        .loadIncoming()
        .catch(() => undefined)
  }

  function removeUser(userId: string) {
    const gone = matches.value.filter((m) => m.other.user_id === userId).map((m) => m.match_id)
    matches.value = matches.value.filter((m) => m.other.user_id !== userId)
    for (const id of gone) {
      delete messages.value[id]
      delete hasMore.value[id]
      delete unread.value[id]
    }
  }

  function reset() {
    matches.value = []
    messages.value = {}
    hasMore.value = {}
    unread.value = {}
    activeMatchId.value = null
    loaded.value = false
  }

  return {
    matches,
    messages,
    hasMore,
    unread,
    loaded,
    hasUnread,
    findMatch,
    loadMatches,
    loadMessages,
    loadOlder,
    send,
    setActive,
    markRead,
    applyEvent,
    removeUser,
    reset,
  }
})
