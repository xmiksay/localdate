<script setup lang="ts">
import { computed, ref } from 'vue'
import { useT } from '@/i18n/typed'
import { ApiError } from '@/api/client'
import { WINDOW_MINUTES, type WindowKind, type WindowMinutes } from '@/api/types'
import { currentPosition } from '@/composables/useGeolocation'
import { useMeStore } from '@/stores/me'
import { useWindowStore } from '@/stores/window'
import { errorMessage } from '@/utils/errors'
import AreaPicker from './AreaPicker.vue'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'
import PillRadios from './ui/PillRadios.vue'

const { t } = useT()
const me = useMeStore()
const win = useWindowStore()

const minutes = ref<WindowMinutes>(me.filter?.default_window_minutes ?? 60)
const kind = ref<WindowKind>('timed')
const areaId = ref<string | null>(null)
const picker = ref<InstanceType<typeof AreaPicker> | null>(null)
const busy = ref(false)
const failure = ref<string | null>(null)
const incomplete = ref(false)

const label = (m: WindowMinutes) =>
  m >= 60 ? t('common.hours', { n: m / 60 }) : t('common.minutes', { n: m })
const durations = computed(() => WINDOW_MINUTES.map((m) => ({ value: m, label: label(m) })))
const kinds = computed(() => [
  { value: 'timed' as const, label: t('area.modeNearby') },
  { value: 'area' as const, label: t('area.modeArea') },
])
const areaMissing = computed(() => kind.value === 'area' && !areaId.value)

async function start() {
  busy.value = true
  failure.value = null
  incomplete.value = false
  try {
    const at = await currentPosition()
    await win.start(
      minutes.value,
      at,
      kind.value === 'area' ? (areaId.value ?? undefined) : undefined,
    )
  } catch (e) {
    if (e === 'denied' || e === 'unavailable') {
      failure.value = t(e === 'denied' ? 'nearby.geoDenied' : 'nearby.geoUnavailable')
    } else if (e instanceof ApiError && e.code === 'profile_incomplete') {
      incomplete.value = true
    } else if (e instanceof ApiError && e.code === 'outside_area') {
      // We moved out since the list was fetched: offer what contains us now.
      failure.value = errorMessage(e)
      void picker.value?.search()
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

    <PillRadios v-model="kind" :label="t('area.mode')" :options="kinds" />
    <AreaPicker v-if="kind === 'area'" ref="picker" v-model="areaId" />
    <PillRadios v-model="minutes" :label="t('nearby.duration')" :options="durations" />

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

    <BaseButton block :loading="busy" :disabled="areaMissing" @click="start">
      {{ busy ? t('nearby.starting') : t('nearby.start') }}
    </BaseButton>
  </section>
</template>
