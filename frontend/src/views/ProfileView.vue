<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import PhotoManager from '@/components/PhotoManager.vue'
import ProfileForm from '@/components/ProfileForm.vue'
import PageHeading from '@/components/ui/PageHeading.vue'

const { t } = useI18n()
</script>

<template>
  <PageHeading :title="t('profile.title')" />
  <div class="flex flex-col gap-8">
    <PhotoManager />
    <ProfileForm :submit-label="t('common.save')" />
  </div>
</template>
