// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest'
import { translated, type Shape } from '../lib/model'
import { History } from './history.svelte'

const shape = (kind: Shape['kind']): Shape => ({
  id: 1,
  kind,
  a: { x: 10, y: 10 },
  b: { x: 110, y: 60 },
  color: '#000',
  size: 2,
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
