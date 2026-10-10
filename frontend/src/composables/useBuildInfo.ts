import { readonly, ref } from 'vue'
import { getHealth } from '@/api/health'
import type { Health } from '@/api/types'

const info = ref<Health | null>(null)
let loading: Promise<void> | null = null

/** The server's version/commit, fetched once per page load and shared by every caller. */
export function useBuildInfo() {
  loading ??= getHealth().then((h) => {
    info.value = h
  })
  return readonly(info)
}

/** Tests only: forget the cached answer. */
export function resetBuildInfo() {
  info.value = null
  loading = null
}
