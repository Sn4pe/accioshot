// SPDX-License-Identifier: GPL-3.0-or-later
// The part of the image on screen: `origin` is the image point at the stage's top-left corner.
import type { Point, Rect } from './model'

export type Size = { width: number; height: number }

/** The whole view fits with a margin, and is never scaled up. */
export function fitScale(view: Rect, stage: Size, margin: number): number {
  return Math.min(1, (stage.width - margin * 2) / view.width, (stage.height - margin * 2) / view.height) || 1
}

export function centered(view: Rect, stage: Size, scale: number): Point {
  return {
    x: view.x + view.width / 2 - stage.width / scale / 2,
    y: view.y + view.height / 2 - stage.height / scale / 2,
  }
}

function clampAxis(origin: number, start: number, length: number, visible: number, margin: number) {
  if (length + margin * 2 <= visible) return start + length / 2 - visible / 2
  return Math.min(Math.max(origin, start - margin), start + length + margin - visible)
}

/** Centered on an axis the view fits in, otherwise kept from scrolling past its edges. */
export function clampOrigin(origin: Point, view: Rect, stage: Size, scale: number, margin: number): Point {
  return {
    x: clampAxis(origin.x, view.x, view.width, stage.width / scale, margin / scale),
    y: clampAxis(origin.y, view.y, view.height, stage.height / scale, margin / scale),
  }
}

/** The origin that keeps the image point under `screen` in place when the scale changes. */
export function zoomAt(origin: Point, scale: number, next: number, screen: Point): Point {
  return {
    x: origin.x + screen.x / scale - screen.x / next,
    y: origin.y + screen.y / scale - screen.y / next,
  }
}

/** One zoom step, landing on 100 % when it would jump over it. */
export function stepScale(scale: number, factor: number): number {
  const next = scale * factor
  return (scale < 1 && next > 1) || (scale > 1 && next < 1) ? 1 : next
}

const FIT_TOLERANCE = 0.001
const WHEEL_STEP = 100
const WHEEL_LINE = 33
const DOM_DELTA_LINE = 1

export function nextZoom(zoom: number | null, next: number | null, fit: number, min: number, max: number) {
  if (next === null) return null
  const limited = Math.min(Math.max(next, min), max)
  const crossed = (limited - fit) * ((zoom ?? fit) - fit) < 0
  return crossed || Math.abs(limited - fit) <= fit * FIT_TOLERANCE ? null : limited
}

type Wheel = { deltaMode: number; deltaX: number; deltaY: number; shiftKey: boolean }

function lineHeight(event: Pick<Wheel, 'deltaMode'>) {
  return event.deltaMode === DOM_DELTA_LINE ? WHEEL_LINE : 1
}

export function wheelDelta(event: Wheel): Point {
  const line = lineHeight(event)
  const sideways = event.shiftKey && !event.deltaX
  return { x: (sideways ? event.deltaY : event.deltaX) * line, y: (sideways ? 0 : event.deltaY) * line }
}

export function wheelZoom(event: Pick<Wheel, 'deltaMode' | 'deltaY'>, step: number): number {
  return step ** ((-event.deltaY * lineHeight(event)) / WHEEL_STEP)
}

export function offsetIn(element: Element, event: { clientX: number; clientY: number }): Point {
  const rect = element.getBoundingClientRect()
  return { x: event.clientX - rect.left, y: event.clientY - rect.top }
}
