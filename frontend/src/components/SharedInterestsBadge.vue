<script setup lang="ts">
import { useT } from '@/i18n/typed'

defineProps<{ count: number }>()
const { t } = useT()
</script>

<template>
  <span v-if="count > 0" class="rounded-full bg-plum px-2.5 py-0.5 text-xs font-bold text-cream">
    {{ t('nearby.sharedInterests', count) }}
  </span>
</template>
