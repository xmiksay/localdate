import { watch } from 'vue'
import { useT } from '@/i18n/typed'
import { usePushStore } from '@/stores/push'
import { toPushLang } from '@/utils/webPush'

/** Re-registers this device's push subscription on start and whenever the UI language changes. */
export function usePushSync() {
  const push = usePushStore()
  const { locale } = useT()
  watch(locale, (l) => void push.resync(toPushLang(l)).catch(() => undefined), { immediate: true })
}
