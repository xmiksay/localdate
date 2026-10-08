<script setup lang="ts">
import { useT } from '@/i18n/typed'

const { t } = useT()
</script>

<template>
  <section role="alert" class="rounded-3xl border-2 border-danger bg-danger/10 p-5 text-danger">
    <h2 class="font-display text-xl font-semibold">{{ t('auth.suspendedTitle') }}</h2>
    <p class="mt-2 text-sm font-medium">{{ t('auth.suspendedBody') }}</p>
  </section>
</template>
