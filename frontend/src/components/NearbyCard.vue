<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { NearbyProfile } from '@/api/types'

defineProps<{ profile: NearbyProfile }>()
const { t } = useI18n()
</script>

<template>
  <RouterLink
    :to="{ name: 'person', params: { userId: profile.user_id } }"
    class="flex gap-4 rounded-3xl border-2 border-line bg-paper p-3 transition hover:border-plum"
  >
    <img
      v-if="profile.photos[0]"
      :src="profile.photos[0].url"
      :alt="profile.display_name"
      loading="lazy"
      class="size-24 shrink-0 rounded-2xl bg-line object-cover"
    />
    <div class="flex min-w-0 flex-1 flex-col gap-1.5">
      <div class="flex items-baseline justify-between gap-2">
        <h3 class="truncate font-display text-lg font-semibold">
          {{ profile.display_name }}, {{ profile.age }}
        </h3>
        <span
          v-if="profile.wave_state !== 'none'"
          class="shrink-0 rounded-full px-2.5 py-0.5 text-xs font-bold"
          :class="profile.wave_state === 'received' ? 'bg-sun text-ink' : 'bg-plum/10 text-plum'"
        >
          {{ t(`wave.${profile.wave_state}`) }}
        </span>
      </div>
      <p class="text-sm font-semibold text-coral">{{ t(`distance.${profile.distance_band}`) }}</p>
      <p class="text-sm text-muted">
        {{ profile.reasons.map((r) => t(`reason.${r}`)).join(' · ') }}
      </p>
      <ul v-if="profile.interests.length" class="flex flex-wrap gap-1.5">
        <li
          v-for="i in profile.interests.slice(0, 4)"
          :key="i.id"
          class="rounded-full bg-cream px-2.5 py-0.5 text-xs font-medium text-plum ring-1 ring-line"
        >
          {{ t(`interest.${i.key}`) }}
        </li>
      </ul>
    </div>
  </RouterLink>
</template>
