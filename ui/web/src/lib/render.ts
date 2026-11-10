// SPDX-License-Identifier: GPL-3.0-or-later
import { bounds, clampRect, handles, rectBetween, stepNumber, type Annotation, type Doc, type Rect, type Shape, type Text } from './model'

export const LINE_HEIGHT = 1.25
const ARROW_HEAD_LENGTH = 5
const ARROW_HEAD_MIN = 12

export function imageRect(image: { width: number; height: number }): Rect {
  return { x: 0, y: 0, width: image.width, height: image.height }
}

/** What the canvas shows: the crop if there is one, otherwise the whole image. */
export function viewRect(doc: Doc, image: { width: number; height: number }): Rect {
  return doc.crop ?? imageRect(image)
}

export function fontOf(size: number, family: string) {
  return `${size}px ${family}`
}

export function measureText(ctx: CanvasRenderingContext2D, text: string, size: number, family: string) {
  ctx.save()
  ctx.font = fontOf(size, family)
  const lines = text.split('\n')
  const width = Math.max(...lines.map((line) => ctx.measureText(line).width))
  ctx.restore()
  return { width, height: lines.length * size * LINE_HEIGHT }
}

/** Dark text on light fills and the reverse, by perceived brightness. */
export function contrastingInk(color: string) {
  const value = parseInt(color.slice(1, 7), 16)
  const luminance = 0.299 * ((value >> 16) & 255) + 0.587 * ((value >> 8) & 255) + 0.114 * (value & 255)
  return luminance > 150 ? '#1b1916' : '#f4f1ea'
}

function drawArrow(ctx: CanvasRenderingContext2D, shape: Shape) {
  const { a, b, size } = shape
  const angle = Math.atan2(b.y - a.y, b.x - a.x)
  const length = Math.hypot(b.x - a.x, b.y - a.y)
  const head = Math.min(Math.max(size * ARROW_HEAD_LENGTH, ARROW_HEAD_MIN), length)
  const spread = Math.PI / 7
  // The shaft stops inside the head, or its round cap would poke through the tip.
  const stop = length - head * 0.7
  ctx.beginPath()
  ctx.moveTo(a.x, a.y)
  ctx.lineTo(a.x + Math.cos(angle) * stop, a.y + Math.sin(angle) * stop)
  ctx.stroke()
  ctx.beginPath()
  ctx.moveTo(b.x, b.y)
  ctx.lineTo(b.x - Math.cos(angle - spread) * head, b.y - Math.sin(angle - spread) * head)
  ctx.lineTo(b.x - Math.cos(angle + spread) * head, b.y - Math.sin(angle + spread) * head)
  ctx.closePath()
  ctx.fill()
}

function effectArea(ctx: CanvasRenderingContext2D, shape: Shape, view: Rect) {
  const rect = clampRect(rectBetween(shape.a, shape.b), view)
  const source = { x: rect.x - view.x, y: rect.y - view.y, width: rect.width, height: rect.height }
  return rect.width >= 1 && rect.height >= 1 ? { rect, source, canvas: ctx.canvas as HTMLCanvasElement } : null
}

function scratch(width: number, height: number) {
  const canvas = document.createElement('canvas')
  canvas.width = Math.max(1, Math.round(width))
  canvas.height = Math.max(1, Math.round(height))
  return { canvas, ctx: canvas.getContext('2d')! }
}

function drawPixelate(ctx: CanvasRenderingContext2D, shape: Shape, view: Rect) {
  const area = effectArea(ctx, shape, view)
  if (!area) return
  const { canvas: small, ctx: smallCtx } = scratch(area.rect.width / shape.size, area.rect.height / shape.size)
  smallCtx.drawImage(area.canvas, area.source.x, area.source.y, area.source.width, area.source.height, 0, 0, small.width, small.height)
  ctx.save()
  ctx.imageSmoothingEnabled = false
  ctx.drawImage(small, area.rect.x, area.rect.y, area.rect.width, area.rect.height)
  ctx.restore()
}

/**
 * Scales down in halves and back up the same way. WebKit has no canvas `filter`, so no blur() here.
 * Going past the strength matters: it has to erase detail, this is often hiding something.
 */
