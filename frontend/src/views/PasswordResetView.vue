<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ApiError } from '@/api/client'
import SuspendedNotice from '@/components/SuspendedNotice.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ErrorNote from '@/components/ui/ErrorNote.vue'
import FormField from '@/components/ui/FormField.vue'
import TextInput from '@/components/ui/TextInput.vue'
import { useT } from '@/i18n/typed'
import { useAuthStore } from '@/stores/auth'
import { errorMessage } from '@/utils/errors'
import { pendingToken, tokenFromHash } from '@/utils/pendingToken'
import { newPasswordError } from '@/utils/validation'

type Phase = 'loading' | 'form' | 'done' | 'invalid' | 'banned' | 'failed'

const { t } = useT()
const route = useRoute()
const router = useRouter()
const auth = useAuthStore()

let token = ''
const phase = ref<Phase>('loading')
const username = ref('')
const password = ref('')
const confirm = ref('')
const submitted = ref(false)
const busy = ref(false)
const failure = ref<string | null>(null)

const problem = computed(() =>
  submitted.value ? newPasswordError(password.value, confirm.value) : null,
)

function fail(e: unknown) {
  if (auth.suspended || (e instanceof ApiError && e.code === 'banned')) phase.value = 'banned'
  else if (e instanceof ApiError && e.code === 'invalid_token') {
    pendingToken.clear('reset')
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
    username.value = (await auth.passwordResetPreview(token)).username
    phase.value = 'form'
  } catch (e) {
    fail(e)
  }
}

async function submit() {
  submitted.value = true
  failure.value = null
  if (problem.value) return
  busy.value = true
  try {
    await auth.passwordReset(token, password.value, username.value)
    pendingToken.clear('reset')
    phase.value = 'done'
  } catch (e) {
    // A refused password keeps the token usable: stay on the form.
    if (e instanceof ApiError && e.code === 'validation') failure.value = errorMessage(e)
    else fail(e)
  } finally {
    busy.value = false
  }
}

onMounted(async () => {
  const fromHash = tokenFromHash(route.hash)
  // Storage, not the URL, keeps the token across a reload and out of history.
  if (fromHash) pendingToken.set('reset', fromHash)
  if (route.hash) await router.replace({ path: route.path, hash: '' })
  token = fromHash || pendingToken.get('reset') || ''
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

    <p v-if="phase === 'loading'" role="status" class="text-muted">{{ t('common.loading') }}</p>

    <form
      v-else-if="phase === 'form'"
      class="flex flex-col gap-5"
      novalidate
      @submit.prevent="submit"
    >
      <h1 class="font-display text-2xl font-semibold">
        {{ t('password.resetTitle', { username }) }}
      </h1>
      <!-- Lets password managers file the new password under the right account. -->
      <input type="text" name="username" autocomplete="username" :value="username" hidden />
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
      <BaseButton type="submit" block :loading="busy">{{ t('password.resetSubmit') }}</BaseButton>
    </form>

    <p
      v-else-if="phase === 'done'"
      role="status"
      class="rounded-2xl border-2 border-plum/40 bg-plum/5 px-4 py-3 font-medium text-plum"
    >
      {{ t('password.resetDone') }}
    </p>

    <SuspendedNotice v-else-if="phase === 'banned'" />

    <div v-else class="flex flex-col gap-4">
      <ErrorNote :message="phase === 'invalid' ? t('auth.emailLinkInvalid') : failure" />
      <BaseButton v-if="phase === 'failed'" block @click="load">{{ t('common.retry') }}</BaseButton>
    </div>

    <RouterLink
      v-if="phase !== 'loading' && phase !== 'form'"
      to="/login"
      class="text-center font-semibold text-coral underline"
    >
      {{ phase === 'done' ? t('auth.loginSubmit') : t('auth.toLogin') }}
    </RouterLink>
  </main>
</template>
