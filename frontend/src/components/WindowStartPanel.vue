<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ApiError } from '@/api/client'
import { WINDOW_MINUTES, type WindowMinutes } from '@/api/types'
import { currentPosition } from '@/composables/useGeolocation'
import { useMeStore } from '@/stores/me'
import { useWindowStore } from '@/stores/window'
import { errorMessage } from '@/utils/errors'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'

const { t } = useI18n()
const me = useMeStore()
const win = useWindowStore()

const minutes = ref<WindowMinutes>(me.filter?.default_window_minutes ?? 60)
const busy = ref(false)
const failure = ref<string | null>(null)
const incomplete = ref(false)

const label = (m: WindowMinutes) =>
  m >= 60 ? t('common.hours', { n: m / 60 }) : t('common.minutes', { n: m })

async function start() {
  busy.value = true
  failure.value = null
  incomplete.value = false
  try {
    const at = await currentPosition()
    await win.start(minutes.value, at)
  } catch (e) {
    if (e === 'denied' || e === 'unavailable') {
      failure.value = t(e === 'denied' ? 'nearby.geoDenied' : 'nearby.geoUnavailable')
    } else if (e instanceof ApiError && e.code === 'profile_incomplete') {
      incomplete.value = true
    } else {
      failure.value = errorMessage(e)
    }
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section class="flex flex-col gap-5 rounded-3xl border-2 border-line bg-paper p-5">
    <p class="text-muted">{{ t('nearby.intro') }}</p>

    <div role="radiogroup" :aria-label="t('nearby.duration')" class="flex flex-col gap-2">
      <span class="text-sm font-semibold text-plum">{{ t('nearby.duration') }}</span>
      <div class="flex flex-wrap gap-2">
        <button
          v-for="m in WINDOW_MINUTES"
          :key="m"
          type="button"
          role="radio"
          :aria-checked="minutes === m"
          class="min-h-11 rounded-full border-2 px-5 text-sm font-semibold transition"
          :class="
            minutes === m
              ? 'border-plum bg-plum text-cream'
              : 'border-line bg-paper hover:border-plum'
          "
          @click="minutes = m"
        >
          {{ label(m) }}
        </button>
      </div>
    </div>

    <div
      v-if="incomplete"
      role="alert"
      class="flex flex-col gap-2 rounded-2xl border-2 border-sun bg-sun/20 px-4 py-3 text-sm font-medium"
    >
      {{ t('nearby.profileIncomplete') }}
      <RouterLink to="/profile" class="font-bold text-coral underline">
        {{ t('nearby.completeProfile') }}
      </RouterLink>
    </div>
    <ErrorNote :message="failure" />

    <BaseButton block :loading="busy" @click="start">
      {{ busy ? t('nearby.starting') : t('nearby.start') }}
    </BaseButton>
  </section>
</template>
