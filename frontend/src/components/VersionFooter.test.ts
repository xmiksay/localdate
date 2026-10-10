import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import * as healthApi from '@/api/health'
import type { Health } from '@/api/types'
import { resetBuildInfo } from '@/composables/useBuildInfo'
import VersionFooter from './VersionFooter.vue'

vi.mock('@/api/health')

const health = (commit: string): Health => ({
  status: 'ok',
  version: '0.1.0',
  commit,
  built_at: null,
  db: 'ok',
})

async function render(h: Health | null) {
  vi.mocked(healthApi.getHealth).mockResolvedValue(h)
  const w = mount(VersionFooter)
  await flushPromises()
  return w
}

beforeEach(() => {
  vi.mocked(healthApi.getHealth).mockReset()
  resetBuildInfo()
})

describe('VersionFooter', () => {
  it('shows version and commit, linking the commit on GitHub', async () => {
    const w = await render(health('fa15e9c'))
    expect(w.text()).toBe('v0.1.0 · fa15e9c')
    expect(w.get('a').attributes('href')).toBe(
      'https://github.com/xmiksay/localdate/commit/fa15e9c',
    )
  })

  it('does not link a "dev" build', async () => {
    const w = await render(health('dev'))
    expect(w.text()).toBe('v0.1.0 · dev')
    expect(w.find('a').exists()).toBe(false)
  })

  it('renders nothing when the health endpoint is unavailable', async () => {
    const w = await render(null)
    expect(w.html()).toBe('<!--v-if-->')
  })

  it('fetches once for every footer on the page', async () => {
    await render(health('fa15e9c'))
    mount(VersionFooter)
    await flushPromises()
    expect(healthApi.getHealth).toHaveBeenCalledTimes(1)
  })
})
