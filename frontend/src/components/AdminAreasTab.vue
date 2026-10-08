<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useT } from '@/i18n/typed'
import type { Area } from '@/api/types'
import { useAreasStore } from '@/stores/areas'
import { AREA_KIND_ICON } from '@/utils/areas'
import { errorMessage } from '@/utils/errors'
import AreaForm from './AreaForm.vue'
import BaseButton from './ui/BaseButton.vue'
import BaseDialog from './ui/BaseDialog.vue'
import ErrorNote from './ui/ErrorNote.vue'

const { t } = useT()
const store = useAreasStore()
const failure = ref<string | null>(null)
/** `'new'` for the create form, an area for its edit form. */
const editing = ref<Area | 'new' | null>(null)
const deleting = ref<Area | null>(null)
const busy = ref(false)

onMounted(async () => {
  try {
    await store.loadAll()
  } catch (e) {
    failure.value = errorMessage(e)
  }
})

async function confirmDelete() {
  const area = deleting.value
  if (!area) return
  busy.value = true
  failure.value = null
  try {
    await store.remove(area.id)
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    busy.value = false
    deleting.value = null
  }
}
</script>

<template>
  <div class="mb-5">
    <BaseButton @click="editing = 'new'">{{ t('adminArea.add') }}</BaseButton>
  </div>

  <ErrorNote :message="failure" class="mb-4" />
  <p v-if="store.loading" class="text-muted">{{ t('common.loading') }}</p>
  <p v-else-if="store.all.length === 0" class="text-muted">{{ t('adminArea.empty') }}</p>
  <ul class="flex flex-col gap-3">
    <li
      v-for="a in store.all"
      :key="a.id"
      class="flex flex-col gap-3 rounded-3xl border-2 border-line bg-paper p-4"
      :class="!a.active && 'opacity-70'"
    >
      <div class="flex items-start gap-3">
        <span aria-hidden="true" class="text-2xl">{{ AREA_KIND_ICON[a.kind] }}</span>
        <div class="min-w-0 flex-1">
          <h2 class="truncate font-display text-lg font-semibold">{{ a.name }}</h2>
          <p class="text-sm text-muted">
            {{ t(`area.kind.${a.kind}`) }} · {{ t('common.meters', { n: a.radius_m }) }}
          </p>
          <p class="text-xs text-muted tabular-nums">{{ a.lat }}, {{ a.lon }}</p>
        </div>
        <span
          class="shrink-0 rounded-full px-2.5 py-0.5 text-xs font-bold"
          :class="a.active ? 'bg-plum/10 text-plum' : 'bg-line text-muted'"
        >
          {{ t(a.active ? 'adminArea.activeBadge' : 'adminArea.inactiveBadge') }}
        </span>
      </div>
      <div class="flex gap-3">
        <BaseButton variant="ghost" class="flex-1" @click="editing = a">
          {{ t('adminArea.edit') }}
        </BaseButton>
        <BaseButton variant="danger" class="flex-1" @click="deleting = a">
          {{ t('adminArea.delete') }}
        </BaseButton>
      </div>
    </li>
  </ul>

  <BaseDialog
    v-if="editing"
    :title="t(editing === 'new' ? 'adminArea.createTitle' : 'adminArea.editTitle')"
    @close="editing = null"
  >
    <AreaForm
      :area="editing === 'new' ? undefined : editing"
      @saved="editing = null"
      @cancel="editing = null"
    />
  </BaseDialog>

  <BaseDialog
    v-if="deleting"
    :title="t('adminArea.deleteTitle', { name: deleting.name })"
    @close="deleting = null"
  >
    <p class="mb-5 text-muted">{{ t('adminArea.deleteBody') }}</p>
    <div class="flex gap-3">
      <BaseButton variant="ghost" class="flex-1" @click="deleting = null">
        {{ t('common.cancel') }}
      </BaseButton>
      <BaseButton variant="danger" class="flex-1" :loading="busy" @click="confirmDelete">
        {{ t('adminArea.delete') }}
      </BaseButton>
    </div>
  </BaseDialog>
</template>
