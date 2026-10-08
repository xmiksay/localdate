<script setup lang="ts">
import { useT } from '@/i18n/typed'
import { useWindowStore } from '@/stores/window'

const { t } = useT()
const win = useWindowStore()
</script>

<template>
  <!-- Lives in the layout: the area can be left while the user is in a chat. -->
  <div role="status" aria-live="polite">
    <div
      v-if="win.endedNotice"
      class="mb-5 flex items-center gap-3 rounded-2xl border-2 border-sun bg-sun/20 px-4 py-3 text-sm font-medium"
    >
      <p class="flex-1">{{ t('area.left') }}</p>
      <button
        type="button"
        class="min-h-10 shrink-0 rounded-full border-2 border-line bg-paper px-4 font-semibold hover:border-plum"
        @click="win.dismissNotice()"
      >
        {{ t('area.dismiss') }}
      </button>
    </div>
  </div>
</template>
