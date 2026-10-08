<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useT } from '@/i18n/typed'
import { currentPosition } from '@/composables/useGeolocation'
import { useAreasStore } from '@/stores/areas'
import { AREA_KIND_ICON } from '@/utils/areas'
import { errorMessage } from '@/utils/errors'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'

const { t } = useT()
const areas = useAreasStore()
const selected = defineModel<string | null>({ required: true })

const searching = ref(false)
const failure = ref<string | null>(null)

/** Keeps the choice when still listed, otherwise preselects the nearest area. */
function syncSelection() {
  if (!areas.here.some((a) => a.id === selected.value)) selected.value = areas.here[0]?.id ?? null
}

async function search() {
  searching.value = true
  failure.value = null
  try {
    await areas.loadHere(await currentPosition())
  } catch (e) {
    areas.here = []
    if (e === 'denied' || e === 'unavailable') {
      failure.value = t(e === 'denied' ? 'nearby.geoDenied' : 'nearby.geoUnavailable')
    } else {
      failure.value = errorMessage(e)
    }
  } finally {
    syncSelection()
    searching.value = false
  }
}

onMounted(search)
defineExpose({ search })
</script>

<template>
  <div class="flex flex-col gap-2">
    <p class="text-sm text-muted">{{ t('area.intro') }}</p>
    <p v-if="searching" class="text-muted" role="status">{{ t('area.searching') }}</p>
    <template v-else-if="failure">
      <ErrorNote :message="failure" />
      <BaseButton variant="ghost" @click="search">{{ t('common.retry') }}</BaseButton>
    </template>
    <p v-else-if="areas.here.length === 0" class="text-muted">{{ t('area.empty') }}</p>
    <fieldset v-else class="flex flex-col gap-2">
      <legend class="mb-1 text-sm font-semibold text-plum">{{ t('area.pick') }}</legend>
      <label
        v-for="a in areas.here"
        :key="a.id"
        class="flex min-h-12 items-center gap-3 rounded-2xl border-2 px-4"
        :class="selected === a.id ? 'border-plum bg-plum/5' : 'border-line'"
      >
        <input v-model="selected" type="radio" name="area" :value="a.id" class="accent-coral" />
        <span aria-hidden="true" class="text-xl">{{ AREA_KIND_ICON[a.kind] }}</span>
        <span class="flex min-w-0 flex-col">
          <span class="truncate font-semibold">{{ a.name }}</span>
          <span class="text-xs text-muted">{{ t(`area.kind.${a.kind}`) }}</span>
        </span>
      </label>
    </fieldset>
  </div>
</template>
