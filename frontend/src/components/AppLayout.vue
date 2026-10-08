<script setup lang="ts">
import { onMounted } from 'vue'
import { useLocationSharing } from '@/composables/useGeolocation'
import { useRealtime } from '@/composables/useRealtime'
import { useMatchesStore } from '@/stores/matches'
import { useWindowStore } from '@/stores/window'
import BottomNav from './BottomNav.vue'
import WindowEndedNotice from './WindowEndedNotice.vue'

// Mounted only for authed + onboarded routes, so these live exactly as long as the session.
useRealtime()
useLocationSharing()
const win = useWindowStore()
const matches = useMatchesStore()
onMounted(() => {
  void win.load().catch(() => undefined)
  void matches.loadMatches().catch(() => undefined)
})
</script>

<template>
  <div class="mx-auto flex min-h-dvh max-w-xl flex-col">
    <main class="flex-1 px-4 pb-28 pt-6">
      <WindowEndedNotice />
      <RouterView />
    </main>
    <BottomNav />
  </div>
</template>
