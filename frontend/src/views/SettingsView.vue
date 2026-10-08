<script setup lang="ts">
import { useT } from '@/i18n/typed'
import FilterForm from '@/components/FilterForm.vue'
import SafetySection from '@/components/SafetySection.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import PageHeading from '@/components/ui/PageHeading.vue'
import { LOCALES, setLocale, type Locale } from '@/i18n'
import { useAuthStore } from '@/stores/auth'
import { useMeStore } from '@/stores/me'

const { t, locale } = useT()
const auth = useAuthStore()
const me = useMeStore()
</script>

<template>
  <PageHeading :title="t('settings.title')" />
  <div class="flex flex-col gap-10">
    <section aria-labelledby="s-filter">
      <h2 id="s-filter" class="mb-4 font-display text-xl font-semibold">
        {{ t('settings.filter') }}
      </h2>
      <FilterForm :submit-label="t('common.save')" />
    </section>

    <section aria-labelledby="s-lang">
      <h2 id="s-lang" class="mb-3 font-display text-xl font-semibold">
        {{ t('settings.language') }}
      </h2>
      <div role="radiogroup" aria-labelledby="s-lang" class="flex gap-2">
        <button
          v-for="l in LOCALES"
          :key="l"
          type="button"
          role="radio"
          :aria-checked="locale === l"
          class="min-h-11 rounded-full border-2 px-5 text-sm font-semibold uppercase transition"
          :class="
            locale === l
              ? 'border-plum bg-plum text-cream'
              : 'border-line bg-paper hover:border-plum'
          "
          @click="setLocale(l as Locale)"
        >
          {{ l }}
        </button>
      </div>
    </section>

    <SafetySection />

    <section aria-labelledby="s-account" class="flex flex-col gap-3">
      <h2 id="s-account" class="font-display text-xl font-semibold">{{ t('settings.account') }}</h2>
      <p v-if="auth.user" class="text-muted">
        {{ t('settings.loggedInAs', { username: auth.user.username }) }}
      </p>
      <RouterLink
        v-if="me.isAdmin"
        :to="{ name: 'admin' }"
        class="flex min-h-12 items-center justify-center rounded-full border-2 border-plum bg-paper px-6 font-semibold text-plum hover:bg-plum/5"
      >
        {{ t('settings.admin') }}
      </RouterLink>
      <BaseButton variant="ghost" block @click="auth.logout()">{{ t('common.logout') }}</BaseButton>
    </section>
  </div>
</template>
