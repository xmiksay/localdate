<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { NearbyProfile } from '@/api/types'
import { useNearbyStore } from '@/stores/nearby'
import { errorMessage } from '@/utils/errors'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'

const props = defineProps<{ profile: NearbyProfile; label?: string }>()
const emit = defineEmits<{ matched: [matchId: string] }>()

const { t } = useI18n()
const nearby = useNearbyStore()
const busy = ref(false)
const failure = ref<string | null>(null)

async function wave() {
  busy.value = true
  failure.value = null
  try {
    const r = await nearby.wave(props.profile.user_id)
    if (r.matched && r.match_id) emit('matched', r.match_id)
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="flex flex-col gap-2">
    <BaseButton
      v-if="profile.wave_state === 'none' || profile.wave_state === 'received'"
      :loading="busy"
      @click="wave"
    >
      {{ label ?? t(profile.wave_state === 'received' ? 'nearby.waveBack' : 'wave.none') }}
    </BaseButton>
    <RouterLink
      v-else-if="profile.wave_state === 'matched' && profile.match_id"
      :to="{ name: 'chat', params: { matchId: profile.match_id } }"
      class="inline-flex min-h-12 items-center justify-center rounded-full bg-plum px-6 font-semibold text-cream"
    >
      {{ t('person.openChat') }}
    </RouterLink>
    <span
      v-else
      class="inline-flex min-h-12 items-center justify-center rounded-full border-2 border-line px-6 font-semibold text-muted"
    >
      {{ t(`wave.${profile.wave_state}`) }}
    </span>
    <ErrorNote :message="failure" />
  </div>
</template>
