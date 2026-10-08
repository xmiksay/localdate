import { request } from './client'
import type { Credentials, EmailPreview, MailLang, Providers, Tokens } from './types'

export const register = (c: Credentials) =>
  request<Tokens>('/auth/register', { method: 'POST', body: c, anon: true })
export const login = (c: Credentials) =>
  request<Tokens>('/auth/login', { method: 'POST', body: c, anon: true })
export const logout = (refresh_token: string) =>
  request<void>('/auth/logout', { method: 'POST', body: { refresh_token }, anon: true })

export const getProviders = () => request<Providers>('/auth/providers', { anon: true })
export const emailStart = (body: { email: string; lang: MailLang }) =>
  request<void>('/auth/email/start', { method: 'POST', body, anon: true })
export const emailPreview = (token: string) =>
  request<EmailPreview>('/auth/email/preview', { method: 'POST', body: { token }, anon: true })
export const emailVerify = (token: string) =>
  request<Tokens>('/auth/email/verify', { method: 'POST', body: { token }, anon: true })
export const emailSignup = (body: { token: string; username: string }) =>
  request<Tokens>('/auth/email/signup', { method: 'POST', body, anon: true })
