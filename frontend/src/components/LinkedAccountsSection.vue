<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import type { OAuthProvider } from '@/api/types'
import { useT } from '@/i18n/typed'
import { useAuthStore } from '@/stores/auth'
import { useIdentitiesStore } from '@/stores/identities'
import { errorMessage } from '@/utils/errors'
import { formatDateTime } from '@/utils/time'
import EmailLinkForm from './EmailLinkForm.vue'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'

const { t, locale } = useT()
const providerName = (p: OAuthProvider) => t(`oauth.provider.${p}`)
const auth = useAuthStore()
const identities = useIdentitiesStore()

const router = useRouter()
const failure = ref<string | null>(null)
const linking = ref<OAuthProvider | null>(null)

const linkable = computed(() =>
  auth.oauthProviders.filter((p) => !identities.identities.some((i) => i.provider === p)),
)

onMounted(() => {
  auth.loadProviders()
  identities.load().catch((e) => (failure.value = errorMessage(e)))
})
// The success note belongs to the visit right after confirming, not to every later one.
onUnmounted(() => {
  identities.justLinked = null
  identities.justLinkedProvider = null
})

async function link(provider: OAuthProvider) {
  failure.value = null
  linking.value = provider
  try {
    const back = router.resolve({ name: 'settings' }).fullPath
    // Full navigation: the provider round trip ends on /auth/oauth/done, then back here.
    window.location.assign(await identities.startOAuthLink(provider, back))
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    // Also reset for a back-navigation that restores this page from the bfcache.
    linking.value = null
  }
}

async function remove(id: string) {
  failure.value = null
  try {
    await identities.remove(id)
  } catch (e) {
    failure.value = errorMessage(e)
  }
}
</script>

<template>
  <section aria-labelledby="s-identities" class="flex flex-col gap-4">
    <h2 id="s-identities" class="font-display text-xl font-semibold">
      {{ t('identities.title') }}
    </h2>
    <p
      v-if="identities.justLinked"
      role="status"
      class="rounded-2xl border-2 border-plum/40 bg-plum/5 px-4 py-3 text-sm font-medium text-plum"
    >
      {{ t('identities.linked', { email: identities.justLinked }) }}
    </p>
    <p
      v-if="identities.justLinkedProvider"
      role="status"
      class="rounded-2xl border-2 border-plum/40 bg-plum/5 px-4 py-3 text-sm font-medium text-plum"
    >
      {{ t('oauth.linked', { provider: providerName(identities.justLinkedProvider) }) }}
    </p>
    <ErrorNote :message="failure" />
    <p v-if="!identities.hasPassword" class="text-sm text-muted">
      {{ t('identities.noPassword') }}
    </p>

    <p v-if="identities.identities.length === 0" class="text-muted">
      {{ t('identities.empty') }}
    </p>
    <ul v-else class="flex flex-col gap-2">
      <li
        v-for="i in identities.identities"
        :key="i.id"
        class="flex items-center justify-between gap-3 rounded-2xl border-2 border-line bg-paper px-4 py-2"
      >
        <div class="min-w-0">
          <template v-if="i.provider === 'email'">
            <p class="text-sm font-semibold text-plum">{{ t('identities.provider.email') }}</p>
            <p class="truncate font-medium">{{ i.subject }}</p>
          </template>
          <!-- An OAuth subject is the provider's opaque account id: nothing a person recognises. -->
          <p v-else class="font-medium">{{ providerName(i.provider) }}</p>
          <p class="text-sm text-muted">
            {{ t('identities.verified', { date: formatDateTime(i.verified_at, locale) }) }}
          </p>
        </div>
        <BaseButton variant="ghost" class="!min-h-10 shrink-0 !text-sm" @click="remove(i.id)">
          {{ t('identities.remove') }}
        </BaseButton>
      </li>
    </ul>

    <BaseButton
      v-for="p in linkable"
      :key="p"
      variant="ghost"
      :loading="linking === p"
      :disabled="linking !== null"
      @click="link(p)"
    >
      {{ t('oauth.link', { provider: providerName(p) }) }}
    </BaseButton>

    <div v-if="auth.emailEnabled" class="flex flex-col gap-3">
      <h3 class="text-sm font-semibold text-plum">{{ t('identities.addEmail') }}</h3>
      <EmailLinkForm :submit-label="t('auth.emailSubmit')" :send="identities.linkEmail" />
    </div>
  </section>
</template>
