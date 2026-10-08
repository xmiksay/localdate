import { describe, expect, it } from 'vitest'
import { createI18n } from 'vue-i18n'
import cs from './cs'
import en from './en'
import { czechPlural } from './plural'

describe('czechPlural', () => {
  it('maps counts to one / few / many', () => {
    expect([0, 1, 2, 3, 4, 5, 11, 22].map((n) => czechPlural(n, 3))).toEqual([
      2, 0, 1, 1, 1, 2, 2, 2,
    ])
  })
  it('falls back to one / other for two choices', () => {
    expect([1, 2, 5].map((n) => czechPlural(n, 2))).toEqual([0, 1, 1])
  })
})

describe('shared interests badge', () => {
  const i18n = createI18n({
    legacy: false,
    locale: 'cs',
    messages: { cs, en },
    pluralRules: { cs: czechPlural },
  })
  const { t, locale } = i18n.global

  it('pluralises in Czech', () => {
    locale.value = 'cs'
    expect(t('nearby.sharedInterests', 1)).toBe('1 společný zájem')
    expect(t('nearby.sharedInterests', 3)).toBe('3 společné zájmy')
    expect(t('nearby.sharedInterests', 5)).toBe('5 společných zájmů')
  })
  it('pluralises in English', () => {
    locale.value = 'en'
    expect(t('nearby.sharedInterests', 1)).toBe('1 shared interest')
    expect(t('nearby.sharedInterests', 4)).toBe('4 shared interests')
  })
})
