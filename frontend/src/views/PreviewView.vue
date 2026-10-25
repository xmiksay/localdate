<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useT } from '@/i18n/typed'
import { getPreview } from '@/api/preview'
import type { Gender, Preview } from '@/api/types'
import { currentPosition } from '@/composables/useGeolocation'
import BaseButton from '@/components/ui/BaseButton.vue'
import ErrorNote from '@/components/ui/ErrorNote.vue'
import PageHeading from '@/components/ui/PageHeading.vue'
import PillRadios from '@/components/ui/PillRadios.vue'
import { errorMessage } from '@/utils/errors'
import type { Coords } from '@/utils/geo'

const { t } = useT()

const me = ref<Gender>('male')
const show = ref<Gender | 'any'>('any')
// Kept in memory only: the guest's point goes to the query and nowhere else.
const at = ref<Coords | null>(null)
const result = ref<Preview | null>(null)
const busy = ref(false)
const failure = ref<string | null>(null)

const genders: Gender[] = ['female', 'male', 'other']
const meOptions = computed(() => genders.map((g) => ({ value: g, label: t(`gender.${g}`) })))
const showOptions = computed(() => [
  { value: 'any' as const, label: t('gender.any') },
  ...meOptions.value,
])
const more = computed(() => (result.value ? result.value.total - result.value.items.length : 0))

async function load() {
  if (!at.value) return
  busy.value = true
  failure.value = null
  try {
    const wanted = show.value === 'any' ? [] : [show.value]
    result.value = await getPreview(at.value.lat, at.value.lon, me.value, wanted)
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}

async function share() {
  busy.value = true
  failure.value = null
  try {
    at.value = await currentPosition()
  } catch (e) {
    busy.value = false
    failure.value = t(e === 'denied' ? 'nearby.geoDenied' : 'nearby.geoUnavailable')
    return
  }
  await load()
}

watch([me, show], load)
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-md flex-col gap-6 px-5 py-10">
    <div>
      <p class="font-display text-5xl font-semibold italic text-coral">{{ t('app.name') }}</p>
      <p class="mt-2 text-muted">{{ t('app.tagline') }}</p>
    </div>

    <PageHeading :title="t('preview.title')" :intro="t('preview.intro')" />

    <PillRadios v-model="me" :label="t('preview.me')" :options="meOptions" />
    <PillRadios v-model="show" :label="t('preview.show')" :options="showOptions" />

    <BaseButton v-if="!at" block :loading="busy" @click="share">
      {{ busy ? t('preview.locating') : t('preview.share') }}
    </BaseButton>
    <ErrorNote :message="failure" />

    <template v-if="result">
      <p v-if="result.items.length === 0" class="text-muted">{{ t('preview.empty') }}</p>
      <ul v-else class="grid grid-cols-2 gap-3" :aria-busy="busy || undefined">
        <li
          v-for="(p, i) in result.items"
          :key="i"
          class="overflow-hidden rounded-3xl border-2 border-line bg-paper"
        >
          <img
            :src="p.photo_url"
            :alt="p.display_name"
            loading="lazy"
            class="aspect-square w-full bg-line object-cover"
          />
          <div class="flex flex-col gap-0.5 p-3">
            <h2 class="truncate font-display text-lg font-semibold">
              {{ p.display_name }}, {{ p.age }}
            </h2>
            <p v-if="p.area" class="truncate text-sm font-semibold text-coral">
              <span aria-hidden="true">📍 </span>{{ p.area.name }}
            </p>
            <p v-else-if="p.distance_band" class="text-sm font-semibold text-coral">
              {{ t(`distance.${p.distance_band}`) }}
            </p>
          </div>
        </li>
      </ul>
      <p v-if="more > 0" class="text-center font-semibold">{{ t('preview.more', more) }}</p>
    </template>

    <section class="flex flex-col gap-3 rounded-3xl bg-plum/10 p-5">
      <p class="font-semibold text-plum">{{ t('preview.cta') }}</p>
      <BaseButton block @click="$router.push({ name: 'register' })">
        {{ t('preview.register') }}
      </BaseButton>
      <BaseButton variant="ghost" block @click="$router.push({ name: 'login' })">
        {{ t('preview.login') }}
      </BaseButton>
    </section>
  </main>
</template>
