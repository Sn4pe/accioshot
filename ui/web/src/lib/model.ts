// SPDX-License-Identifier: GPL-3.0-or-later
// Tool names are ShareX's EditorTool.

export type Point = { x: number; y: number }
export type Rect = { x: number; y: number; width: number; height: number }

export type ShapeKind = 'Rectangle' | 'Ellipse' | 'Line' | 'Arrow' | 'Highlight' | 'Blur' | 'Pixelate'
export type Tool = 'Select' | ShapeKind | 'Freehand' | 'Text' | 'Step' | 'Crop'

export type Shape = { id: number; kind: ShapeKind; a: Point; b: Point; color: string; size: number }
export type Freehand = { id: number; kind: 'Freehand'; points: Point[]; color: string; size: number }
export type Text = {
  id: number
  kind: 'Text'
  at: Point
  text: string
  color: string
  size: number
  font: string
  width: number
  height: number
}
export type Step = { id: number; kind: 'Step'; at: Point; color: string; size: number }
export type Annotation = Shape | Freehand | Text | Step

export type Doc = { annotations: Annotation[]; crop: Rect | null }
export const EMPTY_DOC: Doc = { annotations: [], crop: null }

export const SHAPE_KINDS: readonly string[] = [
  'Rectangle',
  'Ellipse',
  'Line',
  'Arrow',
  'Highlight',
  'Blur',
  'Pixelate',
]

export function isShape(annotation: Annotation): annotation is Shape {
  return SHAPE_KINDS.includes(annotation.kind)
}

function isLineLike(annotation: Annotation): boolean {
  return annotation.kind === 'Line' || annotation.kind === 'Arrow'
}

export function rectBetween(a: Point, b: Point): Rect {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(a.x - b.x),
    height: Math.abs(a.y - b.y),
  }
}

function distanceToSegment(p: Point, a: Point, b: Point): number {
  const dx = b.x - a.x
  const dy = b.y - a.y
  const length = dx * dx + dy * dy
  const t = length === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / length))
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy))
}

function inRect(p: Point, rect: Rect, tolerance = 0): boolean {
  return (
    p.x >= rect.x - tolerance &&
    p.x <= rect.x + rect.width + tolerance &&
    p.y >= rect.y - tolerance &&
    p.y <= rect.y + rect.height + tolerance
  )
}

export function bounds(annotation: Annotation): Rect {
  switch (annotation.kind) {
    case 'Freehand': {
      const xs = annotation.points.map((p) => p.x)
      const ys = annotation.points.map((p) => p.y)
      const x = Math.min(...xs)
      const y = Math.min(...ys)
      return { x, y, width: Math.max(...xs) - x, height: Math.max(...ys) - y }
    }
    case 'Text':
      return { x: annotation.at.x, y: annotation.at.y, width: annotation.width, height: annotation.height }
    case 'Step':
      return {
        x: annotation.at.x - annotation.size / 2,
        y: annotation.at.y - annotation.size / 2,
        width: annotation.size,
        height: annotation.size,
      }
    default:
      return rectBetween(annotation.a, annotation.b)
  }
}

export function hit(annotation: Annotation, p: Point, tolerance: number): boolean {
  switch (annotation.kind) {
    case 'Freehand':
      return annotation.points.some(
        (point, i) =>
          distanceToSegment(p, point, annotation.points[Math.max(0, i - 1)]) <= annotation.size / 2 + tolerance,
      )
    case 'Line':
    case 'Arrow':
      return distanceToSegment(p, annotation.a, annotation.b) <= annotation.size / 2 + tolerance
    case 'Ellipse': {
      const r = bounds(annotation)
      const rx = r.width / 2 + tolerance
      const ry = r.height / 2 + tolerance
      if (rx <= 0 || ry <= 0) return false
      return ((p.x - r.x - r.width / 2) / rx) ** 2 + ((p.y - r.y - r.height / 2) / ry) ** 2 <= 1
    }
    case 'Step':
      return Math.hypot(p.x - annotation.at.x, p.y - annotation.at.y) <= annotation.size / 2 + tolerance
    default:
      return inRect(p, bounds(annotation), tolerance)
  }
}

/** Topmost first, as drawn last is on top. */
export function hitTop(annotations: Annotation[], p: Point, tolerance: number): Annotation | null {
  for (let i = annotations.length - 1; i >= 0; i--) {
    if (hit(annotations[i], p, tolerance)) return annotations[i]
  }
  return null
}

export function translated<T extends Annotation>(annotation: T, dx: number, dy: number): T {
  const move = (p: Point) => ({ x: p.x + dx, y: p.y + dy })
  if (annotation.kind === 'Freehand') return { ...annotation, points: annotation.points.map(move) }
  if (annotation.kind === 'Text' || annotation.kind === 'Step') return { ...annotation, at: move(annotation.at) }
  const shape = annotation as Shape
  return { ...shape, a: move(shape.a), b: move(shape.b) } as T
}

