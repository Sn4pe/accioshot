<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { devicePixelRatio } from 'svelte/reactivity/window'
  import { discard, finishImage, saveState } from '../ipc'
  import { eventCombo, MOVES, type Action, type Move } from '../lib/keys'
  import {
    clampRect,
    constrained,
    EMPTY_DOC,
    hitHandle,
    hitTop,
    inside,
    isShape,
    rectBetween,
    reordered,
    resized,
    translated,
    type Annotation,
    type Doc,
    type HandleId,
    type Layer,
    type Point,
    type Rect,
    type Text,
    type Tool,
  } from '../lib/model'
  import {
    context2d,
    dashedRect,
    exportPng,
    imageRect,
    LINE_HEIGHT,
    measureText,
    renderDoc,
    renderSelection,
    viewRect,
  } from '../lib/render'
  import {
    cursorFor,
    defaultStyle,
    longEnough,
    restyled,
    SIZE_RANGES,
    type EditorSettings,
    type Style,
  } from '../lib/tools'
  import {
    centered,
    clampOrigin,
    fitScale,
    nextZoom,
    offsetIn,
    stepScale,
    wheelDelta,
    wheelZoom,
    zoomAt,
  } from '../lib/viewport'
  import { History } from './history.svelte'
  import Toolbar from './Toolbar.svelte'

  type Editing = { id: number | null; at: Point; text: string; color: string; size: number; font: string }
  type Gesture =
    | { kind: 'draw'; start: Point }
    | { kind: 'move'; id: number; start: Point }
    | { kind: 'resize'; id: number; handle: HandleId }
    | { kind: 'crop'; start: Point }
    | { kind: 'pan'; start: Point; origin: Point }
    | { kind: 'select'; start: Point; base: number[] }
  type StageEvent = MouseEvent & { currentTarget: Element }

  let {
    settings,
    image,
    keys,
    tool: initialTool,
    styles: initialStyles,
  }: {
    settings: EditorSettings
    image: ImageBitmap
    keys: Map<string, Action>
    tool: Tool
    styles: Partial<Record<Tool, Style>>
  } = $props()

  const MIN_DRAG = 3
  const NUDGE_FAR = 10
  const DUPLICATE_OFFSET = 10
  const MIN_ZOOM = 0.05
  const STAGE_MARGIN = 24

  const history = new History()
  const size = $derived({ width: image.width, height: image.height })
  let nextId = 1
  let failure = $state('')
  let saving = $state(false)
  let confirmingDiscard = $state(false)

  let preview = $state.raw<Doc | null>(null)
  let draft = $state.raw<Annotation | null>(null)
  let cropDraft = $state.raw<Rect | null>(null)
  let selectedIds = $state.raw<number[]>([])
  let marquee = $state.raw<Rect | null>(null)
  let editing = $state<Editing | null>(null)
  let tool = $derived(initialTool)
  let styles = $derived(initialStyles)
  let cursor = $derived(cursorFor(initialTool))

  let gesture: Gesture | null = null
  let stageWidth = $state(0)
  let stageHeight = $state(0)
  let zoom: number | null = $derived(
    settings.initial_zoom === 'fit' ? null : Math.min(settings.initial_zoom, settings.max_zoom),
  )
  let origin = $state.raw<Point | null>(null)
  let panKey = $state<string | null>(null)
  let nudged: string | null = null

  const doc = $derived(history.doc)
  const shown = $derived(preview ?? doc)
  const view = $derived(viewRect(shown, size))
  const stageSize = $derived({ width: stageWidth, height: stageHeight })
  const fit = $derived(fitScale(view, stageSize, STAGE_MARGIN))
  const scale = $derived(zoom ?? fit)
  const at = $derived(clampOrigin(origin ?? centered(view, stageSize, scale), view, stageSize, scale, STAGE_MARGIN))
  const selection = $derived(shown.annotations.filter((a) => selectedIds.includes(a.id)))
  const selected = $derived(selection.length === 1 ? selection[0] : null)
  const lead = $derived(selectedIds.map((id) => selection.find((a) => a.id === id)).find((a) => a) ?? null)
  const styleTarget: Tool = $derived(lead?.kind ?? tool)
  const style = $derived(styleOf(styleTarget))
  const dirty = $derived(doc !== EMPTY_DOC)

  function styleOf(target: Tool): Style {
    return styles[target] ?? defaultStyle(settings, target)
  }

  function commit(next: Doc) {
    history.commit(next)
    nudged = null
    preview = null
  }

  function setAnnotation(source: Doc, annotation: Annotation): Doc {
    return { ...source, annotations: source.annotations.map((a) => (a.id === annotation.id ? annotation : a)) }
  }

  function mapSelected(source: Doc, change: (annotation: Annotation) => Annotation): Doc {
    return { ...source, annotations: source.annotations.map((a) => (selectedIds.includes(a.id) ? change(a) : a)) }
  }

  function setTool(next: Tool) {
    commitText()
    tool = next
    selectedIds = []
    cursor = cursorFor(tool)
  }

  function pointAt(event: StageEvent): Point {
    const screen = offsetIn(event.currentTarget, event)
    return { x: screen.x / scale + at.x, y: screen.y / scale + at.y }
  }

  function setZoom(next: number | null, screen: Point = { x: stageWidth / 2, y: stageHeight / 2 }) {
    const limited = nextZoom(zoom, next, fit, zoom === null ? fit : MIN_ZOOM, settings.max_zoom)
    if (limited === null) {
      zoom = null
      origin = null
      return
    }
    origin = zoomAt(at, scale, limited, screen)
    zoom = limited
  }

  function onWheel(event: WheelEvent & StageEvent) {
    event.preventDefault()
    if (event.ctrlKey) {
      setZoom(scale * wheelZoom(event, settings.zoom_step), offsetIn(event.currentTarget, event))
      return
    }
    const delta = wheelDelta(event)
    origin = { x: at.x + delta.x / scale, y: at.y + delta.y / scale }
  }

  function nudge(move: Move, far: boolean) {
    if (!selection.length || editing) return
    const step = far ? NUDGE_FAR : 1
    const next = mapSelected(doc, (a) => translated(a, MOVES[move].x * step, MOVES[move].y * step))
    const key = selectedIds.join()
    if (nudged === key) history.replace(next)
    else commit(next)
    nudged = key
  }

  function duplicate() {
    if (!selection.length || editing) return
    const copies = doc.annotations
      .filter((a) => selectedIds.includes(a.id))
      .map((a) => ({ ...translated(a, DUPLICATE_OFFSET, DUPLICATE_OFFSET), id: nextId++ }))
    commit({ ...doc, annotations: [...doc.annotations, ...copies] })
    selectedIds = copies.map((a) => a.id)
  }

  function reorder(layer: Layer) {
    if (!selection.length || editing) return
    const annotations = reordered(doc.annotations, selectedIds, layer)
    if (annotations.some((a, i) => a !== doc.annotations[i])) commit({ ...doc, annotations })
  }

  function selectAll() {
    commitText()
    selectedIds = doc.annotations.map((a) => a.id)
  }

  function startText(at: Point, existing: Text | null = null) {
    const current = styleOf('Text')
    editing = existing
      ? {
          id: existing.id,
          at: existing.at,
          text: existing.text,
          color: existing.color,
          size: existing.size,
          font: existing.font,
        }
      : { id: null, at, text: '', color: current.color, size: current.size, font: settings.font_family }
    selectedIds = existing ? [existing.id] : []
  }

  function commitText() {
    const current = editing
    if (!current) return
    editing = null
    const existing = current.id === null ? null : doc.annotations.find((a) => a.id === current.id)
    if (!current.text.trim()) {
      if (existing) commit({ ...doc, annotations: doc.annotations.filter((a) => a !== existing) })
      return
    }
    const measured = measureText(current.text, current.size, current.font)
    const text: Text = {
      id: current.id ?? nextId++,
      kind: 'Text',
      at: current.at,
      text: current.text,
      color: current.color,
      size: current.size,
      font: current.font,
      ...measured,
    }
    if (existing) {
      if (existing.kind === 'Text' && existing.text !== text.text) commit(setAnnotation(doc, text))
    } else {
      commit({ ...doc, annotations: [...doc.annotations, text] })
    }
    selectedIds = [text.id]
  }

  function onPointerDown(event: PointerEvent & StageEvent) {
    // A second button mid-drag would leave the first gesture's draft or preview behind.
    if (gesture) return
    nudged = null
    if (event.button === 1 || (event.button === 0 && panKey)) {
      // Also keeps the focus where it is, so a text being edited stays open.
      event.preventDefault()
      event.currentTarget.setPointerCapture(event.pointerId)
      gesture = { kind: 'pan', start: { x: event.clientX, y: event.clientY }, origin: at }
      cursor = 'grabbing'
      return
    }
    if (event.button !== 0) return
    if (editing) {
      if (event.target instanceof HTMLTextAreaElement) return
      commitText()
      return
    }
    const p = pointAt(event)
    const tolerance = 6 / scale
    event.currentTarget.setPointerCapture(event.pointerId)

    // Text and Step tools place new ones, so their handles only answer to Select. Shift toggles instead.
    if (selected && !event.shiftKey && (tool === 'Select' || (tool !== 'Crop' && isShape(selected)))) {
      const handle = hitHandle(selected, p, tolerance + 2 / scale)
      if (handle) {
        gesture = { kind: 'resize', id: selected.id, handle }
        return
      }
    }
    switch (tool) {
      case 'Select': {
        const target = hitTop(doc.annotations, p, tolerance)
        if (event.shiftKey) {
          if (!target) gesture = { kind: 'select', start: p, base: selectedIds }
          else if (selectedIds.includes(target.id)) selectedIds = selectedIds.filter((id) => id !== target.id)
          else selectedIds = [...selectedIds, target.id]
        } else if (target) {
          // Dragging one of several selected annotations moves them all.
          if (!selectedIds.includes(target.id)) selectedIds = [target.id]
          gesture = { kind: 'move', id: target.id, start: p }
        } else {
          selectedIds = []
          gesture = { kind: 'select', start: p, base: [] }
        }
        break
      }
      case 'Crop':
        gesture = { kind: 'crop', start: p }
        break
      case 'Text': {
        const target = hitTop(doc.annotations, p, tolerance)
        startText(p, target?.kind === 'Text' ? target : null)
        break
      }
      case 'Step': {
        const current = styleOf('Step')
        const id = nextId++
        commit({ ...doc, annotations: [...doc.annotations, { id, kind: 'Step', at: p, ...current }] })
        selectedIds = [id]
        break
      }
      case 'Freehand':
        selectedIds = []
        draft = { id: nextId++, kind: 'Freehand', points: [p], ...styleOf('Freehand') }
        gesture = { kind: 'draw', start: p }
        break
      default:
        selectedIds = []
        draft = { id: nextId++, kind: tool, a: p, b: p, ...styleOf(tool) }
        gesture = { kind: 'draw', start: p }
    }
  }

  function onPointerMove(event: PointerEvent & StageEvent) {
    if (gesture?.kind === 'pan') {
      origin = {
        x: gesture.origin.x - (event.clientX - gesture.start.x) / scale,
        y: gesture.origin.y - (event.clientY - gesture.start.y) / scale,
      }
      return
    }
    if (editing) return
    const p = pointAt(event)
    if (!gesture) {
      updateCursor(p)
      return
    }
    switch (gesture.kind) {
      case 'draw':
        if (draft?.kind === 'Freehand') {
          draft = { ...draft, points: [...draft.points, p] }
        } else if (draft && isShape(draft)) {
          draft = { ...draft, b: event.shiftKey ? constrained(draft.kind, gesture.start, p) : p }
        }
        break
      case 'move': {
        const { x, y } = gesture.start
        preview = mapSelected(doc, (a) => translated(a, p.x - x, p.y - y))
        break
      }
      case 'resize': {
        const { id, handle } = gesture
        const original = doc.annotations.find((a) => a.id === id)
        if (original)
          preview = setAnnotation(doc, resized(original, handle, p, SIZE_RANGES[original.kind] ?? [1, Infinity]))
        break
      }
      case 'select': {
        const { start, base } = gesture
        const box = rectBetween(start, p)
        const boxed = doc.annotations.filter((a) => inside(a, box) && !base.includes(a.id)).map((a) => a.id)
        marquee = box
        selectedIds = [...base, ...boxed]
        break
      }
      case 'crop':
        cropDraft = clampRect(rectBetween(gesture.start, p), view)
        break
    }
  }

  function onPointerUp() {
    const current = gesture
    gesture = null
    if (!current) return
    switch (current.kind) {
      case 'pan':
        cursor = panKey ? 'grab' : cursorFor(tool)
        break
      case 'draw':
        if (draft && longEnough(draft, MIN_DRAG / scale)) {
          commit({ ...doc, annotations: [...doc.annotations, draft] })
          selectedIds = draft.kind === 'Freehand' ? [] : [draft.id]
        }
        draft = null
        break
      case 'move':
        if (preview) commit(preview)
        // A click without a drag picks that one out of the selection.
        else selectedIds = [current.id]
        break
      case 'resize': {
        const resizedOne = preview?.annotations.find((a) => a.id === current.id)
        if (preview) commit(preview)
        // The slider follows a step or text resized by its handle.
        if (resizedOne?.kind === 'Text' || resizedOne?.kind === 'Step') {
          styles = { ...styles, [resizedOne.kind]: { ...styleOf(resizedOne.kind), size: resizedOne.size } }
        }
        break
      }
      case 'select':
        marquee = null
        break
      case 'crop':
        if (cropDraft && cropDraft.width >= MIN_DRAG && cropDraft.height >= MIN_DRAG) {
          const { x, y, width, height } = cropDraft
          const crop = { x: Math.round(x), y: Math.round(y), width: Math.round(width), height: Math.round(height) }
          commit({ ...doc, crop: clampRect(crop, imageRect(size)) })
          setZoom(null)
          setTool('Select')
        }
        cropDraft = null
        break
    }
  }

  function updateCursor(p: Point) {
    if (panKey) {
      cursor = 'grab'
      return
    }
    const tolerance = 6 / scale
    if (selected && hitHandle(selected, p, tolerance + 2 / scale)) cursor = 'grab'
    else if (tool !== 'Select') cursor = cursorFor(tool)
    else cursor = hitTop(doc.annotations, p, tolerance) ? 'move' : 'default'
  }

  function onDoubleClick(event: StageEvent) {
    if (tool !== 'Select') return
    const target = hitTop(doc.annotations, pointAt(event), 6 / scale)
    if (target?.kind === 'Text') startText(target.at, target)
  }

  function applyStyle(change: Partial<Style>, final: boolean) {
    styles = { ...styles, [styleTarget]: { ...styleOf(styleTarget), ...change } }
    if (!selection.length) return
    const next = mapSelected(doc, (annotation) => restyled(annotation, change, styleTarget, measureText))
    if (final) commit(next)
    else preview = next
  }

  function remove() {
    if (!selection.length) return
    commit({ ...doc, annotations: doc.annotations.filter((a) => !selectedIds.includes(a.id)) })
    selectedIds = []
  }

  function undo() {
    history.undo()
    nudged = null
  }

  function redo() {
    history.redo()
    nudged = null
  }

  function escape() {
    if (confirmingDiscard) confirmingDiscard = false
    else if (gesture) cancelGesture()
    else if (selection.length) selectedIds = []
    else requestDiscard()
  }

  function cancelGesture() {
    if (!gesture) return
    if (gesture.kind === 'select') selectedIds = gesture.base
    gesture = null
    draft = null
    preview = null
    cropDraft = null
    marquee = null
    cursor = panKey ? 'grab' : cursorFor(tool)
  }

  export function requestDiscard() {
    if (dirty) confirmingDiscard = true
    else void finishDiscard()
  }

  async function rememberState() {
    if (settings.remember_last)
      await saveState({ tool, styles }).catch((error: unknown) => console.error('save_state:', error))
  }

  async function finishDiscard() {
    try {
      await rememberState()
      await discard(true)
    } catch (error) {
      failure = String(error)
    }
  }

  async function finish(command: 'save' | 'copy') {
    if (saving) return
    commitText()
    saving = true
    try {
      const png = await exportPng(doc, image, size)
      await rememberState()
      await finishImage(command, png)
    } catch (error) {
      failure = String(error)
      saving = false
    }
  }

  function perform(action: Action, event: KeyboardEvent) {
    switch (action) {
      case 'undo':
        return undo()
      case 'redo':
        return redo()
      case 'delete':
        return remove()
      case 'save':
      case 'copy':
        return void finish(action)
      case 'discard':
        return escape()
      case 'zoom_in':
        return setZoom(stepScale(scale, settings.zoom_step))
      case 'zoom_out':
        return setZoom(stepScale(scale, 1 / settings.zoom_step))
      case 'zoom_fit':
        return setZoom(null)
      case 'zoom_actual':
        return setZoom(1)
      case 'pan':
        panKey = event.key.toLowerCase()
        if (!gesture) cursor = 'grab'
        return
      case 'move_left':
      case 'move_right':
      case 'move_up':
      case 'move_down':
        return nudge(action, event.shiftKey)
      case 'duplicate':
        return duplicate()
      case 'select_all':
        return selectAll()
      case 'bring_forward':
      case 'send_backward':
      case 'bring_to_front':
      case 'send_to_back':
        return reorder(action)
      default:
        setTool(action)
    }
  }

  function onKeyDown(event: KeyboardEvent) {
    // The size slider keeps its arrow keys.
    if (event.target instanceof HTMLTextAreaElement || event.target instanceof HTMLInputElement) return
    const action = keys.get(eventCombo(event))
    if (!action) return
    // Space still presses a focused button.
    if (action === 'pan' && event.target instanceof HTMLButtonElement) return
    if (!(action in MOVES)) nudged = null
    event.preventDefault()
    // Mid drag the gesture holds a copy of the document, only Esc may interrupt it.
    if (gesture && action !== 'discard') return
    perform(action, event)
  }

  function onKeyUp(event: KeyboardEvent) {
    if (event.key.toLowerCase() !== panKey) return
    event.preventDefault()
    releasePan()
  }

  function releasePan() {
    panKey = null
    if (!gesture) cursor = cursorFor(tool)
  }

  function onTextKey(event: KeyboardEvent) {
    event.stopPropagation()
    if (event.key === 'Escape') editing = null
    else if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      commitText()
    }
  }

  function focusAndSelect(element: HTMLTextAreaElement) {
    element.focus()
    element.select()
  }

  const editorBox = $derived.by(() => {
    if (!editing) return null
    const measured = measureText(editing.text || ' ', editing.size, editing.font)
    return {
      left: (editing.at.x - at.x) * scale,
      top: (editing.at.y - at.y) * scale,
      width: (measured.width + editing.size) * scale,
      height: measured.height * scale,
      font: editing.size * scale,
    }
  })

  const sheet = $derived.by(() => {
    const sheet = document.createElement('canvas')
    sheet.width = Math.round(view.width)
    sheet.height = Math.round(view.height)
    // The text being edited lives in the textarea until it is committed.
    const hidden = editing?.id ?? null
    const visible =
      hidden === null ? shown : { ...shown, annotations: shown.annotations.filter((a) => a.id !== hidden) }
    renderDoc(context2d(sheet), visible, image, size, draft)
    return sheet
  })

  // Panning and zooming only copy the sheet, the blur and pixelate effects are not redone.
  function paint(canvas: HTMLCanvasElement) {
    const ratio = devicePixelRatio.current ?? 1
    const width = Math.round(stageWidth * ratio)
    const height = Math.round(stageHeight * ratio)
    // Setting the size reallocates the canvas, so not on every pan frame.
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width
      canvas.height = height
    }
    const ctx = context2d(canvas)
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.clearRect(0, 0, width, height)
    ctx.setTransform(scale * ratio, 0, 0, scale * ratio, -at.x * scale * ratio, -at.y * scale * ratio)
    // Sharp pixels from 100 % up, to see exactly where a mark lands.
    ctx.imageSmoothingEnabled = scale < 1
    ctx.drawImage(sheet, view.x, view.y, view.width, view.height)
    ctx.imageSmoothingEnabled = true
    if (!editing) for (const annotation of selection) renderSelection(ctx, annotation, scale, selection.length === 1)
    if (marquee) dashedRect(ctx, marquee, 1 / scale)
    if (cropDraft) {
      ctx.fillStyle = 'rgba(0, 0, 0, 0.55)'
      ctx.beginPath()
      ctx.rect(view.x, view.y, view.width, view.height)
      ctx.rect(cropDraft.x, cropDraft.y, cropDraft.width, cropDraft.height)
      ctx.fill('evenodd')
      ctx.strokeStyle = '#f4f1ea'
      ctx.lineWidth = 1 / scale
      ctx.strokeRect(cropDraft.x, cropDraft.y, cropDraft.width, cropDraft.height)
    }
  }
