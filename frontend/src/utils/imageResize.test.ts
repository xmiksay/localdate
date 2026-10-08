import { describe, expect, it, vi } from 'vitest'
import { fitDimensions, prepareUpload, type ImageCodec, type Size } from './imageResize'

const CAP = 1000
const fileOf = (bytes: number, name = 'IMG_1234.png', type = 'image/png') =>
  new File([new Uint8Array(bytes)], name, { type })

/** A codec whose probe reports `width`×`height` (or fails) and whose decode honours the resize. */
function codec(width: number, height: number, { probeFails = false } = {}) {
  const close = vi.fn()
  const c = {
    probe: vi.fn<ImageCodec['probe']>(async () =>
      probeFails ? Promise.reject(new Error('no Image')) : { width, height },
    ),
    decode: vi.fn<ImageCodec['decode']>(async (_f, size?: Size) => ({
      ...(size ?? { width, height }),
      close,
    })),
    encodeJpeg: vi.fn<ImageCodec['encodeJpeg']>(async () => new Blob([new Uint8Array(100)])),
  } satisfies ImageCodec
  return { c, close }
}

describe('fitDimensions', () => {
  it('caps the long edge and keeps the aspect', () => {
    expect(fitDimensions(8000, 6000, 2048)).toEqual({ width: 2048, height: 1536 })
    expect(fitDimensions(3000, 4000, 2048)).toEqual({ width: 1536, height: 2048 })
  })
  it('never upscales', () => {
    expect(fitDimensions(1000, 2048, 2048)).toEqual({ width: 1000, height: 2048 })
  })
  it('keeps a degenerate side at least one pixel', () => {
    expect(fitDimensions(10000, 1, 2048)).toEqual({ width: 2048, height: 1 })
  })
})

describe('prepareUpload', () => {
  it('passes a small accepted image through without decoding it', async () => {
    const { c } = codec(1600, 1200)
    const file = fileOf(500)
    expect(await prepareUpload(file, c, CAP)).toBe(file)
    expect(c.decode).not.toHaveBeenCalled()
    expect(c.encodeJpeg).not.toHaveBeenCalled()
  })

  it('decodes a large image straight at the target size and encodes a JPEG', async () => {
    const { c, close } = codec(8000, 6000)
    const out = await prepareUpload(fileOf(500), c, CAP)
    expect(c.decode).toHaveBeenCalledWith(expect.any(File), { width: 2048, height: 1536 })
    expect(c.encodeJpeg).toHaveBeenCalledWith(expect.anything(), 2048, 1536)
    expect(out.type).toBe('image/jpeg')
    expect(out.name).toBe('IMG_1234.jpg')
    expect(out.size).toBe(100)
    expect(close).toHaveBeenCalledOnce()
  })

  it('scales a portrait image by its height', async () => {
    const { c } = codec(3000, 4000)
    await prepareUpload(fileOf(500), c, CAP)
    expect(c.encodeJpeg).toHaveBeenCalledWith(expect.anything(), 1536, 2048)
  })

  it('re-encodes a small accepted image whose file is over the cap', async () => {
    const { c } = codec(1000, 800)
    const out = await prepareUpload(fileOf(2000), c, CAP)
    expect(c.encodeJpeg).toHaveBeenCalledWith(expect.anything(), 1000, 800)
    expect(out.type).toBe('image/jpeg')
  })

  it.each(['image/heic', 'image/avif', 'image/bmp', ''])(
    're-encodes a small decodable image of type %j',
    async (type) => {
      const { c } = codec(800, 600)
      const out = await prepareUpload(fileOf(500, 'x.heic', type), c, CAP)
      expect(out.type).toBe('image/jpeg')
      expect(out.name).toBe('x.jpg')
    },
  )

  it('returns the JPEG even when it is still over the cap', async () => {
    const { c } = codec(8000, 6000)
    c.encodeJpeg.mockResolvedValueOnce(new Blob([new Uint8Array(CAP + 1)]))
    const out = await prepareUpload(fileOf(500), c, CAP)
    expect(c.encodeJpeg).toHaveBeenCalledOnce()
    expect(out.size).toBe(CAP + 1)
  })

  it('returns the original when decoding fails', async () => {
    const { c } = codec(8000, 6000, { probeFails: true })
    c.decode.mockRejectedValueOnce(new Error('unsupported'))
    const file = fileOf(5000, 'a.heic', 'image/heic')
    expect(await prepareUpload(file, c, CAP)).toBe(file)
    expect(c.encodeJpeg).not.toHaveBeenCalled()
  })

  it('returns the original and closes the bitmap when encoding fails', async () => {
    const { c, close } = codec(8000, 6000)
    c.encodeJpeg.mockRejectedValueOnce(new Error('no canvas'))
    const file = fileOf(500)
    expect(await prepareUpload(file, c, CAP)).toBe(file)
    expect(close).toHaveBeenCalledOnce()
  })

  it('falls back to a full decode when the resized bitmap comes out rotated', async () => {
    const { c, close } = codec(3000, 4000)
    c.decode.mockResolvedValueOnce({ width: 2048, height: 1536, close })
    const out = await prepareUpload(fileOf(500), c, CAP)
    expect(c.decode).toHaveBeenNthCalledWith(1, expect.any(File), { width: 1536, height: 2048 })
    expect(c.decode).toHaveBeenNthCalledWith(2, expect.any(File))
    expect(c.encodeJpeg).toHaveBeenCalledWith(expect.anything(), 1536, 2048)
    expect(out.type).toBe('image/jpeg')
    expect(close).toHaveBeenCalledTimes(2)
  })

  it('accepts a resized bitmap within rounding of the target', async () => {
    const { c } = codec(8000, 6000)
    c.decode.mockImplementationOnce(async () => ({ width: 2047, height: 1537, close: vi.fn() }))
    await prepareUpload(fileOf(500), c, CAP)
    expect(c.decode).toHaveBeenCalledOnce()
  })

  describe('when the cheap dimension probe fails', () => {
    it('decodes at full size and passes a small accepted image through', async () => {
      const { c, close } = codec(1600, 1200, { probeFails: true })
      const file = fileOf(500)
      expect(await prepareUpload(file, c, CAP)).toBe(file)
      expect(c.decode).toHaveBeenCalledWith(file)
      expect(c.encodeJpeg).not.toHaveBeenCalled()
      expect(close).toHaveBeenCalledOnce()
    })

    it('decodes at full size and downscales on encode', async () => {
      const { c, close } = codec(8000, 6000, { probeFails: true })
      const out = await prepareUpload(fileOf(500), c, CAP)
      expect(c.decode).toHaveBeenCalledWith(expect.any(File))
      expect(c.encodeJpeg).toHaveBeenCalledWith(expect.anything(), 2048, 1536)
      expect(out.type).toBe('image/jpeg')
      expect(close).toHaveBeenCalledOnce()
    })
  })
})
