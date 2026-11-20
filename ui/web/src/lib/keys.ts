// SPDX-License-Identifier: GPL-3.0-or-later
import type { Layer, Point, Tool } from './model'

export type Move = 'move_left' | 'move_right' | 'move_up' | 'move_down'
export type Action =
  | Tool
  | Move
  | 'undo'
  | 'redo'
  | 'delete'
  | 'save'
  | 'copy'
  | 'discard'
  | 'zoom_in'
  | 'zoom_out'
  | 'zoom_fit'
  | 'zoom_actual'
  | 'pan'
  | 'duplicate'
  | 'select_all'
  | Layer

export const MOVES: Record<Move, Point> = {
  move_left: { x: -1, y: 0 },
  move_right: { x: 1, y: 0 },
  move_up: { x: 0, y: -1 },
  move_down: { x: 0, y: 1 },
}

export const DEFAULT_KEYS: Record<Action, string> = {
  Select: 'v',
  Rectangle: 'r',
  Ellipse: 'e',
  Line: 'l',
  Arrow: 'a',
  Freehand: 'f',
  Text: 't',
  Step: 's',
  Highlight: 'h',
  Blur: 'b',
  Pixelate: 'p',
  Crop: 'c',
  undo: 'ctrl+z',
  redo: 'ctrl+shift+z, ctrl+y',
  delete: 'delete',
  save: 'ctrl+s',
  copy: 'ctrl+c',
  discard: 'escape',
  // A US layout sends + with Shift, a Spanish one without.
  zoom_in: 'ctrl++, ctrl+shift++, ctrl+=',
  zoom_out: 'ctrl+-',
  zoom_fit: 'ctrl+0',
  zoom_actual: 'ctrl+1',
  // Held down, the left button drags the view.
  pan: 'space',
  move_left: 'arrowleft, shift+arrowleft',
  move_right: 'arrowright, shift+arrowright',
  move_up: 'arrowup, shift+arrowup',
  move_down: 'arrowdown, shift+arrowdown',
  duplicate: 'ctrl+d',
  select_all: 'ctrl+a',
  // Page and Home/End like Inkscape, [ and ] need AltGr on many layouts.
  bring_forward: 'pageup',
  send_backward: 'pagedown',
  bring_to_front: 'home',
  send_to_back: 'end',
}

export function normalizeCombo(combo: string): string {
  const lower = combo.toLowerCase().trim()
  // The + key itself, as in ctrl++.
  const plus = lower === '+' || lower.endsWith('++')
  const parts = (plus ? lower.slice(0, -1) : lower).split('+').map((part) => part.trim())
  const last = parts.pop() ?? ''
  const key = plus ? '+' : last
  const order = ['ctrl', 'alt', 'shift'].filter((modifier) => parts.includes(modifier))
  return [...order, key].join('+')
}

export function eventCombo(event: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey'>): string {
  const parts: string[] = []
  if (event.ctrlKey || event.metaKey) parts.push('ctrl')
  if (event.altKey) parts.push('alt')
  if (event.shiftKey) parts.push('shift')
  parts.push(event.key === ' ' ? 'space' : event.key.toLowerCase())
  return parts.join('+')
}

/** Combination to action, with the user's keys over the defaults. Commas separate alternatives. */
export function keymap(custom: Record<string, string>): Map<string, Action> {
  const unknown = Object.keys(custom).find((name) => !isAction(name))
  if (unknown !== undefined) throw new Error(`[editor.keys] has an unknown action: ${unknown}`)
  const map = new Map<string, Action>()
  for (const action of Object.keys(DEFAULT_KEYS).filter(isAction)) {
    const combos = Object.hasOwn(custom, action) ? custom[action] : DEFAULT_KEYS[action]
    for (const combo of combos.split(',')) {
      if (combo.trim()) map.set(normalizeCombo(combo), action)
    }
  }
  return map
}

function isAction(name: string): name is Action {
  return Object.hasOwn(DEFAULT_KEYS, name)
}

export function shortcutFor(keys: Map<string, Action>, action: Action): string {
  const combo = [...keys].find(([, value]) => value === action)?.[0]
  return combo ? ` (${combo})` : ''
}
