<script setup lang="ts">
import { ref } from 'vue'
import { useT } from '@/i18n/typed'
import AdminAreasTab from '@/components/AdminAreasTab.vue'
import AdminReportsTab from '@/components/AdminReportsTab.vue'
import PageHeading from '@/components/ui/PageHeading.vue'

const SECTIONS = ['reports', 'areas'] as const

const { t } = useT()
const section = ref<(typeof SECTIONS)[number]>('reports')
</script>

<template>
  <PageHeading :title="t('admin.title')" />

  <div
    role="tablist"
    :aria-label="t('admin.sections')"
    class="mb-5 flex gap-2 border-b-2 border-line"
  >
    <button
      v-for="s in SECTIONS"
      :key="s"
      type="button"
      role="tab"
      :aria-selected="section === s"
      class="-mb-0.5 min-h-11 border-b-4 px-4 font-display text-lg font-semibold transition"
      :class="
        section === s ? 'border-coral text-ink' : 'border-transparent text-muted hover:text-ink'
      "
      @click="section = s"
    >
      {{ t(`admin.${s}`) }}
    </button>
  </div>

  <AdminReportsTab v-if="section === 'reports'" />
  <AdminAreasTab v-else />
</template>
