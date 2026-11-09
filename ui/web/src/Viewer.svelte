<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { untrack } from 'svelte'
  import { entryKey, fileName, title, type Item } from './lib/library'
  import type { Point } from './lib/model'
  import { centered, clampOrigin, fitScale, stepScale, zoomAt } from './lib/viewport'

  let {
    item,
    revision,
    position,
    total,
    preview,
    onclose,
  }: { item: Item; revision: number; position: number; total: number; preview: boolean; onclose: () => void } = $props()

  const MAX_ZOOM = 16
  const ZOOM_STEP = 1.25
  const WHEEL_STEP = 100
  const WHEEL_LINE = 33
  const DRAG_SLOP = 3

  let src = $state('')
  let bytes = $state(0)
  let natural = $state({ width: 0, height: 0 })
  let failed = $state('')
  let stageWidth = $state(0)
  let stageHeight = $state(0)
  let zoom = $state<number | null>(null)
  let origin = $state<Point | null>(null)
  let stage = $state<HTMLElement>()
  let panning: { start: Point; origin: Point; moved: boolean } | null = null

  const entry = $derived(item.entry)
  const key = $derived(`${revision}:${entryKey(item.entry)}`)
  const margin = $derived(preview ? 0 : 16)
  const view = $derived({ x: 0, y: 0, width: natural.width, height: natural.height })
  const stageSize = $derived({ width: stageWidth, height: stageHeight })
  const fit = $derived(natural.width ? fitScale(view, stageSize, margin) : 1)
  const scale = $derived(zoom ?? fit)
  const at = $derived(clampOrigin(origin ?? centered(view, stageSize, scale), view, stageSize, scale, margin))

  function megabytes(count: number): string {
    return count >= 1024 * 1024 ? `${(count / 1024 / 1024).toFixed(1)} MB` : `${Math.round(count / 1024)} KB`
  }

  function screenAt(event: MouseEvent): Point {
    const rect = stage!.getBoundingClientRect()
    return { x: event.clientX - rect.left, y: event.clientY - rect.top }
  }

  function setZoom(next: number | null, screen: Point = { x: stageWidth / 2, y: stageHeight / 2 }) {
    const least = preview ? fit : fit / 4
    const limited = next === null ? null : Math.min(Math.max(next, least), MAX_ZOOM)
    const crossed = limited !== null && zoom !== null && (limited - fit) * (zoom - fit) < 0
    if (limited === null || crossed || Math.abs(limited - fit) <= fit * 0.001) {
      zoom = null
      origin = null
      return
    }
    origin = zoomAt(at, scale, limited, screen)
    zoom = limited
  }

  function onWheel(event: WheelEvent) {
    if (!natural.width) return
    event.preventDefault()
    const line = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? WHEEL_LINE : 1
    if (event.ctrlKey || event.metaKey) {
      setZoom(scale * ZOOM_STEP ** ((-event.deltaY * line) / WHEEL_STEP), screenAt(event))
      return
    }
    const sideways = event.shiftKey && !event.deltaX
    const dx = (sideways ? event.deltaY : event.deltaX) * line
    const dy = (sideways ? 0 : event.deltaY) * line
    origin = { x: at.x + dx / scale, y: at.y + dy / scale }
  }

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) return
    event.stopPropagation()
    ;(event.currentTarget as Element).setPointerCapture(event.pointerId)
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
    if (clicked) setZoom(zoom === null && fit < 1 ? 1 : null, screenAt(event))
  }

  function onKeyDown(event: KeyboardEvent) {
    if (!(event.ctrlKey || event.metaKey)) return
    if (event.key === '=' || event.key === '+') setZoom(stepScale(scale, ZOOM_STEP))
    else if (event.key === '-') setZoom(stepScale(scale, 1 / ZOOM_STEP))
    else if (event.key === '0') setZoom(null)
    else return
    event.preventDefault()
  }

  $effect(() => {
    if (!stage) return
    const target = stage
    target.addEventListener('wheel', onWheel, { passive: false })
    return () => target.removeEventListener('wheel', onWheel)
  })

  $effect(() => {
    void key
    const current = untrack(() => item)
    let url = ''
    let cancelled = false
    src = ''
    failed = ''
    natural = { width: 0, height: 0 }
    zoom = null
    origin = null
    if (current.exists) {
      invoke<ArrayBuffer>('image', { entry: current.entry })
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

<svelte:window onkeydown={onKeyDown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="viewer" class:preview role="dialog" aria-label={preview ? 'Preview' : 'Viewer'} tabindex="-1" onclick={onclose}>
  <div class="stage" bind:this={stage} bind:clientWidth={stageWidth} bind:clientHeight={stageHeight}>
    {#if src}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <img
        {src}
        alt={title(entry)}
        draggable="false"
        class:zoomed={zoom !== null}
        style:left="{-at.x * scale}px"
        style:top="{-at.y * scale}px"
        style:width="{natural.width * scale}px"
        style:height="{natural.height * scale}px"
        style:visibility={natural.width ? 'visible' : 'hidden'}
        onload={(event) => {
          const image = event.currentTarget as HTMLImageElement
          natural = { width: image.naturalWidth, height: image.naturalHeight }
        }}
        onclick={(event) => event.stopPropagation()}
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
      <strong>{entry.path ? fileName(entry.path) : title(entry)}</strong>
    {:else}
      {#if natural.width}<span>{natural.width} × {natural.height}</span>{/if}
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
