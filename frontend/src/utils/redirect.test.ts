import { describe, expect, it } from 'vitest'
import { inAppPath, safeRedirect } from './redirect'

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

describe('inAppPath', () => {
  it('accepts in-app paths up to 512 chars', () => {
    expect(inAppPath('/')).toBe('/')
    expect(inAppPath('/matches/abc?x=1#y')).toBe('/matches/abc?x=1#y')
    const long = '/' + 'a'.repeat(511)
    expect(inAppPath(long)).toBe(long)
  })
  it('rejects everything else', () => {
    for (const bad of [
      '',
      'settings',
      '//evil.com',
      '/\\evil.com',
      '/a\\b',
      'https://evil.com',
      '/with space',
      '/tab\there',
      '/nl\n',
      '/čeština',
      '/' + 'a'.repeat(512),
      42,
      undefined,
    ]) {
      expect(inAppPath(bad)).toBeNull()
    }
  })
})
