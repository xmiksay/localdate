import { describe, expect, it } from 'vitest'
import L from 'leaflet'
import { mapLabel } from './mapLabel'

const EVIL = '<img src=x onerror=alert(1)>'

describe('mapLabel', () => {
  it('keeps markup as plain text', () => {
    const el = mapLabel(EVIL)
    expect(el.textContent).toBe(EVIL)
    expect(el.querySelector('img')).toBeNull()
  })

  it('renders as text inside a leaflet tooltip on the map', () => {
    const container = document.createElement('div')
    document.body.appendChild(container)
    const map = L.map(container).setView([50.08, 14.42], 13)
    const circle = L.circle([50.08, 14.42], { radius: 100 })
      .bindTooltip(mapLabel(EVIL), { permanent: true })
      .addTo(map)
    const tooltip = circle.getTooltip()?.getElement()
    expect(tooltip?.textContent).toBe(EVIL)
    expect(tooltip?.querySelector('img')).toBeNull()
    expect(container.querySelector('img[src="x"]')).toBeNull()
    map.remove()
    container.remove()
  })
})
