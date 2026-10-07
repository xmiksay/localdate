<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import MatchMoment from '@/components/MatchMoment.vue'
import PhotoCarousel from '@/components/PhotoCarousel.vue'
import UserActionsMenu from '@/components/UserActionsMenu.vue'
import WaveButton from '@/components/WaveButton.vue'
import ErrorNote from '@/components/ui/ErrorNote.vue'
import { useNearbyStore } from '@/stores/nearby'
import { errorMessage } from '@/utils/errors'

const props = defineProps<{ userId: string }>()
const { t } = useI18n()
const router = useRouter()
const nearby = useNearbyStore()

const loading = ref(false)
const failure = ref<string | null>(null)
const matchId = ref<string | null>(null)
const profile = computed(() => nearby.find(props.userId))

onMounted(async () => {
  if (profile.value) return
  loading.value = true
  try {
    // No single-profile endpoint: a deep link or reload falls back to the nearby list.
    await nearby.refresh()
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    loading.value = false
  }
})

const leave = () => router.replace({ name: 'nearby' })
</script>

<template>
  <div class="mb-4 flex items-center justify-between">
    <RouterLink
      :to="{ name: 'nearby' }"
      class="inline-flex min-h-11 items-center font-semibold text-plum"
    >
      ← {{ t('common.back') }}
    </RouterLink>
    <UserActionsMenu
      v-if="profile"
      :user-id="profile.user_id"
      :name="profile.display_name"
      @done="leave"
    />
  </div>

  <p v-if="loading" class="text-muted">{{ t('common.loading') }}</p>
  <ErrorNote v-else-if="failure" :message="failure" />

  <article v-else-if="profile" class="flex flex-col gap-5">
    <PhotoCarousel :photos="profile.photos" :name="profile.display_name" />
    <header>
      <h1 class="font-display text-3xl font-semibold leading-tight">
        {{ profile.display_name }}, {{ profile.age }}
      </h1>
      <p class="mt-1 font-semibold text-coral">{{ t(`distance.${profile.distance_band}`) }}</p>
    </header>

    <section v-if="profile.bio">
      <h2 class="mb-1 text-sm font-semibold text-plum">{{ t('person.about') }}</h2>
      <p class="whitespace-pre-line">{{ profile.bio }}</p>
    </section>

    <section>
      <h2 class="mb-1.5 text-sm font-semibold text-plum">{{ t('person.reasons') }}</h2>
      <ul class="flex flex-wrap gap-2">
        <li
          v-for="r in profile.reasons"
          :key="r"
          class="rounded-full bg-sun/30 px-3 py-1 text-sm font-semibold"
        >
          {{ t(`reason.${r}`) }}
        </li>
      </ul>
    </section>

    <section v-if="profile.interests.length">
      <h2 class="mb-1.5 text-sm font-semibold text-plum">{{ t('person.interests') }}</h2>
      <ul class="flex flex-wrap gap-2">
        <li
          v-for="i in profile.interests"
          :key="i.id"
          class="rounded-full bg-paper px-3 py-1 text-sm font-medium text-plum ring-1 ring-line"
        >
          {{ t(`interest.${i.key}`) }}
        </li>
      </ul>
    </section>

    <WaveButton :profile="profile" @matched="matchId = $event" />
  </article>

  <div v-else class="flex flex-col items-start gap-3">
    <p class="text-muted">{{ t('person.notNearby') }}</p>
    <RouterLink :to="{ name: 'nearby' }" class="font-bold text-coral underline">
      {{ t('person.toNearby') }}
    </RouterLink>
  </div>

  <MatchMoment v-if="matchId" :match-id="matchId" @close="matchId = null" />
</template>
