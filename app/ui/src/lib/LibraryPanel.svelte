<script lang="ts">
  // Collapsible right panel housing the saved-query Library (billz-a5y.6). The
  // Library feeds query tabs, so it lives next to the workspace, not next to the
  // DB-object sidebar. Open/collapsed state + width persist via the layout store.
  // SavedQueryLibrary itself is mounted unchanged.
  import { ChevronLeft, ChevronRight } from "./icons";
  import { layout, setLibraryOpen, setLibraryWidth } from "./layout.svelte";
  import { MAX_LIBRARY_WIDTH, MIN_LIBRARY_WIDTH } from "./layoutLogic";
  import SavedQueryLibrary from "./SavedQueryLibrary.svelte";

  // Keyboard-resize step (px) for the divider's Arrow keys.
  const STEP = 16;

  // Live drag state. During a pointer drag we track the width locally so the panel
  // resizes smoothly WITHOUT persisting on every move; the store write (and thus
  // localStorage) happens once, on pointerup. `dragging` also kills the width
  // transition so the edge tracks the pointer 1:1.
  let dragging = $state(false);
  let dragWidth = $state(0);
  let startX = 0;
  let startWidth = 0;

  // The width the panel renders at: the live drag value mid-drag, else the stored
  // one. CSS additionally caps it against the viewport (see the style block) so a
  // stored value can never overflow a small window.
  const renderWidth = $derived(dragging ? dragWidth : layout.libraryWidth);

  // billz-a5y.8 nit#2: the divider's aria-valuenow must reflect the ACTUAL rendered
  // width, not the stored value — on a narrow window the CSS `min()` cap shrinks the
  // panel below the stored width. `bind:clientWidth` measures the real width (already
  // capped, auto-updates on resize); before the first measurement fall back to
  // renderWidth. Clamped into [MIN, MAX] so the reported value stays within the
  // separator's advertised range even when the viewport caps it below MIN.
  let panelW = $state(0);
  const effectiveWidth = $derived(panelW > 0 ? panelW : renderWidth);
  const ariaWidth = $derived(
    Math.round(Math.min(MAX_LIBRARY_WIDTH, Math.max(MIN_LIBRARY_WIDTH, effectiveWidth))),
  );

  function onDragStart(e: PointerEvent) {
    e.preventDefault();
    dragging = true;
    startX = e.clientX;
    startWidth = layout.libraryWidth;
    dragWidth = layout.libraryWidth;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onDragMove(e: PointerEvent) {
    if (!dragging) return;
    // The divider is on the panel's LEFT edge; dragging left (clientX decreases)
    // widens the right-anchored panel. Clamp to the drag range for live feedback;
    // the store applies the same MIN/MAX + viewport clamp on commit.
    const next = startWidth + (startX - e.clientX);
    dragWidth = Math.min(MAX_LIBRARY_WIDTH, Math.max(MIN_LIBRARY_WIDTH, next));
  }

  function onDragEnd(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
    setLibraryWidth(dragWidth);
  }

  // Keyboard resize on the separator: ArrowLeft grows the panel (edge moves left),
  // ArrowRight shrinks it. Discrete steps persist immediately via the store.
  function onDividerKey(e: KeyboardEvent) {
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      setLibraryWidth(layout.libraryWidth + STEP);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      setLibraryWidth(layout.libraryWidth - STEP);
    }
  }
</script>

<aside
  class="library-panel"
  class:collapsed={!layout.libraryOpen}
  class:dragging
  style="--lib-width: {renderWidth}px"
  bind:clientWidth={panelW}
