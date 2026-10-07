<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import MatchMoment from '@/components/MatchMoment.vue'
import NearbyCard from '@/components/NearbyCard.vue'
import WaveButton from '@/components/WaveButton.vue'
import WindowStartPanel from '@/components/WindowStartPanel.vue'
import WindowStatusBar from '@/components/WindowStatusBar.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ErrorNote from '@/components/ui/ErrorNote.vue'
import PageHeading from '@/components/ui/PageHeading.vue'
import { useNearbyStore } from '@/stores/nearby'
import { useWindowStore } from '@/stores/window'
import { errorMessage } from '@/utils/errors'

const REFRESH_MS = 60_000

const { t } = useI18n()
const win = useWindowStore()
const nearby = useNearbyStore()

const loading = ref(false)
const failure = ref<string | null>(null)
const matchId = ref<string | null>(null)
let timer: ReturnType<typeof setInterval> | undefined

async function refresh() {
  if (!win.isActive) return
  loading.value = true
  try {
    await nearby.refresh()
    failure.value = null
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    loading.value = false
  }
}

function autoRefresh() {
  if (document.visibilityState === 'visible') void refresh()
}

function stopTimer() {
  clearInterval(timer)
  timer = undefined
}

watch(
  () => win.isActive,
  (active) => {
    stopTimer()
    if (!active) return nearby.reset()
    void refresh()
    timer = setInterval(autoRefresh, REFRESH_MS)
  },
)

onMounted(async () => {
  if (!win.loaded) await win.load().catch((e) => (failure.value = errorMessage(e)))
  if (win.isActive) {
    void refresh()
    timer = setInterval(autoRefresh, REFRESH_MS)
  }
})
onUnmounted(stopTimer)
</script>

<template>
  <PageHeading :title="t('nearby.title')" />

  <p v-if="!win.loaded && !failure" class="text-muted">{{ t('common.loading') }}</p>
  <ErrorNote v-else-if="!win.loaded" :message="failure" />

  <WindowStartPanel v-else-if="!win.isActive" />

  <div v-else class="flex flex-col gap-6">
    <WindowStatusBar />
    <ErrorNote
      :message="
        win.locationError
          ? t(win.locationError === 'denied' ? 'nearby.geoDenied' : 'nearby.geoUnavailable')
          : null
      "
    />

    <section v-if="nearby.incoming.length" aria-labelledby="n-incoming">
      <h2 id="n-incoming" class="mb-3 font-display text-xl font-semibold">
        {{ t('nearby.incoming') }}
      </h2>
      <ul class="flex flex-col gap-3">
        <li
          v-for="p in nearby.incoming"
          :key="p.user_id"
          class="flex flex-col gap-3 rounded-3xl border-2 border-sun bg-sun/10 p-3"
        >
          <NearbyCard :profile="p" class="!border-0 !bg-transparent !p-0" />
          <WaveButton :profile="p" @matched="matchId = $event" />
        </li>
      </ul>
    </section>

    <section aria-labelledby="n-people">
      <div class="mb-3 flex items-center justify-between">
        <h2 id="n-people" class="font-display text-xl font-semibold">{{ t('nearby.title') }}</h2>
        <BaseButton variant="ghost" class="!min-h-10 !text-sm" :loading="loading" @click="refresh">
          {{ t('nearby.refresh') }}
        </BaseButton>
      </div>
      <ErrorNote :message="failure" class="mb-3" />
      <ul v-if="nearby.people.length" class="flex flex-col gap-3">
        <li v-for="p in nearby.people" :key="p.user_id">
          <NearbyCard :profile="p" />
        </li>
      </ul>
      <p v-else-if="nearby.loaded" class="text-muted">{{ t('nearby.empty') }}</p>
    </section>
  </div>

  <MatchMoment v-if="matchId" :match-id="matchId" @close="matchId = null" />
</template>
