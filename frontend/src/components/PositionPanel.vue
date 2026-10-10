<script setup lang="ts">
import { computed, defineAsyncComponent, ref, watch } from 'vue'
import { useT } from '@/i18n/typed'
import { WINDOW_MINUTES, type WindowMinutes } from '@/api/types'
import { currentPosition } from '@/composables/useGeolocation'
import { useAreasStore } from '@/stores/areas'
import { useMeStore } from '@/stores/me'
import { usePinnedLocationStore } from '@/stores/pinnedLocation'
import { useWindowStore } from '@/stores/window'
import { errorMessage } from '@/utils/errors'
import type { Coords } from '@/utils/geo'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'
import PillRadios from './ui/PillRadios.vue'

/** Hand-set position while impersonating: device location sharing is paused then. */
const LocationMap = defineAsyncComponent(() => import('./LocationMap.vue'))

const { t } = useT()
const me = useMeStore()
const win = useWindowStore()
const pinned = usePinnedLocationStore()
const areas = useAreasStore()

const duration = ref<WindowMinutes>(me.filter?.default_window_minutes ?? 60)
const durations = computed(() =>
  WINDOW_MINUTES.map((m) => ({
    value: m,
    label: m >= 60 ? t('common.hours', { n: m / 60 }) : t('common.minutes', { n: m }),
  })),
)
const busy = ref(false)
const failure = ref<string | null>(null)

async function pin(locate: () => Promise<Coords>) {
  busy.value = true
  failure.value = null
  try {
    await pinned.pin(await locate(), duration.value)
  } catch (e) {
    if (e === 'denied' || e === 'unavailable') {
      failure.value = t(e === 'denied' ? 'nearby.geoDenied' : 'nearby.geoUnavailable')
    } else {
      failure.value = errorMessage(e)
    }
  } finally {
    busy.value = false
  }
}

const isAreaWindow = computed(() => win.current?.kind === 'area')
// Only the areas around the pin, and only for an area window: the one fetch the server offers.
watch(
  () => [isAreaWindow.value, pinned.at] as const,
  ([area, at]) => {
    if (area && at) void areas.loadHere(at).catch(() => undefined)
  },
  { immediate: true },
)
const shownAreas = computed(() => (isAreaWindow.value ? areas.here : []))
const fmt = (n: number) => n.toFixed(5)
</script>

<template>
  <section
    aria-labelledby="pos-title"
    class="flex flex-col gap-4 rounded-3xl border-2 border-danger/40 bg-paper p-5"
  >
    <h2 id="pos-title" class="font-display text-xl font-semibold">{{ t('position.title') }}</h2>
    <p class="text-sm text-muted">{{ t('position.intro') }}</p>

    <template v-if="!win.isActive">
      <p class="text-sm font-medium">{{ t('position.startsWindow') }}</p>
      <PillRadios v-model="duration" :label="t('position.duration')" :options="durations" />
    </template>

    <BaseButton variant="ghost" :loading="busy" @click="pin(currentPosition)">
      {{ t('position.useDevice') }}
    </BaseButton>

    <LocationMap
      :at="pinned.at"
      :areas="shownAreas"
      :label="t('position.mapLabel')"
      @pick="(at: Coords) => pin(async () => at)"
    />

    <p class="text-sm tabular-nums" aria-live="polite">
      {{
        busy
          ? t('position.saving')
          : pinned.at
            ? t('position.current', { lat: fmt(pinned.at.lat), lon: fmt(pinned.at.lon) })
            : t('position.none')
      }}
    </p>
    <ErrorNote :message="failure" />
  </section>
</template>
