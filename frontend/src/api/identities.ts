import { del, get, post } from './client'
import type { IdentitiesResponse, Identity, MailLang } from './types'

export const getIdentities = () => get<IdentitiesResponse>('/me/identities')
export const linkEmail = (body: { email: string; lang: MailLang }) =>
  post<void>('/me/identities/email', body)
export const confirmEmailLink = (token: string) =>
  post<Identity>('/me/identities/email/confirm', { token })
export const deleteIdentity = (id: string) => del(`/me/identities/${id}`)
