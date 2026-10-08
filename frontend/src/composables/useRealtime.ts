import { onScopeDispose, watch } from 'vue'
import { freshAccessToken, signalBanned } from '@/api/client'
import { createWsClient } from '@/api/ws'
import { useAuthStore } from '@/stores/auth'
import { useMatchesStore } from '@/stores/matches'
import { useMeStore } from '@/stores/me'
import { watchLongHidden } from '@/utils/visibility'

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

  const wanted = () => auth.isAuthed && me.isOnboarded
  watch(wanted, (on) => (on ? client.start() : client.stop()), { immediate: true })
  // A backgrounded or frozen PWA must not keep its socket: an open socket counts as online and so
  // suppresses Web Push. On return, `ready` triggers the usual resync of missed events.
  const unwatchHidden = watchLongHidden(document, client.stop, () => {
    if (wanted()) client.start()
  })
  onScopeDispose(() => {
    unwatchHidden()
    client.stop()
  })
}
