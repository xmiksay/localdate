<script setup lang="ts">
import { computed, ref } from 'vue'
import { useT } from '@/i18n/typed'
import { errorMessage } from '@/utils/errors'
import { isValidEmail, normalizeEmail } from '@/utils/validation'
import BaseButton from './ui/BaseButton.vue'
import ErrorNote from './ui/ErrorNote.vue'
import FormField from './ui/FormField.vue'
import TextInput from './ui/TextInput.vue'

/** Email input that asks the server to mail a link, then confirms where it went. */
const props = defineProps<{ submitLabel: string; send: (email: string) => Promise<void> }>()
const { t } = useT()

const email = ref('')
const submitted = ref(false)
const busy = ref(false)
const failure = ref<string | null>(null)
const sentTo = ref<string | null>(null)

const emailError = computed(() =>
  submitted.value && !isValidEmail(email.value) ? t('auth.invalidEmail') : undefined,
)

async function submit() {
  submitted.value = true
  failure.value = null
  if (emailError.value) return
  busy.value = true
  try {
    await props.send(email.value)
    sentTo.value = normalizeEmail(email.value)
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}

function again() {
  sentTo.value = null
  email.value = ''
  submitted.value = false
}
</script>

<template>
  <div
    v-if="sentTo"
    role="status"
    class="flex flex-col gap-3 rounded-3xl border-2 border-line bg-paper p-5"
  >
    <h3 class="font-display text-lg font-semibold">{{ t('auth.emailSentTitle') }}</h3>
    <p class="text-sm text-muted">{{ t('auth.emailSentBody', { email: sentTo }) }}</p>
    <BaseButton variant="ghost" @click="again">{{ t('auth.emailOther') }}</BaseButton>
  </div>
  <form v-else class="flex flex-col gap-3" novalidate @submit.prevent="submit">
    <FormField v-slot="f" :label="t('auth.email')" :error="emailError">
      <TextInput
        :id="f.id"
        v-model="email"
        type="email"
        autocomplete="email"
        :maxlength="254"
        :described-by="f.describedBy"
        :invalid="f.invalid"
      />
    </FormField>
    <ErrorNote :message="failure" />
    <BaseButton type="submit" variant="ghost" :loading="busy">{{ submitLabel }}</BaseButton>
  </form>
</template>
