<script setup lang="ts">
import { useId } from 'vue'

defineProps<{ label: string; hint?: string; error?: string }>()
const id = useId()
</script>

<template>
  <div class="flex flex-col gap-1.5">
    <label :for="id" class="text-sm font-semibold text-plum">{{ label }}</label>
    <slot :id="id" :described-by="`${id}-d`" :invalid="!!error" />
    <p v-if="error" :id="`${id}-d`" role="alert" class="text-sm font-medium text-danger">
      {{ error }}
    </p>
    <p v-else-if="hint" :id="`${id}-d`" class="text-sm text-muted">{{ hint }}</p>
  </div>
</template>
