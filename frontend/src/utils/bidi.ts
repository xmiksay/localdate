/**
 * Wraps user-chosen text (a username) in FSI … PDI before it goes into a translated sentence: a
 * name may contain bidi controls (U+202E …), and unisolated they would reorder the rest of it.
 */
export const isolate = (s: string) => `⁨${s}⁩`
