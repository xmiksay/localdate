import { request } from './client'
import type { Credentials, Tokens } from './types'

export const register = (c: Credentials) =>
  request<Tokens>('/auth/register', { method: 'POST', body: c, anon: true })
export const login = (c: Credentials) =>
  request<Tokens>('/auth/login', { method: 'POST', body: c, anon: true })
export const logout = (refresh_token: string) =>
  request<void>('/auth/logout', { method: 'POST', body: { refresh_token }, anon: true })
