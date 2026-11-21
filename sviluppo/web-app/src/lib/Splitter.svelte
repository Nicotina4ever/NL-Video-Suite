<script lang="ts">
  // Draggable split handle. Keeps keyboard parity: arrows move by 1 percent
  // (Shift by 5), exactly like the suite's existing workspace dividers.
  let {
    axis = 'x',
    fraction,
    onchange,
    label,
  }: {
    axis?: 'x' | 'y';
    fraction: number;
    onchange: (value: number) => void;
    label: string;
  } = $props();

  let host = $state<HTMLDivElement | null>(null);
  let dragging = $state(false);

  function point(event: PointerEvent): number | null {
    if (!host?.parentElement) return null;
    const rect = host.parentElement.getBoundingClientRect();
    const raw =
      axis === 'x'
        ? (event.clientX - rect.left) / rect.width
        : (event.clientY - rect.top) / rect.height;
    return Math.min(0.9, Math.max(0.1, raw));
  }

  function down(event: PointerEvent) {
    dragging = true;
    (event.target as Element).setPointerCapture(event.pointerId);
  }
  function move(event: PointerEvent) {
    if (!dragging) return;
    const value = point(event);
    if (value !== null) onchange(value);
  }
  function up() {
    dragging = false;
  }
  function keydown(event: KeyboardEvent) {
    const step = event.shiftKey ? 0.05 : 0.01;
    const forward = axis === 'x' ? ['ArrowRight'] : ['ArrowDown'];
    const back = axis === 'x' ? ['ArrowLeft'] : ['ArrowUp'];
    if (forward.includes(event.key)) onchange(Math.min(0.9, fraction + step));
    else if (back.includes(event.key)) onchange(Math.max(0.1, fraction - step));
    else return;
    event.preventDefault();
  }
</script>

<!-- The ARIA separator pattern requires focus + arrow keys (see Splitter docs). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="splitter"
  data-axis={axis}
  class:dragging
  role="separator"
  aria-orientation={axis === 'x' ? 'vertical' : 'horizontal'}
  aria-label={label}
  aria-valuenow={Math.round(fraction * 100)}
  aria-valuemin={10}
  aria-valuemax={90}
  tabindex="0"
  bind:this={host}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onkeydown={keydown}
></div>

<style>
  .splitter {
    background: var(--line);
    flex: none;
    cursor: col-resize;
    position: relative;
  }
  .splitter[data-axis='y'] { cursor: row-resize; }
  .splitter[data-axis='x'] { width: 5px; }
  .splitter[data-axis='y'] { height: 5px; }
  .splitter:hover, .splitter.dragging, .splitter:focus-visible { background: var(--accent); }
</style>