export type HandleId = 'a' | 'b' | 'ab' | 'ba'
export type Handle = { id: HandleId; at: Point }

/** Line-like shapes resize by their ends, the rest by their corners. */
export function handles(annotation: Annotation): Handle[] {
  if (!isShape(annotation)) return []
  const { a, b } = annotation
  const ends: Handle[] = [
    { id: 'a', at: a },
    { id: 'b', at: b },
  ]
  if (isLineLike(annotation)) return ends
  return [...ends, { id: 'ab', at: { x: a.x, y: b.y } }, { id: 'ba', at: { x: b.x, y: a.y } }]
}

export function hitHandle(annotation: Annotation, p: Point, tolerance: number): HandleId | null {
  const handle = handles(annotation).find((h) => Math.hypot(p.x - h.at.x, p.y - h.at.y) <= tolerance)
  return handle?.id ?? null
}

export function resized(shape: Shape, handle: HandleId, p: Point): Shape {
  switch (handle) {
    case 'a':
      return { ...shape, a: p }
    case 'b':
      return { ...shape, b: p }
    case 'ab':
      return { ...shape, a: { x: p.x, y: shape.a.y }, b: { x: shape.b.x, y: p.y } }
    case 'ba':
      return { ...shape, a: { x: shape.a.x, y: p.y }, b: { x: p.x, y: shape.b.y } }
  }
}

/** With Shift: squares for box shapes, 45 degree steps for lines. */
export function constrained(kind: Tool, a: Point, b: Point): Point {
  const dx = b.x - a.x
  const dy = b.y - a.y
  if (kind === 'Line' || kind === 'Arrow') {
    const angle = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4)
    const length = Math.hypot(dx, dy)
    return { x: a.x + Math.cos(angle) * length, y: a.y + Math.sin(angle) * length }
  }
  const side = Math.max(Math.abs(dx), Math.abs(dy))
  return { x: a.x + Math.sign(dx || 1) * side, y: a.y + Math.sign(dy || 1) * side }
}

/** The position of a step among all steps, so deleting one renumbers the rest. */
export function stepNumber(annotations: Annotation[], id: number): number {
  return annotations.filter((a) => a.kind === 'Step').findIndex((a) => a.id === id) + 1
}

export function clampRect(rect: Rect, limit: Rect): Rect {
  const x = Math.max(rect.x, limit.x)
  const y = Math.max(rect.y, limit.y)
  return {
    x,
    y,
    width: Math.max(0, Math.min(rect.x + rect.width, limit.x + limit.width) - x),
    height: Math.max(0, Math.min(rect.y + rect.height, limit.y + limit.height) - y),
  }
}

export class History {
  private past: Doc[] = []
  private future: Doc[] = []

  constructor(public doc: Doc = EMPTY_DOC) {}

  get canUndo() {
    return this.past.length > 0
  }
  get canRedo() {
    return this.future.length > 0
  }

  commit(doc: Doc) {
    this.past.push(this.doc)
    this.future = []
    this.doc = doc
  }

  undo() {
    const previous = this.past.pop()
    if (!previous) return
    this.future.push(this.doc)
    this.doc = previous
  }

  redo() {
    const next = this.future.pop()
    if (!next) return
    this.past.push(this.doc)
    this.doc = next
  }
}

export type Action = Tool | 'undo' | 'redo' | 'delete' | 'save' | 'copy' | 'discard'

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
}

export function normalizeCombo(combo: string): string {
  const parts = combo.toLowerCase().split('+').map((part) => part.trim())
  const key = parts.pop() ?? ''
  const order = ['ctrl', 'alt', 'shift'].filter((modifier) => parts.includes(modifier))
  return [...order, key].join('+')
}

export function eventCombo(event: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey'>): string {
  const parts: string[] = []
  if (event.ctrlKey || event.metaKey) parts.push('ctrl')
  if (event.altKey) parts.push('alt')
  if (event.shiftKey) parts.push('shift')
  parts.push(event.key.toLowerCase())
  return parts.join('+')
}

/** Combination to action, with the user's keys over the defaults. Commas separate alternatives. */
export function keymap(custom: Record<string, string>): Map<string, Action> {
  const merged = { ...DEFAULT_KEYS, ...custom } as Record<string, string>
  const map = new Map<string, Action>()
  for (const [action, combos] of Object.entries(merged)) {
    if (!(action in DEFAULT_KEYS)) throw new Error(`[editor.keys] has an unknown action: ${action}`)
    for (const combo of combos.split(',')) {
      if (combo.trim()) map.set(normalizeCombo(combo), action as Action)
    }
  }
  return map
}
