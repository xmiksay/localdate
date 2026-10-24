import { describe, expect, it } from 'vitest'
import { isolate } from './bidi'

describe('isolate', () => {
  it('wraps the text in first-strong isolate marks', () => {
    expect(isolate('eva')).toBe('\u2068eva\u2069')
    expect(isolate('')).toBe('\u2068\u2069')
  })
  it('drops bidi controls inside the name, so it cannot close the isolate early', () => {
    expect(isolate('\u2069\u202Eevil')).toBe('\u2068evil\u2069')
    expect(isolate('a\u200Fb\u2066c')).toBe('\u2068abc\u2069')
  })
})
