<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useT } from '@/i18n/typed'
import AdminAreasTab from '@/components/AdminAreasTab.vue'
import AdminAuditTab from '@/components/AdminAuditTab.vue'
import AdminReportsTab from '@/components/AdminReportsTab.vue'
import AdminTestUsersTab from '@/components/AdminTestUsersTab.vue'
import AdminUsersTab from '@/components/AdminUsersTab.vue'
import PageHeading from '@/components/ui/PageHeading.vue'
import { useAdminUsersStore } from '@/stores/adminUsers'

const SECTIONS = ['reports', 'areas', 'testUsers', 'users', 'audit'] as const

const { t } = useT()
const section = ref<(typeof SECTIONS)[number]>('reports')
const adminUsers = useAdminUsersStore()

// Without it the "act as" buttons stay hidden, which is also right when the server is unreachable.
onMounted(() => adminUsers.loadSettings().catch(() => undefined))
</script>

<template>
  <PageHeading :title="t('admin.title')" />

  <div
    role="tablist"
    :aria-label="t('admin.sections')"
    class="mb-5 flex gap-2 overflow-x-auto border-b-2 border-line"
  >
    <button
      v-for="s in SECTIONS"
      :key="s"
      type="button"
      role="tab"
      :aria-selected="section === s"
      class="min-h-11 shrink-0 border-b-4 px-4 font-display text-lg font-semibold transition"
      :class="
        section === s ? 'border-coral text-ink' : 'border-transparent text-muted hover:text-ink'
      "
      @click="section = s"
    >
      {{ t(`admin.${s}`) }}
    </button>
  </div>

  <AdminReportsTab v-if="section === 'reports'" />
  <AdminAreasTab v-else-if="section === 'areas'" />
  <AdminTestUsersTab v-else-if="section === 'testUsers'" />
  <AdminUsersTab v-else-if="section === 'users'" />
  <AdminAuditTab v-else />
</template>
