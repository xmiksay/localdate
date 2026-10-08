<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useT } from '@/i18n/typed'
import { ApiError } from '@/api/client'
import { GENDERS, type Gender } from '@/api/types'
import { useMeStore } from '@/stores/me'
import { errorMessage } from '@/utils/errors'
import { ageFromBirthDate, MAX_BIO, MAX_NAME } from '@/utils/validation'
import InterestPicker from './InterestPicker.vue'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'
import FormField from './ui/FormField.vue'
import TextInput from './ui/TextInput.vue'

defineProps<{ submitLabel: string }>()
const emit = defineEmits<{ saved: [] }>()

const { t } = useT()
const me = useMeStore()

const form = reactive({
  display_name: me.profile?.display_name ?? '',
  birth_date: me.profile?.birth_date ?? '',
  gender: (me.profile?.gender ?? null) as Gender | null,
  bio: me.profile?.bio ?? '',
  interest_ids: me.profile?.interests.map((i) => i.id) ?? [],
})
const busy = ref(false)
const failure = ref<string | null>(null)
const underageFromServer = ref(false)
const interestsFailed = ref(false)
const submitted = ref(false)

onMounted(() => {
  me.loadInterests().catch(() => (interestsFailed.value = true))
})

const today = new Date().toISOString().slice(0, 10)

const dateError = computed(() => {
  if (!submitted.value) return undefined
  const age = ageFromBirthDate(form.birth_date)
  if (age === null) return t('profile.invalidDate')
  if (age < 18 || underageFromServer.value) return t('profile.underage')
  return undefined
})
const nameError = computed(() =>
  submitted.value && form.display_name.trim().length === 0 ? t('common.required') : undefined,
)
const genderError = computed(() =>
  submitted.value && form.gender === null ? t('common.required') : undefined,
)

async function submit() {
  submitted.value = true
  underageFromServer.value = false
  failure.value = null
  if (dateError.value || nameError.value || genderError.value || form.gender === null) return
  busy.value = true
  try {
    await me.saveProfile({
      display_name: form.display_name.trim(),
      birth_date: form.birth_date,
      gender: form.gender,
      bio: form.bio,
      interest_ids: form.interest_ids,
    })
    emit('saved')
  } catch (e) {
    if (e instanceof ApiError && e.code === 'underage') underageFromServer.value = true
    else failure.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <form class="flex flex-col gap-5" novalidate @submit.prevent="submit">
    <FormField v-slot="f" :label="t('profile.displayName')" :error="nameError">
      <TextInput
        :id="f.id"
        v-model="form.display_name"
        :described-by="f.describedBy"
        :invalid="f.invalid"
        :maxlength="MAX_NAME"
        autocomplete="given-name"
      />
    </FormField>

    <FormField
      v-slot="f"
      :label="t('profile.birthDate')"
      :hint="t('profile.birthHint')"
      :error="dateError"
    >
      <TextInput
        :id="f.id"
        v-model="form.birth_date"
        type="date"
        :max="today"
        :described-by="f.describedBy"
        :invalid="f.invalid"
        autocomplete="bday"
      />
    </FormField>

    <fieldset>
      <legend class="mb-1.5 text-sm font-semibold text-plum">{{ t('profile.gender') }}</legend>
      <div role="radiogroup" class="flex flex-wrap gap-2">
        <button
          v-for="g in GENDERS"
          :key="g"
          type="button"
          role="radio"
          :aria-checked="form.gender === g"
          class="min-h-10 rounded-full border-2 px-4 text-sm font-semibold transition"
          :class="
            form.gender === g
              ? 'border-plum bg-plum text-cream'
              : 'border-line bg-paper hover:border-plum'
          "
          @click="form.gender = g"
        >
          {{ t(`gender.${g}`) }}
        </button>
      </div>
      <p v-if="genderError" role="alert" class="mt-1.5 text-sm font-medium text-danger">
        {{ genderError }}
      </p>
    </fieldset>

    <FormField v-slot="f" :label="t('profile.bio')">
      <textarea
        :id="f.id"
        v-model="form.bio"
        rows="4"
        :maxlength="MAX_BIO"
        :placeholder="t('profile.bioPlaceholder')"
        class="w-full rounded-2xl border-2 border-line bg-paper p-4 text-base placeholder:text-muted/60 focus:border-plum"
      />
      <p class="text-right text-xs text-muted" aria-live="polite">
        {{ form.bio.length }}/{{ MAX_BIO }}
      </p>
    </FormField>

    <ErrorNote v-if="interestsFailed" :message="t('profile.interestsLoadFailed')" />
    <InterestPicker v-else v-model="form.interest_ids" :options="me.interests" />

    <ErrorNote :message="failure" />
    <BaseButton type="submit" block :loading="busy">{{ submitLabel }}</BaseButton>
  </form>
</template>
