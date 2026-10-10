<script setup lang="ts">
import ImpersonationBanner from '@/components/ImpersonationBanner.vue'
</script>

<template>
  <!-- Above every route (onboarding included), so no screen hides whose account this is. -->
  <ImpersonationBanner />
  <RouterView />
</template>
