import { createRouter, createWebHistory, type RouteLocationNormalized } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { useMeStore } from '@/stores/me'

declare module 'vue-router' {
  interface RouteMeta {
    requiresAuth?: boolean
    guestOnly?: boolean
    onboarding?: boolean
    requiresAdmin?: boolean
  }
}

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/login',
      name: 'login',
      component: () => import('@/views/AuthView.vue'),
      props: { mode: 'login' },
      meta: { guestOnly: true },
    },
    {
      path: '/register',
      name: 'register',
      component: () => import('@/views/AuthView.vue'),
      props: { mode: 'register' },
      meta: { guestOnly: true },
    },
    {
      // Not guestOnly: the view itself decides what an already logged-in visitor sees.
      path: '/auth/email',
      name: 'email-auth',
      component: () => import('@/views/EmailAuthView.vue'),
    },
    {
      // Not requiresAuth: the guard's redirect would put the token into a query string.
      path: '/auth/email/link',
      name: 'email-link',
      component: () => import('@/views/EmailLinkView.vue'),
    },
    {
      path: '/onboarding',
      name: 'onboarding',
      component: () => import('@/views/OnboardingView.vue'),
      meta: { requiresAuth: true, onboarding: true },
    },
    {
      path: '/',
      component: () => import('@/components/AppLayout.vue'),
      meta: { requiresAuth: true },
      redirect: { name: 'nearby' },
      children: [
        { path: 'nearby', name: 'nearby', component: () => import('@/views/NearbyView.vue') },
        { path: 'matches', name: 'matches', component: () => import('@/views/MatchesView.vue') },
        {
          path: 'matches/:matchId',
          name: 'chat',
          component: () => import('@/views/ChatView.vue'),
          props: true,
        },
        {
          path: 'people/:userId',
          name: 'person',
          component: () => import('@/views/PersonView.vue'),
          props: true,
        },
        { path: 'profile', name: 'profile', component: () => import('@/views/ProfileView.vue') },
        { path: 'settings', name: 'settings', component: () => import('@/views/SettingsView.vue') },
        {
          path: 'admin',
          name: 'admin',
          component: () => import('@/views/AdminView.vue'),
          meta: { requiresAdmin: true },
        },
      ],
    },
    {
      path: '/:pathMatch(.*)*',
      name: 'not-found',
      component: () => import('@/views/NotFoundView.vue'),
    },
  ],
})

/** Unauthenticated access: protected routes go to login and come back afterwards. */
const guestAccess = (to: RouteLocationNormalized) =>
  to.meta.requiresAuth ? { name: 'login', query: { redirect: to.fullPath } } : true

router.beforeEach(async (to) => {
  const auth = useAuthStore()
  const me = useMeStore()

  if (!auth.isAuthed) return guestAccess(to)

  if (!me.loaded) {
    try {
      await me.load()
    } catch {
      // Auth loss clears the token: treat the visit as a guest's (a magic link must still open).
      if (!auth.isAuthed) return guestAccess(to)
      // A network failure keeps the token and lands on login.
      return to.meta.guestOnly ? true : { name: 'login' }
    }
  }

  if (to.meta.guestOnly) return { name: me.isOnboarded ? 'nearby' : 'onboarding' }
  if (to.meta.requiresAuth) {
    if (!me.isOnboarded && !to.meta.onboarding) return { name: 'onboarding' }
    if (me.isOnboarded && to.meta.onboarding) return { name: 'nearby' }
  }
  if (to.meta.requiresAdmin && !me.isAdmin) return { name: 'nearby' }
  return true
})

export default router
