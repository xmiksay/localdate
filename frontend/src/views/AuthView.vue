<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useT } from '@/i18n/typed'
import { useRoute, useRouter } from 'vue-router'
import EmailLinkForm from '@/components/EmailLinkForm.vue'
import SuspendedNotice from '@/components/SuspendedNotice.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ErrorNote from '@/components/ui/ErrorNote.vue'
import FormField from '@/components/ui/FormField.vue'
import TextInput from '@/components/ui/TextInput.vue'
import { useAuthStore } from '@/stores/auth'
import { errorMessage } from '@/utils/errors'
import { safeRedirect } from '@/utils/redirect'
import { isValidPassword, isValidUsername } from '@/utils/validation'

const props = defineProps<{ mode: 'login' | 'register' }>()
const { t } = useT()
const route = useRoute()
const router = useRouter()
const auth = useAuthStore()

const username = ref('')
const password = ref('')
const busy = ref(false)
const failure = ref<string | null>(null)
const submitted = ref(false)
const showEmail = ref(false)

onMounted(() => auth.loadProviders())

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
    await router.replace(safeRedirect(route.query.redirect))
  } catch (e) {
    // A ban is explained by the suspended notice; don't repeat it below the form.
    failure.value = auth.suspended ? null : errorMessage(e)
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

    <SuspendedNotice v-if="auth.suspended" />

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
      <RouterLink
        v-if="!isRegister && auth.emailEnabled"
        :to="{ name: 'password-forgot' }"
        class="self-center text-sm font-semibold text-coral underline"
      >
        {{ t('password.forgotLink') }}
      </RouterLink>
    </form>

    <section v-if="auth.emailEnabled" class="flex flex-col gap-3">
      <BaseButton v-if="!showEmail" variant="ghost" block @click="showEmail = true">
        {{ t('auth.emailOption') }}
      </BaseButton>
      <template v-else>
        <h2 class="font-display text-xl font-semibold">{{ t('auth.emailOption') }}</h2>
        <EmailLinkForm :submit-label="t('auth.emailSubmit')" :send="auth.emailStart" />
      </template>
    </section>

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
