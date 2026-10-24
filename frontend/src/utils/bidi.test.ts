import { describe, expect, it } from 'vitest'
import { isolate } from './bidi'

describe('isolate', () => {
  it('wraps the text in first-strong isolate marks', () => {
    expect(isolate('‮evil')).toBe('⁨‮evil⁩')
    expect(isolate('')).toBe('⁨⁩')
  })
})