function drawBlur(ctx: CanvasRenderingContext2D, shape: Shape, view: Rect) {
  const area = effectArea(ctx, shape, view)
  if (!area) return
  const levels = [scratch(area.rect.width, area.rect.height)]
  levels[0].ctx.drawImage(area.canvas, area.source.x, area.source.y, area.source.width, area.source.height, 0, 0, levels[0].canvas.width, levels[0].canvas.height)
  for (let factor = 2; factor < shape.size * 2; factor *= 2) {
    const next = scratch(area.rect.width / factor, area.rect.height / factor)
    next.ctx.drawImage(levels[levels.length - 1].canvas, 0, 0, next.canvas.width, next.canvas.height)
    levels.push(next)
  }
  for (let i = levels.length - 2; i >= 0; i--) {
    const { canvas, ctx: target } = levels[i]
    target.clearRect(0, 0, canvas.width, canvas.height)
    target.drawImage(levels[i + 1].canvas, 0, 0, canvas.width, canvas.height)
  }
  ctx.drawImage(levels[0].canvas, area.rect.x, area.rect.y, area.rect.width, area.rect.height)
}

function drawText(ctx: CanvasRenderingContext2D, text: Text) {
  ctx.font = fontOf(text.size, text.font)
  // Middle of each line box, like CSS, so the text does not jump when the inline editor closes.
  ctx.textBaseline = 'middle'
  ctx.textAlign = 'left'
  text.text.split('\n').forEach((line, i) => {
    ctx.fillText(line, text.at.x, text.at.y + (i + 0.5) * text.size * LINE_HEIGHT)
  })
}

export function drawAnnotation(ctx: CanvasRenderingContext2D, annotation: Annotation, doc: Doc, view: Rect) {
  ctx.save()
  ctx.strokeStyle = annotation.color
  ctx.fillStyle = annotation.color
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'
  ctx.lineWidth = annotation.size
  switch (annotation.kind) {
    case 'Rectangle': {
      const r = rectBetween(annotation.a, annotation.b)
      ctx.strokeRect(r.x, r.y, r.width, r.height)
      break
    }
    case 'Ellipse': {
      const r = rectBetween(annotation.a, annotation.b)
      ctx.beginPath()
      ctx.ellipse(r.x + r.width / 2, r.y + r.height / 2, r.width / 2, r.height / 2, 0, 0, Math.PI * 2)
      ctx.stroke()
      break
    }
    case 'Line':
      ctx.beginPath()
      ctx.moveTo(annotation.a.x, annotation.a.y)
      ctx.lineTo(annotation.b.x, annotation.b.y)
      ctx.stroke()
      break
    case 'Arrow':
      drawArrow(ctx, annotation)
      break
    case 'Highlight': {
      const r = rectBetween(annotation.a, annotation.b)
      ctx.globalCompositeOperation = 'multiply'
      ctx.fillRect(r.x, r.y, r.width, r.height)
      break
    }
    case 'Blur':
      drawBlur(ctx, annotation, view)
      break
    case 'Pixelate':
      drawPixelate(ctx, annotation, view)
      break
    case 'Freehand':
      ctx.beginPath()
      annotation.points.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)))
      // A single click leaves one point, which only draws with a segment.
      if (annotation.points.length === 1) ctx.lineTo(annotation.points[0].x + 0.01, annotation.points[0].y)
      ctx.stroke()
      break
    case 'Text':
      drawText(ctx, annotation)
      break
    case 'Step': {
      const number = String(stepNumber(doc.annotations, annotation.id))
      ctx.beginPath()
      ctx.arc(annotation.at.x, annotation.at.y, annotation.size / 2, 0, Math.PI * 2)
      ctx.fill()
      ctx.fillStyle = contrastingInk(annotation.color)
      ctx.font = `600 ${annotation.size * 0.56}px sans-serif`
      ctx.textAlign = 'center'
      ctx.textBaseline = 'alphabetic'
      // Centered on the digit's real box, textBaseline middle sits visibly high.
      const metrics = ctx.measureText(number)
      const offset = (metrics.actualBoundingBoxAscent - metrics.actualBoundingBoxDescent) / 2
      ctx.fillText(number, annotation.at.x, annotation.at.y + offset)
      break
    }
  }
  ctx.restore()
}

