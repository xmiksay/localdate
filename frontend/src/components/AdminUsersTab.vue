<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { useT } from '@/i18n/typed'
import { useAdminUsersStore } from '@/stores/adminUsers'
import { errorMessage } from '@/utils/errors'
import AdminUserItem from './AdminUserItem.vue'
import ImpersonateButton from './ImpersonateButton.vue'
import ErrorNote from './ui/ErrorNote.vue'
import FormField from './ui/FormField.vue'
import TextInput from './ui/TextInput.vue'

const DEBOUNCE_MS = 300

const { t } = useT()
const store = useAdminUsersStore()
const q = ref('')
const searched = ref(false)
const failure = ref<string | null>(null)
let timer: ReturnType<typeof setTimeout> | undefined

async function search() {
  failure.value = null
  try {
    await store.search(q.value)
    searched.value = true
  } catch (e) {
    failure.value = errorMessage(e)
  }
}

watch(q, () => {
  clearTimeout(timer)
  timer = setTimeout(search, DEBOUNCE_MS)
})
onMounted(search)
onUnmounted(() => clearTimeout(timer))
</script>

<template>
  <FormField v-slot="f" :label="t('adminUsers.searchLabel')" :hint="t('adminUsers.searchHint')">
    <TextInput
      :id="f.id"
      v-model="q"
      type="search"
      autocomplete="off"
      :maxlength="64"
      :described-by="f.describedBy"
    />
  </FormField>

  <ErrorNote :message="failure" class="mt-4" />
  <p v-if="searched && store.found.length === 0" class="mt-4 text-muted">
    {{ t('adminUsers.empty') }}
  </p>
  <ul class="mt-4 flex flex-col gap-3">
    <li v-for="u in store.found" :key="u.id">
      <AdminUserItem :user="u">
        <ImpersonateButton :user="u" />
      </AdminUserItem>
    </li>
  </ul>
</template>
