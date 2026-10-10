import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import * as oauthApi from '@/api/oauth'
import { ApiError } from '@/api/client'
import { i18n } from '@/i18n'
import { useIdentitiesStore } from '@/stores/identities'
import { useMeStore } from '@/stores/me'
import { pendingToken } from '@/utils/pendingToken'
import OAuthDoneView from './OAuthDoneView.vue'

vi.mock('@/api/oauth')

const tokens = {
  access_token: 'a1',
  refresh_token: 'r1',
  user: { id: 'u1', username: 'bob', created_at: '2026-01-01T00:00:00Z' },
}
const signup = { signup: { token: 'st', provider: 'google' as const, expires_at: 'x' } }
const Stub = { template: '<div />' }

async function open(hash: string) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/auth/oauth/done', component: OAuthDoneView },
      { path: '/', component: Stub },
      { path: '/login', component: Stub },
      { path: '/settings', name: 'settings', component: Stub },
      { path: '/profile', name: 'profile', component: Stub },
      { path: '/onboarding', component: Stub },
      { path: '/matches/:id', component: Stub },
    ],
  })
  await router.push(`/auth/oauth/done${hash}`)
  await router.isReady()
  const wrapper = mount(OAuthDoneView, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return { router, wrapper }
}

beforeEach(() => {
  localStorage.clear()
  sessionStorage.clear()
  setActivePinia(createPinia())
  vi.resetAllMocks()
})

describe('OAuthDoneView', () => {
  it('logs a known account in and follows the redirect', async () => {
    vi.mocked(oauthApi.oauthExchange).mockResolvedValue({ session: tokens })
    const { router } = await open('#code=c1&redirect=%2Fmatches%2Fm1')
    expect(oauthApi.oauthExchange).toHaveBeenCalledWith('c1')
    expect(router.currentRoute.value.fullPath).toBe('/matches/m1')
    expect(localStorage.length).toBeGreaterThan(0)
  })

  it('clears the fragment and asks a new account for a username, retrying after a clash', async () => {
    vi.mocked(oauthApi.oauthExchange).mockResolvedValue(signup)
    const { router, wrapper } = await open('#code=c1')
    expect(router.currentRoute.value.hash).toBe('')
    expect(pendingToken.get('oauthSignup')).toBe('st')

    vi.mocked(oauthApi.oauthSignup).mockRejectedValueOnce(
      new ApiError('username_taken', 409, 'taken'),
    )
    await wrapper.find('input').setValue('bob')
    await wrapper.find('form').trigger('submit')
    await flushPromises()
    expect(wrapper.find('form').exists()).toBe(true)
    expect(wrapper.text()).toContain(i18n.global.t('error.username_taken'))

    vi.mocked(oauthApi.oauthSignup).mockResolvedValue(tokens)
    await wrapper.find('form').trigger('submit')
    await flushPromises()
    expect(oauthApi.oauthSignup).toHaveBeenLastCalledWith({ token: 'st', username: 'bob' })
    expect(pendingToken.get('oauthSignup')).toBeNull()
    expect(router.currentRoute.value.fullPath).toBe('/')
  })

  it('resumes the username step after a reload', async () => {
    pendingToken.set('oauthSignup', 'st')
    const { wrapper } = await open('')
    expect(oauthApi.oauthExchange).not.toHaveBeenCalled()
    expect(wrapper.find('form').exists()).toBe(true)
  })

  it('explains a spent code and links back to login', async () => {
    vi.mocked(oauthApi.oauthExchange).mockRejectedValue(new ApiError('invalid_token', 400, 'x'))
    const { wrapper } = await open('#code=c1')
    expect(wrapper.text()).toContain(i18n.global.t('oauth.invalid'))
    expect(wrapper.find('a[href="/login"]').exists()).toBe(true)
  })

  it('translates callback errors', async () => {
    const { wrapper } = await open('#error=identity_taken')
    expect(wrapper.text()).toContain(i18n.global.t('oauth.error.identity_taken'))
    const other = await open('#error=teapot')
    expect(other.wrapper.text()).toContain(i18n.global.t('oauth.error.unknown'))
  })

  it('a finished picture import returns to the photo manager with its outcome', async () => {
    const { router } = await open('#imported=full')
    expect(router.currentRoute.value.fullPath).toBe('/profile')
    expect(useMeStore().justImportedPhoto).toBe('full')

    const back = await open('#imported=imported&redirect=%2Fonboarding')
    expect(back.router.currentRoute.value.fullPath).toBe('/onboarding')
    expect(useMeStore().justImportedPhoto).toBe('imported')
    expect(oauthApi.oauthExchange).not.toHaveBeenCalled()
  })

  it('explains an import from another provider account', async () => {
    const { wrapper } = await open('#error=identity_mismatch')
    expect(wrapper.text()).toContain(i18n.global.t('oauth.error.identity_mismatch'))
  })

  it('a finished link goes to settings with a success note', async () => {
    const { router } = await open('#linked=google')
    expect(router.currentRoute.value.fullPath).toBe('/settings')
    expect(useIdentitiesStore().justLinkedProvider).toBe('google')
    expect(useIdentitiesStore().justImportedPhoto).toBeNull()
  })

  it('hands the import outcome of a link on to settings', async () => {
    await open('#linked=facebook&photo=imported')
    expect(useIdentitiesStore().justLinkedProvider).toBe('facebook')
    expect(useIdentitiesStore().justImportedPhoto).toBe('imported')
  })

  it('a login to an existing account goes straight on, whatever photo the fragment claims', async () => {
    vi.mocked(oauthApi.oauthExchange).mockResolvedValue({ session: tokens })
    const { router } = await open('#code=c1&photo=failed&redirect=%2Fmatches%2Fm1')
    expect(router.currentRoute.value.fullPath).toBe('/matches/m1')
  })

  it('tells a new account its picture waits, then shows the outcome after sign-up', async () => {
    vi.mocked(oauthApi.oauthExchange).mockResolvedValue(signup)
    const { router, wrapper } = await open('#code=c1&photo=pending')
    expect(wrapper.text()).toContain(i18n.global.t('oauth.photo.pending'))

    vi.mocked(oauthApi.oauthSignup).mockResolvedValue({ ...tokens, photo: 'full' })
    await wrapper.find('input').setValue('bob')
    await wrapper.find('form').trigger('submit')
    await flushPromises()
    expect(wrapper.text()).toContain(i18n.global.t('oauth.photo.full'))
    expect(router.currentRoute.value.path).toBe('/auth/oauth/done')
    await wrapper.find('button').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.fullPath).toBe('/')
  })

  it('a sign-up without a held picture goes straight on', async () => {
    vi.mocked(oauthApi.oauthExchange).mockResolvedValue(signup)
    const { router, wrapper } = await open('#code=c1&photo=none')
    expect(wrapper.text()).toContain(i18n.global.t('oauth.photo.none'))
    vi.mocked(oauthApi.oauthSignup).mockResolvedValue(tokens)
    await wrapper.find('input').setValue('bob')
    await wrapper.find('form').trigger('submit')
    await flushPromises()
    expect(router.currentRoute.value.fullPath).toBe('/')
  })
})
