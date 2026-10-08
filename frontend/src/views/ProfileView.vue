<script setup lang="ts">
import { useT } from '@/i18n/typed'
import PhotoManager from '@/components/PhotoManager.vue'
import ProfileForm from '@/components/ProfileForm.vue'
import PageHeading from '@/components/ui/PageHeading.vue'

const { t } = useT()
</script>

<template>
  <PageHeading :title="t('profile.title')" />
  <div class="flex flex-col gap-8">
    <PhotoManager />
    <ProfileForm :submit-label="t('common.save')" />
  </div>
</template>
