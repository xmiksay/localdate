<script setup lang="ts">
defineProps<{ title: string; intro?: string }>()
</script>

<template>
  <header class="mb-6">
    <h1 class="font-display text-3xl font-semibold leading-tight text-ink">{{ title }}</h1>
    <p v-if="intro" class="mt-2 text-muted">{{ intro }}</p>
  </header>
</template>
