<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { useT } from '@/i18n/typed'
import { AREA_KINDS, type Area, type AreaKind } from '@/api/types'
import { currentPosition } from '@/composables/useGeolocation'
import { useAreasStore } from '@/stores/areas'
import { AREA_KIND_ICON } from '@/utils/areas'
import { errorMessage } from '@/utils/errors'
import { AREA_NAME_MAX, invalidAreaFields, type AreaField } from '@/utils/validation'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'
import FormField from './ui/FormField.vue'
import TextInput from './ui/TextInput.vue'

const props = defineProps<{ area?: Area }>()
const emit = defineEmits<{ saved: []; cancel: [] }>()

const { t } = useT()
const areas = useAreasStore()

const form = reactive({
  name: props.area?.name ?? '',
  kind: (props.area?.kind ?? 'city_centre') as AreaKind,
  lat: props.area ? String(props.area.lat) : '',
  lon: props.area ? String(props.area.lon) : '',
  radius_m: String(props.area?.radius_m ?? 300),
  active: props.area?.active ?? true,
})
const busy = ref(false)
const locating = ref(false)
const failure = ref<string | null>(null)
const touched = ref(false)

// Number('') is 0, a valid latitude; an empty field must stay invalid. Vue's v-model on
// `type="number"` hands back a number despite the string model, hence String().
const num = (v: string | number) => (String(v).trim() === '' ? NaN : Number(v))
const input = computed(() => ({
  name: form.name.trim(),
  kind: form.kind,
  lat: num(form.lat),
  lon: num(form.lon),
  radius_m: num(form.radius_m),
  active: form.active,
}))
const invalid = computed(() => invalidAreaFields(input.value))
const ERROR_KEYS = {
  name: 'adminArea.nameInvalid',
  lat: 'adminArea.latInvalid',
  lon: 'adminArea.lonInvalid',
  radius_m: 'adminArea.radiusInvalid',
} as const
const fieldError = (f: AreaField) =>
  touched.value && invalid.value.includes(f) ? t(ERROR_KEYS[f]) : undefined

async function useMyLocation() {
  locating.value = true
  failure.value = null
  try {
    const at = await currentPosition()
    form.lat = String(at.lat)
    form.lon = String(at.lon)
  } catch (e) {
    failure.value = t(e === 'denied' ? 'nearby.geoDenied' : 'nearby.geoUnavailable')
  } finally {
    locating.value = false
  }
}

async function submit() {
  touched.value = true
  failure.value = null
  if (invalid.value.length) return
  busy.value = true
  try {
    await areas.save(input.value, props.area?.id)
    emit('saved')
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <form class="flex flex-col gap-4" novalidate @submit.prevent="submit">
    <FormField v-slot="f" :label="t('adminArea.name')" :error="fieldError('name')">
      <TextInput
        :id="f.id"
        v-model="form.name"
        :described-by="f.describedBy"
        :invalid="f.invalid"
        :maxlength="AREA_NAME_MAX"
      />
    </FormField>
    <FormField v-slot="f" :label="t('adminArea.kind')">
      <select
        :id="f.id"
        v-model="form.kind"
        class="min-h-12 w-full rounded-2xl border-2 border-line bg-paper px-4 text-base text-ink focus:border-plum"
      >
        <option v-for="k in AREA_KINDS" :key="k" :value="k">
          {{ AREA_KIND_ICON[k] }} {{ t(`area.kind.${k}`) }}
        </option>
      </select>
    </FormField>
    <div class="grid grid-cols-2 gap-3">
      <FormField v-slot="f" :label="t('adminArea.lat')" :error="fieldError('lat')">
        <TextInput
          :id="f.id"
          v-model="form.lat"
          type="number"
          :described-by="f.describedBy"
          :invalid="f.invalid"
        />
      </FormField>
      <FormField v-slot="f" :label="t('adminArea.lon')" :error="fieldError('lon')">
        <TextInput
          :id="f.id"
          v-model="form.lon"
          type="number"
          :described-by="f.describedBy"
          :invalid="f.invalid"
        />
      </FormField>
    </div>
    <BaseButton variant="ghost" :loading="locating" @click="useMyLocation">
      {{ t('adminArea.useMyLocation') }}
    </BaseButton>
    <FormField v-slot="f" :label="t('adminArea.radius')" :error="fieldError('radius_m')">
      <TextInput
        :id="f.id"
        v-model="form.radius_m"
        type="number"
        min="50"
        max="5000"
        :described-by="f.describedBy"
        :invalid="f.invalid"
      />
    </FormField>
    <label class="flex min-h-11 items-center gap-3 text-sm font-semibold">
      <input v-model="form.active" type="checkbox" class="size-5 accent-coral" />
      {{ t('adminArea.active') }}
    </label>

    <ErrorNote :message="failure" />
    <div class="flex gap-3">
      <BaseButton variant="ghost" class="flex-1" @click="emit('cancel')">
        {{ t('common.cancel') }}
      </BaseButton>
      <BaseButton type="submit" class="flex-1" :loading="busy">{{ t('common.save') }}</BaseButton>
    </div>
  </form>
</template>
