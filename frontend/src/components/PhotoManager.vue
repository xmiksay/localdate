<script setup lang="ts">
import { computed, ref } from 'vue'
import { useT } from '@/i18n/typed'
import { useMeStore } from '@/stores/me'
import { errorMessage } from '@/utils/errors'
import { checkPhotoFile, MAX_PHOTOS, moveItem } from '@/utils/validation'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'

const { t } = useT()
const me = useMeStore()
const input = ref<HTMLInputElement | null>(null)
const busy = ref(false)
const failure = ref<string | null>(null)

const canAdd = computed(() => me.photos.length < MAX_PHOTOS)

async function run(action: () => Promise<void>) {
  failure.value = null
  busy.value = true
  try {
    await action()
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}

async function onFiles(e: Event) {
  const el = e.target as HTMLInputElement
  const files = Array.from(el.files ?? [])
  el.value = ''
  failure.value = null
  for (const file of files) {
    if (me.photos.length >= MAX_PHOTOS) break
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
        {{ busy ? t('photos.uploading') : t('photos.add') }}
      </BaseButton>
    </div>
  </section>
</template>
