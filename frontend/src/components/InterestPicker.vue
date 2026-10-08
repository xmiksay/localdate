<script setup lang="ts">
import { computed } from 'vue'
import { interestKey, useT } from '@/i18n/typed'
import type { Interest } from '@/api/types'
import { MAX_INTERESTS } from '@/utils/validation'
import ChipToggle from './ui/ChipToggle.vue'

const props = defineProps<{ options: Interest[] }>()
const selected = defineModel<number[]>({ required: true })
const { t } = useT()

const full = computed(() => selected.value.length >= MAX_INTERESTS)

function toggle(id: number, on: boolean) {
  selected.value = on ? [...selected.value, id] : selected.value.filter((i) => i !== id)
}
</script>

<template>
  <fieldset>
    <legend class="text-sm font-semibold text-plum">
      {{ t('profile.interests') }}
      <span class="font-normal text-muted">({{ selected.length }}/{{ MAX_INTERESTS }})</span>
    </legend>
    <p class="mb-2 text-sm text-muted">
      {{
        full
          ? t('profile.interestsLimit', { max: MAX_INTERESTS })
          : t('profile.interestsHint', { max: MAX_INTERESTS })
      }}
    </p>
    <div class="flex flex-wrap gap-2">
      <ChipToggle
        v-for="i in props.options"
        :key="i.id"
        :model-value="selected.includes(i.id)"
        :disabled="full && !selected.includes(i.id)"
        @update:model-value="toggle(i.id, $event)"
      >
        {{ t(interestKey(i.key)) }}
      </ChipToggle>
    </div>
  </fieldset>
</template>
