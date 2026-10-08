<script setup lang="ts" generic="T extends string | number">
defineProps<{ label: string; options: { value: T; label: string }[] }>()
const model = defineModel<T>({ required: true })
</script>

<template>
  <div role="radiogroup" :aria-label="label" class="flex flex-col gap-2">
    <span class="text-sm font-semibold text-plum">{{ label }}</span>
    <div class="flex flex-wrap gap-2">
      <button
        v-for="o in options"
        :key="o.value"
        type="button"
        role="radio"
        :aria-checked="model === o.value"
        class="min-h-11 rounded-full border-2 px-5 text-sm font-semibold transition"
        :class="
          model === o.value
            ? 'border-plum bg-plum text-cream'
            : 'border-line bg-paper hover:border-plum'
        "
        @click="model = o.value"
      >
        {{ o.label }}
      </button>
    </div>
  </div>
</template>
