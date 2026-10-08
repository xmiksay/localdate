import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import * as authApi from '@/api/auth'
import * as identitiesApi from '@/api/identities'
import * as oauthApi from '@/api/oauth'
import type { Identity, IdentityProvider } from '@/api/types'
import { i18n } from '@/i18n'
import LinkedAccountsSection from './LinkedAccountsSection.vue'

vi.mock('@/api/auth')
vi.mock('@/api/identities')
vi.mock('@/api/oauth')

const identity = (provider: IdentityProvider, subject: string): Identity => ({
  id: `${provider}-1`,
  provider,
  subject,
  verified_at: '2026-10-01T10:00:00Z',
  created_at: '2026-10-01T10:00:00Z',
})

async function render(identities: Identity[], telegram = false) {
  vi.mocked(authApi.getProviders).mockResolvedValue({
    email: false,
    google: true,
    telegram,
    password_reset: false,
  })
  vi.mocked(identitiesApi.getIdentities).mockResolvedValue({ has_password: true, identities })
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/settings', name: 'settings', component: { template: '<div />' } }],
  })
  const wrapper = mount(LinkedAccountsSection, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
})

describe('LinkedAccountsSection', () => {
  it('shows the address of an email row but only the name of an OAuth row', async () => {
    const w = await render([identity('email', 'eva@example.cz'), identity('google', '1093847')])
    expect(w.text()).toContain('eva@example.cz')
    expect(w.text()).toContain('Google')
    expect(w.text()).not.toContain('1093847')
  })

  it('shows a Telegram row by name, never its numeric id, and offers Telegram when enabled', async () => {
    const telegramLink = i18n.global.t('oauth.link', { provider: 'Telegram' })
    const w = await render([identity('telegram', '987654321')], true)
    expect(w.text()).toContain('Telegram')
    expect(w.text()).not.toContain('987654321')
    expect(w.text()).not.toContain(telegramLink)
    expect((await render([], true)).text()).toContain(telegramLink)
    expect((await render([])).text()).not.toContain(telegramLink)
  })

  it('offers linking only for enabled providers not linked yet', async () => {
    const linkLabel = i18n.global.t('oauth.link', { provider: 'Google' })
    expect((await render([])).text()).toContain(linkLabel)
    expect((await render([identity('google', 'x')])).text()).not.toContain(linkLabel)
  })

  it('starts the link flow with the way back to settings', async () => {
    vi.mocked(oauthApi.oauthLink).mockResolvedValue({ url: 'https://accounts.example/x' })
    const assign = vi.fn()
    vi.stubGlobal('location', { ...window.location, assign })
    const w = await render([])
    const button = w.findAll('button').find((b) => b.text().includes('Google'))
    await button!.trigger('click')
    await flushPromises()
    expect(oauthApi.oauthLink).toHaveBeenCalledWith('google', { redirect: '/settings', lang: 'cs' })
    expect(assign).toHaveBeenCalledWith('https://accounts.example/x')
    vi.unstubAllGlobals()
  })
})
