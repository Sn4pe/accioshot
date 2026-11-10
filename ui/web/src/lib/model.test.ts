// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest'
import {
  bounds,
  clampRect,
  constrained,
  eventCombo,
  handles,
  hit,
  hitHandle,
  hitTop,
  History,
  inside,
  keymap,
  normalizeCombo,
  rectBetween,
  reordered,
  resized,
  stepNumber,
  translated,
  type Annotation,
  type Shape,
} from './model'

const shape = (kind: Shape['kind'], id = 1): Shape => ({
  id,
  kind,
  a: { x: 10, y: 10 },
  b: { x: 110, y: 60 },
  color: '#000',
  size: 4,
})
const step = (id: number): Annotation => ({ id, kind: 'Step', at: { x: 50, y: 50 }, color: '#000', size: 20 })

describe('geometry', () => {
  it('normalizes a drag in any direction', () => {
    expect(rectBetween({ x: 30, y: 40 }, { x: 10, y: 5 })).toEqual({ x: 10, y: 5, width: 20, height: 35 })
  })

  it('hits a line near it, not far from it', () => {
    expect(hit(shape('Line'), { x: 60, y: 36 }, 4)).toBe(true)
    expect(hit(shape('Line'), { x: 60, y: 0 }, 4)).toBe(false)
  })

  it('hits an ellipse inside but not in its corners', () => {
    expect(hit(shape('Ellipse'), { x: 60, y: 35 }, 0)).toBe(true)
    expect(hit(shape('Ellipse'), { x: 11, y: 11 }, 0)).toBe(false)
  })

  it('finds the topmost annotation', () => {
    const below = shape('Rectangle', 1)
    const above = shape('Rectangle', 2)
    expect(hitTop([below, above], { x: 50, y: 30 }, 0)?.id).toBe(2)
    expect(hitTop([below, above], { x: 500, y: 30 }, 0)).toBeNull()
  })

  it('moves freehand and text', () => {
    const line: Annotation = { id: 1, kind: 'Freehand', points: [{ x: 0, y: 0 }], color: '', size: 1 }
    expect(translated(line, 2, 3)).toMatchObject({ points: [{ x: 2, y: 3 }] })
    expect(bounds(translated(step(1), 10, 0)).x).toBe(50)
  })

  it('squares and snaps with shift', () => {
    expect(constrained('Rectangle', { x: 0, y: 0 }, { x: 10, y: -4 })).toEqual({ x: 10, y: -10 })
    const end = constrained('Line', { x: 0, y: 0 }, { x: 10, y: 1 })
    expect(end.y).toBeCloseTo(0)
  })

  it('clamps a crop to the image', () => {
    const image = { x: 0, y: 0, width: 100, height: 50 }
    expect(clampRect({ x: -10, y: 40, width: 50, height: 50 }, image)).toEqual({ x: 0, y: 40, width: 40, height: 10 })
  })
})

describe('handles', () => {
  it('gives lines two and boxes four', () => {
    expect(handles(shape('Arrow'))).toHaveLength(2)
    expect(handles(shape('Blur'))).toHaveLength(4)
    expect(handles(step(1))).toHaveLength(4)
  })

  it('resizes from an opposite corner', () => {
    const box = shape('Rectangle')
    expect(hitHandle(box, { x: 110, y: 11 }, 3)).toBe('ba')
    expect(resized(box, 'ba', { x: 200, y: 0 }, [1, 40])).toMatchObject({ a: { x: 10, y: 0 }, b: { x: 200, y: 60 } })
  })

  it('grows a step around its center within the limits', () => {
    const one = { ...step(1), at: { x: 100, y: 100 }, size: 32 }
    expect(resized(one, 'b', { x: 130, y: 110 }, [16, 120])).toMatchObject({ size: 60, at: { x: 100, y: 100 } })
    expect(resized(one, 'b', { x: 101, y: 101 }, [16, 120])).toMatchObject({ size: 16 })
  })

  it('scales text from its corner', () => {
    const text: Annotation = { id: 1, kind: 'Text', at: { x: 0, y: 0 }, text: 'hi', color: '#000', size: 20, font: 'sans', width: 40, height: 25 }
    expect(handles(text)).toEqual([{ id: 'b', at: { x: 40, y: 25 } }])
    expect(resized(text, 'b', { x: 0, y: 50 }, [10, 160])).toMatchObject({ size: 40, width: 80, height: 50, at: { x: 0, y: 0 } })
  })
})

describe('steps', () => {
  it('renumber when one goes away', () => {
    const all = [step(5), step(6), step(7)]
    expect(stepNumber(all, 7)).toBe(3)
    expect(stepNumber(all.filter((a) => a.id !== 5), 7)).toBe(2)
  })

  it('keep their numbers across layers', () => {
    const all = [step(5), step(6), step(7)]
    expect(stepNumber(reordered(all, [7], 'send_to_back'), 7)).toBe(3)
  })
})