</script>

<svelte:window onkeydown={onKeyDown} onkeyup={onKeyUp} onblur={releasePan} />

<Toolbar
  {tool}
  target={styleTarget}
  {style}
  colors={styleTarget === 'Highlight' ? [settings.highlight_color, ...settings.colors] : settings.colors}
  {keys}
  canUndo={history.canUndo}
  canRedo={history.canRedo}
  {scale}
  zoomed={zoom !== null}
  {saving}
  {confirmingDiscard}
  ontool={setTool}
  onstyle={applyStyle}
  onundo={undo}
  onredo={redo}
  onfit={() => setZoom(null)}
  ondiscard={requestDiscard}
  onkeep={() => (confirmingDiscard = false)}
  onconfirm={finishDiscard}
  onfinish={finish}
/>

<main
  bind:clientWidth={stageWidth}
  bind:clientHeight={stageHeight}
  style:cursor
  onwheel={onWheel}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={cancelGesture}
  ondblclick={onDoubleClick}
>
  <canvas {@attach paint}></canvas>
  {#if editing && editorBox}
    <textarea
      bind:value={editing.text}
      {@attach focusAndSelect}
      onkeydown={onTextKey}
      onblur={commitText}
      spellcheck="false"
      aria-label="Text"
      style:left="{editorBox.left}px"
      style:top="{editorBox.top}px"
      style:width="{editorBox.width}px"
      style:height="{editorBox.height}px"
      style:font-size="{editorBox.font}px"
      style:font-family={editing.font}
      style:line-height={LINE_HEIGHT}
      style:color={editing.color}></textarea>
  {/if}
  {#if failure}
    <p class="failure" role="alert">{failure}</p>
  {/if}
</main>

<style>
  main {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    touch-action: none;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  textarea {
    position: absolute;
    margin: 0;
    padding: 0;
    border: 0;
    background: none;
    overflow: hidden;
    resize: none;
    white-space: pre;
    outline: 1px dashed #f4f1ea;
    outline-offset: 3px;
    user-select: text;
  }
  .failure {
    position: absolute;
    top: 16px;
    left: 24px;
    right: 24px;
    padding: 10px 14px;
    border-radius: 4px;
  }
</style>
