import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import * as socialApi from '@/api/social'
import type { MatchSummary, Message } from '@/api/types'
import { useAuthStore } from './auth'
import { PAGE_SIZE, useMatchesStore } from './matches'
import { useNearbyStore } from './nearby'

vi.mock('@/api/social')

const msg = (n: number, sender = 'other', match = 'm1'): Message => ({
  id: `msg-${String(n).padStart(4, '0')}`,
  match_id: match,
  sender_id: sender,
  body: `body ${n}`,
  created_at: new Date(Date.UTC(2026, 0, 1, 0, 0, n)).toISOString(),
})
const match = (id: string, userId = `u-${id}`, last: Message | null = null): MatchSummary => ({
  match_id: id,
  created_at: '2026-01-01T00:00:00.000Z',
  other: { user_id: userId, display_name: userId, photo_url: null },
  last_message: last,
})

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  vi.mocked(socialApi.getMatches).mockResolvedValue([])
  useAuthStore().user = { id: 'me', username: 'me', created_at: '' }
})

describe('matches store pagination', () => {
  it('loads newest-first pages into chronological order', async () => {
    vi.mocked(socialApi.getMessages).mockResolvedValue([msg(3), msg(2), msg(1)])
    const s = useMatchesStore()
    await s.loadMessages('m1')
    expect(s.messages.m1.map((m) => m.id)).toEqual(['msg-0001', 'msg-0002', 'msg-0003'])
    expect(s.hasMore.m1).toBe(false)
  })

  it('loads older pages with the oldest id as cursor and stops on a short page', async () => {
    const full = Array.from({ length: PAGE_SIZE }, (_, i) => msg(100 - i))
    vi.mocked(socialApi.getMessages).mockResolvedValueOnce(full)
    const s = useMatchesStore()
    await s.loadMessages('m1')
    expect(s.hasMore.m1).toBe(true)
    vi.mocked(socialApi.getMessages).mockResolvedValueOnce([msg(50), msg(49)])
    await s.loadOlder('m1')
    expect(socialApi.getMessages).toHaveBeenLastCalledWith('m1', 'msg-0051', PAGE_SIZE)
    expect(s.messages.m1[0].id).toBe('msg-0049')
    expect(s.messages.m1).toHaveLength(PAGE_SIZE + 2)
    expect(s.hasMore.m1).toBe(false)
  })
})

describe('matches store sending', () => {
  it('replaces the optimistic message with the server one', async () => {
    let resolve!: (m: Message) => void
    vi.mocked(socialApi.sendMessage).mockReturnValue(new Promise((r) => (resolve = r)))
    const s = useMatchesStore()
    const p = s.send('m1', 'hi')
    expect(s.messages.m1).toHaveLength(1)
    expect(s.messages.m1[0].id.startsWith('tmp-')).toBe(true)
    resolve(msg(5, 'me'))
    await p
    expect(s.messages.m1.map((m) => m.id)).toEqual(['msg-0005'])
  })

  it('does not duplicate when the WS echo arrives before the response', async () => {
    let resolve!: (m: Message) => void
    vi.mocked(socialApi.sendMessage).mockReturnValue(new Promise((r) => (resolve = r)))
    const s = useMatchesStore()
    s.messages.m1 = []
    const p = s.send('m1', 'hi')
    s.applyEvent({ type: 'message', message: msg(5, 'me') })
    resolve(msg(5, 'me'))
    await p
    expect(s.messages.m1.map((m) => m.id)).toEqual(['msg-0005'])
  })

  it('removes the optimistic message and rethrows on failure', async () => {
    vi.mocked(socialApi.sendMessage).mockRejectedValue(new Error('403'))
    const s = useMatchesStore()
    await expect(s.send('m1', 'hi')).rejects.toThrow()
    expect(s.messages.m1).toEqual([])
  })
})

