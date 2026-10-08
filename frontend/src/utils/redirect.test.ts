import { describe, expect, it } from 'vitest'
import { safeRedirect } from './redirect'

describe('safeRedirect', () => {
  it('keeps internal paths with their query', () => {
    expect(safeRedirect('/settings')).toBe('/settings')
    expect(safeRedirect('/auth/email/link?token=abc')).toBe('/auth/email/link?token=abc')
  })
  it('falls back to / for anything that could leave the origin', () => {
    for (const bad of ['//evil.com', '/\\evil.com', 'https://evil.com', 'settings', '']) {
      expect(safeRedirect(bad)).toBe('/')
    }
  })
  it('falls back to / for non-string query values', () => {
    expect(safeRedirect(undefined)).toBe('/')
    expect(safeRedirect(null)).toBe('/')
    expect(safeRedirect(['/settings'])).toBe('/')
  })
})
