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

/** Full date and time, e.g. for moderation timestamps. */
export function formatDateTime(iso: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }).format(
    new Date(iso),
  )
}

/** The server refuses an end-of-day window this close to midnight (`409 too_close_to_midnight`). */
export const END_OF_DAY_MIN_MS = 30 * 60_000

/** The server never lets a window reach further ahead than this. */
export const MAX_WINDOW_MS = 12 * 3_600_000

/** The next local midnight after `now`. */
export const nextMidnight = (now: Date) =>
  new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1)

/** Milliseconds from `now` to the next local midnight. */
export const msUntilMidnight = (now: Date) => nextMidnight(now).getTime() - now.getTime()

export const canRunUntilMidnight = (now: Date) => msUntilMidnight(now) >= END_OF_DAY_MIN_MS

/** When an end-of-day window started at `now` ends, as the server computes it. */
export const endOfDayEnd = (now: Date) =>
  new Date(Math.min(nextMidnight(now).getTime(), now.getTime() + MAX_WINDOW_MS))

/**
 * Whether to offer "until end of day": only with a device time zone to send, outside the last
 * 30 min, and not before `refusedUntil` (epoch ms) passes once the server refused it.
 */
export const endOfDayOffered = (now: Date, tz: string | undefined, refusedUntil: number | null) =>
  !!tz && canRunUntilMidnight(now) && (refusedUntil === null || now.getTime() >= refusedUntil)

/**
 * When the end-of-day option or its label next changes: 30 min before midnight (option goes), at
 * midnight (it returns), and every minute while the 12 h cap, not midnight, sets the end time.
 */
export function nextEndOfDayChange(now: Date): Date {
  const midnight = nextMidnight(now).getTime()
  const t = now.getTime()
  if (t + MAX_WINDOW_MS < midnight) return new Date(Math.floor(t / 60_000) * 60_000 + 60_000)
  const cutoff = midnight - END_OF_DAY_MIN_MS
  return new Date(t < cutoff ? cutoff : midnight)
}

/** Wall-clock time like `20:00` in the given locale. */
export const formatClock = (d: Date | string, locale: string) =>
  new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' }).format(new Date(d))
