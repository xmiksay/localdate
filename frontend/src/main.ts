import { createApp, watch } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { i18n, setLocale } from './i18n'
import router from './router'
import { useAuthStore } from './stores/auth'
import { useMatchesStore } from './stores/matches'
import { useMeStore } from './stores/me'
import { useNearbyStore } from './stores/nearby'
import { useWindowStore } from './stores/window'
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
    useWindowStore().clear()
    useNearbyStore().reset()
    useMatchesStore().reset()
    if (router.currentRoute.value.meta.requiresAuth) router.replace({ name: 'login' })
  },
)

app.mount('#app')
