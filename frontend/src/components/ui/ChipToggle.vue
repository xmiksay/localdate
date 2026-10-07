<script setup lang="ts">
defineProps<{ disabled?: boolean }>()
const model = defineModel<boolean>({ required: true })
</script>

<template>
  <button
    type="button"
    role="checkbox"
    :aria-checked="model"
    :disabled="disabled"
    class="min-h-10 rounded-full border-2 px-4 text-sm font-semibold transition disabled:cursor-not-allowed disabled:opacity-40"
    :class="
      model ? 'border-plum bg-plum text-cream' : 'border-line bg-paper text-ink hover:border-plum'
    "
    @click="model = !model"
  >
    <slot />
  </button>
</template>
