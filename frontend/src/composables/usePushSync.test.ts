import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import { i18n } from '@/i18n'
import { useImpersonationStore } from '@/stores/impersonation'
import { usePushStore } from '@/stores/push'
import { usePushSync } from './usePushSync'

/** `useT()` needs a component instance with i18n installed. */
function mountSync() {
  const app = createApp({ setup: () => (usePushSync(), () => null) })
  app.use(i18n)
  app.mount(document.createElement('div'))
  return app
}

beforeEach(() => {
  const pinia = createPinia()
  setActivePinia(pinia)
})

describe('push sync', () => {
  it('re-registers on start', () => {
    const resync = vi.spyOn(usePushStore(), 'resync').mockResolvedValue(undefined)
    mountSync().unmount()
    expect(resync).toHaveBeenCalledTimes(1)
  })

  it('does nothing while impersonating', () => {
    useImpersonationStore().active = {
      access_token: 'imp-a',
      expires_at: new Date(Date.now() + 3_600_000).toISOString(),
      user: { id: 't', username: 'tester', created_at: '' },
      admin: null,
    }
    const resync = vi.spyOn(usePushStore(), 'resync').mockResolvedValue(undefined)
    mountSync().unmount()
    expect(resync).not.toHaveBeenCalled()
  })
})
