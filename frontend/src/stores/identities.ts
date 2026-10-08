import { ref } from 'vue'
import { defineStore } from 'pinia'
import * as identitiesApi from '@/api/identities'
import { oauthLink } from '@/api/oauth'
import type { Identity, OAuthProvider, PhotoImportOutcome } from '@/api/types'
import { mailLang } from '@/i18n'
import { normalizeEmail } from '@/utils/validation'

export const useIdentitiesStore = defineStore('identities', () => {
  const identities = ref<Identity[]>([])
  const hasPassword = ref(true)
  /** `hasPassword` is only known after the first load. */
  const loaded = ref(false)
  /** Address confirmed by the link flow, shown once as a success note in settings. */
  const justLinked = ref<string | null>(null)
  /** Provider an OAuth link flow just added, shown once like `justLinked`. */
  const justLinkedProvider = ref<OAuthProvider | null>(null)
  /** What became of a picture import asked for with that link, shown alongside it. */
  const justImportedPhoto = ref<PhotoImportOutcome | null>(null)

  let loading: Promise<void> | null = null

  /** Several sections load on the same page; they share one request. */
  function load() {
    loading ??= identitiesApi
      .getIdentities()
      .then((r) => {
        identities.value = r.identities
        hasPassword.value = r.has_password
        loaded.value = true
      })
      .finally(() => {
        loading = null
      })
    return loading
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

  /** The provider URL to send the browser to; the flow returns via `/auth/oauth/done`. */
  async function startOAuthLink(provider: OAuthProvider, redirect: string, importPhoto = false) {
    const body = { redirect, lang: mailLang(), ...(importPhoto ? { import_photo: true } : {}) }
    return (await oauthLink(provider, body)).url
  }

  async function remove(id: string) {
    await identitiesApi.deleteIdentity(id)
    identities.value = identities.value.filter((i) => i.id !== id)
  }

  function reset() {
    identities.value = []
    hasPassword.value = true
    loaded.value = false
    justLinked.value = null
    justLinkedProvider.value = null
    justImportedPhoto.value = null
  }

  return {
    identities,
    hasPassword,
    loaded,
    justLinked,
    justLinkedProvider,
    justImportedPhoto,
    load,
    linkEmail,
    startOAuthLink,
    confirm,
    remove,
    reset,
  }
})
