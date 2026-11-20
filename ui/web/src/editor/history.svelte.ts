// SPDX-License-Identifier: GPL-3.0-or-later
import { EMPTY_DOC, type Doc } from '../lib/model'

export class History {
  #past = $state.raw<Doc[]>([])
  #future = $state.raw<Doc[]>([])
  doc = $state.raw<Doc>(EMPTY_DOC)

  get canUndo() {
    return this.#past.length > 0
  }
  get canRedo() {
    return this.#future.length > 0
  }

  commit(doc: Doc) {
    this.#past = [...this.#past, this.doc]
    this.#future = []
    this.doc = doc
  }

  undo() {
    const previous = this.#past.at(-1)
    if (!previous) return
    this.#past = this.#past.slice(0, -1)
    this.#future = [...this.#future, this.doc]
    this.doc = previous
  }

  redo() {
    const next = this.#future.at(-1)
    if (!next) return
    this.#future = this.#future.slice(0, -1)
    this.#past = [...this.#past, this.doc]
    this.doc = next
  }

  replace(doc: Doc) {
    this.doc = doc
  }
}
