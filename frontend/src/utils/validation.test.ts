import { describe, expect, it } from 'vitest'
import {
  ageFromBirthDate,
  checkPhotoFile,
  invalidAreaFields,
  isAdult,
  isValidPassword,
  isValidUsername,
  moveItem,
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
  it('normalizes and validates usernames', () => {
    expect(normalizeUsername('  Bob_1 ')).toBe('bob_1')
    expect(isValidUsername(' Bob_1 ')).toBe(true)
    expect(isValidUsername('ab')).toBe(false)
    expect(isValidUsername('bad name')).toBe(false)
    expect(isValidUsername('a'.repeat(33))).toBe(false)
  })
  it('validates password length', () => {
    expect(isValidPassword('123456789')).toBe(false)
    expect(isValidPassword('1234567890')).toBe(true)
    expect(isValidPassword('x'.repeat(129))).toBe(false)
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
