<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useT } from '@/i18n/typed'
import type { AdminReport, ReportStatus } from '@/api/types'
import { useAdminStore } from '@/stores/admin'
import { errorMessage } from '@/utils/errors'
import AdminReportCard from './AdminReportCard.vue'
import BaseButton from './ui/BaseButton.vue'
import BaseDialog from './ui/BaseDialog.vue'
import ErrorNote from './ui/ErrorNote.vue'

const TABS: ReportStatus[] = ['open', 'resolved']

const { t } = useT()
const store = useAdminStore()
const failure = ref<string | null>(null)
const confirming = ref<{ kind: 'dismiss' | 'ban'; report: AdminReport } | null>(null)
const busy = ref(false)

async function show(status: ReportStatus) {
  failure.value = null
  try {
    await store.load(status)
  } catch (e) {
    failure.value = errorMessage(e)
  }
}

onMounted(() => show(store.status))

async function run(action: () => Promise<void>) {
  busy.value = true
  failure.value = null
  try {
    await action()
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    busy.value = false
    confirming.value = null
  }
}

function confirm() {
  const c = confirming.value
  if (!c) return
  void run(() =>
    c.kind === 'dismiss' ? store.dismiss(c.report.id) : store.ban(c.report.subject.id),
  )
}
</script>

<template>
  <div role="tablist" :aria-label="t('admin.tabs')" class="mb-5 flex gap-2">
    <button
      v-for="s in TABS"
      :key="s"
      type="button"
      role="tab"
      :aria-selected="store.status === s"
      class="min-h-11 rounded-full border-2 px-5 text-sm font-semibold transition"
      :class="
        store.status === s
          ? 'border-plum bg-plum text-cream'
          : 'border-line bg-paper hover:border-plum'
      "
      @click="show(s)"
    >
      {{ t(`admin.${s}`) }}
    </button>
  </div>

  <ErrorNote :message="failure" class="mb-4" />
  <p v-if="store.loading" class="text-muted">{{ t('common.loading') }}</p>
  <p v-else-if="store.reports.length === 0" class="text-muted">
    {{ t(store.status === 'open' ? 'admin.emptyOpen' : 'admin.emptyResolved') }}
  </p>
  <ul class="flex flex-col gap-4">
    <li v-for="r in store.reports" :key="r.id">
      <AdminReportCard
        :report="r"
        @dismiss="confirming = { kind: 'dismiss', report: r }"
        @ban="confirming = { kind: 'ban', report: r }"
        @unban="run(() => store.unban(r.subject.id))"
      />
    </li>
  </ul>

  <BaseDialog
    v-if="confirming"
    :title="
      t(confirming.kind === 'dismiss' ? 'admin.dismissTitle' : 'admin.banTitle', {
        username: confirming.report.subject.username,
      })
    "
    @close="confirming = null"
  >
    <p class="mb-5 text-muted">
      {{
        t(confirming.kind === 'dismiss' ? 'admin.dismissBody' : 'admin.banBody', {
          username: confirming.report.subject.username,
        })
      }}
    </p>
    <div class="flex gap-3">
      <BaseButton variant="ghost" class="flex-1" @click="confirming = null">
        {{ t('common.cancel') }}
      </BaseButton>
      <BaseButton
        :variant="confirming.kind === 'ban' ? 'danger' : 'primary'"
        class="flex-1"
        :loading="busy"
        @click="confirm"
      >
        {{ t(confirming.kind === 'dismiss' ? 'admin.dismiss' : 'admin.ban') }}
      </BaseButton>
    </div>
  </BaseDialog>
</template>
