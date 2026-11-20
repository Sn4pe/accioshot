<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { image } from './ipc'
  import { onMount } from 'svelte'
  import { eventCombo, keymap, withCtrl } from './lib/keys'
  import { title, type Item } from './lib/library'
  import type { Point } from './lib/model'
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
  } from './lib/viewport'

  let {
    item,
    position,
    total,
    preview,
    onclose,
  }: { item: Item; position: number; total: number; preview: boolean; onclose: () => void } = $props()

  const MAX_ZOOM = 16
  const ZOOM_STEP = 1.25
  const DRAG_SLOP = 3
  const KEYS = keymap({})

  let src = $state('')
  let bytes = $state(0)
  let naturalWidth = $state(0)
  let naturalHeight = $state(0)
  let failed = $state('')
  let stageWidth = $state(0)
  let stageHeight = $state(0)
  let zoom = $state<number | null>(null)
  let origin = $state<Point | null>(null)
  let stage = $state<HTMLElement>()
  let panning: { start: Point; origin: Point; moved: boolean } | null = null

  const entry = $derived(item.entry)
  const margin = $derived(preview ? 0 : 16)
  const view = $derived({ x: 0, y: 0, width: naturalWidth, height: naturalHeight })
  const stageSize = $derived({ width: stageWidth, height: stageHeight })
  const fit = $derived(naturalWidth ? fitScale(view, stageSize, margin) : 1)
  const scale = $derived(zoom ?? fit)
  const at = $derived(clampOrigin(origin ?? centered(view, stageSize, scale), view, stageSize, scale, margin))

  function megabytes(count: number): string {
    return count >= 1024 * 1024 ? `${(count / 1024 / 1024).toFixed(1)} MB` : `${Math.round(count / 1024)} KB`
  }

  function setZoom(next: number | null, screen: Point = { x: stageWidth / 2, y: stageHeight / 2 }) {
    const least = preview ? fit : fit / 4
    const limited = nextZoom(zoom, next, fit, least, MAX_ZOOM)
    if (limited === null) {
      zoom = null
      origin = null
      return
    }
    origin = zoomAt(at, scale, limited, screen)
    zoom = limited
  }

  function onWheel(event: WheelEvent & { currentTarget: Element }) {
    if (!naturalWidth) return
    event.preventDefault()
    if (withCtrl(event)) {
      setZoom(scale * wheelZoom(event, ZOOM_STEP), offsetIn(event.currentTarget, event))
      return
    }
    const delta = wheelDelta(event)
    origin = { x: at.x + delta.x / scale, y: at.y + delta.y / scale }
  }

  function onPointerDown(event: PointerEvent & { currentTarget: Element }) {
    if (event.button !== 0) return
    event.stopPropagation()
    event.currentTarget.setPointerCapture(event.pointerId)
    panning = { start: { x: event.clientX, y: event.clientY }, origin: at, moved: false }
  }

  function onPointerMove(event: PointerEvent) {
    if (!panning) return
    const dx = event.clientX - panning.start.x
    const dy = event.clientY - panning.start.y
    if (!panning.moved && Math.hypot(dx, dy) < DRAG_SLOP) return
    panning.moved = true
    origin = { x: panning.origin.x - dx / scale, y: panning.origin.y - dy / scale }
  }

  function onPointerUp(event: PointerEvent) {
    if (!panning) return
    const clicked = !panning.moved
    panning = null
    if (clicked) setZoom(zoom === null && fit < 1 ? 1 : null, stage && offsetIn(stage, event))
  }

  function closeOutside(event: MouseEvent) {
    if (!(event.target instanceof HTMLImageElement)) onclose()
  }

  function onKeyDown(event: KeyboardEvent) {
    const action = KEYS.get(eventCombo(event))
    if (action === 'zoom_in') setZoom(stepScale(scale, ZOOM_STEP))
    else if (action === 'zoom_out') setZoom(stepScale(scale, 1 / ZOOM_STEP))
    else if (action === 'zoom_fit') setZoom(null)
    else return
    event.preventDefault()
  }

  onMount(() => {
    let url = ''
    let cancelled = false
    if (item.exists) {
      image(item.entry)
        .then((data) => {
          if (cancelled) return
          bytes = data.byteLength
          url = URL.createObjectURL(new Blob([data], { type: 'image/png' }))
          src = url
        })
        .catch((error) => {
          if (!cancelled) failed = String(error)
        })
    }
    return () => {
      cancelled = true
      if (url) URL.revokeObjectURL(url)
    }
  })
</script>

<div
  class="viewer"
  class:preview
  role="dialog"
  aria-label={preview ? 'Preview' : 'Viewer'}
  tabindex="-1"
  onclick={closeOutside}
  onkeydown={onKeyDown}
  {@attach (node) => node.focus()}
>
  <div class="stage" bind:this={stage} onwheel={onWheel} bind:clientWidth={stageWidth} bind:clientHeight={stageHeight}>
    {#if src}
      <img
        {src}
        alt={title(entry)}
        draggable="false"
        class:zoomed={zoom !== null}
        style:left="{-at.x * scale}px"
        style:top="{-at.y * scale}px"
        style:width="{naturalWidth * scale}px"
        style:height="{naturalHeight * scale}px"
        style:visibility={naturalWidth ? 'visible' : 'hidden'}
        bind:naturalWidth
        bind:naturalHeight
        onpointerdown={onPointerDown}
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}
        onpointercancel={() => (panning = null)}
      />
    {:else}
      <p class="note">{failed || (item.exists ? '' : entry.url ? 'Uploaded only' : 'File is gone')}</p>
    {/if}
  </div>
  <footer>
    {#if preview}
      <strong>{title(entry)}</strong>
    {:else}
      {#if naturalWidth}<span>{naturalWidth} × {naturalHeight}</span>{/if}
      {#if src}<span>{megabytes(bytes)}</span>{/if}
    {/if}
    {#if src}<span class="zoom">{Math.round(scale * 100)} %</span>{/if}
    {#if !preview}<span class="position">{position} / {total}</span>{/if}
  </footer>
</div>

<style>
  .viewer {
    position: absolute;
    inset: 0;
    z-index: 10;
    display: flex;
    flex-direction: column;
    background: var(--bar);
    outline: none;
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .stage img {
    position: absolute;
    max-width: none;
    cursor: zoom-in;
    user-select: none;
    touch-action: none;
  }
  .stage img.zoomed {
    cursor: grab;
  }
  .stage img.zoomed:active {
    cursor: grabbing;
  }
  .note {
    margin: 48px 16px;
    color: var(--muted);
  }
  footer {
    display: flex;
    gap: 16px;
    align-items: center;
    padding: 10px 16px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  strong {
    color: var(--strong);
    font-weight: 600;
  }
  .position {
    margin-left: auto;
  }
  .preview {
    padding: 6vh 8vw;
    background: color-mix(in srgb, black 45%, transparent);
  }
  .preview .stage img {
    border-radius: 8px;
    box-shadow: 0 12px 40px color-mix(in srgb, black 50%, transparent);
  }
  .preview footer {
    justify-content: center;
    padding-top: 12px;
  }
  .preview strong,
  .preview .zoom {
    color: white;
  }
</style>
