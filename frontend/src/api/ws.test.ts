import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { backoffDelay, CLOSE_BANNED, CLOSE_RESYNC, createWsClient, wsUrl } from './ws'
import type { WsEvent } from './types'

class FakeWebSocket {
  static instances: FakeWebSocket[] = []
  sent: string[] = []
  closed = false
  onopen: (() => void) | null = null
  onmessage: ((m: { data: unknown }) => void) | null = null
  onclose: ((e: { code: number }) => void) | null = null
  constructor(public url: string) {
    FakeWebSocket.instances.push(this)
  }
  send(d: string) {
    this.sent.push(d)
  }
  close() {
    this.closed = true
  }
  open() {
    this.onopen?.()
  }
  receive(data: unknown) {
    this.onmessage?.({ data: typeof data === 'string' ? data : JSON.stringify(data) })
  }
  drop(code = 1006) {
    this.onclose?.({ code })
  }
}

const last = () => FakeWebSocket.instances[FakeWebSocket.instances.length - 1]

beforeEach(() => {
  vi.useFakeTimers()
  FakeWebSocket.instances = []
  vi.stubGlobal('WebSocket', FakeWebSocket)
})
afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

function setup(getToken = vi.fn().mockResolvedValue('tok')) {
  const events: WsEvent[] = []
  const onBanned = vi.fn()
  const client = createWsClient({ getToken, onEvent: (e) => events.push(e), onBanned })
  return { client, events, getToken, onBanned }
}

describe('backoffDelay', () => {
  it('doubles from 1 s and caps at 30 s', () => {
    expect([0, 1, 2, 3, 4, 5, 6, 10].map(backoffDelay)).toEqual([
      1000, 2000, 4000, 8000, 16000, 30000, 30000, 30000,
    ])
  })
})

describe('wsUrl', () => {
  it('picks ws/wss from the page protocol', () => {
    expect(wsUrl({ protocol: 'http:', host: 'a:5173' })).toBe('ws://a:5173/api/ws')
    expect(wsUrl({ protocol: 'https:', host: 'a.cz' })).toBe('wss://a.cz/api/ws')
  })
})

describe('ws client', () => {
  it('sends auth as the first frame and dispatches typed events', async () => {
    const { client, events } = setup()
    client.start()
    await vi.advanceTimersByTimeAsync(0)
    last().open()
    expect(JSON.parse(last().sent[0])).toEqual({ type: 'auth', token: 'tok' })
    last().receive({ type: 'ready' })
    last().receive({ type: 'wave', from_user_id: 'u2' })
    last().receive('not json')
    last().receive({ type: 'bogus' })
    expect(events).toEqual([{ type: 'ready' }, { type: 'wave', from_user_id: 'u2' }])
  })

  it('reconnects with growing backoff and resets after ready', async () => {
    const { client } = setup()
    client.start()
    await vi.advanceTimersByTimeAsync(0)
    last().drop()
    await vi.advanceTimersByTimeAsync(999)
    expect(FakeWebSocket.instances).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(1)
    expect(FakeWebSocket.instances).toHaveLength(2)
    last().drop()
    await vi.advanceTimersByTimeAsync(2000)
    expect(FakeWebSocket.instances).toHaveLength(3)
    last().open()
    last().receive({ type: 'ready' })
    last().drop()
    await vi.advanceTimersByTimeAsync(1000)
    expect(FakeWebSocket.instances).toHaveLength(4)
  })

  it('forces a token refresh when the previous connection never became ready', async () => {
    const { client, getToken } = setup()
    client.start()
    await vi.advanceTimersByTimeAsync(0)
    expect(getToken).toHaveBeenLastCalledWith(false)
    last().drop()
    await vi.advanceTimersByTimeAsync(1000)
    expect(getToken).toHaveBeenLastCalledWith(true)
    last().receive({ type: 'ready' })
    last().drop()
    await vi.advanceTimersByTimeAsync(1000)
    expect(getToken).toHaveBeenLastCalledWith(false)
  })

  it('stop closes the socket and cancels reconnects', async () => {
    const { client } = setup()
    client.start()
    await vi.advanceTimersByTimeAsync(0)
    const ws = last()
    client.stop()
    expect(ws.closed).toBe(true)
    ws.drop()
    await vi.advanceTimersByTimeAsync(60_000)
    expect(FakeWebSocket.instances).toHaveLength(1)
  })

  it('retries later when no token is available', async () => {
    const getToken = vi.fn().mockResolvedValueOnce(null).mockResolvedValue('tok')
    const { client } = setup(getToken)
    client.start()
    await vi.advanceTimersByTimeAsync(0)
    expect(FakeWebSocket.instances).toHaveLength(0)
    await vi.advanceTimersByTimeAsync(1000)
    expect(FakeWebSocket.instances).toHaveLength(1)
  })

  it('stops for good and signals a ban on close code 4403', async () => {
    const { client, onBanned } = setup()
    client.start()
    await vi.advanceTimersByTimeAsync(0)
    last().open()
    last().receive({ type: 'ready' })
    last().drop(CLOSE_BANNED)
    expect(onBanned).toHaveBeenCalledOnce()
    await vi.advanceTimersByTimeAsync(60_000)
    expect(FakeWebSocket.instances).toHaveLength(1)
  })

  it('adds 0-5 s of jitter before reconnecting after a 1012 resync close', async () => {
    vi.spyOn(Math, 'random').mockReturnValue(0.5)
    const { client, onBanned } = setup()
    client.start()
    await vi.advanceTimersByTimeAsync(0)
    last().open()
    last().receive({ type: 'ready' })
    last().drop(CLOSE_RESYNC)
    // backoff 1000 ms + 0.5 * 5000 ms jitter
    await vi.advanceTimersByTimeAsync(3499)
    expect(FakeWebSocket.instances).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(1)
    expect(FakeWebSocket.instances).toHaveLength(2)
    expect(onBanned).not.toHaveBeenCalled()
    vi.restoreAllMocks()
  })

  it('does not signal a ban on other close codes', async () => {
    const { client, onBanned } = setup()
    client.start()
    await vi.advanceTimersByTimeAsync(0)
    last().drop(4401)
    expect(onBanned).not.toHaveBeenCalled()
  })
})
