import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import * as authApi from '@/api/auth'
import { i18n } from '@/i18n'
import AuthView from './AuthView.vue'

vi.mock('@/api/auth')

const Stub = { template: '<div />' }

async function render() {
  vi.mocked(authApi.getProviders).mockResolvedValue({
    email: false,
    google: true,
    telegram: true,
    facebook: true,
    password_reset: false,
  })
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/login', component: Stub },
      { path: '/register', component: Stub },
      { path: '/password/forgot', name: 'password-forgot', component: Stub },
    ],
  })
  await router.push('/login?redirect=%2Fmatches')
  await router.isReady()
  const wrapper = mount(AuthView, {
    props: { mode: 'login' },
    global: { plugins: [router, i18n] },
  })
  await flushPromises()
  return wrapper
}

const providerButton = (w: Awaited<ReturnType<typeof render>>, name: string) =>
  w.findAll('button').find((b) => b.text().includes(name))!

let assign: ReturnType<typeof vi.fn>

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  assign = vi.fn()
  vi.stubGlobal('location', { ...window.location, assign })
})
afterEach(() => vi.unstubAllGlobals())

describe('AuthView provider buttons', () => {
  it('offers the picture import next to Facebook only', async () => {
    const w = await render()
    expect(w.findAll('input[type="checkbox"]')).toHaveLength(1)
    expect(w.text()).toContain(i18n.global.t('oauth.importPhotoNew', { from: 'z Facebooku' }))
  })

  it('starts Facebook with the import when ticked, Google never', async () => {
    const w = await render()
    await providerButton(w, 'Facebook').trigger('click')
    expect(assign).toHaveBeenLastCalledWith('/api/auth/oauth/facebook/start?redirect=%2Fmatches')

    await w.find('input[type="checkbox"]').setValue(true)
    await providerButton(w, 'Facebook').trigger('click')
    expect(assign).toHaveBeenLastCalledWith(
      '/api/auth/oauth/facebook/start?redirect=%2Fmatches&import_photo=1',
    )
    await providerButton(w, 'Google').trigger('click')
    expect(assign).toHaveBeenLastCalledWith('/api/auth/oauth/google/start?redirect=%2Fmatches')
  })
})
