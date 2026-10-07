import { describe, expect, it } from 'vitest'
import { haversineMeters, shouldSendLocation } from './geo'

const prague = { lat: 50.0755, lon: 14.4378 }
// ~0.001 deg latitude is ~111 m
const north = (m: number) => ({ lat: prague.lat + m / 111_195, lon: prague.lon })

describe('haversineMeters', () => {
  it('is zero for identical points', () => {
    expect(haversineMeters(prague, prague)).toBe(0)
  })
  it('matches a known distance (Prague to Brno ~184 km)', () => {
    const km = haversineMeters(prague, { lat: 49.1951, lon: 16.6068 }) / 1000
    expect(km).toBeGreaterThan(182)
    expect(km).toBeLessThan(186)
  })
  it('is symmetric', () => {
    const b = { lat: 48.2, lon: 16.37 }
    expect(haversineMeters(prague, b)).toBeCloseTo(haversineMeters(b, prague), 6)
  })
})

describe('shouldSendLocation', () => {
  const t0 = 1_000_000
  it('sends the first fix', () => {
    expect(shouldSendLocation(null, prague, null, t0)).toBe(true)
  })
  it('holds back small moves inside the interval', () => {
    expect(shouldSendLocation(prague, north(50), t0, t0 + 10_000)).toBe(false)
  })
  it('sends immediately after moving over 100 m', () => {
    expect(shouldSendLocation(prague, north(150), t0, t0 + 5_000)).toBe(true)
  })
  it('sends after 2 minutes even when standing still', () => {
    expect(shouldSendLocation(prague, prague, t0, t0 + 120_000)).toBe(true)
    expect(shouldSendLocation(prague, prague, t0, t0 + 119_999)).toBe(false)
  })
})
