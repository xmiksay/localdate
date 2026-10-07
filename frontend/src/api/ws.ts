import type { WsEvent } from './types'

const MAX_BACKOFF_MS = 30_000
const EVENT_TYPES = ['ready', 'message', 'match', 'wave']

export const backoffDelay = (attempt: number) => Math.min(MAX_BACKOFF_MS, 1000 * 2 ** attempt)

export function wsUrl(loc: Pick<Location, 'protocol' | 'host'> = location): string {
  return `${loc.protocol === 'https:' ? 'wss' : 'ws'}://${loc.host}/api/ws`
}

function parseEvent(data: unknown): WsEvent | null {
  if (typeof data !== 'string') return null
  try {
    const ev = JSON.parse(data)
    return EVENT_TYPES.includes(ev?.type) ? (ev as WsEvent) : null
  } catch {
    return null
  }
}

export interface WsClientOptions {
  /** `force` is set after a connection died before `ready`, i.e. the token was likely rejected. */
  getToken: (force: boolean) => Promise<string | null>
  onEvent: (e: WsEvent) => void
}

export function createWsClient({ getToken, onEvent }: WsClientOptions) {
  let socket: WebSocket | null = null
  let timer: ReturnType<typeof setTimeout> | undefined
  let attempt = 0
  let running = false
  let gotReady = true

  function scheduleReconnect() {
    if (!running) return
    timer = setTimeout(connect, backoffDelay(attempt++))
  }

  async function connect() {
    timer = undefined
    const token = await getToken(!gotReady)
    // stop() may have run while the token was being fetched
    if (!running) return
    if (!token) return scheduleReconnect()
    gotReady = false
    const ws = new WebSocket(wsUrl())
    socket = ws
    ws.onopen = () => ws.send(JSON.stringify({ type: 'auth', token }))
    ws.onmessage = (m) => {
      const ev = parseEvent(m.data)
      if (!ev) return
      if (ev.type === 'ready') {
        gotReady = true
        attempt = 0
      }
      onEvent(ev)
    }
    ws.onclose = () => {
      if (socket !== ws) return
      socket = null
      scheduleReconnect()
    }
  }

  function start() {
    if (running) return
    running = true
    attempt = 0
    gotReady = true
    void connect()
  }

  function stop() {
    running = false
    clearTimeout(timer)
    timer = undefined
    const ws = socket
    socket = null
    ws?.close()
  }

  return { start, stop }
}
