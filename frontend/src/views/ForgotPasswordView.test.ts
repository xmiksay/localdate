import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import * as authApi from '@/api/auth'
import { i18n } from '@/i18n'
import ForgotPasswordView from './ForgotPasswordView.vue'

vi.mock('@/api/auth')

async function render(email: boolean) {
  vi.mocked(authApi.getProviders).mockResolvedValue({
    email,
    google: false,
    telegram: true,
    password_reset: true,
  })
  vi.mocked(authApi.passwordForgot).mockResolvedValue(undefined)
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/login', component: { template: '<div />' } }],
  })
  const w = mount(ForgotPasswordView, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return w
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
})

describe('ForgotPasswordView', () => {
  it('with only the Telegram bot asks for a username and refuses an address', async () => {
    const w = await render(false)
    expect(w.text()).toContain(i18n.global.t('password.forgotIntroTelegram'))
    await w.find('input').setValue('eva@example.cz')
    await w.find('form').trigger('submit')
    await flushPromises()
    expect(authApi.passwordForgot).not.toHaveBeenCalled()
    expect(w.text()).toContain(i18n.global.t('auth.invalidUsername'))

    await w.find('input').setValue('eva')
    await w.find('form').trigger('submit')
    await flushPromises()
    expect(authApi.passwordForgot).toHaveBeenCalledWith({ login: 'eva', lang: 'cs' })
    expect(w.text()).toContain(i18n.global.t('password.forgotSentBodyTelegram'))
  })

  it('with a mailer accepts an address and uses the general texts', async () => {
    const w = await render(true)
    expect(w.text()).toContain(i18n.global.t('password.forgotIntro'))
    await w.find('input').setValue('eva@example.cz')
    await w.find('form').trigger('submit')
    await flushPromises()
    expect(authApi.passwordForgot).toHaveBeenCalledWith({ login: 'eva@example.cz', lang: 'cs' })
  })
})
