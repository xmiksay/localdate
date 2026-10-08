import type { WsEvent } from './types'

const MAX_BACKOFF_MS = 30_000
const EVENT_TYPES = ['ready', 'message', 'match', 'wave']
/** Server close code for a suspended account, see docs/api.md. */
export const CLOSE_BANNED = 4403
/** Server close code: it may have missed events for this socket; reconnect and refetch. */
export const CLOSE_RESYNC = 1012
/** A replica closes all its sockets at once with 1012; spread their reconnects over this window. */
export const RESYNC_JITTER_MS = 5000

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
  /** Called once when the server closes with 4403; the client stops and does not reconnect. */
  onBanned: () => void
}

export function createWsClient({ getToken, onEvent, onBanned }: WsClientOptions) {
  let socket: WebSocket | null = null
  let timer: ReturnType<typeof setTimeout> | undefined
  let attempt = 0
  let running = false
  let gotReady = true

  function scheduleReconnect(extraDelay = 0) {
    if (!running) return
    timer = setTimeout(connect, backoffDelay(attempt++) + extraDelay)
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
    ws.onclose = (e) => {
      if (socket !== ws) return
      socket = null
      if (e.code === CLOSE_BANNED) {
        running = false
        onBanned()
        return
      }
      scheduleReconnect(e.code === CLOSE_RESYNC ? Math.random() * RESYNC_JITTER_MS : 0)
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
