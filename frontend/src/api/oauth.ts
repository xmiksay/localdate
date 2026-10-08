import { post, request } from './client'
import type { MailLang, OAuthExchange, OAuthProvider, Tokens } from './types'

/**
 * Login / sign-up start: the browser navigates here itself (no fetch), so the server can set the
 * flow cookie and answer with the 302 to the provider.
 */
export function oauthStartUrl(provider: OAuthProvider, redirect?: string | null): string {
  const base = `/api/auth/oauth/${provider}/start`
  return redirect ? `${base}?${new URLSearchParams({ redirect })}` : base
}

/**
 * Linking needs the bearer token, so it is a POST that answers with the provider URL. `lang` is
 * the language of the "new sign-in method" notice mailed to the account's linked addresses.
 */
export const oauthLink = (provider: OAuthProvider, body: { redirect?: string; lang?: MailLang }) =>
  post<{ url: string }>(`/auth/oauth/${provider}/link`, body)

// A same-origin fetch sends the HttpOnly `ld_oauth` flow cookie the server checks here.
export const oauthExchange = (code: string) =>
  request<OAuthExchange>('/auth/oauth/exchange', { method: 'POST', body: { code }, anon: true })
export const oauthSignup = (body: { token: string; username: string }) =>
  request<Tokens>('/auth/oauth/signup', { method: 'POST', body, anon: true })