describe('layers', () => {
  const ids = (list: Annotation[]) => list.map((a) => a.id)
  const all = [shape('Line', 1), shape('Line', 2), shape('Line', 3), shape('Line', 4)]

  it('moves to the front and back keeping the picked order', () => {
    expect(ids(reordered(all, [1, 3], 'bring_to_front'))).toEqual([2, 4, 1, 3])
    expect(ids(reordered(all, [2, 4], 'send_to_back'))).toEqual([2, 4, 1, 3])
  })

  it('moves one step past the next unpicked annotation', () => {
    expect(ids(reordered(all, [1, 2], 'bring_forward'))).toEqual([3, 1, 2, 4])
    expect(ids(reordered(all, [4], 'bring_forward'))).toEqual([1, 2, 3, 4])
    expect(ids(reordered(all, [3], 'send_backward'))).toEqual([1, 3, 2, 4])
  })
})

describe('inside', () => {
  it('needs the whole annotation in the box', () => {
    expect(inside(shape('Rectangle'), { x: 0, y: 0, width: 120, height: 70 })).toBe(true)
    expect(inside(shape('Rectangle'), { x: 0, y: 0, width: 100, height: 70 })).toBe(false)
  })
})

describe('history', () => {
  it('undoes and redoes', () => {
    const history = new History()
    const doc = { annotations: [shape('Line')], crop: null }
    history.commit(doc)
    history.undo()
    expect(history.doc.annotations).toHaveLength(0)
    expect(history.canRedo).toBe(true)
    history.redo()
    expect(history.doc).toBe(doc)
  })

  it('drops the redo branch on a new change', () => {
    const history = new History()
    history.commit({ annotations: [shape('Line')], crop: null })
    history.undo()
    history.commit({ annotations: [], crop: null })
    expect(history.canRedo).toBe(false)
  })

  it('does nothing past the ends', () => {
    const history = new History()
    history.undo()
    history.redo()
    expect(history.doc.annotations).toHaveLength(0)
  })
})

describe('keys', () => {
  it('orders modifiers the same way', () => {
    expect(normalizeCombo('Shift + Ctrl + Z')).toBe('ctrl+shift+z')
    expect(eventCombo({ key: 'Z', ctrlKey: true, metaKey: false, altKey: false, shiftKey: true })).toBe('ctrl+shift+z')
  })

  it('lets the user override', () => {
    const map = keymap({ save: 'ctrl+enter' })
    expect(map.get('ctrl+enter')).toBe('save')
    expect(map.get('ctrl+s')).toBeUndefined()
    expect(map.get('a')).toBe('Arrow')
  })

  it('rejects unknown actions', () => {
    expect(() => keymap({ nonsense: 'x' })).toThrow('nonsense')
  })

  it('copies with ctrl+c unless turned off', () => {
    expect(keymap({}).get('ctrl+c')).toBe('copy')
    expect(keymap({ copy: '' }).get('ctrl+c')).toBeUndefined()
  })

  it('accepts alternatives', () => {
    const map = keymap({})
    expect(map.get('ctrl+y')).toBe('redo')
    expect(map.get('ctrl+shift+z')).toBe('redo')
  })

  it('lets the user turn a key off', () => {
    expect(keymap({ Arrow: '' }).get('a')).toBeUndefined()
  })
})

describe('zoom and move keys', () => {
  it('parses the + key', () => {
    expect(normalizeCombo('ctrl++')).toBe('ctrl++')
    expect(normalizeCombo('Shift + Ctrl ++')).toBe('ctrl+shift++')
    expect(normalizeCombo('+')).toBe('+')
    expect(keymap({}).get('ctrl++')).toBe('zoom_in')
  })

  it('names the space bar', () => {
    const combo = eventCombo({ key: ' ', ctrlKey: false, metaKey: false, altKey: false, shiftKey: false })
    expect(combo).toBe('space')
    expect(keymap({}).get(combo)).toBe('pan')
  })

  it('moves with and without Shift', () => {
    const map = keymap({})
    expect(map.get('arrowleft')).toBe('move_left')
    expect(map.get('shift+arrowleft')).toBe('move_left')
  })
})

describe('History.replace', () => {
  it('changes the state without an undo step', () => {
    const history = new History()
    const first = { annotations: [shape('Rectangle')], crop: null }
    const moved = { annotations: [translated(shape('Rectangle'), 1, 0)], crop: null }
    history.commit(first)
    history.replace(moved)
    expect(history.doc).toBe(moved)
    history.undo()
    expect(history.doc.annotations).toEqual([])
  })
})
