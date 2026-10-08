<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useT } from '@/i18n/typed'
import AvatarImage from '@/components/ui/AvatarImage.vue'
import ErrorNote from '@/components/ui/ErrorNote.vue'
import PageHeading from '@/components/ui/PageHeading.vue'
import { useAuthStore } from '@/stores/auth'
import { useMatchesStore } from '@/stores/matches'
import { errorMessage } from '@/utils/errors'
import { formatMessageTime } from '@/utils/time'

const { t, locale } = useT()
const auth = useAuthStore()
const store = useMatchesStore()
const failure = ref<string | null>(null)

onMounted(() => store.loadMatches().catch((e) => (failure.value = errorMessage(e))))
</script>

<template>
  <PageHeading :title="t('matches.title')" />
  <ErrorNote :message="failure" class="mb-4" />
  <p v-if="!store.loaded && !failure" class="text-muted">{{ t('common.loading') }}</p>
  <p v-else-if="store.loaded && store.matches.length === 0" class="text-muted">
    {{ t('matches.empty') }}
  </p>
  <ul class="flex flex-col gap-3">
    <li v-for="m in store.matches" :key="m.match_id">
      <RouterLink
        :to="{ name: 'chat', params: { matchId: m.match_id } }"
        class="flex items-center gap-3 rounded-3xl border-2 border-line bg-paper p-3 transition hover:border-plum"
      >
        <div class="size-14 shrink-0">
          <AvatarImage :src="m.other.photo_url" :name="m.other.display_name" />
        </div>
        <div class="min-w-0 flex-1">
          <div class="flex items-baseline justify-between gap-2">
            <h2 class="truncate font-display text-lg font-semibold">{{ m.other.display_name }}</h2>
            <time
              v-if="m.last_message"
              :datetime="m.last_message.created_at"
              class="shrink-0 text-xs text-muted"
            >
              {{ formatMessageTime(m.last_message.created_at, locale) }}
            </time>
          </div>
          <p
            class="truncate text-sm"
            :class="store.unread[m.match_id] ? 'font-bold text-ink' : 'text-muted'"
          >
            <template v-if="m.last_message">
              <span v-if="m.last_message.sender_id === auth.user?.id"
                >{{ t('matches.you') }}:
              </span>
              {{ m.last_message.body }}
            </template>
            <template v-else>{{ t('matches.noMessages') }}</template>
          </p>
        </div>
        <span
          v-if="store.unread[m.match_id]"
          class="size-3 shrink-0 rounded-full bg-coral"
          role="img"
          :aria-label="t('matches.unread')"
        />
      </RouterLink>
    </li>
  </ul>
</template>
