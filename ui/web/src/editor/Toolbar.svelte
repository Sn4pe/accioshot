<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import Glyph from '../Glyph.svelte'
  import { shortcutFor, type Action } from '../lib/keys'
  import type { Tool } from '../lib/model'
  import { COLORED, SIZE_RANGES, TOOLS, type Style } from '../lib/tools'

  let {
    tool,
    target,
    style,
    colors,
    keys,
    canUndo,
    canRedo,
    scale,
    zoomed,
    saving,
    confirmingDiscard,
    ontool,
    onstyle,
    onundo,
    onredo,
    onfit,
    ondiscard,
    onkeep,
    onconfirm,
    onfinish,
  }: {
    tool: Tool
    target: Tool
    style: Style
    colors: string[]
    keys: Map<string, Action>
    canUndo: boolean
    canRedo: boolean
    scale: number
    zoomed: boolean
    saving: boolean
    confirmingDiscard: boolean
    ontool: (tool: Tool) => void
    onstyle: (change: Partial<Style>, final: boolean) => void
    onundo: () => void
    onredo: () => void
    onfit: () => void
    ondiscard: () => void
    onkeep: () => void
    onconfirm: () => void
    onfinish: (command: 'save' | 'copy') => void
  } = $props()

  const sizeRange = $derived(SIZE_RANGES[target])
  const percent = $derived(Math.round(scale * 100))

  function shortcut(action: Action) {
    return shortcutFor(keys, action)
  }
</script>

<header>
  <div class="tools" role="toolbar" aria-label="Tools">
    {#each TOOLS as entry (entry)}
      <button
        class="tool"
        class:active={tool === entry}
        aria-pressed={tool === entry}
        aria-label={entry}
        title={entry + shortcut(entry)}
        onclick={() => ontool(entry)}
      >
        <Glyph name={entry} />
      </button>
    {/each}
  </div>

  <div class="style">
    {#if COLORED.includes(target)}
      <div class="swatches" role="group" aria-label="Color">
        {#each new Set(colors) as color (color)}
          <button
            class="swatch"
            class:current={style.color === color}
            style:background={color}
            aria-label={color}
            aria-pressed={style.color === color}
            onclick={() => onstyle({ color }, true)}
          ></button>
        {/each}
      </div>
    {/if}
    {#if sizeRange}
      <label class="size">
        <span>{target === 'Blur' || target === 'Pixelate' ? 'Strength' : 'Size'}</span>
        <input
          type="range"
          min={sizeRange[0]}
          max={sizeRange[1]}
          value={style.size}
          oninput={(event) => onstyle({ size: event.currentTarget.valueAsNumber }, false)}
          onchange={(event) => onstyle({ size: event.currentTarget.valueAsNumber }, true)}
        />
        <output>{style.size}</output>
      </label>
    {/if}
  </div>

  <div class="history">
    <button class="tool" aria-label="Undo" title={'Undo' + shortcut('undo')} disabled={!canUndo} onclick={onundo}>
      <Glyph name="undo" />
    </button>
    <button class="tool" aria-label="Redo" title={'Redo' + shortcut('redo')} disabled={!canRedo} onclick={onredo}>
      <Glyph name="redo" />
    </button>
    <button
      class="tool zoom"
      aria-label="{percent}%, fit to the window"
      title={'Fit to the window' + shortcut('zoom_fit')}
      disabled={!zoomed}
      onclick={onfit}
    >
      {percent}%
    </button>
  </div>

  <div class="finish">
    {#if confirmingDiscard}
      <span class="question">Discard your changes?</span>
      <button class="quiet" onclick={onkeep}>Keep editing</button>
      <button class="primary danger" onclick={onconfirm}>Discard</button>
    {:else}
      <button class="quiet" onclick={ondiscard}>Discard</button>
      <button
        class="quiet"
        title={'Copy to the clipboard and close, without saving' + shortcut('copy')}
        disabled={saving}
        onclick={() => onfinish('copy')}
      >
        Copy
      </button>
      <button class="primary" title={'Save' + shortcut('save')} disabled={saving} onclick={() => onfinish('save')}>
        Save
      </button>
    {/if}
  </div>
</header>

<style>
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 20px;
    padding: 6px 12px;
    background: var(--bar);
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
  .tool.zoom {
    width: auto;
    min-width: 56px;
    padding: 0 6px;
    font-variant-numeric: tabular-nums;
  }
  .tool {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 4px;
    color: var(--muted);
    transition:
      color 100ms,
      background-color 100ms;
  }
  .tool:hover:not(:disabled) {
    color: var(--text);
  }
  .tool.active {
    background: var(--active);
    color: var(--strong);
  }
  .tool:disabled {
    color: var(--disabled);
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
    outline: 2px solid var(--strong);
    outline-offset: 2px;
  }
  .size {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }
  .size input {
    width: 110px;
    margin: 0;
    accent-color: var(--soft);
  }
  .size output {
    min-width: 3ch;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }
  .question {
    margin-right: 4px;
  }
</style>
