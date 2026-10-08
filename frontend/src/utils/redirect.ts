const MAX_REDIRECT = 512

/**
 * An in-app path or null: a single leading `/`, printable ASCII only, no `\`, ≤ 512 chars — the
 * rule the server applies to OAuth redirects. `//host` and `/\host` are protocol-relative to
 * browsers, so they would leave the origin.
 */
export function inAppPath(raw: unknown): string | null {
  if (typeof raw !== 'string' || raw.length > MAX_REDIRECT) return null
  if (!raw.startsWith('/') || raw.startsWith('//') || raw.includes('\\')) return null
  return /^[\x21-\x7e]+$/.test(raw) ? raw : null
}

/** The `?redirect=` target after login, if it stays inside the app. */
export function safeRedirect(raw: unknown): string {
  return inAppPath(raw) ?? '/'
}
