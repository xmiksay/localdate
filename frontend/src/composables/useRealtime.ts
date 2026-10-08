import { onScopeDispose, watch } from 'vue'
import { freshAccessToken, signalBanned } from '@/api/client'
import { createWsClient } from '@/api/ws'
import { useAuthStore } from '@/stores/auth'
import { useMatchesStore } from '@/stores/matches'
import { useMeStore } from '@/stores/me'

/** Keeps the WebSocket open while the user is logged in and onboarded. */
export function useRealtime() {
  const auth = useAuthStore()
  const me = useMeStore()
  const matches = useMatchesStore()
  const client = createWsClient({
    getToken: freshAccessToken,
    onEvent: (ev) => {
      // After a reconnect we may have missed events; resync the list and the open chat.
      if (ev.type === 'ready') void matches.resync().catch(() => undefined)
      else matches.applyEvent(ev)
    },
    onBanned: signalBanned,
  })

  watch(
    () => auth.isAuthed && me.isOnboarded,
    (on) => (on ? client.start() : client.stop()),
    { immediate: true },
  )
  onScopeDispose(client.stop)
}
