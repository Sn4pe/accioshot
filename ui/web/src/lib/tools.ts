// SPDX-License-Identifier: GPL-3.0-or-later
import { isShape, type Annotation, type Tool } from './model'

export type Style = { color: string; size: number }
export type SavedState = { tool?: unknown; styles?: Partial<Record<Tool, Style>> }
export type EditorSettings = {
  tool: string
  colors: string[]
  highlight_color: string
  stroke_width: number
  font_family: string
  font_size: number
  step_size: number
  blur_strength: number
  pixel_size: number
  remember_last: boolean
  initial_zoom: number | 'fit'
  zoom_step: number
  max_zoom: number
  keys: Record<string, string>
}
type Measure = (text: string, size: number, family: string) => { width: number; height: number }

export const TOOLS: readonly Tool[] = [
  'Select',
  'Rectangle',
  'Ellipse',
  'Line',
  'Arrow',
  'Freehand',
  'Text',
  'Step',
  'Highlight',
  'Blur',
  'Pixelate',
  'Crop',
]
export const SIZE_RANGES: Partial<Record<Tool, [number, number]>> = {
  Rectangle: [1, 40],
  Ellipse: [1, 40],
  Line: [1, 40],
  Arrow: [1, 40],
  Freehand: [1, 40],
  Text: [10, 160],
  Step: [16, 120],
  Blur: [2, 64],
  Pixelate: [2, 64],
}
export const COLORED: readonly Tool[] = [
  'Rectangle',
  'Ellipse',
  'Line',
  'Arrow',
  'Freehand',
  'Text',
  'Step',
  'Highlight',
]

export function isTool(value: unknown): value is Tool {
  return TOOLS.some((tool) => tool === value)
}

export function cursorFor(tool: Tool): string {
  if (tool === 'Select') return 'default'
  return tool === 'Text' ? 'text' : 'crosshair'
}

export function defaultStyle(settings: EditorSettings, tool: Tool): Style {
  switch (tool) {
    case 'Highlight':
      return { color: settings.highlight_color, size: 0 }
    case 'Text':
      return { color: settings.colors[0], size: settings.font_size }
    case 'Step':
      return { color: settings.colors[0], size: settings.step_size }
    case 'Blur':
      return { color: '', size: settings.blur_strength }
    case 'Pixelate':
      return { color: '', size: settings.pixel_size }
    default:
      return { color: settings.colors[0], size: settings.stroke_width }
  }
}

export function longEnough(annotation: Annotation, minLength: number): boolean {
  if (annotation.kind === 'Freehand') return true
  if (!isShape(annotation)) return false
  return Math.hypot(annotation.a.x - annotation.b.x, annotation.a.y - annotation.b.y) >= minLength
}

export function restyled(annotation: Annotation, change: Partial<Style>, target: Tool, measure: Measure): Annotation {
  const color = change.color !== undefined && COLORED.includes(annotation.kind) ? change.color : annotation.color
  const size = change.size !== undefined && annotation.kind === target ? change.size : annotation.size
  if (annotation.kind !== 'Text') return { ...annotation, color, size }
  return { ...annotation, color, size, ...measure(annotation.text, size, annotation.font) }
}
