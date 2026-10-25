// Embeddings, overrides, isolates, directional marks and paragraph separators (which close every
// open isolate): what lets a name reorder text around it.
// eslint-disable-next-line no-control-regex -- U+001C–U+001E are bidi paragraph separators
const BIDI_CONTROLS = /[\u061C\u200E\u200F\u202A-\u202E\u2066-\u2069\n\r\u001C-\u001E\u0085\u2029]/g

export const withoutBidiControls = (s: string) => s.replace(BIDI_CONTROLS, '')

/**
 * User-chosen text (a username) for a translated sentence: its own bidi controls removed — a
 * stray PDI would otherwise close the isolate early — then wrapped in FSI … PDI, so a
 * right-to-left name cannot reorder the rest of the sentence either.
 */
export const isolate = (s: string) => `\u2068${withoutBidiControls(s)}\u2069`
