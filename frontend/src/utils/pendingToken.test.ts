import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { pendingToken, tokenFromHash } from './pendingToken'

beforeEach(() => sessionStorage.clear())
afterEach(() => vi.restoreAllMocks())

describe('tokenFromHash', () => {
  it('reads the token from the fragment', () => {
    expect(tokenFromHash('#token=abc-_9')).toBe('abc-_9')
    expect(tokenFromHash('token=abc')).toBe('abc')
  })
  it('is empty without a token', () => {
    expect(tokenFromHash('')).toBe('')
    expect(tokenFromHash('#other=1')).toBe('')
  })
})

describe('pendingToken', () => {
  it('stores, reads and clears per kind', () => {
    pendingToken.set('link', 'l1')
    pendingToken.set('signup', 's1')
    expect(pendingToken.get('link')).toBe('l1')
    pendingToken.clear('link')
    expect(pendingToken.get('link')).toBeNull()
    expect(pendingToken.get('signup')).toBe('s1')
  })
  it('survives unavailable storage', () => {
    const boom = () => {
      throw new Error('denied')
    }
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(boom)
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(boom)
    vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(boom)
    expect(() => pendingToken.set('link', 'x')).not.toThrow()
    expect(pendingToken.get('link')).toBeNull()
    expect(() => pendingToken.clear('link')).not.toThrow()
  })
})
