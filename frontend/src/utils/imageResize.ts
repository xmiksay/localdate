import { MAX_PHOTO_BYTES, PHOTO_TYPES } from './validation'

/** Long edge the browser downscales to before upload; the server makes its own 1280 px copy. */
export const UPLOAD_MAX_EDGE = 2048
const JPEG_QUALITY = 0.9

export interface Size {
  width: number
  height: number
}

/** Output size with the long edge capped at `maxEdge`, aspect kept, never upscaled. */
export function fitDimensions(width: number, height: number, maxEdge: number): Size {
  const long = Math.max(width, height)
  if (long <= maxEdge) return { width, height }
  const scale = (side: number) => Math.max(1, Math.round((side * maxEdge) / long))
  return { width: scale(width), height: scale(height) }
}

export interface DecodedImage extends Size {
  close(): void
}

export interface ImageCodec {
  /** Oriented dimensions without keeping a full-resolution bitmap around. */
  probe(file: File): Promise<Size>
  /** EXIF-oriented bitmap, scaled to `size` while decoding when given. */
  decode(file: File, size?: Size): Promise<DecodedImage>
  encodeJpeg(img: DecodedImage, width: number, height: number): Promise<Blob>
}

const browserCodec: ImageCodec = {
  async probe(file) {
    const url = URL.createObjectURL(file)
    try {
      const el = new Image()
      el.src = url
      await el.decode()
      return { width: el.naturalWidth, height: el.naturalHeight }
    } finally {
      URL.revokeObjectURL(url)
    }
  },
  decode: (file, size) =>
    createImageBitmap(file, {
      imageOrientation: 'from-image',
      ...(size && { resizeWidth: size.width, resizeHeight: size.height, resizeQuality: 'high' }),
    }),
  async encodeJpeg(img, width, height) {
    const canvas: OffscreenCanvas | HTMLCanvasElement =
      typeof OffscreenCanvas === 'undefined'
        ? Object.assign(document.createElement('canvas'), { width, height })
        : new OffscreenCanvas(width, height)
    const ctx = canvas.getContext('2d') as
      OffscreenCanvasRenderingContext2D | CanvasRenderingContext2D | null
    if (!ctx) throw new Error('2d canvas unavailable')
    // JPEG has no alpha: transparent PNG areas would otherwise turn black.
    ctx.fillStyle = '#fff'
    ctx.fillRect(0, 0, width, height)
    ctx.imageSmoothingEnabled = true
    ctx.imageSmoothingQuality = 'high'
    ctx.drawImage(img as ImageBitmap, 0, 0, width, height)
    const type = 'image/jpeg'
    if ('convertToBlob' in canvas) return canvas.convertToBlob({ type, quality: JPEG_QUALITY })
    return new Promise((resolve, reject) =>
      canvas.toBlob(
        (b) => (b ? resolve(b) : reject(new Error('JPEG encoding failed'))),
        type,
        JPEG_QUALITY,
      ),
    )
  },
}

const near = (a: number, b: number) => Math.abs(a - b) <= 2

/**
 * Decodes already scaled to `target`, or at full size when the browser's resized bitmap does not come
 * out as asked: a browser that resizes before applying EXIF orientation would squash rotated photos.
 */
async function decodeAt(file: File, target: Size, codec: ImageCodec): Promise<DecodedImage> {
  const img = await codec.decode(file, target)
  if (near(img.width, target.width) && near(img.height, target.height)) return img
  img.close()
  return codec.decode(file)
}

const jpegName = (name: string) => `${name.replace(/\.[^./]*$/, '') || 'photo'}.jpg`

/**
 * Turns a picked photo into something the server accepts: an accepted type (JPEG/PNG/WebP) within
 * 2048 px and the byte cap goes up untouched (the server applies EXIF orientation itself); anything
 * else the browser can decode (oversized shots, HEIC on Safari, AVIF, …) is re-encoded as a 2048 px
 * JPEG, transparency flattened onto white. An undecodable file is returned as is, so the caller's type
 * check rejects it (or the server does). A JPEG still over the cap is returned anyway; the server decides.
 */
export async function prepareUpload(
  file: File,
  codec: ImageCodec = browserCodec,
  maxBytes: number = MAX_PHOTO_BYTES,
): Promise<File> {
  const passThrough = (s: Size) =>
    PHOTO_TYPES.includes(file.type) &&
    Math.max(s.width, s.height) <= UPLOAD_MAX_EDGE &&
    file.size <= maxBytes
  const probed = await codec.probe(file).catch(() => null)
  if (probed && passThrough(probed)) return file

  let img: DecodedImage
  try {
    img = probed
      ? await decodeAt(file, fitDimensions(probed.width, probed.height, UPLOAD_MAX_EDGE), codec)
      : await codec.decode(file)
  } catch {
    return file
  }
  try {
    if (!probed && passThrough(img)) return file
    const { width, height } = fitDimensions(img.width, img.height, UPLOAD_MAX_EDGE)
    const blob = await codec.encodeJpeg(img, width, height)
    return new File([blob], jpegName(file.name), { type: 'image/jpeg' })
  } catch {
    return file
  } finally {
    img.close()
  }
}
