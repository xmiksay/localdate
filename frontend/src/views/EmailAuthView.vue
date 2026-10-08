<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ApiError } from '@/api/client'
import type { EmailPreview } from '@/api/types'
import SuspendedNotice from '@/components/SuspendedNotice.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ErrorNote from '@/components/ui/ErrorNote.vue'
import FormField from '@/components/ui/FormField.vue'
import TextInput from '@/components/ui/TextInput.vue'
import { useT } from '@/i18n/typed'
import { useAuthStore } from '@/stores/auth'
import { errorMessage } from '@/utils/errors'
import { pendingToken, tokenFromHash } from '@/utils/pendingToken'
import { isValidUsername } from '@/utils/validation'

type Phase = 'loading' | 'login' | 'signup' | 'invalid' | 'banned' | 'failed'

const { t } = useT()
const route = useRoute()
const router = useRouter()
const auth = useAuthStore()

let token = ''
const phase = ref<Phase>('loading')
const failure = ref<string | null>(null)
const preview = ref<EmailPreview | null>(null)
const username = ref('')
const submitted = ref(false)
const busy = ref(false)

const usernameError = computed(() =>
  submitted.value && !isValidUsername(username.value) ? t('auth.invalidUsername') : undefined,
)

function fail(e: unknown) {
  if (auth.suspended) phase.value = 'banned'
  else if (e instanceof ApiError && e.code === 'invalid_token') {
    pendingToken.clear('signup')
    phase.value = 'invalid'
  } else {
    phase.value = 'failed'
    failure.value = errorMessage(e)
  }
}

// Opening the link spends nothing: mail scanners and prefetchers load pages too.
async function load() {
  phase.value = 'loading'
  failure.value = null
  try {
    const p = await auth.emailPreview(token)
    preview.value = p
    if (p.purpose === 'signup') pendingToken.set('signup', token)
    else pendingToken.clear('signup')
    phase.value = p.purpose === 'link' ? 'invalid' : p.purpose
  } catch (e) {
    fail(e)
  }
}

async function login() {
  busy.value = true
  failure.value = null
  try {
    await auth.emailVerify(token)
    await router.replace('/')
  } catch (e) {
    fail(e)
  } finally {
    busy.value = false
  }
}

async function signup() {
  submitted.value = true
  failure.value = null
  if (usernameError.value) return
  busy.value = true
  try {
    await auth.emailSignup(token, username.value)
    pendingToken.clear('signup')
    // The guard sends a fresh account on to onboarding.
    await router.replace('/')
  } catch (e) {
    // username_taken / validation keep the token usable: stay on the form.
    if (e instanceof ApiError && (e.code === 'username_taken' || e.code === 'validation')) {
      failure.value = errorMessage(e)
    } else fail(e)
  } finally {
    busy.value = false
  }
}

onMounted(async () => {
  // A session already exists: leave the token unspent instead of silently switching accounts.
  if (auth.isAuthed) {
    await router.replace('/')
    return
  }
  const fromHash = tokenFromHash(route.hash)
  // Keep the single-use secret out of browser history.
  if (route.hash) await router.replace({ path: route.path, hash: '' })
  // No fragment: a reload mid sign-up resumes from storage.
  token = fromHash || pendingToken.get('signup') || ''
  if (!token) {
    phase.value = 'invalid'
    return
  }
  await load()
})
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-md flex-col justify-center gap-8 px-5 py-10">
    <p class="font-display text-5xl font-semibold italic text-coral">{{ t('app.name') }}</p>

    <p v-if="phase === 'loading'" role="status" class="text-muted">
      {{ t('common.loading') }}
    </p>

    <div v-else-if="phase === 'login'" class="flex flex-col gap-4">
      <h1 class="font-display text-2xl font-semibold">{{ t('auth.loginTitle') }}</h1>
      <p class="text-muted">{{ preview?.email }}</p>
      <BaseButton block :loading="busy" @click="login">
        {{ t('auth.emailLoginAs', { username: preview?.username ?? '' }) }}
      </BaseButton>
    </div>

    <SuspendedNotice v-else-if="phase === 'banned'" />

    <form
      v-else-if="phase === 'signup'"
      class="flex flex-col gap-5"
      novalidate
      @submit.prevent="signup"
    >
      <div>
        <h1 class="font-display text-2xl font-semibold">{{ t('auth.emailSignupTitle') }}</h1>
        <p class="mt-2 text-muted">
          {{ t('auth.emailSignupIntro', { email: preview?.email ?? '' }) }}
        </p>
      </div>
      <FormField
        v-slot="f"
        :label="t('auth.username')"
        :hint="t('auth.usernameHint')"
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
      <ErrorNote :message="failure" />
      <BaseButton type="submit" block :loading="busy">{{ t('auth.registerSubmit') }}</BaseButton>
    </form>

    <div v-else class="flex flex-col gap-4">
      <ErrorNote :message="phase === 'invalid' ? t('auth.emailLinkInvalid') : failure" />
      <BaseButton v-if="phase === 'failed'" block @click="load">
        {{ t('common.retry') }}
      </BaseButton>
    </div>

    <RouterLink
      v-if="phase === 'invalid' || phase === 'banned' || phase === 'failed'"
      to="/login"
      class="text-center font-semibold text-coral underline"
    >
      {{ t('auth.toLogin') }}
    </RouterLink>
  </main>
</template>
