import { get, patch, post, request } from './client'
import type { PushConfig, PushPrefs, PushSubscriptionBody } from './types'

export const getPushConfig = () => request<PushConfig>('/push/config', { anon: true })
export const subscribePush = (body: PushSubscriptionBody) =>
  post<void>('/me/push/subscriptions', body)
export const unsubscribePush = (endpoint: string) =>
  request<void>('/me/push/subscriptions', { method: 'DELETE', body: { endpoint } })
export const getPushPrefs = () => get<PushPrefs>('/me/push/prefs')
export const patchPushPrefs = (p: Partial<PushPrefs>) => patch<PushPrefs>('/me/push/prefs', p)
