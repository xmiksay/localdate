<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { useT } from '@/i18n/typed'
import { GENDERS, REASONS, WINDOW_MINUTES, type Gender, type Reason } from '@/api/types'
import { DEFAULT_FILTER, useMeStore } from '@/stores/me'
import { errorMessage } from '@/utils/errors'
import BaseButton from './ui/BaseButton.vue'
import ChipToggle from './ui/ChipToggle.vue'
import ErrorNote from './ui/ErrorNote.vue'
import FormField from './ui/FormField.vue'
import TextInput from './ui/TextInput.vue'

defineProps<{ submitLabel: string }>()
const emit = defineEmits<{ saved: [] }>()

const { t } = useT()
const me = useMeStore()

const form = reactive({ ...DEFAULT_FILTER, ...me.filter })
const busy = ref(false)
const failure = ref<string | null>(null)
const justSaved = ref(false)

const ageMin = computed({
  get: () => String(form.age_min),
  set: (v) => (form.age_min = Number(v)),
})
const ageMax = computed({
  get: () => String(form.age_max),
  set: (v) => (form.age_max = Number(v)),
})

const distanceLabel = computed(() =>
  form.max_distance_m >= 1000
    ? t('common.kilometers', { n: form.max_distance_m / 1000 })
    : t('common.meters', { n: form.max_distance_m }),
)
const ageRangeError = computed(() =>
  form.age_min < 18 || form.age_max > 99 || form.age_min > form.age_max
    ? t('filter.ageInvalid')
    : undefined,
)
const reasonsError = computed(() =>
  form.reasons.length === 0 ? t('filter.reasonsRequired') : undefined,
)

function toggleGender(g: Gender, on: boolean) {
  form.genders = on ? [...form.genders, g] : form.genders.filter((x) => x !== g)
}
function toggleReason(r: Reason, on: boolean) {
  form.reasons = on ? [...form.reasons, r] : form.reasons.filter((x) => x !== r)
}

async function submit() {
  failure.value = null
  justSaved.value = false
  if (ageRangeError.value || reasonsError.value) return
  busy.value = true
  try {
    await me.saveFilter({ ...form })
    justSaved.value = true
    emit('saved')
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <form class="flex flex-col gap-6" novalidate @submit.prevent="submit">
    <FormField v-slot="f" :label="`${t('filter.maxDistance')}: ${distanceLabel}`">
      <input
        :id="f.id"
        v-model.number="form.max_distance_m"
        type="range"
        min="200"
        max="10000"
        step="100"
        class="h-10 w-full accent-coral"
      />
    </FormField>

    <fieldset>
      <legend class="mb-1.5 text-sm font-semibold text-plum">{{ t('filter.genders') }}</legend>
      <div class="flex flex-wrap gap-2">
        <ChipToggle
          :model-value="form.genders.length === 0"
          @update:model-value="form.genders = []"
        >
          {{ t('gender.any') }}
        </ChipToggle>
        <ChipToggle
          v-for="g in GENDERS"
          :key="g"
          :model-value="form.genders.includes(g)"
          @update:model-value="toggleGender(g, $event)"
        >
          {{ t(`gender.${g}`) }}
        </ChipToggle>
      </div>
    </fieldset>

    <fieldset>
      <legend class="mb-1.5 text-sm font-semibold text-plum">{{ t('filter.ageRange') }}</legend>
      <div class="grid grid-cols-2 gap-3">
        <FormField v-slot="f" :label="t('filter.ageMin')">
          <TextInput
            :id="f.id"
            v-model="ageMin"
            type="number"
            min="18"
            :invalid="!!ageRangeError"
          />
        </FormField>
        <FormField v-slot="f" :label="t('filter.ageMax')">
          <TextInput
            :id="f.id"
            v-model="ageMax"
            type="number"
            max="99"
            :invalid="!!ageRangeError"
          />
        </FormField>
      </div>
      <p v-if="ageRangeError" role="alert" class="mt-1.5 text-sm font-medium text-danger">
        {{ ageRangeError }}
      </p>
    </fieldset>

    <fieldset>
      <legend class="mb-1.5 text-sm font-semibold text-plum">{{ t('filter.reasons') }}</legend>
      <div class="flex flex-wrap gap-2">
        <ChipToggle
          v-for="r in REASONS"
          :key="r"
          :model-value="form.reasons.includes(r)"
          @update:model-value="toggleReason(r, $event)"
        >
          {{ t(`reason.${r}`) }}
        </ChipToggle>
      </div>
      <p v-if="reasonsError" role="alert" class="mt-1.5 text-sm font-medium text-danger">
        {{ reasonsError }}
      </p>
    </fieldset>

    <fieldset>
      <legend class="mb-1.5 text-sm font-semibold text-plum">
        {{ t('filter.defaultWindow') }}
      </legend>
      <div role="radiogroup" class="grid grid-cols-4 gap-2">
        <button
          v-for="m in WINDOW_MINUTES"
          :key="m"
          type="button"
          role="radio"
          :aria-checked="form.default_window_minutes === m"
          class="min-h-11 rounded-2xl border-2 text-sm font-semibold transition"
          :class="
            form.default_window_minutes === m
              ? 'border-plum bg-plum text-cream'
              : 'border-line bg-paper hover:border-plum'
          "
          @click="form.default_window_minutes = m"
        >
          {{ t('common.minutes', { n: m }) }}
        </button>
      </div>
    </fieldset>

    <ErrorNote :message="failure" />
    <p v-if="justSaved" role="status" class="text-sm font-semibold text-plum">
      {{ t('common.saved') }}
    </p>
    <BaseButton type="submit" block :loading="busy">{{ submitLabel }}</BaseButton>
  </form>
</template>
