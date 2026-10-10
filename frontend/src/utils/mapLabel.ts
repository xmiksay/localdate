/**
 * Map tooltip / popup content for user- or admin-entered text. Leaflet puts a string into
 * `innerHTML`, so a name like `<img src=x onerror=…>` would run; an element whose text is set
 * through `textContent` is appended as is and never parsed as HTML.
 */
export function mapLabel(text: string): HTMLElement {
  const el = document.createElement('span')
  el.textContent = text
  return el
}
