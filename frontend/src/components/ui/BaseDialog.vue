<script setup lang="ts">
import { onMounted, ref, useId } from 'vue'

defineProps<{ title: string }>()
const emit = defineEmits<{ close: [] }>()

const el = ref<HTMLDialogElement | null>(null)
const titleId = useId()

// Native <dialog> gives focus trap, Esc and inert background for free.
onMounted(() => el.value?.showModal())
</script>

<template>
  <dialog
    ref="el"
    :aria-labelledby="titleId"
    class="m-auto w-[min(92vw,28rem)] rounded-3xl border-2 border-line bg-paper p-6 text-ink shadow-xl backdrop:bg-ink/50"
    @close="emit('close')"
    @click.self="emit('close')"
  >
    <h2 :id="titleId" class="mb-4 font-display text-2xl font-semibold">{{ title }}</h2>
    <slot />
  </dialog>
</template>
