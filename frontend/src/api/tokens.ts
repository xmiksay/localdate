// Plain module (not a store) so the API client has no dependency on Pinia.
// localStorage is a deliberate product decision, see docs/architecture/auth.md.
const ACCESS = 'localdate.access'
const REFRESH = 'localdate.refresh'

function read(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

export const tokenStorage = {
  access: () => read(ACCESS),
  refresh: () => read(REFRESH),
  set(access: string, refresh: string) {
    try {
      localStorage.setItem(ACCESS, access)
      localStorage.setItem(REFRESH, refresh)
    } catch {
      /* storage unavailable: session lives until reload */
    }
  },
  clear() {
    try {
      localStorage.removeItem(ACCESS)
      localStorage.removeItem(REFRESH)
    } catch {
      /* nothing to clear */
    }
  },
}
