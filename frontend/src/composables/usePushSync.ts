import { watch } from 'vue'
import { useT } from '@/i18n/typed'
import { useImpersonationStore } from '@/stores/impersonation'
import { usePushStore } from '@/stores/push'
import { toPushLang } from '@/utils/webPush'

/** Re-registers this device's push subscription on start and whenever the UI language changes. */
export function usePushSync() {
  const push = usePushStore()
  const impersonation = useImpersonationStore()
  const { locale } = useT()
  watch(
    locale,
    (l) => {
      // This device's subscription stays the admin's; the server refuses the call anyway.
      if (impersonation.isActive) return
      void push.resync(toPushLang(l)).catch(() => undefined)
    },
    { immediate: true },
  )
}
