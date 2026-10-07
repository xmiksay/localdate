<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import BaseButton from '@/components/ui/BaseButton.vue'
import ErrorNote from '@/components/ui/ErrorNote.vue'
import FormField from '@/components/ui/FormField.vue'
import TextInput from '@/components/ui/TextInput.vue'
import { useAuthStore } from '@/stores/auth'
import { errorMessage } from '@/utils/errors'
import { isValidPassword, isValidUsername } from '@/utils/validation'

const props = defineProps<{ mode: 'login' | 'register' }>()
const { t } = useI18n()
const router = useRouter()
const auth = useAuthStore()

const username = ref('')
const password = ref('')
const busy = ref(false)
const failure = ref<string | null>(null)
const submitted = ref(false)

const isRegister = computed(() => props.mode === 'register')
// Login stays lenient: the server is the judge for existing accounts.
const usernameError = computed(() =>
  submitted.value && isRegister.value && !isValidUsername(username.value)
    ? t('auth.invalidUsername')
    : undefined,
)
const passwordError = computed(() =>
  submitted.value && isRegister.value && !isValidPassword(password.value)
    ? t('auth.invalidPassword')
    : undefined,
)

async function submit() {
  submitted.value = true
  failure.value = null
  if (usernameError.value || passwordError.value) return
  busy.value = true
  try {
    const creds = { username: username.value, password: password.value }
    await (isRegister.value ? auth.register(creds) : auth.login(creds))
    await router.replace('/')
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-md flex-col justify-center gap-8 px-5 py-10">
    <div>
      <p class="font-display text-5xl font-semibold italic text-coral">{{ t('app.name') }}</p>
      <p class="mt-2 text-muted">{{ t('app.tagline') }}</p>
    </div>

    <form class="flex flex-col gap-5" novalidate @submit.prevent="submit">
      <h1 class="font-display text-2xl font-semibold">
        {{ isRegister ? t('auth.registerTitle') : t('auth.loginTitle') }}
      </h1>
      <FormField
        v-slot="f"
        :label="t('auth.username')"
        :hint="isRegister ? t('auth.usernameHint') : undefined"
        :error="usernameError"
      >
        <TextInput
          :id="f.id"
          v-model="username"
          :described-by="f.describedBy"
          :invalid="f.invalid"
          autocomplete="username"
        />
      </FormField>
      <FormField
        v-slot="f"
        :label="t('auth.password')"
        :hint="isRegister ? t('auth.passwordHint') : undefined"
        :error="passwordError"
      >
        <TextInput
          :id="f.id"
          v-model="password"
          type="password"
          :described-by="f.describedBy"
          :invalid="f.invalid"
          :autocomplete="isRegister ? 'new-password' : 'current-password'"
        />
      </FormField>
      <ErrorNote :message="failure" />
      <BaseButton type="submit" block :loading="busy">
        {{ isRegister ? t('auth.registerSubmit') : t('auth.loginSubmit') }}
      </BaseButton>
    </form>

    <p class="text-center text-muted">
      {{ isRegister ? t('auth.registerSwitch') : t('auth.loginSwitch') }}
      <RouterLink
        :to="isRegister ? '/login' : '/register'"
        class="font-semibold text-coral underline"
      >
        {{ isRegister ? t('auth.loginSubmit') : t('auth.registerSubmit') }}
      </RouterLink>
    </p>
  </main>
</template>
