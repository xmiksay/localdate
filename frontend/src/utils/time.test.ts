import { describe, expect, it } from 'vitest'
import { formatCountdown, formatDateTime } from './time'

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
