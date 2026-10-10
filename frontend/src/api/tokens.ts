import type { Impersonation, User } from './types'

// Plain module (not a store) so the API client has no dependency on Pinia.
// localStorage is a deliberate product decision, see docs/architecture/auth.md.
const ACCESS = 'localdate.access'
/** The admin's own access token; other tabs watch it to notice a logout. */
export const ADMIN_ACCESS_KEY = ACCESS
const REFRESH = 'localdate.refresh'
const IMPERSONATION = 'localdate.impersonation'

function read(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

/** An admin acting as `user`, plus who the admin was, to restore the session afterwards. */
export interface StoredImpersonation extends Impersonation {
  admin: User | null
}

/**
 * Lives in sessionStorage, layered over the admin's own tokens, which stay untouched in
 * localStorage: other tabs keep the admin session, the admin's refresh token keeps rotating there,
 * and this tab survives a reload still acting as the target.
 */
export const impersonationStorage = {
  get(): StoredImpersonation | null {
    try {
      const raw = sessionStorage.getItem(IMPERSONATION)
      return raw ? (JSON.parse(raw) as StoredImpersonation) : null
    } catch {
      return null
    }
  },
  set(v: StoredImpersonation) {
    try {
      sessionStorage.setItem(IMPERSONATION, JSON.stringify(v))
    } catch {
      /* storage unavailable: impersonation lives until reload */
    }
  },
  clear() {
    try {
      sessionStorage.removeItem(IMPERSONATION)
    } catch {
      /* nothing to clear */
    }
  },
}

export const tokenStorage = {
  access: () => impersonationStorage.get()?.access_token ?? read(ACCESS),
  /** Never while impersonating: that token has no refresh, the admin's must not be used for it. */
  refresh: () => (impersonationStorage.get() ? null : read(REFRESH)),
  set(access: string, refresh: string) {
    try {
      localStorage.setItem(ACCESS, access)
      localStorage.setItem(REFRESH, refresh)
    } catch {
      /* storage unavailable: session lives until reload */
    }
  },
  /** The admin session ending ends any impersonation riding on it. */
  clear() {
    impersonationStorage.clear()
    try {
      localStorage.removeItem(ACCESS)
      localStorage.removeItem(REFRESH)
    } catch {
      /* nothing to clear */
    }
  },
}
