<script setup lang="ts">
import { type MessageKey, useT } from '@/i18n/typed'
import { useMatchesStore } from '@/stores/matches'

const { t } = useT()
const matches = useMatchesStore()

// Add new top-level destinations here; keep to ≤ 5 so labels fit at 360 px.
const items: { to: string; label: MessageKey; icon: string }[] = [
  {
    to: '/nearby',
    label: 'nav.nearby',
    icon: 'M12 21s-7-6.2-7-11.5A7 7 0 0 1 19 9.5C19 14.8 12 21 12 21Zm0-8.5a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z',
  },
  { to: '/matches', label: 'nav.matches', icon: 'M4 5h16v11H9l-5 4V5Z' },
  {
    to: '/profile',
    label: 'nav.profile',
    icon: 'M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8Zm-8 9a8 8 0 0 1 16 0H4Z',
  },
  { to: '/settings', label: 'nav.settings', icon: 'M4 7h10M18 7h2M4 17h2M10 17h10M16 4v6M8 14v6' },
]
</script>

<template>
  <nav
    :aria-label="t('app.name')"
    class="fixed inset-x-0 bottom-0 z-10 border-t-2 border-line bg-paper/95 pb-[env(safe-area-inset-bottom)] backdrop-blur"
  >
    <ul class="mx-auto flex max-w-xl">
      <li v-for="i in items" :key="i.to" class="flex-1">
        <RouterLink
          :to="i.to"
          class="flex min-h-16 flex-col items-center justify-center gap-1 text-xs font-semibold text-muted"
          active-class="!text-coral"
        >
          <span class="relative">
            <svg
              viewBox="0 0 24 24"
              class="size-6"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path :d="i.icon" />
            </svg>
            <span
              v-if="i.to === '/matches' && matches.hasUnread"
              role="img"
              :aria-label="t('matches.unread')"
              class="absolute -right-1 -top-0.5 size-3 rounded-full border-2 border-paper bg-coral"
            />
          </span>
          {{ t(i.label) }}
        </RouterLink>
      </li>
    </ul>
  </nav>
</template>
