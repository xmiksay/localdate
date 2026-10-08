<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useT } from '@/i18n/typed'
import { useRouter } from 'vue-router'
import UserActionsMenu from '@/components/UserActionsMenu.vue'
import AvatarImage from '@/components/ui/AvatarImage.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import ErrorNote from '@/components/ui/ErrorNote.vue'
import { useAuthStore } from '@/stores/auth'
import { isPendingMessage, useMatchesStore } from '@/stores/matches'
import { errorMessage } from '@/utils/errors'
import { formatMessageTime } from '@/utils/time'

const MAX_BODY = 2000
const props = defineProps<{ matchId: string }>()
const { t, locale } = useT()
const router = useRouter()
const auth = useAuthStore()
const store = useMatchesStore()

const match = computed(() => store.findMatch(props.matchId))
const list = computed(() => store.messages[props.matchId] ?? [])
const draft = ref('')
const failure = ref<string | null>(null)
const ready = ref(false)
const loadingOlder = ref(false)
const sending = ref(false)

const canSend = computed(() => draft.value.trim().length > 0 && !sending.value)
const nearBottom = () =>
  document.documentElement.scrollHeight - window.scrollY - window.innerHeight < 160
const scrollToBottom = () => window.scrollTo({ top: document.documentElement.scrollHeight })

async function loadOlder() {
  if (loadingOlder.value || !store.hasMore[props.matchId]) return
  loadingOlder.value = true
  const before = document.documentElement.scrollHeight
  try {
    await store.loadOlder(props.matchId)
    await nextTick()
    // Keep the message the reader was looking at in place.
    window.scrollBy({ top: document.documentElement.scrollHeight - before })
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    loadingOlder.value = false
  }
}

function onScroll() {
  if (window.scrollY < 80) void loadOlder()
}

async function send() {
  const body = draft.value.trim()
  if (!body) return
  sending.value = true
  failure.value = null
  draft.value = ''
  try {
    await store.send(props.matchId, body)
  } catch (e) {
    draft.value = body
    failure.value = errorMessage(e)
  } finally {
    sending.value = false
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
    e.preventDefault()
    if (canSend.value) void send()
  }
}

watch(
  () => list.value[list.value.length - 1]?.id,
  async () => {
    const mine = list.value[list.value.length - 1]?.sender_id === auth.user?.id
    const stick = mine || nearBottom()
    await nextTick()
    if (stick) scrollToBottom()
  },
)

onMounted(async () => {
  store.setActive(props.matchId)
  try {
    if (!store.loaded) await store.loadMatches()
    await store.loadMessages(props.matchId)
  } catch (e) {
    failure.value = errorMessage(e)
  }
  ready.value = true
  await nextTick()
  scrollToBottom()
  window.addEventListener('scroll', onScroll, { passive: true })
})
onUnmounted(() => {
  store.setActive(null)
  window.removeEventListener('scroll', onScroll)
})
</script>

<template>
  <header
    class="sticky top-0 z-10 -mx-4 -mt-6 mb-4 flex items-center gap-3 bg-cream/95 px-4 py-3 backdrop-blur"
  >
    <RouterLink
      :to="{ name: 'matches' }"
      :aria-label="t('common.back')"
      class="flex size-11 shrink-0 items-center justify-center text-2xl font-semibold text-plum"
    >
      ←
    </RouterLink>
    <template v-if="match">
      <div class="size-11 shrink-0">
        <AvatarImage :src="match.other.photo_url" :name="match.other.display_name" />
      </div>
      <h1 class="min-w-0 flex-1 truncate font-display text-xl font-semibold">
        {{ match.other.display_name }}
      </h1>
      <UserActionsMenu
        :user-id="match.other.user_id"
        :name="match.other.display_name"
        @done="router.replace({ name: 'matches' })"
      />
    </template>
    <h1 v-else class="font-display text-xl font-semibold">{{ t('chat.title') }}</h1>
  </header>

  <p v-if="!ready" class="text-muted">{{ t('common.loading') }}</p>
  <div v-else-if="!match" class="flex flex-col items-start gap-3">
    <p class="text-muted">{{ t('chat.notFound') }}</p>
    <RouterLink :to="{ name: 'matches' }" class="font-bold text-coral underline">
      {{ t('chat.toChats') }}
    </RouterLink>
  </div>

  <template v-else>
    <div v-if="store.hasMore[matchId]" class="mb-3 flex justify-center">
      <BaseButton
        variant="ghost"
        class="!min-h-10 !text-sm"
        :loading="loadingOlder"
        @click="loadOlder"
      >
        {{ t('chat.loadOlder') }}
      </BaseButton>
    </div>
    <p v-if="list.length === 0" class="text-center text-muted">{{ t('chat.empty') }}</p>
    <ul class="flex flex-col gap-2" aria-live="polite">
      <li
        v-for="m in list"
        :key="m.id"
        class="flex max-w-[85%] flex-col"
        :class="m.sender_id === auth.user?.id ? 'items-end self-end' : 'items-start self-start'"
      >
        <p
          class="whitespace-pre-wrap break-words rounded-3xl px-4 py-2"
          :class="[
            m.sender_id === auth.user?.id ? 'bg-plum text-cream' : 'bg-paper ring-1 ring-line',
            isPendingMessage(m) && 'opacity-60',
          ]"
        >
          {{ m.body }}
        </p>
        <time :datetime="m.created_at" class="px-2 text-xs text-muted">
          {{ formatMessageTime(m.created_at, locale) }}
        </time>
      </li>
    </ul>

    <form
      class="sticky bottom-[calc(4.5rem+env(safe-area-inset-bottom))] -mx-4 mt-4 flex flex-col gap-2 bg-cream/95 px-4 py-3 backdrop-blur"
      @submit.prevent="send"
    >
      <ErrorNote :message="failure" />
      <div class="flex items-end gap-2">
        <textarea
          v-model="draft"
          rows="1"
          :maxlength="MAX_BODY"
          :placeholder="t('chat.placeholder')"
          :aria-label="t('chat.placeholder')"
          class="max-h-32 min-h-12 flex-1 resize-none rounded-3xl border-2 border-line bg-paper px-4 py-3 text-base focus:border-plum"
          @keydown="onKeydown"
        />
        <BaseButton type="submit" :disabled="!canSend">{{ t('chat.send') }}</BaseButton>
      </div>
    </form>
  </template>
</template>
