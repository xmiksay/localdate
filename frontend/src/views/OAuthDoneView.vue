<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ApiError } from '@/api/client'
import type { OAuthError, PhotoImportOutcome } from '@/api/types'
import PhotoImportNote from '@/components/PhotoImportNote.vue'
import SuspendedNotice from '@/components/SuspendedNotice.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ErrorNote from '@/components/ui/ErrorNote.vue'
import FormField from '@/components/ui/FormField.vue'
import TextInput from '@/components/ui/TextInput.vue'
import { useT } from '@/i18n/typed'
import { useAuthStore } from '@/stores/auth'
import { useIdentitiesStore } from '@/stores/identities'
import { errorMessage } from '@/utils/errors'
import { parseOAuthFragment } from '@/utils/oauthFragment'
import { pendingToken } from '@/utils/pendingToken'
import { isValidUsername } from '@/utils/validation'

type Phase =
  'loading' | 'signup' | 'photoNotice' | 'invalid' | 'banned' | 'failed' | 'callbackError'

const { t } = useT()
const route = useRoute()
const router = useRouter()
const auth = useAuthStore()
const identities = useIdentitiesStore()

let code = ''
let redirect: string | null = null
let signupToken = ''
/** Outcome of a picture import asked for at start; only a fresh callback carries it. */
const photo = ref<PhotoImportOutcome | null>(null)
const phase = ref<Phase>('loading')
const callbackError = ref<OAuthError | 'unknown'>('unknown')
const failure = ref<string | null>(null)
const username = ref('')
const submitted = ref(false)
const busy = ref(false)

const usernameError = computed(() =>
  submitted.value && !isValidUsername(username.value) ? t('auth.invalidUsername') : undefined,
)

function fail(e: unknown) {
  if (auth.suspended) phase.value = 'banned'
  else if (e instanceof ApiError && e.code === 'invalid_token') {
    pendingToken.clear('oauthSignup')
    signupToken = ''
    phase.value = 'invalid'
  } else {
    phase.value = 'failed'
    failure.value = errorMessage(e)
  }
}

async function exchange() {
  phase.value = 'loading'
  failure.value = null
  try {
    const r = await auth.oauthExchange(code)
    if (!r) {
      // Logging into an existing account never imports.
      await goOn()
      return
    }
    signupToken = r.signupToken
    // Survives a reload of this page; the code itself is spent.
    pendingToken.set('oauthSignup', signupToken)
    phase.value = 'signup'
  } catch (e) {
    fail(e)
  }
}

async function signup() {
  submitted.value = true
  failure.value = null
  if (usernameError.value) return
  busy.value = true
  try {
    const imported = await auth.oauthSignup(signupToken, username.value)
    pendingToken.clear('oauthSignup')
    // A new account starts at onboarding (the guard), whatever redirect the flow carried.
    redirect = null
    // The held picture's real outcome gets a moment before the app takes over.
    if (imported) {
      photo.value = imported
      phase.value = 'photoNotice'
    } else await goOn()
  } catch (e) {
    // username_taken / validation keep the token usable: stay on the form.
    if (e instanceof ApiError && (e.code === 'username_taken' || e.code === 'validation')) {
      failure.value = errorMessage(e)
    } else fail(e)
  } finally {
    busy.value = false
  }
}

// The guard sends a not yet onboarded account on to onboarding.
async function goOn() {
  await router.replace(redirect ?? '/')
}

function retry() {
  failure.value = null
  if (signupToken) phase.value = 'signup'
  else void exchange()
}

onMounted(async () => {
  const outcome = parseOAuthFragment(route.hash)
  // Keep the one-time code out of browser history.
  if (route.hash) await router.replace({ path: route.path, hash: '' })

  if (outcome.kind === 'linked') {
    identities.justLinkedProvider = outcome.provider
    identities.justImportedPhoto = outcome.photo
    await router.replace(outcome.redirect ?? { name: 'settings' })
    return
  }
  if (outcome.kind === 'error') {
    callbackError.value = outcome.error
    phase.value = 'callbackError'
    return
  }
  // A session already exists: don't silently switch accounts.
  if (auth.isAuthed) {
    await router.replace('/')
    return
  }
  if (outcome.kind === 'code') {
    code = outcome.code
    redirect = outcome.redirect
    photo.value = outcome.photo
    pendingToken.clear('oauthSignup')
    await exchange()
    return
  }
  // No fragment: a reload mid sign-up resumes from storage.
  signupToken = pendingToken.get('oauthSignup') ?? ''
  phase.value = signupToken ? 'signup' : 'invalid'
})
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-md flex-col justify-center gap-8 px-5 py-10">
    <p class="font-display text-5xl font-semibold italic text-coral">{{ t('app.name') }}</p>

    <p v-if="phase === 'loading'" role="status" class="text-muted">
      {{ t('common.loading') }}
    </p>

    <SuspendedNotice v-else-if="phase === 'banned'" />

    <div v-else-if="phase === 'photoNotice' && photo" class="flex flex-col gap-4">
      <PhotoImportNote :outcome="photo" />
      <BaseButton block @click="goOn">{{ t('common.next') }}</BaseButton>
    </div>

    <form
      v-else-if="phase === 'signup'"
      class="flex flex-col gap-5"
      novalidate
      @submit.prevent="signup"
    >
      <div>
        <h1 class="font-display text-2xl font-semibold">{{ t('oauth.signupTitle') }}</h1>
        <p class="mt-2 text-muted">{{ t('oauth.signupIntro') }}</p>
      </div>
      <PhotoImportNote v-if="photo" :outcome="photo" />
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
      <ErrorNote
        :message="
          phase === 'invalid'
            ? t('oauth.invalid')
            : phase === 'callbackError'
              ? t(`oauth.error.${callbackError}`)
              : failure
        "
      />
      <BaseButton v-if="phase === 'failed'" block @click="retry">
        {{ t('common.retry') }}
      </BaseButton>
    </div>

    <template v-if="phase !== 'loading' && phase !== 'signup' && phase !== 'photoNotice'">
      <RouterLink
        v-if="auth.isAuthed"
        :to="{ name: 'settings' }"
        class="text-center font-semibold text-coral underline"
      >
        {{ t('identities.toSettings') }}
      </RouterLink>
      <RouterLink v-else to="/login" class="text-center font-semibold text-coral underline">
        {{ t('auth.toLogin') }}
      </RouterLink>
    </template>
  </main>
</template>
