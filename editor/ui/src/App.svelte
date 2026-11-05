<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { onMount } from 'svelte'
  import Glyph from './Glyph.svelte'
  import {
    clampRect,
    constrained,
    EMPTY_DOC,
    eventCombo,
    hitHandle,
    hitTop,
    History,
    isShape,
    keymap,
    rectBetween,
    resized,
    SHAPE_KINDS,
    translated,
    type Action,
    type Annotation,
    type Doc,
    type HandleId,
    type Point,
    type Rect,
    type Shape,
    type ShapeKind,
    type Text,
    type Tool,
  } from './lib/model'
  import { exportPng, imageRect, LINE_HEIGHT, measureText, renderDoc, renderSelection, viewRect } from './lib/render'

  type Settings = {
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
    keys: Record<string, string>
  }
  type Style = { color: string; size: number }
  type Editing = { id: number | null; at: Point; text: string; color: string; size: number; font: string }
  type Gesture =
    | { kind: 'draw'; start: Point }
    | { kind: 'move'; id: number; start: Point }
    | { kind: 'resize'; id: number; handle: HandleId }
    | { kind: 'crop'; start: Point }

  const TOOLS: { tool: Tool; label: string }[] = [
    { tool: 'Select', label: 'Select' },
    { tool: 'Rectangle', label: 'Rectangle' },
    { tool: 'Ellipse', label: 'Ellipse' },
    { tool: 'Line', label: 'Line' },
    { tool: 'Arrow', label: 'Arrow' },
    { tool: 'Freehand', label: 'Freehand' },
    { tool: 'Text', label: 'Text' },
    { tool: 'Step', label: 'Step' },
    { tool: 'Highlight', label: 'Highlight' },
    { tool: 'Blur', label: 'Blur' },
    { tool: 'Pixelate', label: 'Pixelate' },
    { tool: 'Crop', label: 'Crop' },
  ]
  const SIZE_RANGES: Partial<Record<Tool, [number, number]>> = {
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
  const COLORED: readonly Tool[] = ['Rectangle', 'Ellipse', 'Line', 'Arrow', 'Freehand', 'Text', 'Step', 'Highlight']
  const MIN_DRAG = 3
  const STAGE_MARGIN = 24
  const LOAD_TIMEOUT_MS = 10_000
  const CLOSE_HINT = 'Close this window, the original capture is kept in the captures folder.'

  const history = new History()
  let nextId = 1
  let settings = $state.raw<Settings | null>(null)
  let keys = new Map<string, Action>()
  let image = $state.raw<HTMLImageElement | null>(null)
  let failure = $state('')
  let saving = $state(false)
  let confirmingDiscard = $state(false)

  let doc = $state.raw<Doc>(EMPTY_DOC)
  let preview = $state.raw<Doc | null>(null)
  let draft = $state.raw<Annotation | null>(null)
  let cropDraft = $state.raw<Rect | null>(null)
  let selectedId = $state<number | null>(null)
  let editing = $state<Editing | null>(null)
  let tool = $state<Tool>('Arrow')
  let styles = $state<Partial<Record<Tool, Style>>>({})
  let cursor = $state('default')
  let canUndo = $state(false)
  let canRedo = $state(false)

  let gesture: Gesture | null = null
  let canvas: HTMLCanvasElement | undefined = $state()
  let stageWidth = $state(0)
  let stageHeight = $state(0)

  const shown = $derived(preview ?? doc)
  const size = $derived(image ? { width: image.naturalWidth, height: image.naturalHeight } : { width: 1, height: 1 })
  const view = $derived(viewRect(shown, size))
  const scale = $derived(
    Math.min(1, (stageWidth - STAGE_MARGIN * 2) / view.width, (stageHeight - STAGE_MARGIN * 2) / view.height) || 1,
  )
  const selected = $derived(shown.annotations.find((a) => a.id === selectedId) ?? null)
  const styleTarget: Tool = $derived(selected?.kind ?? tool)
  const style = $derived(styleOf(styleTarget))
  const dirty = $derived(doc !== EMPTY_DOC)

  function defaultStyle(target: Tool): Style {
    const s = settings!
    switch (target) {
      case 'Highlight':
        return { color: s.highlight_color, size: 0 }
      case 'Text':
        return { color: s.colors[0], size: s.font_size }
      case 'Step':
        return { color: s.colors[0], size: s.step_size }
      case 'Blur':
        return { color: '', size: s.blur_strength }
      case 'Pixelate':
        return { color: '', size: s.pixel_size }
      default:
        return { color: s.colors[0], size: s.stroke_width }
    }
  }

  function styleOf(target: Tool): Style {
    if (!settings) return { color: '#000000', size: 1 }
    return styles[target] ?? defaultStyle(target)
  }

  function isTool(value: unknown): value is Tool {
    return TOOLS.some((entry) => entry.tool === value)
  }

  function syncHistory() {
    doc = history.doc
    canUndo = history.canUndo
    canRedo = history.canRedo
  }

  function commit(next: Doc) {
    history.commit(next)
    preview = null
    syncHistory()
  }

  function setAnnotation(source: Doc, annotation: Annotation): Doc {
    return { ...source, annotations: source.annotations.map((a) => (a.id === annotation.id ? annotation : a)) }
  }

  function setTool(next: Tool) {
    commitText()
    tool = next
    selectedId = null
    cursor = next === 'Select' ? 'default' : 'crosshair'
  }

  function pointAt(event: PointerEvent): Point {
    const rect = canvas!.getBoundingClientRect()
    return { x: (event.clientX - rect.left) / scale + view.x, y: (event.clientY - rect.top) / scale + view.y }
  }

  function startText(at: Point, existing: Text | null = null) {
    const current = styleOf('Text')
    editing = existing
      ? { id: existing.id, at: existing.at, text: existing.text, color: existing.color, size: existing.size, font: existing.font }
      : { id: null, at, text: '', color: current.color, size: current.size, font: settings!.font_family }
    selectedId = existing?.id ?? null
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
    const measured = measureText(canvas!.getContext('2d')!, current.text, current.size, current.font)
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
    selectedId = text.id
  }

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0 || !canvas) return
    if (editing) {
      if (event.target instanceof HTMLTextAreaElement) return
      commitText()
      return
    }
    const p = pointAt(event)
    const tolerance = 6 / scale
    ;(event.currentTarget as Element).setPointerCapture(event.pointerId)

    if (selected && tool !== 'Crop') {
      const handle = hitHandle(selected, p, tolerance + 2 / scale)
      if (handle) {
        gesture = { kind: 'resize', id: selected.id, handle }
        return
      }
    }
    switch (tool) {
      case 'Select': {
        const target = hitTop(doc.annotations, p, tolerance)
        selectedId = target?.id ?? null
        if (target) gesture = { kind: 'move', id: target.id, start: p }
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
        selectedId = id
        break
      }
      case 'Freehand':
        selectedId = null
        draft = { id: nextId++, kind: 'Freehand', points: [p], ...styleOf('Freehand') }
        gesture = { kind: 'draw', start: p }
        break
      default:
        selectedId = null
        draft = { id: nextId++, kind: tool as ShapeKind, a: p, b: p, ...styleOf(tool) }
        gesture = { kind: 'draw', start: p }
    }
  }

  function onPointerMove(event: PointerEvent) {
    if (!canvas || editing) return
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
        const original = doc.annotations.find((a) => a.id === (gesture as { id: number }).id)
        if (original) preview = setAnnotation(doc, translated(original, p.x - gesture.start.x, p.y - gesture.start.y))
        break
      }
      case 'resize': {
        const original = doc.annotations.find((a) => a.id === (gesture as { id: number }).id)
        if (original && isShape(original)) preview = setAnnotation(doc, resized(original, gesture.handle, p))
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
      case 'draw':
        if (draft && longEnough(draft)) {
          commit({ ...doc, annotations: [...doc.annotations, draft] })
          selectedId = draft.kind === 'Freehand' ? null : draft.id
        }
        draft = null
        break
      case 'move':
      case 'resize':
        if (preview) commit(preview)
        break
      case 'crop':
        if (cropDraft && cropDraft.width >= MIN_DRAG && cropDraft.height >= MIN_DRAG) {
          const { x, y, width, height } = cropDraft
          const crop = { x: Math.round(x), y: Math.round(y), width: Math.round(width), height: Math.round(height) }
          commit({ ...doc, crop: clampRect(crop, imageRect(size)) })
          setTool('Select')
        }
        cropDraft = null
        break
    }
  }

  function longEnough(annotation: Annotation) {
    if (annotation.kind === 'Freehand') return true
    if (!isShape(annotation)) return false
    return Math.hypot(annotation.a.x - annotation.b.x, annotation.a.y - annotation.b.y) >= MIN_DRAG / scale
  }

  function updateCursor(p: Point) {
    const tolerance = 6 / scale
    if (selected && hitHandle(selected, p, tolerance + 2 / scale)) cursor = 'grab'
    else if (tool !== 'Select') cursor = tool === 'Text' ? 'text' : 'crosshair'
    else cursor = hitTop(doc.annotations, p, tolerance) ? 'move' : 'default'
  }

  function onDoubleClick(event: MouseEvent) {
    if (tool !== 'Select' || !canvas) return
    const target = hitTop(doc.annotations, pointAt(event as PointerEvent), 6 / scale)
    if (target?.kind === 'Text') startText(target.at, target)
  }

  /** Changes the selected annotation too, and remembers the style for its tool. */
  function applyStyle(change: Partial<Style>, final: boolean) {
    styles[styleTarget] = { ...styleOf(styleTarget), ...change }
    if (!selected) return
    const updated = { ...selected, ...change } as Annotation
    if (updated.kind === 'Text') {
      const measured = measureText(canvas!.getContext('2d')!, updated.text, updated.size, updated.font)
      Object.assign(updated, measured)
    }
    const next = setAnnotation(doc, updated)
    if (final) commit(next)
    else preview = next
  }

  function remove() {
    if (selectedId === null) return
    commit({ ...doc, annotations: doc.annotations.filter((a) => a.id !== selectedId) })
    selectedId = null
  }

  function undo() {
    history.undo()
    syncHistory()
  }

  function redo() {
    history.redo()
    syncHistory()
  }

  /** Esc: drop what is in progress first, close only when nothing is. */
  function escape() {
    if (confirmingDiscard) confirmingDiscard = false
    else if (gesture) {
      gesture = null
      draft = null
      preview = null
      cropDraft = null
    } else if (selectedId !== null) selectedId = null
    else requestDiscard()
  }

  function requestDiscard() {
    if (dirty) confirmingDiscard = true
    else void finishDiscard()
  }

  async function rememberState() {
    if (settings?.remember_last) await invoke('save_state', { state: { tool, styles } }).catch(() => {})
  }

  async function finishDiscard() {
    await rememberState()
    await invoke('discard', { loaded: image !== null })
  }

  async function finish(command: 'save' | 'copy') {
    if (!image || saving) return
    commitText()
    saving = true
    try {
      const png = await exportPng(doc, image, size)
      await rememberState()
      await invoke(command, png)
    } catch (error) {
      failure = String(error)
      saving = false
    }
  }

  function perform(action: Action) {
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
      default:
        setTool(action)
    }
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.target instanceof HTMLTextAreaElement) return
    const action = keys.get(eventCombo(event))
    if (!action) return
    event.preventDefault()
    // Mid drag the gesture holds a copy of the document, only Esc may interrupt it.
    if (gesture && action !== 'discard') return
    perform(action)
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
    if (!editing || !canvas) return null
    const measured = measureText(canvas.getContext('2d')!, editing.text || ' ', editing.size, editing.font)
    return {
      left: (editing.at.x - view.x) * scale,
      top: (editing.at.y - view.y) * scale,
      width: (measured.width + editing.size) * scale,
      height: measured.height * scale,
      font: editing.size * scale,
    }
  })

  $effect(() => {
    if (!canvas || !image) return
    canvas.width = Math.round(view.width)
    canvas.height = Math.round(view.height)
    const ctx = canvas.getContext('2d')!
    // The text being edited lives in the textarea until it is committed.
    const hidden = editing?.id ?? null
    const visible = hidden === null ? shown : { ...shown, annotations: shown.annotations.filter((a) => a.id !== hidden) }
    renderDoc(ctx, visible, image, size, draft)
    if (selected && !editing) renderSelection(ctx, selected, scale)
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
  })

  onMount(() => {
    const unlisten = getCurrentWindow().onCloseRequested((event) => {
      event.preventDefault()
      requestDiscard()
    })
    void load()
    return () => void unlisten.then((stop) => stop())
  })

  async function load() {
    let step = 'settings'
    // A promise that never settles shows no error by itself.
    const stalled = setTimeout(() => {
      failure = `The capture did not load, stuck on ${step}. ${CLOSE_HINT}`
    }, LOAD_TIMEOUT_MS)
    try {
      const loaded = await invoke<Settings>('settings')
      keys = keymap(loaded.keys)
      settings = loaded
      tool = isTool(loaded.tool) ? loaded.tool : 'Arrow'
      if (loaded.remember_last) {
        step = 'load_state'
        const state = await invoke<{ tool?: unknown; styles?: Partial<Record<Tool, Style>> } | null>('load_state')
        if (isTool(state?.tool)) tool = state.tool
        if (state?.styles) styles = state.styles
      }
      cursor = tool === 'Select' ? 'default' : 'crosshair'
      step = 'read_image'
      const bytes = await invoke<ArrayBuffer>('read_image')
      step = 'decode'
      const url = URL.createObjectURL(new Blob([bytes], { type: 'image/png' }))
      const element = new Image()
      element.src = url
      await element.decode()
      URL.revokeObjectURL(url)
      image = element
      failure = ''
    } catch (error) {
      failure = `${error}. ${CLOSE_HINT}`
    } finally {
      clearTimeout(stalled)
    }
  }

  function shortcut(action: Action) {
    const combo = [...keys].find(([, value]) => value === action)?.[0]
    return combo ? ` (${combo})` : ''
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="editor">
  <header>
    <div class="tools" role="toolbar" aria-label="Tools">
      {#each TOOLS as entry (entry.tool)}
        <button
          class="tool"
          class:active={tool === entry.tool}
          aria-pressed={tool === entry.tool}
          aria-label={entry.label}
          title={entry.label + shortcut(entry.tool)}
          onclick={() => setTool(entry.tool)}
        >
          <Glyph name={entry.tool} />
        </button>
      {/each}
    </div>

    <div class="style">
      {#if settings && COLORED.includes(styleTarget)}
        <div class="swatches" role="group" aria-label="Color">
          {#each styleTarget === 'Highlight' ? [settings.highlight_color, ...settings.colors] : settings.colors as color (color)}
            <button
              class="swatch"
              class:current={style.color === color}
              style:background={color}
              aria-label={color}
              aria-pressed={style.color === color}
              onclick={() => applyStyle({ color }, true)}
            ></button>
          {/each}
        </div>
      {/if}
      {#if SIZE_RANGES[styleTarget]}
        {@const [min, max] = SIZE_RANGES[styleTarget]!}
        <label class="size">
          <span>{styleTarget === 'Blur' || styleTarget === 'Pixelate' ? 'Strength' : 'Size'}</span>
          <input
            type="range"
            {min}
            {max}
            value={style.size}
            oninput={(event) => applyStyle({ size: event.currentTarget.valueAsNumber }, false)}
            onchange={(event) => applyStyle({ size: event.currentTarget.valueAsNumber }, true)}
          />
          <output>{style.size}</output>
        </label>
      {/if}
    </div>

    <div class="history">
      <button class="tool" aria-label="Undo" title={'Undo' + shortcut('undo')} disabled={!canUndo} onclick={undo}>
        <Glyph name="undo" />
      </button>
      <button class="tool" aria-label="Redo" title={'Redo' + shortcut('redo')} disabled={!canRedo} onclick={redo}>
        <Glyph name="redo" />
      </button>
    </div>

    <div class="finish">
      {#if confirmingDiscard}
        <span class="question">Discard your changes?</span>
        <button class="quiet" onclick={() => (confirmingDiscard = false)}>Keep editing</button>
        <button class="primary danger" onclick={finishDiscard}>Discard</button>
      {:else}
        <button class="quiet" onclick={requestDiscard}>Discard</button>
        <button
          class="quiet"
          title={'Copy to the clipboard and close, without saving' + shortcut('copy')}
          disabled={!image || saving}
          onclick={() => finish('copy')}
        >
          Copy
        </button>
        <button class="primary" title={'Save' + shortcut('save')} disabled={!image || saving} onclick={() => finish('save')}>
          Save
        </button>
      {/if}
    </div>
  </header>

  <main
    bind:clientWidth={stageWidth}
    bind:clientHeight={stageHeight}
    style:cursor
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    ondblclick={onDoubleClick}
  >
    {#if failure}
      <p class="failure" role="alert">{failure}</p>
    {:else if !image}
      <p class="loading">Loading capture</p>
    {/if}
    {#if image}
      <div class="sheet" style:width="{view.width * scale}px" style:height="{view.height * scale}px">
        <canvas bind:this={canvas} style:width="{view.width * scale}px" style:height="{view.height * scale}px"></canvas>
        {#if editing && editorBox}
          <textarea
            bind:value={editing.text}
            use:focusAndSelect
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
            style:color={editing.color}
          ></textarea>
        {/if}
      </div>
    {/if}
  </main>
</div>

<style>
  :global(html, body) {
    height: 100%;
    margin: 0;
    background: #1b1916;
    color: #e6dfd2;
    font: 13px/1.4 system-ui, sans-serif;
    overflow: hidden;
    user-select: none;
  }
  :global(#app) {
    height: 100%;
  }
  .editor {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 20px;
    padding: 6px 12px;
    background: #2a2621;
  }
  .tools,
  .history {
    display: flex;
    gap: 1px;
  }
  .style {
    display: flex;
    align-items: center;
    gap: 16px;
    min-height: 32px;
  }
  .finish {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }
  button {
    font: inherit;
    color: inherit;
    border: 0;
    background: none;
  }
  button:focus-visible,
  input:focus-visible {
    outline: 2px solid #e6dfd2;
    outline-offset: 1px;
  }
  .tool {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 4px;
    color: #a89f8f;
    transition: color 100ms, background-color 100ms;
  }
  .tool:hover:not(:disabled) {
    color: #e6dfd2;
  }
  .tool.active {
    background: #443e35;
    color: #f4f1ea;
  }
  .tool:disabled {
    color: #5c5549;
  }
  .swatches {
    display: flex;
    gap: 6px;
  }
  .swatch {
    width: 18px;
    height: 18px;
    padding: 0;
    border-radius: 3px;
  }
  .swatch.current {
    outline: 2px solid #f4f1ea;
    outline-offset: 2px;
  }
  .size {
    display: flex;
    align-items: center;
    gap: 8px;
    color: #a89f8f;
  }
  .size input {
    width: 110px;
    margin: 0;
    accent-color: #c9bfae;
  }
  .size output {
    min-width: 3ch;
    color: #e6dfd2;
    font-variant-numeric: tabular-nums;
  }
  .quiet,
  .primary {
    height: 32px;
    padding: 0 14px;
    border-radius: 4px;
    transition: background-color 100ms;
  }
  .quiet:hover:not(:disabled) {
    background: #38332c;
  }
  .quiet:disabled {
    color: #5c5549;
  }
  .primary {
    background: #e6dfd2;
    color: #1b1916;
    font-weight: 600;
  }
  .primary:hover:not(:disabled) {
    background: #f4f1ea;
  }
  .primary:disabled {
    background: #443e35;
    color: #8c8374;
  }
  .primary.danger {
    background: #c9443f;
    color: #fff;
  }
  .primary.danger:hover {
    background: #d9524c;
  }
  .question {
    margin-right: 4px;
  }
  main {
    position: relative;
    flex: 1;
    min-height: 0;
    display: grid;
    place-items: center;
    overflow: hidden;
    touch-action: none;
  }
  .sheet {
    position: relative;
  }
  canvas {
    display: block;
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
  .loading {
    margin: 0;
    color: #a89f8f;
  }
  .failure {
    position: absolute;
    top: 16px;
    left: 24px;
    right: 24px;
    margin: 0;
    padding: 10px 14px;
    background: #4a2320;
    color: #f6d9d6;
    border-radius: 4px;
    user-select: text;
  }
</style>
