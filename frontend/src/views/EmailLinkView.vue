<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ApiError } from '@/api/client'
import type { EmailPreview } from '@/api/types'
import BaseButton from '@/components/ui/BaseButton.vue'
import ErrorNote from '@/components/ui/ErrorNote.vue'
import { useT } from '@/i18n/typed'
import { useAuthStore } from '@/stores/auth'
import { useIdentitiesStore } from '@/stores/identities'
import { errorMessage } from '@/utils/errors'
import { pendingToken, tokenFromHash } from '@/utils/pendingToken'

const { t } = useT()
const route = useRoute()
const router = useRouter()
const auth = useAuthStore()
const identities = useIdentitiesStore()

let token = ''
const preview = ref<EmailPreview | null>(null)
const failure = ref<string | null>(null)
const busy = ref(false)

function fail(e: unknown) {
  if (e instanceof ApiError && e.code === 'invalid_token') {
    pendingToken.clear('link')
    failure.value = t('identities.linkInvalid')
  } else failure.value = errorMessage(e)
}

async function confirm() {
  busy.value = true
  failure.value = null
  try {
    await identities.confirm(token)
    pendingToken.clear('link')
    await router.replace({ name: 'settings' })
  } catch (e) {
    fail(e)
  } finally {
    busy.value = false
  }
}

onMounted(async () => {
  const fromHash = tokenFromHash(route.hash)
  // Storage, not the URL, carries the token through a login detour and out of history.
  if (fromHash) pendingToken.set('link', fromHash)
  if (route.hash) await router.replace({ path: route.path, hash: '' })
  if (!auth.isAuthed) {
    await router.replace({ name: 'login', query: { redirect: '/auth/email/link' } })
    return
  }
  token = pendingToken.get('link') ?? ''
  if (!token) {
    failure.value = t('identities.linkInvalid')
    return
  }
  try {
    const p = await auth.emailPreview(token)
    // The server would refuse anyway; saying so now beats a confirm button that cannot work.
    if (p.purpose !== 'link' || p.username !== auth.user?.username) {
      pendingToken.clear('link')
      failure.value = t('identities.linkInvalid')
      return
    }
    preview.value = p
  } catch (e) {
    fail(e)
  }
})
</script>

<template>
  <main class="mx-auto flex min-h-dvh max-w-md flex-col justify-center gap-6 px-5 py-10">
    <template v-if="failure">
      <ErrorNote :message="failure" />
      <RouterLink :to="{ name: 'settings' }" class="text-center font-semibold text-coral underline">
        {{ t('identities.toSettings') }}
      </RouterLink>
    </template>
    <div v-else-if="preview" class="flex flex-col gap-4">
      <h1 class="font-display text-2xl font-semibold">
        {{ t('identities.linkTitle', { email: preview.email, username: preview.username ?? '' }) }}
      </h1>
      <BaseButton block :loading="busy" @click="confirm">{{
        t('identities.linkSubmit')
      }}</BaseButton>
      <RouterLink :to="{ name: 'settings' }" class="text-center font-semibold text-coral underline">
        {{ t('common.cancel') }}
      </RouterLink>
    </div>
    <p v-else role="status" class="text-muted">{{ t('common.loading') }}</p>
  </main>
</template>
