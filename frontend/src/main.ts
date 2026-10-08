import { createApp, watch } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { i18n, setLocale } from './i18n'
import router from './router'
import { useAdminStore } from './stores/admin'
import { useAreasStore } from './stores/areas'
import { useAuthStore } from './stores/auth'
import { useIdentitiesStore } from './stores/identities'
import { useMatchesStore } from './stores/matches'
import { useMeStore } from './stores/me'
import { useNearbyStore } from './stores/nearby'
import { usePushStore } from './stores/push'
import { navigateTarget } from './sw/push'
import { useWindowStore } from './stores/window'
import { reloadAfterPreloadError } from './utils/preloadReload'
import './assets/main.css'

const app = createApp(App)
app.use(createPinia())
app.use(i18n)
app.use(router)

setLocale(i18n.global.locale.value)

// Covers both explicit logout and the API client giving up on a refresh.
const auth = useAuthStore()
watch(
  () => auth.isAuthed,
  (authed) => {
    if (authed) return
    useMeStore().reset()
    useWindowStore().reset()
    useNearbyStore().reset()
    useMatchesStore().reset()
    useAdminStore().reset()
    useAreasStore().reset()
    useIdentitiesStore().reset()
    usePushStore().reset()
    if (router.currentRoute.value.meta.requiresAuth) router.replace({ name: 'login' })
  },
)

// A notification click on an already open tab: the service worker asks it to route there.
navigator.serviceWorker?.addEventListener('message', (event) => {
  const url = navigateTarget(event.data, window.location.origin)
  if (url) void router.push(url)
})

window.addEventListener('vite:preloadError', (event) => {
  const reloading = reloadAfterPreloadError(
    () => window.sessionStorage,
    () => window.location.reload(),
  )
  if (reloading) event.preventDefault()
})

app.mount('#app')
