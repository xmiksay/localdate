<script setup lang="ts">
import { computed } from 'vue'
import type { PhotoImportOutcome } from '@/api/types'
import { useT } from '@/i18n/typed'

const props = defineProps<{ outcome: PhotoImportOutcome }>()
const { t } = useT()

const text = computed(() => t(`oauth.photo.${props.outcome}`))
</script>

<template>
  <p
    role="status"
    class="rounded-2xl border-2 px-4 py-3 text-sm font-medium"
    :class="
      outcome === 'imported' || outcome === 'pending'
        ? 'border-plum/40 bg-plum/5 text-plum'
        : 'border-line bg-paper text-muted'
    "
  >
    {{ text }}
  </p>
</template>
