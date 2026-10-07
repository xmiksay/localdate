import { ApiError } from '@/api/client'
import { ERROR_CODES } from '@/api/types'
import { i18n } from '@/i18n'

/** Translated message for any thrown value; unknown server codes fall back to `error.unknown`. */
export function errorMessage(e: unknown): string {
  const t = i18n.global.t
  const code = e instanceof ApiError ? e.code : 'unknown'
  return t(`error.${(ERROR_CODES as string[]).includes(code) ? code : 'unknown'}`)
}
