<script setup lang="ts">
import { useT } from '@/i18n/typed'
import BaseButton from './ui/BaseButton.vue'
import BaseDialog from './ui/BaseDialog.vue'

defineProps<{ matchId: string }>()
const emit = defineEmits<{ close: [] }>()
const { t } = useT()
</script>

<template>
  <BaseDialog :title="t('person.matchedTitle')" @close="emit('close')">
    <p class="mb-6 text-muted">{{ t('person.matchedBody') }}</p>
    <div class="flex flex-col gap-3">
      <RouterLink
        :to="{ name: 'chat', params: { matchId } }"
        class="inline-flex min-h-12 items-center justify-center rounded-full bg-coral px-6 font-semibold text-white shadow-[0_4px_0_var(--color-coral-dark)]"
      >
        {{ t('person.openChat') }}
      </RouterLink>
      <BaseButton variant="ghost" @click="emit('close')">{{ t('person.keepBrowsing') }}</BaseButton>
    </div>
  </BaseDialog>
</template>