>
  {#if layout.libraryOpen}
    <!-- Drag divider on the panel's left edge. Keyboard-resizable (Arrow keys).
         The focusable resize separator IS the WAI-ARIA window-splitter widget
         pattern, but Svelte's a11y lint models role="separator" as non-interactive
         and so flags the (correct) tabindex + handlers — silence the two false
         positives rather than drop them. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="divider"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize library panel"
      aria-valuemin={MIN_LIBRARY_WIDTH}
      aria-valuemax={MAX_LIBRARY_WIDTH}
      aria-valuenow={ariaWidth}
      tabindex="0"
      onpointerdown={onDragStart}
      onpointermove={onDragMove}
      onpointerup={onDragEnd}
      onpointercancel={onDragEnd}
      onkeydown={onDividerKey}
    ></div>
    <div class="panel-inner">
      <div class="panel-header">
        <span class="panel-title">Library</span>
        <button
          class="collapse-btn"
          onclick={() => setLibraryOpen(false)}
          aria-expanded={true}
          aria-controls="library-panel-body"
          aria-label="Collapse library"
          title="Collapse library"
        ><ChevronRight size={16} /></button>
      </div>
      <div id="library-panel-body" class="panel-body">
        <SavedQueryLibrary />
      </div>
    </div>
  {:else}
    <!-- Collapsed rail: a full-height expand affordance. No aria-controls — the
         panel body only exists in the open branch, so it would dangle here;
         aria-expanded=false already conveys the collapsed state. -->
    <button
      class="rail"
      onclick={() => setLibraryOpen(true)}
      aria-expanded={false}
      aria-label="Expand library"
      title="Expand library"
    >
      <ChevronLeft size={16} />
      <span class="rail-label">Library</span>
    </button>
  {/if}
</aside>

<style>
  .library-panel {
    /* The `auto` third grid track sizes to this. `min()` caps the width against
       the viewport (sidebar 20rem + a 3rem workspace floor) so a wide stored value
       never overflows <main> at any window size — no resize listener needed. */
    width: min(var(--lib-width), calc(100vw - 20rem - 3rem));
    display: flex;
    /* overflow + min-width:0 so a long saved-query name can't widen the auto
       track past --lib-width (same guard <section> uses). */
    overflow: hidden;
    min-width: 0;
    border-left: 1px solid var(--border);
    background: var(--panel);
    transition: width var(--dur-fast) var(--ease);
  }
  /* Mid-drag: track the pointer 1:1, no easing. */
  .library-panel.dragging {
    transition: none;
  }
  .library-panel.collapsed {
    width: 2.25rem;
  }
  .divider {
    flex: none;
    width: 5px;
    margin-right: -2px; /* overlap the border so the grab target straddles the edge */
    cursor: col-resize;
    background: transparent;
    transition: background var(--dur-fast) var(--ease);
    z-index: 1;
  }
  .divider:hover {
    background: color-mix(in srgb, var(--accent) 40%, transparent);
  }
  /* Keyboard focus is a STRONGER, wider accent bar than hover so it's clearly
     distinct (billz-a5y.8) — the bar is the focus indicator that replaces the
     global ring (which reads oddly on a 5px-wide splitter). */
  .divider:focus-visible {
    background: var(--accent);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 45%, transparent);
    outline: none;
  }
  .panel-inner {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
  }
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-2);
    padding: var(--sp-2) var(--sp-3);
    border-bottom: 1px solid var(--border);
  }
  .panel-title {
    font-weight: 600;
    font-size: var(--fs-sm);
    color: var(--text);
    letter-spacing: -0.01em;
  }
  .collapse-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.2rem 0.3rem;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--faint);
    cursor: pointer;
    transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
  }
  .collapse-btn:hover {
    color: var(--muted);
    background: color-mix(in srgb, var(--brand) 8%, transparent);
  }
  .collapse-btn :global(svg) { color: inherit; }
  .panel-body {
    flex: 1;
    min-height: 0;
    min-width: 0;
    overflow: auto;
  }
  /* Collapsed rail: an icon-over-vertical-label button spanning the full height. */
  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-2);
    width: 100%;
    padding: var(--sp-3) 0;
    border: none;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
  }
  .rail:hover { background: var(--raised); color: var(--text); }
  .rail :global(svg) { color: inherit; flex: none; }
  .rail-label {
    writing-mode: vertical-rl;
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.02em;
  }
</style>
