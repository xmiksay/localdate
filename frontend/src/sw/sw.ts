/// <reference lib="webworker" />
// Custom service worker (vite-plugin-pwa injectManifest): Workbox precaching as before, plus push.
import { clientsClaim } from 'workbox-core'
import {
  cleanupOutdatedCaches,
  createHandlerBoundToURL,
  precacheAndRoute,
} from 'workbox-precaching'
import { NavigationRoute, registerRoute } from 'workbox-routing'
import { notificationFor, openOrFocus, parsePayload } from './push'

declare let self: ServiceWorkerGlobalScope

// registerType 'autoUpdate': a new worker takes over open tabs at once.
void self.skipWaiting()
clientsClaim()

precacheAndRoute(self.__WB_MANIFEST)
cleanupOutdatedCaches()
registerRoute(
  new NavigationRoute(createHandlerBoundToURL('index.html'), {
    denylist: [/^\/api/, /^\/media/],
  }),
)

self.addEventListener('push', (event) => {
  const { title, options } = notificationFor(parsePayload(event.data?.text(), self.location.origin))
  event.waitUntil(self.registration.showNotification(title, options))
})

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  const data: unknown = event.notification.data
  const url = data && typeof data === 'object' ? (data as { url?: unknown }).url : undefined
  event.waitUntil(openOrFocus(self.clients, self.location.origin, url))
})
