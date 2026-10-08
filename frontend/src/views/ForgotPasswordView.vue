<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ErrorNote from '@/components/ui/ErrorNote.vue'
import FormField from '@/components/ui/FormField.vue'
import TextInput from '@/components/ui/TextInput.vue'
import { useT } from '@/i18n/typed'
import { useAuthStore } from '@/stores/auth'
import { errorMessage } from '@/utils/errors'
import { isValidLogin, isValidUsername } from '@/utils/validation'

const { t } = useT()
const auth = useAuthStore()

const login = ref('')
const submitted = ref(false)
const busy = ref(false)
const sent = ref(false)
const failure = ref<string | null>(null)

onMounted(() => auth.loadProviders())

// With only the Telegram bot, an address cannot be reached: ask for the username alone.
const usernameOnly = computed(() => auth.resetByUsernameOnly)
const loginError = computed(() => {
  if (!submitted.value) return undefined
  if (usernameOnly.value)
    return isValidUsername(login.value) ? undefined : t('auth.invalidUsername')
  return isValidLogin(login.value) ? undefined : t('password.invalidLogin')
})

async function submit() {
  submitted.value = true
  failure.value = null
  if (loginError.value) return
  busy.value = true
  try {
    await auth.passwordForgot(login.value)
    sent.value = true
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-md flex-col justify-center gap-8 px-5 py-10">
    <p class="font-display text-5xl font-semibold italic text-coral">{{ t('app.name') }}</p>

    <div
      v-if="sent"
      role="status"
      class="flex flex-col gap-3 rounded-3xl border-2 border-line bg-paper p-5"
    >
      <h1 class="font-display text-xl font-semibold">{{ t('auth.emailSentTitle') }}</h1>
      <p class="text-sm text-muted">
        {{ usernameOnly ? t('password.forgotSentBodyTelegram') : t('password.forgotSentBody') }}
      </p>
    </div>

    <form v-else class="flex flex-col gap-5" novalidate @submit.prevent="submit">
      <div>
        <h1 class="font-display text-2xl font-semibold">{{ t('password.forgotTitle') }}</h1>
        <p class="mt-2 text-muted">
          {{ usernameOnly ? t('password.forgotIntroTelegram') : t('password.forgotIntro') }}
        </p>
      </div>
      <FormField
        v-slot="f"
        :label="usernameOnly ? t('auth.username') : t('password.login')"
        :error="loginError"
      >
        <TextInput
          :id="f.id"
          v-model="login"
          :described-by="f.describedBy"
          :invalid="f.invalid"
          autocomplete="username"
          :maxlength="254"
        />
      </FormField>
      <ErrorNote :message="failure" />
      <BaseButton type="submit" block :loading="busy">{{ t('password.forgotSubmit') }}</BaseButton>
    </form>

    <RouterLink to="/login" class="text-center font-semibold text-coral underline">
      {{ t('auth.toLogin') }}
    </RouterLink>
  </main>
</template>
