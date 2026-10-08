<script setup lang="ts">
import { ref } from 'vue'
import { useT } from '@/i18n/typed'
import { WINDOW_MINUTES, type WindowMinutes } from '@/api/types'
import { useWindowStore } from '@/stores/window'
import { errorMessage } from '@/utils/errors'
import { formatCountdown } from '@/utils/time'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'

const { t } = useT()
const win = useWindowStore()

const picking = ref(false)
const busy = ref(false)
const failure = ref<string | null>(null)

async function run(action: () => Promise<void>) {
  busy.value = true
  failure.value = null
  try {
    await action()
    picking.value = false
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}
const extend = (m: WindowMinutes) => run(() => win.extend(m))
const end = () => run(() => win.end())
</script>

<template>
  <section class="flex flex-col gap-3 rounded-3xl bg-plum p-4 text-cream">
    <div class="flex items-center justify-between gap-3">
      <p class="font-display text-2xl font-semibold tabular-nums" role="timer">
        {{ t('nearby.remaining', { time: formatCountdown(win.remainingMs) }) }}
      </p>
      <p class="text-sm font-semibold text-sun">
        {{ t('nearby.wavesLeft', { n: win.wavesLeft }) }}
      </p>
    </div>

    <div v-if="picking" class="flex flex-wrap gap-2">
      <button
        v-for="m in WINDOW_MINUTES"
        :key="m"
        type="button"
        :disabled="busy"
        class="min-h-10 rounded-full border-2 border-cream/40 px-4 text-sm font-semibold hover:border-sun disabled:opacity-50"
        @click="extend(m)"
      >
        {{ t('nearby.extendBy', { n: m }) }}
      </button>
    </div>

    <div class="flex gap-2">
      <button
        type="button"
        class="min-h-10 flex-1 rounded-full border-2 border-cream/40 px-4 text-sm font-semibold hover:border-sun"
        :aria-expanded="picking"
        @click="picking = !picking"
      >
        {{ t('nearby.extend') }}
      </button>
      <BaseButton variant="ghost" class="!min-h-10 flex-1 !text-sm" :loading="busy" @click="end">
        {{ t('nearby.end') }}
      </BaseButton>
    </div>
    <ErrorNote :message="failure" />
  </section>
</template>
