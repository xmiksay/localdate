import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import * as authApi from '@/api/auth'
import * as meApi from '@/api/me'
import * as oauthApi from '@/api/oauth'
import type { Photo } from '@/api/types'
import { i18n } from '@/i18n'
import { prepareUpload } from '@/utils/imageResize'
import { useMeStore } from '@/stores/me'
import { MAX_PHOTO_BYTES, MAX_PHOTOS } from '@/utils/validation'
import PhotoManager from './PhotoManager.vue'

vi.mock('@/api/auth')
vi.mock('@/api/me')
vi.mock('@/api/oauth')
vi.mock('@/utils/imageResize')

const t = (key: string) => i18n.global.t(key)
const fileOf = (bytes: number, name: string, type: string) =>
  new File([new Uint8Array(bytes)], name, { type })

const importLabel = i18n.global.t('photos.importFrom', {
  from: i18n.global.t('oauth.from.facebook'),
})
const photo = (i: number): Photo => ({ id: `p${i}`, url: `/media/p${i}.webp`, position: i })

async function render(facebook: boolean, photos = 0) {
  vi.mocked(authApi.getProviders).mockResolvedValue({
    email: false,
    google: true,
    telegram: false,
    facebook,
    password_reset: false,
  })
  useMeStore().photos = Array.from({ length: photos }, (_, i) => photo(i))
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/profile', component: { template: '<div />' } }],
  })
  await router.push('/profile')
  await router.isReady()
  const w = mount(PhotoManager, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return w
}

const importButton = (w: Awaited<ReturnType<typeof render>>) =>
  w.findAll('button').find((b) => b.text() === importLabel)

async function pick(file: File) {
  const w = mount(PhotoManager, { global: { plugins: [i18n] } })
  const input = w.get('input[type="file"]')
  Object.defineProperty(input.element, 'files', { value: [file] })
  await input.trigger('change')
  return w
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
})

describe('PhotoManager', () => {
  it('shows the processing label while the photo is being prepared', async () => {
    let finish: (f: File) => void = () => {}
    vi.mocked(prepareUpload).mockReturnValue(new Promise((r) => (finish = r)))
    vi.mocked(meApi.uploadPhoto).mockResolvedValue({ id: 'p1', url: '/media/p1.webp', position: 0 })
    const w = await pick(fileOf(10, 'a.jpg', 'image/jpeg'))
    expect(w.text()).toContain(t('photos.processing'))

    finish(fileOf(10, 'a.jpg', 'image/jpeg'))
    await flushPromises()
    expect(w.text()).not.toContain(t('photos.processing'))
    expect(w.text()).toContain(t('photos.add'))
  })

  it('validates and uploads the prepared file, not the oversized original', async () => {
    const original = fileOf(MAX_PHOTO_BYTES + 1, 'big.png', 'image/png')
    const prepared = fileOf(100, 'big.jpg', 'image/jpeg')
    vi.mocked(prepareUpload).mockResolvedValue(prepared)
    vi.mocked(meApi.uploadPhoto).mockResolvedValue({ id: 'p1', url: '/media/p1.webp', position: 0 })
    const w = await pick(original)
    await flushPromises()
    expect(prepareUpload).toHaveBeenCalledWith(original)
    expect(meApi.uploadPhoto).toHaveBeenCalledWith(prepared)
    expect(w.text()).not.toContain(t('photos.errSize'))
  })

  it('rejects a prepared file of an unsupported type without uploading', async () => {
    vi.mocked(prepareUpload).mockImplementation(async (f) => f)
    const w = await pick(fileOf(100, 'a.heic', 'image/heic'))
    await flushPromises()
    expect(w.text()).toContain(t('photos.errType'))
    expect(meApi.uploadPhoto).not.toHaveBeenCalled()
  })

  it('offers the Facebook import only when Facebook is enabled', async () => {
    expect(importButton(await render(false))).toBeUndefined()
    const w = await render(true)
    expect(importButton(w)?.attributes('disabled')).toBeUndefined()
    expect(w.text()).not.toContain(i18n.global.t('photos.importFull'))
  })

  it('starts the import and comes back to the page it started from', async () => {
    vi.mocked(oauthApi.oauthImport).mockResolvedValue({ url: 'https://fb.example/x' })
    const assign = vi.fn()
    vi.stubGlobal('location', { ...window.location, assign })
    const w = await render(true, 2)
    await importButton(w)!.trigger('click')
    await flushPromises()
    expect(oauthApi.oauthImport).toHaveBeenCalledWith('facebook', {
      redirect: '/profile',
      lang: 'cs',
    })
    expect(assign).toHaveBeenCalledWith('https://fb.example/x')
    vi.unstubAllGlobals()
  })

  it('disables the import with a hint when every photo slot is taken', async () => {
    const w = await render(true, MAX_PHOTOS)
    expect(importButton(w)?.attributes('disabled')).toBeDefined()
    expect(w.text()).toContain(i18n.global.t('photos.importFull'))
  })

  it('shows the outcome of a finished import once', async () => {
    const w = await render(true, 1)
    useMeStore().justImportedPhoto = 'none'
    await flushPromises()
    expect(w.text()).toContain(i18n.global.t('oauth.photo.none'))
    w.unmount()
    expect(useMeStore().justImportedPhoto).toBeNull()
  })
})
