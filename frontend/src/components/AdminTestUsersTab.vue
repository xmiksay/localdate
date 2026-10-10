<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useT } from '@/i18n/typed'
import type { AdminUserRow } from '@/api/types'
import { useAdminUsersStore } from '@/stores/adminUsers'
import { errorMessage } from '@/utils/errors'
import AdminTestUserForm from './AdminTestUserForm.vue'
import AdminUserItem from './AdminUserItem.vue'
import ImpersonateButton from './ImpersonateButton.vue'
import BaseButton from './ui/BaseButton.vue'
import BaseDialog from './ui/BaseDialog.vue'
import ErrorNote from './ui/ErrorNote.vue'

const { t } = useT()
const store = useAdminUsersStore()
const loading = ref(true)
const failure = ref<string | null>(null)
const creating = ref(false)
const deleting = ref<AdminUserRow | null>(null)
const busy = ref(false)

onMounted(async () => {
  try {
    await store.loadTestUsers()
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    loading.value = false
  }
})

function created(row: AdminUserRow, photoError: string | null) {
  creating.value = false
  failure.value = photoError
    ? t('testUsers.photoFailed', { username: row.username, error: photoError })
    : null
}

async function confirmDelete() {
  const user = deleting.value
  if (!user) return
  busy.value = true
  failure.value = null
  try {
    await store.deleteTestUser(user.id)
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    busy.value = false
    deleting.value = null
  }
}
</script>

<template>
  <p class="mb-4 text-sm text-muted">{{ t('testUsers.intro') }}</p>
  <p
    v-if="store.settings && !store.settings.impersonation"
    class="mb-4 rounded-2xl bg-paper p-3 text-sm"
  >
    {{ t('testUsers.impersonationOff') }}
  </p>
  <div class="mb-5">
    <BaseButton @click="creating = true">{{ t('testUsers.add') }}</BaseButton>
  </div>

  <ErrorNote :message="failure" class="mb-4" />
  <p v-if="loading" class="text-muted">{{ t('common.loading') }}</p>
  <p v-else-if="store.testUsers.length === 0" class="text-muted">{{ t('testUsers.empty') }}</p>
  <ul class="flex flex-col gap-3">
    <li v-for="u in store.testUsers" :key="u.id">
      <AdminUserItem :user="u">
        <div class="flex flex-col gap-2 sm:flex-row">
          <ImpersonateButton :user="u" class="flex-1" />
          <BaseButton variant="danger" class="!min-h-10 flex-1 !text-sm" @click="deleting = u">
            {{ t('testUsers.delete') }}
          </BaseButton>
        </div>
      </AdminUserItem>
    </li>
  </ul>

  <BaseDialog v-if="creating" :title="t('testUsers.add')" @close="creating = false">
    <AdminTestUserForm @created="created" @cancel="creating = false" />
  </BaseDialog>

  <BaseDialog
    v-if="deleting"
    :title="t('testUsers.deleteTitle', { username: deleting.username })"
    @close="deleting = null"
  >
    <p class="mb-5 text-muted">{{ t('testUsers.deleteBody') }}</p>
    <div class="flex gap-3">
      <BaseButton variant="ghost" class="flex-1" @click="deleting = null">
        {{ t('common.cancel') }}
      </BaseButton>
      <BaseButton variant="danger" class="flex-1" :loading="busy" @click="confirmDelete">
        {{ t('testUsers.delete') }}
      </BaseButton>
    </div>
  </BaseDialog>
</template>
