/** `mm:ss`, or `h:mm:ss` from an hour up. */
export function formatCountdown(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000))
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  const pad = (n: number) => String(n).padStart(2, '0')
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`
}

/** Clock time for today's messages, short date otherwise. */
export function formatMessageTime(iso: string, locale: string, now = new Date()): string {
  const d = new Date(iso)
  const sameDay = d.toDateString() === now.toDateString()
  return new Intl.DateTimeFormat(
    locale,
    sameDay ? { hour: '2-digit', minute: '2-digit' } : { day: 'numeric', month: 'short' },
  ).format(d)
}
