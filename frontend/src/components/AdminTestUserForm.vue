<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useT } from '@/i18n/typed'
import { ApiError } from '@/api/client'
import { GENDERS, type AdminUserRow, type Gender } from '@/api/types'
import { useAdminUsersStore } from '@/stores/adminUsers'
import { useMeStore } from '@/stores/me'
import { errorMessage } from '@/utils/errors'
import { prepareUpload } from '@/utils/imageResize'
import {
  ageFromBirthDate,
  checkPhotoFile,
  isValidUsername,
  MAX_BIO,
  MAX_NAME,
  normalizeUsername,
  PHOTO_TYPES,
} from '@/utils/validation'
import InterestPicker from './InterestPicker.vue'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'
import FormField from './ui/FormField.vue'
import PillRadios from './ui/PillRadios.vue'
import TextInput from './ui/TextInput.vue'

/** `photoError`: the account exists, only its photo upload failed. */
const emit = defineEmits<{ created: [row: AdminUserRow, photoError: string | null]; cancel: [] }>()
const { t } = useT()
const store = useAdminUsersStore()
const me = useMeStore()

const form = reactive({
  username: '',
  display_name: '',
  birth_date: '',
  gender: 'female' as Gender,
  bio: '',
  interest_ids: [] as number[],
})
const photo = ref<File | null>(null)
const submitted = ref(false)
const busy = ref(false)
const failure = ref<string | null>(null)
const interestsFailed = ref(false)
const today = new Date().toISOString().slice(0, 10)
const genders = computed(() => GENDERS.map((g) => ({ value: g, label: t(`gender.${g}`) })))

onMounted(() => me.loadInterests().catch(() => (interestsFailed.value = true)))

const usernameError = computed(() =>
  submitted.value && !isValidUsername(form.username) ? t('testUsers.usernameInvalid') : undefined,
)
const nameError = computed(() =>
  submitted.value && form.display_name.trim() === '' ? t('common.required') : undefined,
)
const dateError = computed(() => {
  if (!submitted.value) return undefined
  const age = ageFromBirthDate(form.birth_date)
  if (age === null) return t('profile.invalidDate')
  return age < 18 ? t('profile.underage') : undefined
})

function onFile(e: Event) {
  photo.value = (e.target as HTMLInputElement).files?.[0] ?? null
}

async function submit() {
  submitted.value = true
  failure.value = null
  if (usernameError.value || nameError.value || dateError.value) return
  busy.value = true
  try {
    const file = photo.value ? await prepareUpload(photo.value) : null
    const check = file ? checkPhotoFile(file) : 'ok'
    if (check !== 'ok') {
      failure.value = t(check === 'type' ? 'photos.errType' : 'photos.errSize')
      return
    }
    const { row, photoError } = await store.createTestUser(
      {
        username: normalizeUsername(form.username),
        display_name: form.display_name.trim(),
        gender: form.gender,
        birth_date: form.birth_date,
        bio: form.bio.trim() || undefined,
        interest_ids: form.interest_ids,
      },
      file,
    )
    emit('created', row, photoError === null ? null : errorMessage(photoError))
  } catch (e) {
    failure.value =
      e instanceof ApiError && e.code === 'underage' ? t('profile.underage') : errorMessage(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <form class="flex flex-col gap-4" novalidate @submit.prevent="submit">
    <FormField v-slot="f" :label="t('testUsers.username')" :error="usernameError">
      <TextInput
        :id="f.id"
        v-model="form.username"
        autocomplete="off"
        :described-by="f.describedBy"
        :invalid="f.invalid"
      />
    </FormField>
    <FormField v-slot="f" :label="t('profile.displayName')" :error="nameError">
      <TextInput
        :id="f.id"
        v-model="form.display_name"
        autocomplete="off"
        :maxlength="MAX_NAME"
        :described-by="f.describedBy"
        :invalid="f.invalid"
      />
    </FormField>
    <FormField v-slot="f" :label="t('profile.birthDate')" :error="dateError">
      <TextInput
        :id="f.id"
        v-model="form.birth_date"
        type="date"
        :max="today"
        :described-by="f.describedBy"
        :invalid="f.invalid"
      />
    </FormField>
    <PillRadios v-model="form.gender" :label="t('testUsers.gender')" :options="genders" />
    <FormField v-slot="f" :label="t('profile.bio')">
      <textarea
        :id="f.id"
        v-model="form.bio"
        rows="3"
        :maxlength="MAX_BIO"
        class="w-full rounded-2xl border-2 border-line bg-paper p-4 text-base focus:border-plum"
      />
    </FormField>
    <ErrorNote v-if="interestsFailed" :message="t('profile.interestsLoadFailed')" />
    <InterestPicker v-else v-model="form.interest_ids" :options="me.interests" />
    <FormField v-slot="f" :label="t('testUsers.photo')" :hint="t('testUsers.photoHint')">
      <input
        :id="f.id"
        type="file"
        :accept="[...PHOTO_TYPES, 'image/*'].join(',')"
        :aria-describedby="f.describedBy"
        class="text-sm"
        @change="onFile"
      />
    </FormField>

    <ErrorNote :message="failure" />
    <div class="flex gap-3">
      <BaseButton variant="ghost" class="flex-1" @click="emit('cancel')">
        {{ t('common.cancel') }}
      </BaseButton>
      <BaseButton type="submit" class="flex-1" :loading="busy">
        {{ t('testUsers.create') }}
      </BaseButton>
    </div>
  </form>
</template>
