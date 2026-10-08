import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { watchLongHidden } from './visibility'

function fakeDoc() {
  const target = new EventTarget()
  const doc = {
    visibilityState: 'visible' as DocumentVisibilityState,
    addEventListener: target.addEventListener.bind(target),
    removeEventListener: target.removeEventListener.bind(target),
    set(state: DocumentVisibilityState) {
      doc.visibilityState = state
      target.dispatchEvent(new Event('visibilitychange'))
    },
  }
  return doc
}

beforeEach(() => vi.useFakeTimers())
afterEach(() => vi.useRealTimers())

describe('watchLongHidden', () => {
  it('fires only after the page stayed hidden long enough', () => {
    const doc = fakeDoc()
    const hidden = vi.fn()
    const visible = vi.fn()
    watchLongHidden(doc, hidden, visible, 30_000)

    doc.set('hidden')
    vi.advanceTimersByTime(29_000)
    doc.set('visible')
    vi.advanceTimersByTime(60_000)
    expect(hidden).not.toHaveBeenCalled()
    expect(visible).toHaveBeenCalledTimes(1)

    doc.set('hidden')
    vi.advanceTimersByTime(30_000)
    expect(hidden).toHaveBeenCalledTimes(1)
    doc.set('visible')
    expect(visible).toHaveBeenCalledTimes(2)
  })

  it('stops listening once disposed', () => {
    const doc = fakeDoc()
    const hidden = vi.fn()
    const dispose = watchLongHidden(doc, hidden, vi.fn(), 1000)
    doc.set('hidden')
    dispose()
    vi.advanceTimersByTime(5000)
    doc.set('visible')
    doc.set('hidden')
    vi.advanceTimersByTime(5000)
    expect(hidden).not.toHaveBeenCalled()
  })
})
