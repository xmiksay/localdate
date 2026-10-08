import { ApiError } from '@/api/client'
import { ERROR_CODES } from '@/api/types'
import { i18n } from '@/i18n'
import type { TypedT } from '@/i18n/typed'

/** Translated message for any thrown value; unknown server codes fall back to `error.unknown`. */
export function errorMessage(e: unknown): string {
  const t = i18n.global.t as TypedT
  const raw = e instanceof ApiError ? e.code : 'unknown'
  const code = ERROR_CODES.find((c) => c === raw) ?? 'unknown'
  return t(`error.${code}`)
}
