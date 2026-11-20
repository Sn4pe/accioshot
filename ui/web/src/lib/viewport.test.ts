// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest'
import { centered, clampOrigin, fitScale, nextZoom, stepScale, wheelDelta, wheelZoom, zoomAt } from './viewport'

const view = { x: 0, y: 0, width: 1000, height: 500 }
const stage = { width: 548, height: 548 }

describe('viewport', () => {
  it('fits with the margin and never scales up', () => {
    expect(fitScale(view, stage, 24)).toBe(0.5)
    expect(fitScale(view, { width: 4000, height: 4000 }, 24)).toBe(1)
  })

  it('centers an axis that fits', () => {
    const origin = clampOrigin({ x: 0, y: 900 }, view, stage, 0.5, 24)
    expect(origin.y).toBe(centered(view, stage, 0.5).y)
  })

  it('keeps a zoomed view inside its edges plus the margin', () => {
    expect(clampOrigin({ x: -500, y: 0 }, view, stage, 2, 24).x).toBe(-12)
    expect(clampOrigin({ x: 5000, y: 0 }, view, stage, 2, 24).x).toBe(1000 + 12 - 274)
  })

  it('zooms around the point under the cursor', () => {
    const origin = { x: 100, y: 100 }
    const screen = { x: 200, y: 50 }
    const next = zoomAt(origin, 1, 4, screen)
    expect(next.x + screen.x / 4).toBe(origin.x + screen.x)
    expect(next.y + screen.y / 4).toBe(origin.y + screen.y)
  })

  it('lands on 100 % when a step jumps over it', () => {
    expect(stepScale(0.9, 1.25)).toBe(1)
    expect(stepScale(1.1, 0.8)).toBe(1)
    expect(stepScale(2, 1.25)).toBe(2.5)
  })

  it('goes back to fitting when a zoom reaches or crosses the fit', () => {
    expect(nextZoom(2, 0.4, 0.5, 0.05, 8)).toBeNull()
    expect(nextZoom(0.25, 0.6, 0.5, 0.05, 8)).toBeNull()
    expect(nextZoom(2, 0.5004, 0.5, 0.05, 8)).toBeNull()
    expect(nextZoom(2, null, 0.5, 0.05, 8)).toBeNull()
  })

  it('zooms within its limits and away from the fit', () => {
    expect(nextZoom(null, 2, 0.5, 0.05, 8)).toBe(2)
    expect(nextZoom(null, 0.25, 0.5, 0.05, 8)).toBe(0.25)
    expect(nextZoom(null, 0.25, 0.5, 0.5, 8)).toBeNull()
    expect(nextZoom(2, 40, 0.5, 0.05, 8)).toBe(8)
  })

  it('turns a wheel into a pan in pixels', () => {
    const wheel = { deltaMode: 0, deltaX: 0, deltaY: 10, shiftKey: false }
    expect(wheelDelta(wheel)).toEqual({ x: 0, y: 10 })
    expect(wheelDelta({ ...wheel, shiftKey: true })).toEqual({ x: 10, y: 0 })
    expect(wheelDelta({ ...wheel, deltaMode: 1, deltaY: 3 })).toEqual({ x: 0, y: 99 })
  })

  it('zooms one step per mouse notch', () => {
    expect(wheelZoom({ deltaMode: 0, deltaY: -100 }, 1.25)).toBe(1.25)
    expect(wheelZoom({ deltaMode: 1, deltaY: 3 }, 2)).toBeCloseTo(2 ** -0.99)
  })
})
