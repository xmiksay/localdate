import {
  OAUTH_ERRORS,
  OAUTH_PROVIDERS,
  PHOTO_IMPORT_OUTCOMES,
  type OAuthError,
  type OAuthProvider,
  type PhotoImportOutcome,
} from '@/api/types'
import { inAppPath } from './redirect'

/** What the OAuth callback put into the `/auth/oauth/done#…` fragment (docs/api.md). */
export type OAuthOutcome =
  | { kind: 'code'; code: string; redirect: string | null; photo: PhotoImportOutcome | null }
  | {
      kind: 'linked'
      provider: OAuthProvider
      redirect: string | null
      photo: PhotoImportOutcome | null
    }
  | { kind: 'error'; error: OAuthError | 'unknown'; redirect: string | null }
  | { kind: 'none' }

export function parseOAuthFragment(hash: string): OAuthOutcome {
  const p = new URLSearchParams(hash.replace(/^#/, ''))
  // The server already filtered it; re-check because anyone can craft this URL.
  const redirect = inAppPath(p.get('redirect'))
  // Only present when the import was asked for; an unknown value is ignored.
  const photo = PHOTO_IMPORT_OUTCOMES.find((o) => o === p.get('photo')) ?? null
  const code = p.get('code')
  if (code) return { kind: 'code', code, redirect, photo }
  const linked = OAUTH_PROVIDERS.find((o) => o === p.get('linked'))
  if (linked) return { kind: 'linked', provider: linked, redirect, photo }
  const error = p.get('error')
  if (error !== null) {
    const known = OAUTH_ERRORS.find((e) => e === error)
    return { kind: 'error', error: known ?? 'unknown', redirect }
  }
  return { kind: 'none' }
}
