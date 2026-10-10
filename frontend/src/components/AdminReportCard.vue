<script setup lang="ts">
import { computed } from 'vue'
import { useT } from '@/i18n/typed'
import type { AdminReport } from '@/api/types'
import { formatDateTime } from '@/utils/time'
import ImpersonateButton from './ImpersonateButton.vue'
import UserBadges from './UserBadges.vue'
import AvatarImage from './ui/AvatarImage.vue'
import BaseButton from './ui/BaseButton.vue'

const props = defineProps<{ report: AdminReport }>()
const emit = defineEmits<{ dismiss: []; ban: []; unban: [] }>()
const { t, locale } = useT()

const subject = computed(() => props.report.subject)
const name = computed(() => subject.value.display_name ?? subject.value.username)
const isOpen = computed(() => props.report.resolved_at === null)
const canBan = computed(() => !subject.value.is_admin && subject.value.banned_at === null)
const resolution = computed(() => {
  const r = props.report
  if (!r.resolved_at || !r.resolution) return null
  return t('admin.resolvedBy', {
    resolution: t(r.resolution === 'banned' ? 'admin.resolvedBanned' : 'admin.resolvedDismissed'),
    username: r.resolved_by?.username ?? t('admin.deletedAccount'),
    date: formatDateTime(r.resolved_at, locale.value),
  })
})
</script>

<template>
  <article class="flex flex-col gap-4 rounded-3xl border-2 border-line bg-paper p-4">
    <header class="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
      <h2 class="font-display text-lg font-semibold">{{ t(`report.reason.${report.reason}`) }}</h2>
      <time :datetime="report.created_at" class="text-xs text-muted">
        {{ formatDateTime(report.created_at, locale) }}
      </time>
    </header>

    <p v-if="report.note" class="whitespace-pre-line break-words rounded-2xl bg-cream p-3 text-sm">
      {{ report.note }}
    </p>
    <p class="text-sm text-muted">
      {{
        t('admin.reportedBy', { username: report.reporter?.username ?? t('admin.deletedAccount') })
      }}
    </p>

    <div class="flex items-center gap-3 rounded-2xl border-2 border-line p-3">
      <div class="size-12 shrink-0">
        <AvatarImage :src="subject.photo_url" :name="name" />
      </div>
      <div class="min-w-0 flex-1">
        <p class="truncate font-semibold">{{ name }}</p>
        <p class="truncate text-sm text-muted">@{{ subject.username }}</p>
        <p class="text-xs text-muted">{{ t('admin.openReports', { n: subject.open_reports }) }}</p>
      </div>
      <UserBadges
        :is-test="subject.is_test"
        :is-admin="subject.is_admin"
        :banned-at="subject.banned_at"
      />
    </div>
    <ImpersonateButton :user="subject" />

    <p v-if="resolution" class="text-sm font-semibold text-plum">{{ resolution }}</p>

    <div v-if="isOpen" class="flex flex-wrap gap-3">
      <BaseButton variant="ghost" class="flex-1" @click="emit('dismiss')">
        {{ t('admin.dismiss') }}
      </BaseButton>
      <BaseButton v-if="canBan" variant="danger" class="flex-1" @click="emit('ban')">
        {{ t('admin.ban') }}
      </BaseButton>
    </div>
    <BaseButton v-else-if="subject.banned_at" variant="ghost" @click="emit('unban')">
      {{ t('admin.unban') }}
    </BaseButton>
  </article>
</template>
