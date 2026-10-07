import { onScopeDispose, watch } from 'vue'
import { updateLocation } from '@/api/window'
import { useWindowStore } from '@/stores/window'
import { shouldSendLocation, type Coords } from '@/utils/geo'

export type GeoFailure = 'denied' | 'unavailable'

function toFailure(e: GeolocationPositionError): GeoFailure {
  return e.code === e.PERMISSION_DENIED ? 'denied' : 'unavailable'
}

/** One-shot fix for starting a window. Rejects with a `GeoFailure`. */
export function currentPosition(): Promise<Coords> {
  return new Promise((resolve, reject) => {
    if (!('geolocation' in navigator)) return reject('unavailable' satisfies GeoFailure)
    navigator.geolocation.getCurrentPosition(
      (p) => resolve({ lat: p.coords.latitude, lon: p.coords.longitude }),
      (e) => reject(toFailure(e)),
      { enableHighAccuracy: true, timeout: 15_000, maximumAge: 10_000 },
    )
  })
}

/** Watches position while a window is active and keeps the server location fresh. */
export function useLocationSharing() {
  const win = useWindowStore()
  let watchId: number | null = null
  let lastSent: { at: Coords; time: number } | null = null

  async function onFix(p: GeolocationPosition) {
    const next = { lat: p.coords.latitude, lon: p.coords.longitude }
    const now = Date.now()
    if (!shouldSendLocation(lastSent?.at ?? null, next, lastSent?.time ?? null, now)) return
    // Claim the slot first so overlapping fixes don't double-send; retry on the next fix if it fails.
    const previous = lastSent
    lastSent = { at: next, time: now }
    try {
      await updateLocation(next.lat, next.lon)
      win.locationError = null
    } catch {
      lastSent = previous
    }
  }

  function stop() {
    if (watchId !== null) navigator.geolocation.clearWatch(watchId)
    watchId = null
    lastSent = null
  }

  function start() {
    if (watchId !== null) return
    if (!('geolocation' in navigator)) {
      win.locationError = 'unavailable'
      return
    }
    watchId = navigator.geolocation.watchPosition(
      onFix,
      (e) => (win.locationError = toFailure(e)),
      {
        enableHighAccuracy: true,
        maximumAge: 10_000,
      },
    )
  }

  watch(
    () => win.isActive,
    (active) => (active ? start() : stop()),
    { immediate: true },
  )
  onScopeDispose(stop)
}
