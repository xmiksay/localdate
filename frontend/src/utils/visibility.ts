/** How long a page may stay hidden before its realtime socket is closed. */
export const HIDDEN_CLOSE_MS = 30_000

type VisibilityDoc = Pick<Document, 'visibilityState' | 'addEventListener' | 'removeEventListener'>

/**
 * Calls `onLongHidden` once the page has been hidden for `hiddenMs`, and `onVisible` whenever it
 * becomes visible again. Returns the unsubscribe function.
 */
export function watchLongHidden(
  doc: VisibilityDoc,
  onLongHidden: () => void,
  onVisible: () => void,
  hiddenMs = HIDDEN_CLOSE_MS,
): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined
  const onChange = () => {
    clearTimeout(timer)
    timer = undefined
    if (doc.visibilityState === 'hidden') timer = setTimeout(onLongHidden, hiddenMs)
    else onVisible()
  }
  doc.addEventListener('visibilitychange', onChange)
  return () => {
    clearTimeout(timer)
    doc.removeEventListener('visibilitychange', onChange)
  }
}
