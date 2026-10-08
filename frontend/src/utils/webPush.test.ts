import { describe, expect, it } from 'vitest'
import {
  base64UrlToBytes,
  detectSupport,
  sameKey,
  subscriptionBody,
  toPushLang,
  type PushEnv,
} from './webPush'

const env = (o: Partial<PushEnv>): PushEnv => ({
  userAgent: 'Mozilla/5.0 (X11; Linux x86_64) Chrome/140',
  maxTouchPoints: 0,
  standalone: false,
  hasPush: true,
  ...o,
})
const IPHONE = 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Safari/604.1'
const IPAD = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Safari/605.1.15'

describe('push support detection', () => {
  it('asks iOS users to install to the home screen first', () => {
    expect(detectSupport(env({ userAgent: IPHONE, hasPush: false }))).toBe('ios_needs_install')
    expect(detectSupport(env({ userAgent: IPAD, maxTouchPoints: 5 }))).toBe('ios_needs_install')
    expect(detectSupport(env({ userAgent: IPHONE, standalone: true }))).toBe('supported')
  })

  it('a desktop Mac or other browser is judged by its APIs', () => {
    expect(detectSupport(env({ userAgent: IPAD, maxTouchPoints: 0 }))).toBe('supported')
    expect(detectSupport(env({}))).toBe('supported')
    expect(detectSupport(env({ hasPush: false }))).toBe('unsupported')
  })
})

describe('key helpers', () => {
  it('decodes base64url with or without padding', () => {
    expect([...base64UrlToBytes('_-8')]).toEqual([255, 239])
    expect([...base64UrlToBytes('AQID')]).toEqual([1, 2, 3])
    expect([...base64UrlToBytes('AQ==')]).toEqual([1])
  })

  it('compares a subscription key with the server key', () => {
    const key = new Uint8Array([1, 2, 3]).buffer
    expect(sameKey(key, 'AQID')).toBe(true)
    expect(sameKey(key, 'AQIE')).toBe(false)
    expect(sameKey(null, 'AQID')).toBe(false)
  })

  it('builds the request body from a browser subscription', () => {
    const sub = {
      endpoint: 'https://fcm.googleapis.com/x',
      toJSON: () => ({
        endpoint: 'https://fcm.googleapis.com/x',
        keys: { p256dh: 'P', auth: 'A' },
      }),
    } as unknown as PushSubscription
    expect(subscriptionBody(sub, 'en')).toEqual({
      endpoint: 'https://fcm.googleapis.com/x',
      keys: { p256dh: 'P', auth: 'A' },
      lang: 'en',
    })
    expect(toPushLang('en')).toBe('en')
    expect(toPushLang('de')).toBe('cs')
  })
})