export function renderDoc(
  ctx: CanvasRenderingContext2D,
  doc: Doc,
  image: CanvasImageSource,
  size: { width: number; height: number },
  draft: Annotation | null = null,
) {
  const view = viewRect(doc, size)
  ctx.setTransform(1, 0, 0, 1, 0, 0)
  ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height)
  ctx.translate(-view.x, -view.y)
  ctx.drawImage(image, 0, 0)
  const draftDoc = draft ? { ...doc, annotations: [...doc.annotations, draft] } : doc
  for (const annotation of draftDoc.annotations) drawAnnotation(ctx, annotation, draftDoc, view)
}

/**
 * Selection marks, in image pixels but sized for the current zoom. Not part of the export.
 * Part of a multiple selection, shapes get a box instead of their handles.
 */
export function renderSelection(ctx: CanvasRenderingContext2D, annotation: Annotation, scale: number, withHandles = true) {
  const pixel = 1 / scale
  ctx.save()
  if (!withHandles || annotation.kind === 'Text' || annotation.kind === 'Step' || annotation.kind === 'Freehand') {
    const r =
      annotation.kind === 'Freehand'
        ? freehandBox(annotation)
        : annotation.kind === 'Text' || annotation.kind === 'Step'
          ? box(annotation)
          : padded(bounds(annotation), STROKED.includes(annotation.kind) ? annotation.size / 2 + 4 : 4)
    dashedRect(ctx, r, pixel)
  }
  for (const handle of withHandles ? handles(annotation) : []) {
    const half = 5 * pixel
    ctx.fillStyle = '#f4f1ea'
    ctx.strokeStyle = '#1b1916'
    ctx.lineWidth = pixel
    ctx.fillRect(handle.at.x - half, handle.at.y - half, half * 2, half * 2)
    ctx.strokeRect(handle.at.x - half, handle.at.y - half, half * 2, half * 2)
  }
  ctx.restore()
}

/** Light dashes over a dark line, so the box shows on any image. */
export function dashedRect(ctx: CanvasRenderingContext2D, r: Rect, pixel: number) {
  ctx.save()
  ctx.lineWidth = pixel
  ctx.strokeStyle = '#1b1916'
  ctx.strokeRect(r.x, r.y, r.width, r.height)
  ctx.setLineDash([4 * pixel, 3 * pixel])
  ctx.strokeStyle = '#f4f1ea'
  ctx.strokeRect(r.x, r.y, r.width, r.height)
  ctx.restore()
}

// For Blur and Pixelate the size is a strength, not a line around the box.
const STROKED: readonly string[] = ['Rectangle', 'Ellipse', 'Line', 'Arrow']

function padded(r: Rect, margin: number): Rect {
  return { x: r.x - margin, y: r.y - margin, width: r.width + margin * 2, height: r.height + margin * 2 }
}

function box(annotation: Text | Extract<Annotation, { kind: 'Step' }>): Rect {
  if (annotation.kind === 'Text') {
    return { x: annotation.at.x - 4, y: annotation.at.y - 4, width: annotation.width + 8, height: annotation.height + 8 }
  }
  const half = annotation.size / 2 + 4
  return { x: annotation.at.x - half, y: annotation.at.y - half, width: half * 2, height: half * 2 }
}

function freehandBox(annotation: Extract<Annotation, { kind: 'Freehand' }>): Rect {
  const xs = annotation.points.map((p) => p.x)
  const ys = annotation.points.map((p) => p.y)
  const margin = annotation.size / 2 + 4
  const x = Math.min(...xs) - margin
  const y = Math.min(...ys) - margin
  return { x, y, width: Math.max(...xs) - x + margin, height: Math.max(...ys) - y + margin }
}

export async function exportPng(doc: Doc, image: CanvasImageSource, size: { width: number; height: number }) {
  const view = viewRect(doc, size)
  const canvas = document.createElement('canvas')
  canvas.width = Math.round(view.width)
  canvas.height = Math.round(view.height)
  renderDoc(canvas.getContext('2d')!, doc, image, size)
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'))
  if (!blob) throw new Error('could not encode the image')
  return new Uint8Array(await blob.arrayBuffer())
}
