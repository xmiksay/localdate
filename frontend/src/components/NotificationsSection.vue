<script setup lang="ts">
import { onMounted, ref } from 'vue'
import type { PushPrefs } from '@/api/types'
import { useT } from '@/i18n/typed'
import { usePushStore } from '@/stores/push'
import { errorMessage } from '@/utils/errors'
import { toPushLang } from '@/utils/webPush'
import BaseButton from './ui/BaseButton.vue'
import ChipToggle from './ui/ChipToggle.vue'
import ErrorNote from './ui/ErrorNote.vue'

const { t, locale } = useT()
const push = usePushStore()
const failure = ref<string | null>(null)
const busy = ref(false)
const KINDS: (keyof PushPrefs)[] = ['waves', 'matches', 'messages']

onMounted(async () => {
  try {
    await Promise.all([push.loadConfig(), push.loadPrefs()])
  } catch (e) {
    failure.value = errorMessage(e)
  }
})

async function run(action: () => Promise<void>) {
  busy.value = true
  failure.value = null
  try {
    await action()
  } catch (e) {
    failure.value = errorMessage(e)
  } finally {
    busy.value = false
  }
}

// No await before `enable`: the permission prompt needs the click's user activation.
const enable = () => run(() => push.enable(toPushLang(locale.value)))
const disable = () => run(() => push.disable())
const toggle = (kind: keyof PushPrefs, on: boolean) => run(() => push.setPref(kind, on))
</script>

<template>
  <section aria-labelledby="s-push" class="flex flex-col gap-3">
    <h2 id="s-push" class="font-display text-xl font-semibold">{{ t('push.title') }}</h2>
    <p class="text-sm text-muted">{{ t('push.intro') }}</p>
    <ErrorNote :message="failure" />

    <p v-if="push.support === 'ios_needs_install'" class="rounded-2xl bg-paper p-4 text-sm">
      {{ t('push.iosInstall') }}
    </p>
    <p v-else-if="push.support === 'unsupported'" class="text-muted">{{ t('push.unsupported') }}</p>
    <p v-else-if="push.config && !push.config.enabled" class="text-muted">
      {{ t('push.unavailable') }}
    </p>
    <template v-else-if="push.available">
      <p v-if="push.permission === 'denied'" class="text-sm text-danger">{{ t('push.denied') }}</p>
      <template v-else-if="push.subscribed">
        <p class="text-sm">{{ t('push.enabledHere') }}</p>
        <BaseButton variant="ghost" block :loading="busy" @click="disable">
          {{ t('push.disable') }}
        </BaseButton>
      </template>
      <BaseButton v-else block :loading="busy" @click="enable">{{ t('push.enable') }}</BaseButton>

      <div v-if="push.prefs">
        <h3 class="mb-2 text-sm font-semibold text-plum">{{ t('push.kinds') }}</h3>
        <div class="flex flex-wrap gap-2">
          <ChipToggle
            v-for="kind in KINDS"
            :key="kind"
            :model-value="push.prefs[kind]"
            :disabled="busy"
            @update:model-value="toggle(kind, $event)"
          >
            {{ t(`push.${kind}`) }}
          </ChipToggle>
        </div>
      </div>
    </template>
  </section>
</template>
