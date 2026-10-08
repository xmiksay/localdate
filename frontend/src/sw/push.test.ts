import { describe, expect, it, vi } from 'vitest'
import { navigateTarget, notificationFor, openOrFocus, parsePayload, safePath } from './push'

const ORIGIN = 'https://app.example'

describe('service worker push handling', () => {
  it('parses the server payload into a notification', () => {
    const p = parsePayload(
      JSON.stringify({
        type: 'message',
        title: 'localdate',
        body: 'Nová zpráva',
        url: '/matches/m1',
        tag: 'message-m1',
      }),
      ORIGIN,
    )
    const n = notificationFor(p)
    expect(n.title).toBe('localdate')
    expect(n.options).toMatchObject({
      body: 'Nová zpráva',
      tag: 'message-m1',
      icon: '/icon-192.png',
      badge: '/badge-96.png',
      data: { url: '/matches/m1' },
    })
  })

  it('falls back to a generic notification on garbage', () => {
    for (const bad of [undefined, null, '', 'not json', '42', '{"url":"https://evil.example"}']) {
      const p = parsePayload(bad, ORIGIN)
      expect(p.title).toBe('localdate')
      expect(p.url).toBe('/')
    }
  })

  it('keeps same-origin paths with query and hash', () => {
    expect(safePath('/nearby', ORIGIN)).toBe('/nearby')
    expect(safePath('/matches/m1?x=1#end', ORIGIN)).toBe('/matches/m1?x=1#end')
    expect(safePath('https://app.example/settings', ORIGIN)).toBe('/settings')
  })

  it('sends everything off-origin or ambiguous to the app root', () => {
    for (const bad of [
      '//evil.example/x',
      'https://evil.example',
      'https://app.example.evil.com/x',
      '/\\evil.com',
      '\\\\evil.com',
      '/\t/evil.com',
      '/\n/evil.com',
      'javascript:alert(1)',
      42,
      null,
    ]) {
      expect(safePath(bad, ORIGIN), String(bad)).toBe('/')
    }
  })

  it('reuses an open tab of the app: routes it and focuses it', async () => {
    const tab = {
      url: 'https://app.example/settings',
      focus: vi.fn(async () => tab),
      postMessage: vi.fn(),
    }
    const lookalike = {
      url: 'https://app.example.evil.com/',
      focus: vi.fn(),
      postMessage: vi.fn(),
    }
    const clients = { matchAll: vi.fn(async () => [lookalike, tab]), openWindow: vi.fn() }
    await openOrFocus(clients, ORIGIN, '/matches/m1')
    expect(tab.postMessage).toHaveBeenCalledWith({ type: 'navigate', url: '/matches/m1' })
    expect(tab.focus).toHaveBeenCalled()
    expect(lookalike.focus).not.toHaveBeenCalled()
    expect(clients.openWindow).not.toHaveBeenCalled()
  })

  it('opens a new window when no tab is open', async () => {
    const clients = { matchAll: vi.fn(async () => []), openWindow: vi.fn(async () => null) }
    await openOrFocus(clients, ORIGIN, 'https://evil.example')
    expect(clients.openWindow).toHaveBeenCalledWith('/')
  })

  it('accepts only well-formed navigate messages on the page side', () => {
    expect(navigateTarget({ type: 'navigate', url: '/nearby' }, ORIGIN)).toBe('/nearby')
    expect(navigateTarget({ type: 'navigate', url: '//x' }, ORIGIN)).toBe('/')
    expect(navigateTarget({ type: 'other', url: '/nearby' }, ORIGIN)).toBeNull()
    expect(navigateTarget('navigate', ORIGIN)).toBeNull()
  })
})
