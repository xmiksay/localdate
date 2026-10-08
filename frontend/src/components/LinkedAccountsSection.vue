<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { useT } from '@/i18n/typed'
import { useAuthStore } from '@/stores/auth'
import { useIdentitiesStore } from '@/stores/identities'
import { errorMessage } from '@/utils/errors'
import { formatDateTime } from '@/utils/time'
import EmailLinkForm from './EmailLinkForm.vue'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'

const { t, locale } = useT()
const auth = useAuthStore()
const identities = useIdentitiesStore()

const failure = ref<string | null>(null)

onMounted(() => {
  if (!auth.emailEnabled) auth.loadProviders()
  identities.load().catch((e) => (failure.value = errorMessage(e)))
})
// The success note belongs to the visit right after confirming, not to every later one.
onUnmounted(() => (identities.justLinked = null))

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
          <p class="text-sm font-semibold text-plum">
            {{ t(`identities.provider.${i.provider}`) }}
          </p>
          <p class="truncate font-medium">{{ i.subject }}</p>
          <p class="text-sm text-muted">
            {{ t('identities.verified', { date: formatDateTime(i.verified_at, locale) }) }}
          </p>
        </div>
        <BaseButton variant="ghost" class="!min-h-10 shrink-0 !text-sm" @click="remove(i.id)">
          {{ t('identities.remove') }}
        </BaseButton>
      </li>
    </ul>

    <div v-if="auth.emailEnabled" class="flex flex-col gap-3">
      <h3 class="text-sm font-semibold text-plum">{{ t('identities.addEmail') }}</h3>
      <EmailLinkForm :submit-label="t('auth.emailSubmit')" :send="identities.linkEmail" />
    </div>
  </section>
</template>
