/**
 * The `?redirect=` target after login, if it stays inside the app. `//host` and `/\host` are
 * protocol-relative to browsers, so they would leave the origin.
 */
export function safeRedirect(raw: unknown): string {
  if (typeof raw !== 'string' || !raw.startsWith('/')) return '/'
  if (raw.startsWith('//') || raw.startsWith('/\\')) return '/'
  return raw
}
