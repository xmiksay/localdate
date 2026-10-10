import { describe, expect, it } from 'vitest'
import {
  ageFromBirthDate,
  checkPhotoFile,
  invalidAreaFields,
  isAdult,
  isValidEmail,
  isValidLogin,
  isValidPassword,
  isValidUsername,
  moveItem,
  newPasswordError,
  normalizeEmail,
  normalizeLogin,
  normalizeUsername,
} from './validation'

describe('age', () => {
  const now = new Date(2026, 5, 15)
  it('counts whole years', () => {
    expect(ageFromBirthDate('2000-06-15', now)).toBe(26)
    expect(ageFromBirthDate('2000-06-16', now)).toBe(25)
  })
  it('rejects malformed and impossible dates', () => {
    expect(ageFromBirthDate('nope', now)).toBeNull()
    expect(ageFromBirthDate('2001-02-30', now)).toBeNull()
  })
  it('18+ boundary', () => {
    expect(isAdult('2008-06-15', now)).toBe(true)
    expect(isAdult('2008-06-16', now)).toBe(false)
    expect(isAdult('', now)).toBe(false)
  })
})

describe('credentials', () => {
  it('trims and NFC-normalizes usernames but keeps their case', () => {
    expect(normalizeUsername('  Petr Novák ')).toBe('Petr Novák')
    expect(normalizeUsername('Nova\u0301k')).toBe('Novák')
    expect(normalizeUsername(' Petr  Novák ')).toBe('Petr  Novák')
    // U+FEFF is not White_Space: kept like the server keeps it, then refused.
    expect(normalizeUsername('\uFEFFeva')).toBe('\uFEFFeva')
  })
  it('accepts free-form usernames', () => {
    for (const ok of ['Petr Novák', '👨\u200D👩\u200D👧', '🦊', 'a', 'a@b.cz', 'dash-name!', 'x'.repeat(64), '🦊'.repeat(64)]) {
      expect(isValidUsername(ok), ok).toBe(true)
    }
  })
  it('rejects empty, overlong, invisible, control- and format-character usernames', () => {
    for (const bad of [
      '',
      '   ',
      '\u3000\u00A0',
      'x'.repeat(65),
      'a\nb',
      '\u0007',
      'a\u2028b',
      'admin\u200B',
      '\u202Enimda',
      '\u200B',
      '\uFEFFeva',
      '\u3164',
      '\u2800\u115F',
      '\u0301',
      '\u200D',
    ]) {
      expect(isValidUsername(bad), JSON.stringify(bad)).toBe(false)
    }
  })
  it('validates password length', () => {
    expect(isValidPassword('123456')).toBe(false)
    expect(isValidPassword('1234567')).toBe(true)
    expect(isValidPassword('x'.repeat(128))).toBe(true)
    expect(isValidPassword('x'.repeat(129))).toBe(false)
  })
})

describe('new password', () => {
  it('checks the policy before the repetition', () => {
    expect(newPasswordError('short', 'short')).toBe('invalid')
    expect(newPasswordError('x'.repeat(7), 'x'.repeat(8))).toBe('mismatch')
    expect(newPasswordError('x'.repeat(7), 'x'.repeat(7))).toBeNull()
  })

  it('counts code points like the server', () => {
    expect(isValidPassword('😀'.repeat(7))).toBe(true)
    expect(isValidPassword('😀'.repeat(129))).toBe(false)
  })
})

describe('forgot-password login', () => {
  it('sends the login as typed and accepts a username or an address', () => {
    expect(normalizeLogin(' Eva@Example.CZ ')).toBe('Eva@Example.CZ')
    expect(normalizeLogin(' Eva Nová ')).toBe('Eva Nová')
    expect(isValidLogin('eva@example.cz')).toBe(true)
    expect(isValidLogin('Eva Nová')).toBe(true)
    expect(isValidLogin('eva@')).toBe(true)
    // Too long for a username, still a valid address.
    expect(isValidLogin(`${'e'.repeat(60)}@example.cz`)).toBe(true)
    expect(isValidLogin('   ')).toBe(false)
    expect(isValidLogin('a\nb')).toBe(false)
  })
})

