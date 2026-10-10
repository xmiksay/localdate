<script setup lang="ts">
import { computed } from 'vue'
import { useBuildInfo } from '@/composables/useBuildInfo'

const info = useBuildInfo()
// "dev" (no git at build time) is no commit GitHub knows.
const commitUrl = computed(() =>
  info.value && /^[0-9a-f]{7,40}$/.test(info.value.commit)
    ? `https://github.com/xmiksay/localdate/commit/${info.value.commit}`
    : null,
)
</script>

<template>
  <p v-if="info" class="truncate px-4 text-center text-[10px] leading-4 text-muted/70">
    v{{ info.version }} ·
    <a
      v-if="commitUrl"
      :href="commitUrl"
      target="_blank"
      rel="noopener noreferrer"
      class="hover:underline"
      >{{ info.commit }}</a
    >
    <template v-else>{{ info.commit }}</template>
  </p>
</template>
