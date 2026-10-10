<script setup lang="ts">
import { ref } from 'vue'
import { useT } from '@/i18n/typed'
import { useImpersonationStore } from '@/stores/impersonation'
import { useSafetyStore } from '@/stores/safety'
import { errorMessage } from '@/utils/errors'
import ReportDialog from './ReportDialog.vue'
import BaseButton from './ui/BaseButton.vue'
import BaseDialog from './ui/BaseDialog.vue'
import ErrorNote from './ui/ErrorNote.vue'

const props = defineProps<{ userId: string; name: string }>()
/** Fired after a successful block or report; the parent leaves the screen. */
const emit = defineEmits<{ done: [] }>()

const { t } = useT()
const safety = useSafetyStore()
const impersonation = useImpersonationStore()
const open = ref(false)
const dialog = ref<'block' | 'report' | null>(null)
const busy = ref(false)
const failure = ref<string | null>(null)

function show(d: 'block' | 'report') {
  open.value = false
  failure.value = null
  dialog.value = d
}

async function block() {
  busy.value = true
  failure.value = null
  try {
    await safety.block(props.userId)
    emit('done')
  } catch (e) {
    failure.value = errorMessage(e)
    busy.value = false
  }
}
</script>

<template>
  <!-- Blocking and reporting are refused to an impersonation token (403 impersonation_forbidden). -->
  <div v-if="!impersonation.isActive" class="relative">
    <button
      type="button"
      :aria-label="t('safety.menu')"
      :aria-expanded="open"
      class="flex size-11 items-center justify-center rounded-full border-2 border-line bg-paper hover:border-plum"
      @click="open = !open"
      @keydown.esc="open = false"
    >
      <svg viewBox="0 0 24 24" class="size-5" fill="currentColor" aria-hidden="true">
        <circle cx="5" cy="12" r="2" />
        <circle cx="12" cy="12" r="2" />
        <circle cx="19" cy="12" r="2" />
      </svg>
    </button>
    <ul
      v-if="open"
      class="absolute right-0 z-20 mt-2 w-44 overflow-hidden rounded-2xl border-2 border-line bg-paper shadow-lg"
    >
      <li>
        <button
          type="button"
          class="min-h-11 w-full px-4 text-left font-medium hover:bg-cream"
          @click="show('report')"
        >
          {{ t('safety.report') }}
        </button>
      </li>
      <li>
        <button
          type="button"
          class="min-h-11 w-full px-4 text-left font-medium text-danger hover:bg-cream"
          @click="show('block')"
        >
          {{ t('safety.block') }}
        </button>
      </li>
    </ul>

    <BaseDialog
      v-if="dialog === 'block'"
      :title="t('safety.blockTitle', { name })"
      @close="dialog = null"
    >
      <p class="mb-4 text-muted">{{ t('safety.blockBody') }}</p>
      <ErrorNote :message="failure" class="mb-4" />
      <div class="flex gap-3">
        <BaseButton variant="ghost" class="flex-1" @click="dialog = null">
          {{ t('common.cancel') }}
        </BaseButton>
        <BaseButton variant="danger" class="flex-1" :loading="busy" @click="block">
          {{ t('safety.block') }}
        </BaseButton>
      </div>
    </BaseDialog>
    <ReportDialog
      v-if="dialog === 'report'"
      :user-id="userId"
      :name="name"
      @close="dialog = null"
      @done="emit('done')"
    />
  </div>
</template>
