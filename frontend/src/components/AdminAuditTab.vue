<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useT } from '@/i18n/typed'
import type { UserRef } from '@/api/types'
import { useAdminUsersStore } from '@/stores/adminUsers'
import { errorMessage } from '@/utils/errors'
import { formatDateTime } from '@/utils/time'
import ErrorNote from './ui/ErrorNote.vue'

const { t, locale } = useT()
const store = useAdminUsersStore()
const loading = ref(true)
const failure = ref<string | null>(null)
const who = (u: UserRef | null) => (u ? `@${u.username}` : t('audit.deleted'))

onMounted(async () => {
  try {
    await store.loadAudit()
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <p class="mb-4 text-sm text-muted">{{ t('audit.intro') }}</p>
  <ErrorNote :message="failure" class="mb-4" />
  <p v-if="loading" class="text-muted">{{ t('common.loading') }}</p>
  <p v-else-if="store.audit.length === 0" class="text-muted">{{ t('audit.empty') }}</p>
  <ul class="flex flex-col gap-2">
    <li
      v-for="e in store.audit"
      :key="e.id"
      class="flex flex-col gap-0.5 rounded-2xl border-2 border-line bg-paper px-4 py-3 text-sm"
    >
      <div class="flex flex-wrap items-baseline justify-between gap-x-3">
        <span class="font-semibold">{{ t(`audit.action.${e.action}`) }}</span>
        <time :datetime="e.created_at" class="text-xs text-muted">
          {{ formatDateTime(e.created_at, locale) }}
        </time>
      </div>
      <p class="break-words">{{ who(e.admin) }} → {{ who(e.target) }}</p>
      <p v-if="e.meta.method || e.meta.route" class="break-all font-mono text-xs text-muted">
        {{ [e.meta.method, e.meta.route].filter(Boolean).join(' ') }}
      </p>
    </li>
  </ul>
</template>
