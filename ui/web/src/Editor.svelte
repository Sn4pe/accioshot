<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { onMount } from 'svelte'
  import Canvas from './editor/Canvas.svelte'
  import { discard, editorSettings, loadState, readImage } from './ipc'
  import { eventCombo, keymap, type Action } from './lib/keys'
  import type { Tool } from './lib/model'
  import { isTool, type EditorSettings, type Style } from './lib/tools'

  type Session = {
    settings: EditorSettings
    image: ImageBitmap
    keys: Map<string, Action>
    tool: Tool
    styles: Partial<Record<Tool, Style>>
  }

  const CLOSE_HINT = 'Close this window, the original capture is kept in the captures folder.'

  let session = $state.raw<Session | null>(null)
  let failure = $state('')
  let canvas = $state<ReturnType<typeof Canvas>>()

  async function load() {
    try {
      const settings = await editorSettings()
      const keys = keymap(settings.keys)
      let tool: Tool = isTool(settings.tool) ? settings.tool : 'Arrow'
      let styles: Partial<Record<Tool, Style>> = {}
      if (settings.remember_last) {
        const state = await loadState()
        if (isTool(state?.tool)) tool = state.tool
        if (state?.styles) styles = state.styles
      }
      const image = await createImageBitmap(new Blob([await readImage()], { type: 'image/png' }))
      session = { settings, image, keys, tool, styles }
    } catch (error) {
      failure = `${error}. ${CLOSE_HINT}`
    }
  }

  async function close() {
    try {
      await discard(false)
    } catch (error) {
      failure = String(error)
    }
  }

  function requestClose() {
    if (canvas) canvas.requestDiscard()
    else void close()
  }

  function onKeyDown(event: KeyboardEvent) {
    if (!session && eventCombo(event) === 'escape') void close()
  }

  onMount(() => {
    const unlisten = getCurrentWindow().onCloseRequested((event) => {
      event.preventDefault()
      requestClose()
    })
    void load()
    return () => void unlisten.then((stop) => stop())
  })
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="editor">
  {#if session}
    <Canvas bind:this={canvas} {...session} />
  {:else}
    <main>
      {#if failure}
        <p class="failure" role="alert">{failure}</p>
      {:else}
        <p class="loading">Loading capture</p>
      {/if}
    </main>
  {/if}
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  main {
    flex: 1;
    display: grid;
    place-items: center;
    padding: 16px 24px;
  }
  .loading {
    margin: 0;
    color: var(--muted);
  }
  .failure {
    margin: 0;
    padding: 10px 14px;
    background: var(--failure-bg);
    color: var(--failure-text);
    border-radius: 4px;
    user-select: text;
  }
</style>
