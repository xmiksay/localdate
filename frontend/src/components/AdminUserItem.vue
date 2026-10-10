<script setup lang="ts">
import { computed } from 'vue'
import { useT } from '@/i18n/typed'
import type { AdminUserRow } from '@/api/types'
import UserBadges from './UserBadges.vue'
import AvatarImage from './ui/AvatarImage.vue'

const props = defineProps<{ user: AdminUserRow }>()
const { t } = useT()

const name = computed(() => props.user.display_name ?? props.user.username)
const details = computed(() =>
  [
    props.user.age === null ? null : t('adminUsers.age', { n: props.user.age }),
    props.user.gender === null ? null : t(`gender.${props.user.gender}`),
  ]
    .filter(Boolean)
    .join(' · '),
)
</script>

<template>
  <article class="flex flex-col gap-3 rounded-3xl border-2 border-line bg-paper p-4">
    <div class="flex items-center gap-3">
      <div class="size-12 shrink-0">
        <AvatarImage :src="user.photo_url" :name="name" />
      </div>
      <div class="min-w-0 flex-1">
        <p class="truncate font-semibold">{{ name }}</p>
        <p class="truncate text-sm text-muted">@{{ user.username }}</p>
        <p v-if="details" class="text-xs text-muted">{{ details }}</p>
      </div>
      <UserBadges :is-test="user.is_test" :is-admin="user.is_admin" :banned-at="user.banned_at" />
    </div>
    <slot />
  </article>
</template>
