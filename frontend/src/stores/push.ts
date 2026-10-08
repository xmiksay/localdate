import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import * as pushApi from '@/api/push'
import type { PushConfig, PushLang, PushPrefs } from '@/api/types'
import * as webPush from '@/utils/webPush'

/** This device's Web Push subscription plus the account's per-kind preferences. */
export const usePushStore = defineStore('push', () => {
  const support = ref(webPush.detectSupport(webPush.currentEnv()))
  const config = ref<PushConfig | null>(null)
  const permission = ref<NotificationPermission>('default')
  const subscribed = ref(false)
  const prefs = ref<PushPrefs | null>(null)

  const available = computed(() => support.value === 'supported' && config.value?.enabled === true)

  async function loadConfig(): Promise<PushConfig> {
    config.value ??= await pushApi.getPushConfig()
    if (support.value === 'supported') permission.value = webPush.permission()
    return config.value
  }

  async function loadPrefs() {
    prefs.value = await pushApi.getPushPrefs()
  }

  async function register(publicKey: string, lang: PushLang) {
    const sub = await webPush.subscribeBrowser(publicKey)
    await pushApi.subscribePush(webPush.subscriptionBody(sub, lang))
    subscribed.value = true
  }

  /**
   * On app start / language change: re-posts an existing subscription, since the push service may
   * have rotated its endpoint and the server stores the device's language. Never subscribes anew.
   */
  async function resync(lang: PushLang) {
    const cfg = await loadConfig()
    if (!available.value || permission.value !== 'granted' || !cfg.public_key) return
    if (!(await webPush.existingSubscription())) {
      subscribed.value = false
      return
    }
    await register(cfg.public_key, lang)
  }

  /** Must run straight from a click: Safari only shows the permission prompt inside a user gesture. */
  async function enable(lang: PushLang) {
    const asked = webPush.requestPermission()
    permission.value = await asked
    if (permission.value !== 'granted') return
    const cfg = await loadConfig()
    if (!available.value || !cfg.public_key) return
    await register(cfg.public_key, lang)
  }

  async function disable() {
    const sub = await webPush.existingSubscription()
    if (sub) {
      // Unsubscribing locally is what stops notifications; the server row also goes on the next 410.
      await pushApi.unsubscribePush(sub.endpoint).catch(() => undefined)
      await sub.unsubscribe()
    }
    subscribed.value = false
  }

  async function setPref(key: keyof PushPrefs, value: boolean) {
    prefs.value = await pushApi.patchPushPrefs({ [key]: value })
  }

  /**
   * Explicit logout only, called while the session is still valid: the server forgets this device,
   * then the browser drops the subscription, so nobody's notifications show up here any more.
   * Best effort: logging out must never fail because of push.
   */
  async function forgetDevice() {
    if (support.value === 'supported') await disable().catch(() => undefined)
    reset()
  }

  /**
   * Session lost without a logout (refresh failed, ban): keep the browser subscription. The next
   * login's `resync` re-registers it, which moves it to whoever logs in.
   */
  function reset() {
    config.value = null
    prefs.value = null
    subscribed.value = false
  }

  return {
    support,
    config,
    permission,
    subscribed,
    prefs,
    available,
    loadConfig,
    loadPrefs,
    resync,
    enable,
    disable,
    setPref,
    forgetDevice,
    reset,
  }
})
