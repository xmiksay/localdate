<script setup lang="ts">
withDefaults(
  defineProps<{
    variant?: 'primary' | 'ghost' | 'danger'
    type?: 'button' | 'submit'
    loading?: boolean
    disabled?: boolean
    block?: boolean
  }>(),
  { variant: 'primary', type: 'button' },
)
</script>

<template>
  <button
    :type="type"
    :disabled="disabled || loading"
    :aria-busy="loading || undefined"
    class="inline-flex min-h-12 items-center justify-center gap-2 rounded-full px-6 font-semibold transition active:scale-[0.98] disabled:cursor-not-allowed disabled:opacity-50"
    :class="[
      block && 'w-full',
      variant === 'primary' &&
        'bg-coral text-white shadow-[0_4px_0_var(--color-coral-dark)] hover:bg-coral-dark',
      variant === 'ghost' && 'border-2 border-line bg-paper text-ink hover:border-plum',
      variant === 'danger' && 'border-2 border-danger bg-paper text-danger hover:bg-danger/10',
    ]"
  >
    <span
      v-if="loading"
      class="size-4 animate-spin rounded-full border-2 border-current border-t-transparent"
      aria-hidden="true"
    />
    <slot />
  </button>
</template>
