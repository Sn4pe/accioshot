// SPDX-License-Identifier: GPL-3.0-or-later
import type { Tool } from './model'

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

export function cursorFor(tool: Tool): string {
  if (tool === 'Select') return 'default'
  return tool === 'Text' ? 'text' : 'crosshair'
}
