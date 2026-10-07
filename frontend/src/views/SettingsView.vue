<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import FilterForm from '@/components/FilterForm.vue'
import SafetySection from '@/components/SafetySection.vue'
import BaseButton from '@/components/ui/BaseButton.vue'
import PageHeading from '@/components/ui/PageHeading.vue'
import { LOCALES, setLocale, type Locale } from '@/i18n'
import { useAuthStore } from '@/stores/auth'

const { t, locale } = useI18n()
const auth = useAuthStore()
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
      <BaseButton variant="ghost" block @click="auth.logout()">{{ t('common.logout') }}</BaseButton>
    </section>
  </div>
</template>
