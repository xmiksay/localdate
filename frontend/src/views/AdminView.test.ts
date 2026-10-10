import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { i18n } from '@/i18n'
import AdminView from './AdminView.vue'

vi.mock('@/api/admin')

describe('AdminView', () => {
  it('wraps its tabs instead of scrolling them sideways on a phone', () => {
    setActivePinia(createPinia())
    const w = mount(AdminView, { global: { plugins: [i18n], stubs: { AdminReportsTab: true } } })
    const bar = w.get('[role=tablist]')
    expect(bar.classes()).toContain('flex-wrap')
    expect(bar.classes()).not.toContain('overflow-x-auto')
    expect(w.findAll('[role=tab]')).toHaveLength(5)
  })
})
