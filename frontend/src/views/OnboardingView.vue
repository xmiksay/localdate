<script setup lang="ts">
import { ref } from 'vue'
import { useT } from '@/i18n/typed'
import { useRouter } from 'vue-router'
import FilterForm from '@/components/FilterForm.vue'
import PhotoManager from '@/components/PhotoManager.vue'
import ProfileForm from '@/components/ProfileForm.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ErrorNote from '@/components/ui/ErrorNote.vue'
import PageHeading from '@/components/ui/PageHeading.vue'
import { useMeStore } from '@/stores/me'

const TOTAL = 3
const { t } = useT()
const router = useRouter()
const me = useMeStore()

// Resume where the user left off after a reload.
const step = ref(!me.profile ? 1 : me.photos.length === 0 ? 2 : 3)
const photoError = ref(false)

function toPhotos() {
  step.value = 2
}
function toFilter() {
  photoError.value = me.photos.length === 0
  if (!photoError.value) step.value = 3
}
function finish() {
  router.replace({ name: 'nearby' })
}
</script>

<template>
  <main class="mx-auto max-w-xl px-5 pb-12 pt-8">
    <div class="mb-6 flex items-center justify-between">
      <p class="font-display text-xl font-semibold italic text-coral">{{ t('app.name') }}</p>
      <p class="text-sm font-semibold text-muted" aria-live="polite">
        {{ t('onboarding.step', { n: step, total: TOTAL }) }}
      </p>
    </div>
    <div class="mb-8 flex gap-1.5" aria-hidden="true">
      <span
        v-for="n in TOTAL"
        :key="n"
        class="h-1.5 flex-1 rounded-full"
        :class="n <= step ? 'bg-coral' : 'bg-line'"
      />
    </div>

    <section v-if="step === 1">
      <PageHeading :title="t('onboarding.profileTitle')" :intro="t('onboarding.profileIntro')" />
      <ProfileForm :submit-label="t('common.next')" @saved="toPhotos" />
    </section>

    <section v-else-if="step === 2" class="flex flex-col gap-5">
      <PageHeading :title="t('onboarding.photosTitle')" :intro="t('onboarding.photosIntro')" />
      <PhotoManager />
      <ErrorNote :message="photoError ? t('onboarding.photosRequired') : null" />
      <div class="flex gap-3">
        <BaseButton variant="ghost" @click="step = 1">{{ t('common.back') }}</BaseButton>
        <BaseButton class="flex-1" @click="toFilter">{{ t('common.next') }}</BaseButton>
      </div>
    </section>

    <section v-else>
      <PageHeading :title="t('onboarding.filterTitle')" :intro="t('onboarding.filterIntro')" />
      <FilterForm :submit-label="t('onboarding.finish')" @saved="finish" />
      <BaseButton class="mt-3" variant="ghost" @click="step = 2">{{ t('common.back') }}</BaseButton>
    </section>
  </main>
</template>
