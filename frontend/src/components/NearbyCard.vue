<script setup lang="ts">
import { interestKey, useT } from '@/i18n/typed'
import { computed } from 'vue'
import type { NearbyProfile } from '@/api/types'
import SharedInterestsBadge from '@/components/SharedInterestsBadge.vue'
import { sharedFirst } from '@/utils/interests'

const props = defineProps<{ profile: NearbyProfile }>()
const { t } = useT()
const chips = computed(() =>
  sharedFirst(props.profile.interests, props.profile.shared_interests).slice(0, 4),
)
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
      <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
        <p v-if="profile.area" class="truncate text-sm font-semibold text-coral">
          <span aria-hidden="true">📍 </span>{{ profile.area.name }}
        </p>
        <p v-else-if="profile.distance_band" class="text-sm font-semibold text-coral">
          {{ t(`distance.${profile.distance_band}`) }}
        </p>
        <SharedInterestsBadge :count="profile.shared_interests.length" />
      </div>
      <p class="text-sm text-muted">
        {{ profile.reasons.map((r) => t(`reason.${r}`)).join(' · ') }}
      </p>
      <ul v-if="chips.length" class="flex flex-wrap gap-1.5">
        <li
          v-for="i in chips"
          :key="i.id"
          class="rounded-full px-2.5 py-0.5 text-xs font-medium ring-1"
          :class="i.shared ? 'bg-plum/10 text-plum ring-plum' : 'bg-cream text-plum ring-line'"
        >
          <span v-if="i.shared" aria-hidden="true">★ </span>{{ t(interestKey(i.key)) }}
          <span v-if="i.shared" class="sr-only">({{ t('person.sharedMark') }})</span>
        </li>
      </ul>
    </div>
  </RouterLink>
</template>
