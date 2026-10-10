<script setup lang="ts">
import { computed, ref } from 'vue'
import { useT } from '@/i18n/typed'
import { useAdminUsersStore } from '@/stores/adminUsers'
import { useImpersonationStore } from '@/stores/impersonation'
import { errorMessage } from '@/utils/errors'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'

const props = defineProps<{
  user: { id: string; username: string; is_admin: boolean; banned_at: string | null }
}>()
const { t } = useT()
const admin = useAdminUsersStore()
const impersonation = useImpersonationStore()
const busy = ref(false)
const failure = ref<string | null>(null)

// The server refuses admins and banned accounts (`409 cannot_impersonate`): don't offer it.
const offered = computed(
  () =>
    admin.settings?.impersonation === true && !props.user.is_admin && props.user.banned_at === null,
)

async function actAs() {
  busy.value = true
  failure.value = null
  try {
    await impersonation.start(props.user.id)
  } catch (e) {
    failure.value = errorMessage(e)
    busy.value = false
  }
}
</script>

<template>
  <div v-if="offered" class="flex flex-col gap-2">
    <BaseButton
      variant="ghost"
      class="!min-h-10 !text-sm"
      :loading="busy"
      :aria-label="t('impersonation.actAsLabel', { username: user.username })"
      @click="actAs"
    >
      {{ t('impersonation.actAs') }}
    </BaseButton>
    <ErrorNote :message="failure" />
  </div>
</template>
