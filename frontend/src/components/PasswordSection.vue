<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ApiError } from '@/api/client'
import { useT } from '@/i18n/typed'
import { useAuthStore } from '@/stores/auth'
import { useIdentitiesStore } from '@/stores/identities'
import { errorMessage } from '@/utils/errors'
import { newPasswordError } from '@/utils/validation'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'
import FormField from './ui/FormField.vue'
import TextInput from './ui/TextInput.vue'

/** Change the password, or set a first one on an account created by email. */
const { t } = useT()
const auth = useAuthStore()
const identities = useIdentitiesStore()

const current = ref('')
const password = ref('')
const confirm = ref('')
const submitted = ref(false)
const busy = ref(false)
const saved = ref(false)
const failure = ref<string | null>(null)

// has_password may have changed elsewhere (a reset, another device): always ask afresh.
onMounted(() => identities.load().catch((e) => (failure.value = errorMessage(e))))

const problem = computed(() =>
  submitted.value ? newPasswordError(password.value, confirm.value) : null,
)
const currentMissing = computed(
  () => submitted.value && identities.hasPassword && current.value === '',
)

async function submit() {
  submitted.value = true
  saved.value = false
  failure.value = null
  if (problem.value || currentMissing.value) return
  busy.value = true
  const sentCurrent = identities.hasPassword
  try {
    await auth.changePassword(password.value, sentCurrent ? current.value : undefined)
    identities.hasPassword = true
    saved.value = true
    submitted.value = false
    current.value = password.value = confirm.value = ''
  } catch (e) {
    const code = e instanceof ApiError ? e.code : null
    if (code === 'validation' && !sentCurrent) {
      // A password was set meanwhile (another device, a reset): ask for it now.
      await identities.load().catch(() => undefined)
      failure.value = t('password.currentNeeded')
    } else if (code === 'invalid_credentials') {
      // Here that means the current password, not the login.
      failure.value = t('password.wrongCurrent')
    } else failure.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section v-if="identities.loaded" aria-labelledby="s-password" class="flex flex-col gap-4">
    <h2 id="s-password" class="font-display text-xl font-semibold">{{ t('password.title') }}</h2>
    <p v-if="!identities.hasPassword" class="text-sm text-muted">
      {{ t('password.noPasswordIntro') }}
    </p>
    <p
      v-if="saved"
      role="status"
      class="rounded-2xl border-2 border-plum/40 bg-plum/5 px-4 py-3 text-sm font-medium text-plum"
    >
      {{ t('password.saved') }}
    </p>
    <form class="flex flex-col gap-4" novalidate @submit.prevent="submit">
      <input
        type="text"
        name="username"
        autocomplete="username"
        :value="auth.user?.username ?? ''"
        hidden
      />
      <FormField
        v-if="identities.hasPassword"
        v-slot="f"
        :label="t('password.current')"
        :error="currentMissing ? t('common.required') : undefined"
      >
        <TextInput
          :id="f.id"
          v-model="current"
          type="password"
          autocomplete="current-password"
          :described-by="f.describedBy"
          :invalid="f.invalid"
        />
      </FormField>
      <FormField
        v-slot="f"
        :label="t('password.new')"
        :hint="t('auth.passwordHint')"
        :error="problem === 'invalid' ? t('auth.invalidPassword') : undefined"
      >
        <TextInput
          :id="f.id"
          v-model="password"
          type="password"
          autocomplete="new-password"
          :described-by="f.describedBy"
          :invalid="f.invalid"
        />
      </FormField>
      <FormField
        v-slot="f"
        :label="t('password.confirm')"
        :error="problem === 'mismatch' ? t('password.mismatch') : undefined"
      >
        <TextInput
          :id="f.id"
          v-model="confirm"
          type="password"
          autocomplete="new-password"
          :described-by="f.describedBy"
          :invalid="f.invalid"
        />
      </FormField>
      <ErrorNote :message="failure" />
      <BaseButton type="submit" variant="ghost" :loading="busy">
        {{ identities.hasPassword ? t('password.changeSubmit') : t('password.setSubmit') }}
      </BaseButton>
    </form>
  </section>
</template>
