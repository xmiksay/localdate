import { ref } from 'vue'
import { defineStore } from 'pinia'
import * as identitiesApi from '@/api/identities'
import type { Identity } from '@/api/types'
import { mailLang } from '@/i18n'
import { normalizeEmail } from '@/utils/validation'

export const useIdentitiesStore = defineStore('identities', () => {
  const identities = ref<Identity[]>([])
  const hasPassword = ref(true)
  /** Address confirmed by the link flow, shown once as a success note in settings. */
  const justLinked = ref<string | null>(null)

  async function load() {
    const r = await identitiesApi.getIdentities()
    identities.value = r.identities
    hasPassword.value = r.has_password
  }

  async function linkEmail(email: string) {
    await identitiesApi.linkEmail({ email: normalizeEmail(email), lang: mailLang() })
  }

  async function confirm(token: string) {
    const identity = await identitiesApi.confirmEmailLink(token)
    if (!identities.value.some((i) => i.id === identity.id)) identities.value.push(identity)
    justLinked.value = identity.subject
    return identity
  }

  async function remove(id: string) {
    await identitiesApi.deleteIdentity(id)
    identities.value = identities.value.filter((i) => i.id !== id)
  }

  function reset() {
    identities.value = []
    hasPassword.value = true
    justLinked.value = null
  }

  return { identities, hasPassword, justLinked, load, linkEmail, confirm, remove, reset }
})
