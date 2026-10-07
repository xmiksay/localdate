<script setup lang="ts">
defineProps<{ src: string | null; name: string }>()
</script>

<template>
  <img
    v-if="src"
    :src="src"
    :alt="name"
    loading="lazy"
    class="size-full rounded-full bg-line object-cover"
  />
  <span
    v-else
    class="flex size-full items-center justify-center rounded-full bg-plum font-display text-lg font-semibold text-cream"
    aria-hidden="true"
  >
    {{ name.charAt(0).toUpperCase() }}
  </span>
</template>
