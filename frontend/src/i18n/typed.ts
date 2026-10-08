import { useI18n } from 'vue-i18n'
import type cs from './cs'

type Leaves<T, P extends string = ''> = {
  [K in keyof T & string]: T[K] extends string ? `${P}${K}` : Leaves<T[K], `${P}${K}.`>
}[keyof T & string]

/** Every message path in `i18n/cs/`, the source of truth (e.g. `'nearby.title'`). */
export type MessageKey = Leaves<typeof cs>

type Named = Record<string, unknown>

/**
 * vue-i18n's own `t` also accepts any `string`, so typing its resources never rejects a typo;
 * this signature does.
 */
export interface TypedT {
  (key: MessageKey, plural?: number): string
  (key: MessageKey, named: Named, plural?: number): string
}

export function useT() {
  const { t, locale } = useI18n()
  return { t: t as TypedT, locale }
}

/** Interest keys come from the server; the i18n test guarantees all seeded ones exist. */
export const interestKey = (key: string) => `interest.${key}` as MessageKey