describe('email', () => {
  it('trims and lowercases', () => {
    expect(normalizeEmail('  Eva@Example.CZ ')).toBe('eva@example.cz')
  })
  it('accepts plain addresses', () => {
    for (const ok of [' Eva@Example.cz ', 'a.b+tag@mail.example.com', 'x_-1@a-b.c0.cz']) {
      expect(isValidEmail(ok)).toBe(true)
    }
  })
  it('rejects display names, quotes, lists and extra recipients', () => {
    for (const bad of ['n<victim@b.cz>', '"a"@b.cz', 'a,b@c.cz', 'a@b.cz,c@d.cz', 'x <a@b.cz>']) {
      expect(isValidEmail(bad)).toBe(false)
    }
  })
  it('rejects bad dots, hyphens, @ counts and single-label domains', () => {
    for (const bad of [
      '',
      'eva',
      '@b.cz',
      'a@b@c.cz',
      'eva@localhost',
      'a@-b.cz',
      'a@b-.cz',
      'a@b..cz',
      'a@b.cz.',
      '.a@b.cz',
      'a.@b.cz',
      'a..b@c.cz',
      'e va@b.cz',
      'a%b@c.cz',
      'a@1.2.3.4',
      'a@b.123',
    ]) {
      expect(isValidEmail(bad)).toBe(false)
    }
  })
  it('caps the local part at 64 and labels at 63 characters', () => {
    expect(isValidEmail(`${'x'.repeat(64)}@b.cz`)).toBe(true)
    expect(isValidEmail(`${'x'.repeat(65)}@b.cz`)).toBe(false)
    expect(isValidEmail(`a@${'b'.repeat(63)}.cz`)).toBe(true)
    expect(isValidEmail(`a@${'b'.repeat(64)}.cz`)).toBe(false)
  })
  it('caps the whole address at 254 characters', () => {
    const label = 'd'.repeat(63)
    const at254 = `${'x'.repeat(64)}@${label}.${label}.${'d'.repeat(61)}`
    expect(at254).toHaveLength(254)
    expect(isValidEmail(at254)).toBe(true)
    expect(isValidEmail(`${at254.slice(0, -2)}dd.cz`)).toBe(false)
  })
})

describe('photo check', () => {
  it('checks type and size', () => {
    expect(checkPhotoFile({ type: 'image/gif', size: 1 })).toBe('type')
    expect(checkPhotoFile({ type: 'image/png', size: 10 * 1024 * 1024 + 1 })).toBe('size')
    expect(checkPhotoFile({ type: 'image/webp', size: 10 })).toBe('ok')
  })
})

describe('moveItem', () => {
  it('moves within bounds only', () => {
    expect(moveItem([1, 2, 3], 2, 0)).toEqual([3, 1, 2])
    expect(moveItem([1, 2, 3], 0, -1)).toEqual([1, 2, 3])
    expect(moveItem([1, 2, 3], 2, 3)).toEqual([1, 2, 3])
  })
})

describe('area validation', () => {
  const ok = { name: ' Hlavní nádraží ', lat: 50.08, lon: 14.43, radius_m: 300 }
  it('accepts a valid area', () => {
    expect(invalidAreaFields(ok)).toEqual([])
    expect(invalidAreaFields({ name: 'x', lat: -90, lon: 180, radius_m: 50 })).toEqual([])
    expect(invalidAreaFields({ ...ok, name: 'x'.repeat(80), radius_m: 5000 })).toEqual([])
  })
  it('trims the name and enforces 1–80 chars', () => {
    expect(invalidAreaFields({ ...ok, name: '   ' })).toEqual(['name'])
    expect(invalidAreaFields({ ...ok, name: 'x'.repeat(81) })).toEqual(['name'])
  })
  it('counts code points like the server, so an emoji is one character', () => {
    expect(invalidAreaFields({ ...ok, name: '🎡'.repeat(80) })).toEqual([])
    expect(invalidAreaFields({ ...ok, name: '🎡'.repeat(81) })).toEqual(['name'])
  })
  it('checks coordinate ranges and rejects NaN', () => {
    expect(invalidAreaFields({ ...ok, lat: 90.1, lon: -180.1 })).toEqual(['lat', 'lon'])
    expect(invalidAreaFields({ ...ok, lat: NaN, lon: NaN })).toEqual(['lat', 'lon'])
  })
  it('requires a whole radius of 50–5000 m', () => {
    for (const r of [49, 5001, 120.5, NaN]) {
      expect(invalidAreaFields({ ...ok, radius_m: r })).toEqual(['radius_m'])
    }
  })
})
