const KEY = 'localdate:preload-reload-at'
// Long enough that a reload which fails again stops instead of looping; short enough that a
// later deploy in the same tab still gets its one recovery reload.
const GUARD_MS = 10_000

/**
 * After a deploy the old build's lazy chunks are gone, so a stale tab fails to import them.
 * Reloads once to pick up the new index.html; returns whether it did.
 */
export function reloadAfterPreloadError(
  storage: () => Storage,
  reload: () => void,
  now: number = Date.now(),
): boolean {
  try {
    const last = Number(storage().getItem(KEY) ?? 0)
    if (now - last < GUARD_MS) return false
    storage().setItem(KEY, String(now))
  } catch {
    // Without storage there is no loop guard, so let the error surface instead.
    return false
  }
  reload()
  return true
}
