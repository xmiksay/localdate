import { describe, expect, it } from 'vitest'
import { createI18n } from 'vue-i18n'
import cs from './cs'
import en from './en'
import { ERROR_CODES } from '@/api/types'

function keys(o: object, prefix = ''): string[] {
  return Object.entries(o).flatMap(([k, v]) =>
    typeof v === 'object' ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`],
  )
}

describe('i18n', () => {
  it('cs and en have identical key sets', () => {
    expect(keys(en).sort()).toEqual(keys(cs).sort())
  })
  it('has all 40 interests', () => {
    expect(Object.keys(cs.interest)).toHaveLength(40)
  })
  it('translates every error code', () => {
    const errs = Object.keys(cs.error)
    for (const c of ERROR_CODES) expect(errs).toContain(c)
  })
  // A message vue-i18n cannot compile throws during render and takes the whole component with it
  // (an unescaped `@` is linked-message syntax): every message must compile in both locales.
  it('compiles every message', () => {
    for (const locale of ['cs', 'en'] as const) {
      const { t } = createI18n({ legacy: false, locale, messages: { cs, en } }).global
      for (const k of keys(locale === 'cs' ? cs : en)) {
        expect(() => t(k, { username: 'eva' }), `${locale}: ${k}`).not.toThrow()
      }
    }
  })
  it('renders a literal @ before a username', () => {
    const { t } = createI18n({ legacy: false, locale: 'cs', messages: { cs, en } }).global
    expect(t('impersonation.actAsLabel', { username: 'eva' })).toBe('Přihlásit se jako @eva')
  })
})
