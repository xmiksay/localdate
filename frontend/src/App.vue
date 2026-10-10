<script setup lang="ts">
import { useRoute } from 'vue-router'
import ImpersonationBanner from '@/components/ImpersonationBanner.vue'
import VersionFooter from '@/components/VersionFooter.vue'

const route = useRoute()
</script>

<template>
  <div class="relative min-h-dvh">
    <!-- Above every route (onboarding included), so no screen hides whose account this is. -->
    <ImpersonationBanner />
    <RouterView />
    <!-- Standalone screens all keep ≥ 2.5rem of bottom padding, which this sits in. -->
    <VersionFooter v-if="!route.meta.appLayout" class="absolute inset-x-0 bottom-2" />
  </div>
</template>
