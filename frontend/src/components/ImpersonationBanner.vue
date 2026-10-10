<script setup lang="ts">
import { ref } from 'vue'
import { useT } from '@/i18n/typed'
import { useImpersonationStore } from '@/stores/impersonation'

const { t } = useT()
const impersonation = useImpersonationStore()
const busy = ref(false)

async function stop() {
  busy.value = true
  try {
    await impersonation.stop()
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div
    role="status"
    aria-live="polite"
    :class="impersonation.target && 'sticky top-0 z-30 bg-danger text-white shadow'"
  >
    <div
      v-if="impersonation.target"
      class="mx-auto flex max-w-xl items-center gap-3 px-4 py-2 text-sm font-semibold"
    >
      <p class="min-w-0 flex-1 truncate">
        {{ t('impersonation.banner', { username: `@${impersonation.target.username}` }) }}
      </p>
      <button
        type="button"
        :disabled="busy"
        class="min-h-10 shrink-0 rounded-full border-2 border-white px-4 font-bold hover:bg-white/15 disabled:opacity-60"
        @click="stop"
      >
        {{ t('impersonation.stop') }}
      </button>
    </div>
    <div
      v-else-if="impersonation.notice"
      class="mx-4 mt-4 flex items-center sm:mx-auto sm:max-w-[34rem] gap-3 rounded-2xl border-2 border-sun bg-sun/20 px-4 py-3 text-sm font-medium"
    >
      <p class="flex-1">{{ t(`impersonation.${impersonation.notice}`) }}</p>
      <button
        type="button"
        class="min-h-10 shrink-0 rounded-full border-2 border-line bg-paper px-4 font-semibold hover:border-plum"
        @click="impersonation.dismissNotice()"
      >
        {{ t('impersonation.dismiss') }}
      </button>
    </div>
  </div>
</template>
