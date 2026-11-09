// SPDX-License-Identifier: GPL-3.0-or-later
// The part of the image on screen: `origin` is the image point at the stage's top-left corner.
import type { Point, Rect } from './model'

export type Size = { width: number; height: number }

/** The whole view fits with a margin, and is never scaled up. */
export function fitScale(view: Rect, stage: Size, margin: number): number {
  return Math.min(1, (stage.width - margin * 2) / view.width, (stage.height - margin * 2) / view.height) || 1
}

export function centered(view: Rect, stage: Size, scale: number): Point {
  return { x: view.x + view.width / 2 - stage.width / scale / 2, y: view.y + view.height / 2 - stage.height / scale / 2 }
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
