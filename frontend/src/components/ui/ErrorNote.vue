<script setup lang="ts">
defineProps<{ message: string | null }>()
</script>

<template>
  <p
    v-if="message"
    role="alert"
    class="rounded-2xl border-2 border-danger/40 bg-danger/10 px-4 py-3 text-sm font-medium text-danger"
  >
    {{ message }}
  </p>
</template>