describe('matches store websocket events', () => {
  it('merges a message once, updates the preview and moves the match to the top', () => {
    const s = useMatchesStore()
    s.matches = [match('m1'), match('m2')]
    s.messages.m2 = [msg(1, 'other', 'm2')]
    const ev = { type: 'message', message: msg(2, 'other', 'm2') } as const
    s.applyEvent(ev)
    s.applyEvent(ev)
    expect(s.messages.m2).toHaveLength(2)
    expect(s.matches[0].match_id).toBe('m2')
    expect(s.matches[0].last_message?.id).toBe('msg-0002')
  })

  it('flags unread for incoming messages outside the open chat only', () => {
    const s = useMatchesStore()
    s.matches = [match('m1'), match('m2')]
    s.setActive('m1')
    s.applyEvent({ type: 'message', message: msg(1, 'other', 'm1') })
    s.applyEvent({ type: 'message', message: msg(2, 'other', 'm2') })
    s.applyEvent({ type: 'message', message: msg(3, 'me', 'm2') })
    expect(s.unread).toMatchObject({ m1: false, m2: true })
    expect(s.hasUnread).toBe(true)
    s.markRead('m2')
    expect(s.hasUnread).toBe(false)
  })

  it('opening a chat clears its unread flag', () => {
    const s = useMatchesStore()
    s.matches = [match('m1')]
    s.applyEvent({ type: 'message', message: msg(1, 'other', 'm1') })
    expect(s.hasUnread).toBe(true)
    s.setActive('m1')
    expect(s.hasUnread).toBe(false)
  })

  it('a match event adds the match once', () => {
    const s = useMatchesStore()
    s.applyEvent({ type: 'match', match: match('m9') })
    s.applyEvent({ type: 'match', match: match('m9') })
    expect(s.matches).toHaveLength(1)
  })

  it('a message for an unknown match reloads the list', async () => {
    vi.mocked(socialApi.getMatches).mockResolvedValue([match('m7')])
    const s = useMatchesStore()
    s.applyEvent({ type: 'message', message: msg(1, 'other', 'm7') })
    await vi.waitFor(() => expect(s.matches).toHaveLength(1))
    expect(s.unread.m7).toBe(true)
  })

  it('a wave event refreshes incoming waves', () => {
    vi.mocked(socialApi.getIncomingWaves).mockResolvedValue([])
    useMatchesStore().applyEvent({ type: 'wave', from_user_id: 'u2' })
    expect(socialApi.getIncomingWaves).toHaveBeenCalledOnce()
    expect(useNearbyStore().incoming).toEqual([])
  })

  it('removeUser drops the match and its history', () => {
    const s = useMatchesStore()
    s.matches = [match('m1', 'bad'), match('m2', 'good')]
    s.messages.m1 = [msg(1)]
    s.unread.m1 = true
    s.removeUser('bad')
    expect(s.matches.map((m) => m.match_id)).toEqual(['m2'])
    expect(s.messages.m1).toBeUndefined()
    expect(s.hasUnread).toBe(false)
  })
})

describe('matches store resync', () => {
  it('refetches the list and the open thread without duplicating messages', async () => {
    vi.mocked(socialApi.getMessages).mockResolvedValueOnce([msg(2), msg(1)])
    const s = useMatchesStore()
    await s.loadMessages('m1')
    s.setActive('m1')
    vi.mocked(socialApi.getMatches).mockResolvedValue([match('m1')])
    vi.mocked(socialApi.getMessages).mockResolvedValueOnce([msg(3), msg(2), msg(1)])
    await s.resync()
    expect(s.matches.map((m) => m.match_id)).toEqual(['m1'])
    expect(s.messages.m1.map((m) => m.id)).toEqual(['msg-0001', 'msg-0002', 'msg-0003'])
  })

  it('only refetches the list when no chat is open', async () => {
    const s = useMatchesStore()
    await s.resync()
    expect(socialApi.getMatches).toHaveBeenCalledOnce()
    expect(socialApi.getMessages).not.toHaveBeenCalled()
  })
})
