<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { PHOTO_IMPORT_PROVIDERS, type OAuthProvider } from '@/api/types'
import { useT } from '@/i18n/typed'
import { useAuthStore } from '@/stores/auth'
import { useMeStore } from '@/stores/me'
import { errorMessage } from '@/utils/errors'
import { prepareUpload } from '@/utils/imageResize'
import { checkPhotoFile, MAX_PHOTOS, moveItem } from '@/utils/validation'
import PhotoImportNote from './PhotoImportNote.vue'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'

const { t } = useT()
const me = useMeStore()
const auth = useAuthStore()
const route = useRoute()
const input = ref<HTMLInputElement | null>(null)
const phase = ref<'idle' | 'processing' | 'uploading'>('idle')
const busy = computed(() => phase.value !== 'idle')
const failure = ref<string | null>(null)

const canAdd = computed(() => me.photos.length < MAX_PHOTOS)
/** Enabled providers whose profile picture can be (re-)imported. */
const importable = computed(() =>
  auth.oauthProviders.filter((p) => PHOTO_IMPORT_PROVIDERS.includes(p)),
)
const importing = ref<OAuthProvider | null>(null)

onMounted(() => auth.loadProviders())
// The import note belongs to the visit right after the round trip, not to every later one.
onUnmounted(() => (me.justImportedPhoto = null))

async function importFrom(provider: OAuthProvider) {
  failure.value = null
  importing.value = provider
  try {
    // Full navigation: the provider round trip ends on /auth/oauth/done, then back here.
    window.location.assign(await me.startPhotoImport(provider, route.fullPath))
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    // Also reset for a back-navigation that restores this page from the bfcache.
    importing.value = null
  }
}

async function run(action: () => Promise<void>) {
  failure.value = null
  phase.value = 'uploading'
  try {
    await action()
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    phase.value = 'idle'
  }
}

async function onFiles(e: Event) {
  const el = e.target as HTMLInputElement
  const files = Array.from(el.files ?? [])
  el.value = ''
  failure.value = null
  for (const picked of files) {
    if (me.photos.length >= MAX_PHOTOS) break
    let file: File
    phase.value = 'processing'
    try {
      file = await prepareUpload(picked)
    } finally {
      phase.value = 'idle'
    }
    const check = checkPhotoFile(file)
    if (check !== 'ok') {
      failure.value = t(check === 'type' ? 'photos.errType' : 'photos.errSize')
      return
    }
    await run(() => me.addPhoto(file))
    if (failure.value) return
  }
}

function move(from: number, to: number) {
  const ids = moveItem(
    me.photos.map((p) => p.id),
    from,
    to,
  )
  return run(() => me.reorderPhotos(ids))
}
</script>

<template>
  <section>
    <div class="mb-2 flex items-baseline justify-between">
      <h2 class="text-sm font-semibold text-plum">{{ t('photos.title') }}</h2>
      <span class="text-sm text-muted">
        {{ t('photos.count', { n: me.photos.length, max: MAX_PHOTOS }) }}
      </span>
    </div>
    <p class="mb-3 text-sm text-muted">{{ t('photos.hint') }}</p>
    <PhotoImportNote v-if="me.justImportedPhoto" :outcome="me.justImportedPhoto" class="mb-3" />

    <ul class="grid grid-cols-2 gap-3">
      <li
        v-for="(p, i) in me.photos"
        :key="p.id"
        class="relative overflow-hidden rounded-2xl border-2 border-line bg-paper"
      >
        <img
          :src="p.url"
          :alt="t('photos.alt', { n: i + 1 })"
          class="aspect-[3/4] w-full object-cover"
        />
        <span
          v-if="i === 0"
          class="absolute left-2 top-2 rounded-full bg-sun px-3 py-1 text-xs font-bold text-ink"
        >
          {{ t('photos.primary') }}
        </span>
        <div class="flex items-center justify-between gap-1 p-1.5">
          <div class="flex gap-1">
            <button
              type="button"
              class="grid size-10 place-items-center rounded-full border-2 border-line text-lg disabled:opacity-30"
              :disabled="busy || i === 0"
              :aria-label="t('photos.moveUp')"
              @click="move(i, i - 1)"
            >
              ←
            </button>
            <button
              type="button"
              class="grid size-10 place-items-center rounded-full border-2 border-line text-lg disabled:opacity-30"
              :disabled="busy || i === me.photos.length - 1"
              :aria-label="t('photos.moveDown')"
              @click="move(i, i + 1)"
            >
              →
            </button>
          </div>
          <button
            type="button"
            class="grid size-10 place-items-center rounded-full border-2 border-danger/50 text-lg text-danger disabled:opacity-30"
            :disabled="busy"
            :aria-label="t('photos.remove')"
            @click="run(() => me.removePhoto(p.id))"
          >
            ✕
          </button>
        </div>
      </li>
    </ul>

    <div class="mt-3 flex flex-col gap-3">
      <ErrorNote :message="failure" />
      <input
        ref="input"
        type="file"
        accept="image/jpeg,image/png,image/webp"
        multiple
        class="sr-only"
        tabindex="-1"
        @change="onFiles"
      />
      <BaseButton v-if="canAdd" variant="ghost" block :loading="busy" @click="input?.click()">
        {{
          phase === 'processing'
            ? t('photos.processing')
            : busy
              ? t('photos.uploading')
              : t('photos.add')
        }}
      </BaseButton>
      <BaseButton
        v-for="p in importable"
        :key="p"
        variant="ghost"
        block
        :disabled="!canAdd || busy"
        :loading="importing === p"
        @click="importFrom(p)"
      >
        {{ t('photos.importFrom', { from: t(`oauth.from.${p}`) }) }}
      </BaseButton>
      <p v-if="importable.length > 0 && !canAdd" class="text-sm text-muted">
        {{ t('photos.importFull') }}
      </p>
    </div>
  </section>
</template>
