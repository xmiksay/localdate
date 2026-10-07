<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import PageHeading from '@/components/ui/PageHeading.vue'

const { t } = useI18n()
</script>

<template>
  <main class="mx-auto max-w-xl px-4 py-16">
    <PageHeading :title="t('notFound.title')" />
    <RouterLink to="/" class="font-semibold text-coral underline">{{
      t('notFound.home')
    }}</RouterLink>
  </main>
</template>
