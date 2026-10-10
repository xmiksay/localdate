<script setup lang="ts">
import { useT } from '@/i18n/typed'

defineProps<{ isTest: boolean; isAdmin: boolean; bannedAt: string | null }>()
const { t } = useT()
</script>

<template>
  <div class="flex shrink-0 flex-col items-end gap-1">
    <span v-if="bannedAt" class="rounded-full bg-danger px-2.5 py-0.5 text-xs font-bold text-white">
      {{ t('admin.bannedBadge') }}
    </span>
    <span v-if="isAdmin" class="rounded-full bg-plum/10 px-2.5 py-0.5 text-xs font-bold text-plum">
      {{ t('admin.adminBadge') }}
    </span>
    <span v-if="isTest" class="rounded-full bg-sun/30 px-2.5 py-0.5 text-xs font-bold text-ink">
      {{ t('admin.testBadge') }}
    </span>
  </div>
</template>
