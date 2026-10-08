import { createI18n } from 'vue-i18n'
import cs from './cs'
import en from './en'
import { czechPlural } from './plural'

export type Locale = 'cs' | 'en'
export const LOCALES: Locale[] = ['cs', 'en']
const KEY = 'localdate.locale'

function initialLocale(): Locale {
  try {
    const saved = localStorage.getItem(KEY)
    if (saved === 'cs' || saved === 'en') return saved
  } catch {
    /* storage unavailable */
  }
  return 'cs'
}

export const i18n = createI18n({
  legacy: false,
  locale: initialLocale(),
  fallbackLocale: 'cs',
  messages: { cs, en },
  pluralRules: { cs: czechPlural },
})

export function setLocale(l: Locale) {
  i18n.global.locale.value = l
  document.documentElement.lang = l
  try {
    localStorage.setItem(KEY, l)
  } catch {
    /* preference just won't persist */
  }
}
