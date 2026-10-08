<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useT } from '@/i18n/typed'
import { useAuthStore } from '@/stores/auth'
import { useSafetyStore } from '@/stores/safety'
import { errorMessage } from '@/utils/errors'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'
import FormField from './ui/FormField.vue'
import TextInput from './ui/TextInput.vue'

const { t } = useT()
const auth = useAuthStore()
const safety = useSafetyStore()

const failure = ref<string | null>(null)
const typed = ref('')
const deleting = ref(false)
const username = computed(() => auth.user?.username ?? '')
const canDelete = computed(() => username.value !== '' && typed.value.trim() === username.value)

onMounted(() => safety.loadBlocked().catch((e) => (failure.value = errorMessage(e))))

async function unblock(userId: string) {
  failure.value = null
  try {
    await safety.unblock(userId)
  } catch (e) {
    failure.value = errorMessage(e)
  }
}

async function deleteAccount() {
  if (!canDelete.value) return
  deleting.value = true
  failure.value = null
  try {
    // Clearing the session makes the app-level auth watcher redirect to /login.
    await safety.deleteAccount()
  } catch (e) {
    failure.value = errorMessage(e)
    deleting.value = false
  }
}
</script>

<template>
  <section id="safety" aria-labelledby="s-safety" class="flex flex-col gap-6">
    <h2 id="s-safety" class="font-display text-xl font-semibold">{{ t('safety.title') }}</h2>
    <ErrorNote :message="failure" />

    <div>
      <h3 class="mb-2 text-sm font-semibold text-plum">{{ t('safety.blockedTitle') }}</h3>
      <p v-if="safety.blocked.length === 0" class="text-muted">{{ t('safety.blockedEmpty') }}</p>
      <ul v-else class="flex flex-col gap-2">
        <li
          v-for="b in safety.blocked"
          :key="b.user_id"
          class="flex items-center justify-between gap-3 rounded-2xl border-2 border-line bg-paper px-4 py-2"
        >
          <span class="truncate font-medium">{{ b.display_name }}</span>
          <BaseButton
            variant="ghost"
            class="!min-h-10 shrink-0 !text-sm"
            @click="unblock(b.user_id)"
          >
            {{ t('safety.unblock') }}
          </BaseButton>
        </li>
      </ul>
    </div>

    <form
      class="flex flex-col gap-3 rounded-3xl border-2 border-danger/40 p-4"
      @submit.prevent="deleteAccount"
    >
      <h3 class="font-display text-lg font-semibold text-danger">{{ t('safety.deleteTitle') }}</h3>
      <p class="text-sm text-muted">{{ t('safety.deleteBody', { username }) }}</p>
      <FormField v-slot="f" :label="t('safety.deleteConfirmLabel')">
        <TextInput :id="f.id" v-model="typed" autocomplete="off" :described-by="f.describedBy" />
      </FormField>
      <BaseButton type="submit" variant="danger" :disabled="!canDelete" :loading="deleting">
        {{ t('safety.deleteButton') }}
      </BaseButton>
    </form>
  </section>
</template>
