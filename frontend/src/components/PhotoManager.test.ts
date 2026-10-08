import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import * as meApi from '@/api/me'
import { i18n } from '@/i18n'
import { prepareUpload } from '@/utils/imageResize'
import { MAX_PHOTO_BYTES } from '@/utils/validation'
import PhotoManager from './PhotoManager.vue'

vi.mock('@/api/me')
vi.mock('@/utils/imageResize')

const t = (key: string) => i18n.global.t(key)
const fileOf = (bytes: number, name: string, type: string) =>
  new File([new Uint8Array(bytes)], name, { type })

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
})
