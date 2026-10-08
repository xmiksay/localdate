import { OAUTH_ERRORS, OAUTH_PROVIDERS, type OAuthError, type OAuthProvider } from '@/api/types'
import { inAppPath } from './redirect'

/** What the OAuth callback put into the `/auth/oauth/done#…` fragment (docs/api.md). */
export type OAuthOutcome =
  | { kind: 'code'; code: string; redirect: string | null }
  | { kind: 'linked'; provider: OAuthProvider; redirect: string | null }
  | { kind: 'error'; error: OAuthError | 'unknown'; redirect: string | null }
  | { kind: 'none' }

export function parseOAuthFragment(hash: string): OAuthOutcome {
  const p = new URLSearchParams(hash.replace(/^#/, ''))
  // The server already filtered it; re-check because anyone can craft this URL.
  const redirect = inAppPath(p.get('redirect'))
  const code = p.get('code')
  if (code) return { kind: 'code', code, redirect }
  const linked = OAUTH_PROVIDERS.find((o) => o === p.get('linked'))
  if (linked) return { kind: 'linked', provider: linked, redirect }
  const error = p.get('error')
  if (error !== null) {
    const known = OAUTH_ERRORS.find((e) => e === error)
    return { kind: 'error', error: known ?? 'unknown', redirect }
  }
  return { kind: 'none' }
}
