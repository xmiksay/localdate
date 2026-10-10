import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { i18n } from '@/i18n'
import { useAdminUsersStore } from '@/stores/adminUsers'
import ImpersonateButton from './ImpersonateButton.vue'

vi.mock('@/api/admin')
vi.mock('@/router', () => ({ default: { replace: vi.fn() } }))

const user = { id: 'u1', username: 'eva', is_admin: false, banned_at: null }

function render(impersonation: boolean, u = user) {
  useAdminUsersStore().settings = { impersonation }
  return mount(ImpersonateButton, { props: { user: u }, global: { plugins: [i18n] } })
}

beforeEach(() => {
  setActivePinia(createPinia())
  i18n.global.locale.value = 'cs'
})

describe('ImpersonateButton', () => {
  it('offers acting as an unbanned non-admin, labelled with the username', () => {
    const b = render(true).find('button')
    expect(b.text()).toBe('Přihlásit se jako')
    expect(b.attributes('aria-label')).toBe('Přihlásit se jako @eva')
  })

  it('stays hidden when the server has it off, and for admins or banned accounts', () => {
    expect(render(false).find('button').exists()).toBe(false)
    expect(
      render(true, { ...user, is_admin: true })
        .find('button')
        .exists(),
    ).toBe(false)
    expect(
      render(true, { ...user, banned_at: '2026-01-01T00:00:00Z' } as never)
        .find('button')
        .exists(),
    ).toBe(false)
  })
})
