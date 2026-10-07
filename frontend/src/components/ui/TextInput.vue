<script setup lang="ts">
defineProps<{
  id?: string
  describedBy?: string
  invalid?: boolean
  type?: string
  autocomplete?: string
  maxlength?: number
  max?: string
  min?: string
}>()
const model = defineModel<string>({ required: true })
</script>

<template>
  <input
    :id="id"
    v-model="model"
    :type="type ?? 'text'"
    :autocomplete="autocomplete"
    :maxlength="maxlength"
    :max="max"
    :min="min"
    :aria-describedby="describedBy"
    :aria-invalid="invalid || undefined"
    class="min-h-12 w-full rounded-2xl border-2 border-line bg-paper px-4 text-base text-ink placeholder:text-muted/60 focus:border-plum"
    :class="invalid && 'border-danger'"
  />
</template>
