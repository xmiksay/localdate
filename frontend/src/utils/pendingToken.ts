/** Email-link tokens arrive in the URL fragment (`#token=…`) so they never reach server logs. */
export function tokenFromHash(hash: string): string {
  return new URLSearchParams(hash.replace(/^#/, '')).get('token') ?? ''
}

export type PendingKind = 'signup' | 'link' | 'reset' | 'oauthSignup'
const KEYS: Record<PendingKind, string> = {
  signup: 'localdate.signupToken',
  link: 'localdate.linkToken',
  reset: 'localdate.resetToken',
  oauthSignup: 'localdate.oauthSignupToken',
}

/**
 * A token that must survive a reload or a detour through login. sessionStorage, not a query
 * string: it stays out of URLs and history and dies with the tab. Storage can be unavailable
 * (private mode, blocked site data), which only costs the resume.
 */
export const pendingToken = {
  get(kind: PendingKind): string | null {
    try {
      return sessionStorage.getItem(KEYS[kind])
    } catch {
      return null
    }
  },
  set(kind: PendingKind, token: string) {
    try {
      sessionStorage.setItem(KEYS[kind], token)
    } catch {
      /* resume just won't work */
    }
  },
  clear(kind: PendingKind) {
    try {
      sessionStorage.removeItem(KEYS[kind])
    } catch {
      /* nothing stored then */
    }
  },
}
