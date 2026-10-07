<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { REPORT_REASONS, type ReportReason } from '@/api/types'
import { useSafetyStore } from '@/stores/safety'
import { errorMessage } from '@/utils/errors'
import BaseButton from './ui/BaseButton.vue'
import BaseDialog from './ui/BaseDialog.vue'
import ErrorNote from './ui/ErrorNote.vue'

const MAX_NOTE = 1000
const props = defineProps<{ userId: string; name: string }>()
const emit = defineEmits<{ close: []; done: [] }>()

const { t } = useI18n()
const safety = useSafetyStore()
const reason = ref<ReportReason>('spam')
const note = ref('')
const busy = ref(false)
const failure = ref<string | null>(null)

async function submit() {
  busy.value = true
  failure.value = null
  try {
    await safety.report(props.userId, reason.value, note.value.trim())
    emit('done')
  } catch (e) {
    failure.value = errorMessage(e)
    busy.value = false
  }
}
</script>

<template>
  <BaseDialog :title="t('safety.reportTitle', { name })" @close="emit('close')">
    <form class="flex flex-col gap-4" @submit.prevent="submit">
      <p class="text-sm text-muted">{{ t('safety.reportHint') }}</p>
      <fieldset class="flex flex-col gap-2">
        <legend class="mb-1 text-sm font-semibold text-plum">{{ t('safety.reportReason') }}</legend>
        <label
          v-for="r in REPORT_REASONS"
          :key="r"
          class="flex min-h-11 items-center gap-3 rounded-2xl border-2 px-4"
          :class="reason === r ? 'border-plum bg-plum/5' : 'border-line'"
        >
          <input v-model="reason" type="radio" name="reason" :value="r" class="accent-coral" />
          {{ t(`report.reason.${r}`) }}
        </label>
      </fieldset>
      <label class="flex flex-col gap-1.5 text-sm font-semibold text-plum">
        {{ t('safety.reportNote') }}
        <textarea
          v-model="note"
          rows="3"
          :maxlength="MAX_NOTE"
          class="w-full rounded-2xl border-2 border-line bg-paper p-3 text-base font-normal text-ink focus:border-plum"
        />
      </label>
      <ErrorNote :message="failure" />
      <div class="flex gap-3">
        <BaseButton variant="ghost" class="flex-1" @click="emit('close')">
          {{ t('common.cancel') }}
        </BaseButton>
        <BaseButton type="submit" variant="danger" class="flex-1" :loading="busy">
          {{ t('safety.reportSubmit') }}
        </BaseButton>
      </div>
    </form>
  </BaseDialog>
</template>
