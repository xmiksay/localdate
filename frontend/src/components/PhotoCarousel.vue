<script setup lang="ts">
import { ref } from 'vue'
import { useT } from '@/i18n/typed'
import type { Photo } from '@/api/types'

defineProps<{ photos: Photo[]; name: string }>()
const { t } = useT()
const track = ref<HTMLElement | null>(null)
const index = ref(0)

function onScroll() {
  const el = track.value
  if (el) index.value = Math.round(el.scrollLeft / el.clientWidth)
}
function go(i: number) {
  track.value?.scrollTo({ left: i * track.value.clientWidth, behavior: 'smooth' })
}
</script>

<template>
  <div class="relative overflow-hidden rounded-3xl border-2 border-line bg-line">
    <div
      ref="track"
      class="flex snap-x snap-mandatory overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      @scroll.passive="onScroll"
    >
      <img
        v-for="(p, i) in photos"
        :key="p.id"
        :src="p.url"
        :alt="`${name}, ${t('photos.alt', { n: i + 1 })}`"
        class="aspect-[4/5] w-full shrink-0 snap-center object-cover"
      />
    </div>
    <div v-if="photos.length > 1" class="absolute inset-x-0 bottom-3 flex justify-center gap-2">
      <button
        v-for="(p, i) in photos"
        :key="p.id"
        type="button"
        :aria-label="t('person.photoNav', { n: i + 1, total: photos.length })"
        :aria-current="i === index || undefined"
        class="size-3 rounded-full border-2 border-paper transition"
        :class="i === index ? 'bg-sun' : 'bg-ink/40'"
        @click="go(i)"
      />
    </div>
  </div>
</template>
