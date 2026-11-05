<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  let { name }: { name: string } = $props()
</script>

<svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
  {#if name === 'Select'}
    <path class="fill" d="M5 2.5v13.2l3.5-3.1 2.3 5 2.2-1-2.3-4.9 4.6-.4z" />
  {:else if name === 'Rectangle'}
    <rect x="3" y="5" width="14" height="10" />
  {:else if name === 'Ellipse'}
    <ellipse cx="10" cy="10" rx="7.5" ry="5.5" />
  {:else if name === 'Line'}
    <path d="M3.5 16.5l13-13" />
  {:else if name === 'Arrow'}
    <path d="M3.5 16.5L13 7" />
    <path class="fill" d="M17 3l-1.6 7.2-5.6-5.6z" />
  {:else if name === 'Freehand'}
    <path d="M2.5 13c2-4.5 3.8-7.5 5.2-7 2 .7-2.2 8 0 8.6 1.9.5 2.9-5 4.8-4.6 1.5.3.3 3.6 1.7 4 .9.2 2-.6 3.3-2" />
  {:else if name === 'Text'}
    <path class="fill" d="M4 3.5h12v3.6h-1.2c-.2-1.6-.7-2.3-2.3-2.3h-1.4v9.9c0 .9.3 1.2 1.6 1.3v1H7.3v-1c1.3-.1 1.6-.4 1.6-1.3V4.8H7.5c-1.6 0-2.1.7-2.3 2.3H4z" />
  {:else if name === 'Step'}
    <circle class="fill" cx="10" cy="10" r="7.5" />
    <path class="cut" d="M8.2 7.2L10.6 5.8V14.5" />
  {:else if name === 'Highlight'}
    <path d="M7 12.5l6.5-9 3.5 2.5-6.5 9-3.8.2z" />
    <path class="fill" d="M2.5 17.5h15v-1.2h-15z" />
  {:else if name === 'Blur'}
    <g class="fill">
      <circle cx="4" cy="4" r="1" /><circle cx="8" cy="4" r="1.4" /><circle cx="12" cy="4" r="1.4" /><circle cx="16" cy="4" r="1" />
      <circle cx="4" cy="8" r="1.4" /><circle cx="8" cy="8" r="2" /><circle cx="12" cy="8" r="2" /><circle cx="16" cy="8" r="1.4" />
      <circle cx="4" cy="12" r="1.4" /><circle cx="8" cy="12" r="2" /><circle cx="12" cy="12" r="2" /><circle cx="16" cy="12" r="1.4" />
      <circle cx="4" cy="16" r="1" /><circle cx="8" cy="16" r="1.4" /><circle cx="12" cy="16" r="1.4" /><circle cx="16" cy="16" r="1" />
    </g>
  {:else if name === 'Pixelate'}
    <path class="fill" d="M3 3h4.6v4.6H3zM12.4 3H17v4.6h-4.6zM7.7 7.7h4.6v4.6H7.7zM3 12.4h4.6V17H3zM12.4 12.4H17V17h-4.6z" />
  {:else if name === 'Crop'}
    <path d="M6 2v12h12" />
    <path d="M2 6h12v12" />
  {:else if name === 'undo'}
    <path d="M7.5 4L3.5 8l4 4" />
    <path d="M3.5 8h8.2a4.3 4.3 0 010 8.6H8.5" />
  {:else if name === 'redo'}
    <path d="M12.5 4l4 4-4 4" />
    <path d="M16.5 8H8.3a4.3 4.3 0 000 8.6h3.2" />
  {/if}
</svg>

<style>
  svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linecap: square;
  }
  .fill,
  .fill :global(circle) {
    fill: currentColor;
    stroke: none;
  }
  .cut {
    stroke: #2a2621;
  }
</style>
