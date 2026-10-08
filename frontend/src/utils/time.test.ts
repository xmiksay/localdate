import { describe, expect, it } from 'vitest'
import {
  canRunUntilMidnight,
  endOfDayEnd,
  endOfDayOffered,
  formatClock,
  formatCountdown,
  formatDateTime,
  msUntilMidnight,
  nextEndOfDayChange,
  nextMidnight,
} from './time'

describe('formatCountdown', () => {
  it('formats minutes and seconds', () => {
    expect(formatCountdown(65_000)).toBe('01:05')
  })
  it('rounds partial seconds up', () => {
    expect(formatCountdown(1_200)).toBe('00:02')
  })
  it('shows hours from 60 minutes', () => {
    expect(formatCountdown(3_661_000)).toBe('1:01:01')
  })
  it('never goes negative', () => {
    expect(formatCountdown(-5)).toBe('00:00')
  })
})

describe('formatDateTime', () => {
  it('includes date and time in the given locale', () => {
    const s = formatDateTime('2026-03-15T12:00:00Z', 'en')
    expect(s).toContain('2026')
    expect(s).toMatch(/Mar/)
    expect(s).toMatch(/\d{1,2}:\d{2}/)
  })
})

describe('until end of day', () => {
  // Local-time constructors keep these independent of the machine's time zone.
  it('measures the time to the next local midnight', () => {
    expect(msUntilMidnight(new Date(2026, 5, 1, 22, 0))).toBe(2 * 3_600_000)
    expect(msUntilMidnight(new Date(2026, 5, 1, 0, 0))).toBe(24 * 3_600_000)
  })

  it('is offered until 30 minutes before midnight', () => {
    expect(canRunUntilMidnight(new Date(2026, 5, 1, 23, 30))).toBe(true)
    expect(canRunUntilMidnight(new Date(2026, 5, 1, 23, 30, 1))).toBe(false)
    expect(canRunUntilMidnight(new Date(2026, 5, 2, 0, 1))).toBe(true)
  })
})

describe('end-of-day label and availability', () => {
  it('ends at midnight, or 12 h from now when midnight is further', () => {
    expect(endOfDayEnd(new Date(2026, 5, 1, 20, 0))).toEqual(new Date(2026, 5, 2))
    expect(endOfDayEnd(new Date(2026, 5, 1, 8, 15))).toEqual(new Date(2026, 5, 1, 20, 15))
  })

  it('formats the end as a wall-clock time in the locale', () => {
    expect(formatClock(new Date(2026, 5, 1, 20, 0), 'cs')).toBe('20:00')
    expect(formatClock(new Date(2026, 5, 1, 20, 0), 'en-GB')).toBe('20:00')
  })

  it('needs a device time zone', () => {
    const noon = new Date(2026, 5, 1, 12, 0)
    expect(endOfDayOffered(noon, 'Europe/Prague', null)).toBe(true)
    expect(endOfDayOffered(noon, undefined, null)).toBe(false)
    expect(endOfDayOffered(noon, '', null)).toBe(false)
  })

  it('stays hidden after a refusal until the next midnight passes', () => {
    const refusedAt = new Date(2026, 5, 1, 23, 20)
    const until = nextMidnight(refusedAt).getTime()
    // Our own 30 min check alone would offer it again (device clock behind the server).
    expect(endOfDayOffered(new Date(2026, 5, 1, 23, 0), 'Europe/Prague', until)).toBe(false)
    expect(endOfDayOffered(new Date(2026, 5, 1, 23, 59), 'Europe/Prague', until)).toBe(false)
    expect(endOfDayOffered(new Date(2026, 5, 2, 0, 0), 'Europe/Prague', until)).toBe(true)
  })
})

describe('nextEndOfDayChange', () => {
  it('ticks each minute while the 12 h cap sets the end', () => {
    expect(nextEndOfDayChange(new Date(2026, 5, 1, 8, 15, 30))).toEqual(new Date(2026, 5, 1, 8, 16))
  })

  it('then waits for 30 min before midnight, then for midnight', () => {
    expect(nextEndOfDayChange(new Date(2026, 5, 1, 12, 0))).toEqual(new Date(2026, 5, 1, 23, 30))
    expect(nextEndOfDayChange(new Date(2026, 5, 1, 23, 30))).toEqual(new Date(2026, 5, 2))
  })
})
