<script setup lang="ts">
import { useT } from '@/i18n/typed'
import PageHeading from '@/components/ui/PageHeading.vue'

const { t } = useT()
</script>

<template>
  <main class="mx-auto max-w-xl px-4 py-16">
    <PageHeading :title="t('notFound.title')" />
    <RouterLink to="/" class="font-semibold text-coral underline">{{
      t('notFound.home')
    }}</RouterLink>
  </main>
</template>
