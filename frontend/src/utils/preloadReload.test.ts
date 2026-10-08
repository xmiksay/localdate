import { describe, expect, it, vi } from 'vitest'
import { reloadAfterPreloadError } from './preloadReload'

function memoryStorage(): Storage {
  const data = new Map<string, string>()
  return {
    get length() {
      return data.size
    },
    clear: () => data.clear(),
    getItem: (k) => data.get(k) ?? null,
    key: (i) => [...data.keys()][i] ?? null,
    removeItem: (k) => void data.delete(k),
    setItem: (k, v) => void data.set(k, v),
  }
}

describe('reloadAfterPreloadError', () => {
  it('reloads once, then holds off inside the guard window', () => {
    const storage = memoryStorage()
    const reload = vi.fn()
    expect(reloadAfterPreloadError(() => storage, reload, 1_000_000)).toBe(true)
    expect(reloadAfterPreloadError(() => storage, reload, 1_005_000)).toBe(false)
    expect(reload).toHaveBeenCalledTimes(1)
  })

  it('reloads again for a later deploy', () => {
    const storage = memoryStorage()
    const reload = vi.fn()
    reloadAfterPreloadError(() => storage, reload, 1_000_000)
    expect(reloadAfterPreloadError(() => storage, reload, 1_060_000)).toBe(true)
    expect(reload).toHaveBeenCalledTimes(2)
  })

  it('does not reload when storage is unavailable', () => {
    const reload = vi.fn()
    const broken = () => {
      throw new Error('blocked')
    }
    expect(reloadAfterPreloadError(broken, reload)).toBe(false)
    expect(reload).not.toHaveBeenCalled()
  })
})
