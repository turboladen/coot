<script lang="ts">
  import { tick } from "svelte";
  import type { ConnectionConfig } from "./api";
  import { conns, remove } from "./connections.svelte";
  import { clampMenuPosition } from "./contextMenuLogic";
  import { databasesFor } from "./databases.svelte";
  import { ChevronDown, ChevronRight, MoreHorizontal, Pencil, Trash2 } from "./icons";
  import { sidebar, toggleRoot, expandRoot } from "./sidebar.svelte";
  import { connectionStatus, type ConnStatus } from "./sidebarLogic";
  import { setActiveConnection } from "./tabs.svelte";
  import { refreshNonce } from "./tree/refresh.svelte";
  import ObjectTree from "./tree/ObjectTree.svelte";

  // billz-a5y.3: ONE connection as a collapsible root with its own object tree.
  // `lockedIds` is App's session-lock state (surfaced as the status dot + the tree's
  // locked empty-state); `onedit` opens the form; `onunlock` bubbles the locked
  // empty-state's "Enter password" up to App to prompt for THIS connection.
  let {
    conn,
    lockedIds,
    onedit,
    onunlock,
  }: {
    conn: ConnectionConfig;
    lockedIds: Set<string>;
    onedit: (cfg: ConnectionConfig) => void;
    onunlock: (id: string) => void;
  } = $props();

  const expanded = $derived(sidebar.expanded.has(conn.id));
  const locked = $derived(lockedIds.has(conn.id));
  // billz-a5y.5: honest passive dot — locked, or the per-connection store's own state.
  const status = $derived(connectionStatus({ locked, dbStatus: databasesFor(conn.id).status }));

  const STATUS_TITLE: Record<ConnStatus, string> = {
    locked: "Locked — session password needed",
    loaded: "Object tree loaded this session",
    loading: "Loading databases…",
    error: "Couldn't load databases",
    neutral: "Not loaded yet this session",
  };

  async function onDelete() {
    if (confirm(`Delete connection "${conn.name}"?`)) {
      await remove(conn.id);
    }
  }

  // billz-a5y.4: compact row controls. Hover/focus-within reveals the Edit/Delete/⋯
  // icon buttons; the ⋯ button (and right-click on the name) open a small context
  // menu. Menu state + the trigger/item refs are PER-INSTANCE ($state / bind:this),
  // so the window keydown handler must gate on THIS node's `menu` before acting —
  // otherwise every mounted node's listener would fire on any Escape and the last
  // one would steal focus to a random connection's ⋯ button.
  // TODO(billz-1hz): extract shared ContextMenu (this + TableNode's inline menu).
  const EST_MENU_W = 180; // ~= .ctx-menu min-width; for the pre-render clamp only.
  const EST_MENU_H = 88; // ~= two .ctx-item rows + padding.
  let menu = $state<{ x: number; y: number } | null>(null);
  let triggerEl: HTMLButtonElement | undefined = $state();
  let menuEl: HTMLElement | undefined = $state();
  let firstItemEl: HTMLButtonElement | undefined = $state();

  async function focusFirstItem() {
    await tick();
    firstItemEl?.focus();
  }

  // Right-click the connection name → menu at the cursor. Does NOT retarget the
  // active tab (contextmenu never fires onclick, and we preventDefault): the
  // billz-a5y.3 gesture split keeps retarget on a plain .name click only.
  function openAtCursor(e: MouseEvent) {
    e.preventDefault();
    menu = clampMenuPosition({
      x: e.clientX,
      y: e.clientY,
      menuW: EST_MENU_W,
      menuH: EST_MENU_H,
      viewportW: window.innerWidth,
      viewportH: window.innerHeight,
    });
    focusFirstItem();
  }

  // ⋯ button → the same menu, anchored right-aligned under the button. The
  // mouse-free keyboard path (Enter/Space activates the button).
  function openAtButton() {
    if (!triggerEl) return;
    const r = triggerEl.getBoundingClientRect();
    menu = clampMenuPosition({
      x: r.right - EST_MENU_W,
      y: r.bottom,
      menuW: EST_MENU_W,
      menuH: EST_MENU_H,
      viewportW: window.innerWidth,
      viewportH: window.innerHeight,
    });
    focusFirstItem();
  }

  function closeMenu(returnFocus: boolean) {
    menu = null;
    if (returnFocus) triggerEl?.focus();
  }

  // ArrowUp/Down roving between the two menu items (wraps). Guarded on `menu` so
  // only the node with an open menu reacts.
  function roveMenu(dir: 1 | -1) {
    if (!menuEl) return;
    const items = Array.from(menuEl.querySelectorAll<HTMLButtonElement>("button.ctx-item"));
    if (items.length === 0) return;
    const idx = items.indexOf(document.activeElement as HTMLButtonElement);
    // Focus not on a menu item yet (idx === -1): Down → first, Up → last.
    const next = idx < 0 ? (dir === 1 ? 0 : items.length - 1) : (idx + dir + items.length) % items.length;
    items[next].focus();
  }

  function onWindowKeydown(e: KeyboardEvent) {
    if (!menu) return; // PER-INSTANCE guard — see the menu comment above.
    if (e.key === "Escape") {
      e.preventDefault();
      closeMenu(true);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      roveMenu(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      roveMenu(-1);
    }
  }
</script>

<li class:active={conns.activeId === conn.id}>
  <!-- Three SEPARATE sibling buttons (no nesting): a chevron click physically
       cannot reach the name's retarget handler. billz-a5y.4 will compact
       Edit/Delete into hover/context-menu affordances; the structure is set up here. -->
  <div class="root-row">
    <!-- BROWSE: expand/collapse this root's tree (focus follows only on expand).
         Never touches conns.activeId — browsing must not retarget the current tab. -->
    <button
      class="twisty"
      aria-expanded={expanded}
      title={expanded ? "Collapse" : "Expand"}
      onclick={(e) => {
        e.stopPropagation();
        toggleRoot(conn.id);
      }}
    >
      {#if expanded}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}
    </button>
    <!-- RETARGET: point the active tab at this connection (billz-a5y.1's tab-owned
         retarget) + expand its tree. The explicit "use this connection" gesture. -->
    <button
      class="name"
      title="Use this connection for the current tab"
      onclick={() => { setActiveConnection(conn.id); expandRoot(conn.id); }}
      oncontextmenu={openAtCursor}
    >
      <span class="dot {status}" title={STATUS_TITLE[status]}></span>
      <span class="meta">
        <strong>{conn.name}</strong>
        <span class="server">{conn.server}</span>
      </span>
    </button>
    <!-- billz-a5y.4: compact overlay controls, revealed on hover/focus-within.
         The buttons stay in the tab order at rest (opacity:0, NOT display/visibility/
         tabindex=-1) ON PURPOSE — tabbing into a hidden button fires :focus-within,
         which is what reveals the cluster for keyboard users. Don't "optimize" the
         focusability away. Cost: 3 extra tab stops per row (accepted — the ⋯ button
         is the keyboard menu path; Shift+F10 is intentionally not wired). -->
    <div class="actions">
      <button title="Edit connection" aria-label="Edit connection" onclick={() => onedit(conn)}>
        <Pencil size={14} />
      </button>
      <button title="Delete connection" aria-label="Delete connection" onclick={onDelete}>
        <Trash2 size={14} />
      </button>
      <button
        bind:this={triggerEl}
        title="More actions"
        aria-label="More actions"
        aria-haspopup="menu"
        aria-expanded={menu !== null}
        onclick={openAtButton}
      >
        <MoreHorizontal size={14} />
      </button>
    </div>
  </div>

  {#if expanded}
    <!-- The key remounts the subtree on a Refresh bump (rqb.5 — drops node-local memos
         so the invalidated core cache re-queries). Per connection (billz-a5y.2) so one
         root's refresh never collapses another's. -->
    {#key `${conn.id}:${refreshNonce(conn.id)}`}
      <ObjectTree id={conn.id} {locked} {onunlock} />
    {/key}
  {/if}
</li>

<svelte:window onkeydown={onWindowKeydown} />

{#if menu}
  <button class="menu-backdrop" aria-label="Close menu" onclick={() => closeMenu(false)}></button>
  <div class="ctx-menu" role="menu" bind:this={menuEl} style="left: {menu.x}px; top: {menu.y}px;">
    <button
      class="ctx-item"
      role="menuitem"
      bind:this={firstItemEl}
      onclick={() => { closeMenu(false); onedit(conn); }}
    >
      <Pencil size={14} /> Edit
    </button>
    <button class="ctx-item" role="menuitem" onclick={() => { closeMenu(false); onDelete(); }}>
      <Trash2 size={14} /> Delete
    </button>
  </div>
{/if}

<style>
  li {
    list-style: none;
    border-radius: var(--r-sm);
    margin-bottom: 0.15rem;
  }
  li.active {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .root-row {
    position: relative; /* anchor for the absolute .actions overlay (billz-a5y.4) */
    display: flex;
    align-items: center;
    gap: 0.2rem;
    padding: 0.15rem 0.3rem;
  }
  .twisty {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.1rem;
    height: 1.1rem;
    flex: none;
    background: none;
    border: none;
    border-radius: var(--r-sm);
    color: var(--muted);
    cursor: pointer;
  }
  .twisty:hover { background: color-mix(in srgb, var(--brand) 8%, transparent); }
  .twisty :global(svg) { color: inherit; }
  .name {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    flex: 1;
    min-width: 0;
    /* Reset the global button base's justify-content:center (app.css). */
    justify-content: flex-start;
    padding: 0.15rem 0.3rem;
    background: none;
    border: none;
    border-radius: var(--r-sm);
    font: inherit;
    text-align: left;
    cursor: pointer;
    color: var(--text);
  }
  .name:hover { background: color-mix(in srgb, var(--brand) 8%, transparent); }
  li.active .name strong { font-weight: 700; }
  .meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .meta strong {
    font-size: 0.9rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .server {
    color: var(--muted);
    font-size: 0.78rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Honest passive status dot (billz-a5y.5) — within the locked token set. Four
     legible treatments; `loading` is a faint/transient variant of neutral. */
  .dot {
    display: inline-block;
    width: 0.6rem;
    height: 0.6rem;
    border-radius: var(--r-pill);
    flex: none;
  }
  .dot.loaded {
    background: var(--ok);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 22%, transparent);
  }
  .dot.locked {
    background: var(--warn);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--warn) 20%, transparent);
  }
  .dot.error {
    background: var(--danger);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--danger) 20%, transparent);
  }
  .dot.neutral {
    background: transparent;
    border: 1.5px solid var(--faint);
  }
  .dot.loading {
    background: color-mix(in srgb, var(--faint) 45%, transparent);
  }
  @media (prefers-reduced-motion: no-preference) {
    .dot.loading { animation: pulse 1.1s ease-in-out infinite; }
  }
  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 1; }
  }
  /* Compact overlay controls (billz-a5y.4). Absolutely positioned so at rest they
     reserve no layout width (no shift) and never underlap the name in flow; the
     --raised backing + shadow keep the icons legible over any name tail on hover. */
  .actions {
    position: absolute;
    right: 0.3rem;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    gap: 0.1rem;
    padding: 0.1rem;
    border-radius: var(--r-sm);
    background: var(--raised);
    box-shadow: var(--shadow-sm);
    /* Hidden at rest; revealed on hover OR keyboard focus-within. opacity (not
       display/visibility) keeps the buttons focusable so Tab reveals the cluster;
       pointer-events:none stops the invisible overlay intercepting name clicks. */
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--dur-fast) var(--ease);
  }
  .root-row:hover .actions,
  .root-row:focus-within .actions {
    opacity: 1;
    pointer-events: auto;
  }
  /* Reset the global button base (border + raised bg + wide padding) — compact
     icon-only buttons, mirroring .twisty. */
  .actions button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.5rem;
    height: 1.5rem;
    padding: 0;
    background: none;
    border: none;
    border-radius: var(--r-sm);
    color: var(--muted);
    cursor: pointer;
  }
  .actions button:hover { background: color-mix(in srgb, var(--brand) 12%, transparent); }
  .actions button :global(svg) { color: inherit; }

  /* Context menu — mirrors tree/TableNode.svelte's inline menu (same z-index
     layering). TODO(billz-1hz): extract a shared ContextMenu component. */
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: none;
    border: none;
    padding: 0;
    cursor: default;
  }
  .ctx-menu {
    position: fixed;
    z-index: 41;
    min-width: 11rem;
    background: var(--raised);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-md);
    padding: 0.25rem;
    font-size: 0.85rem;
  }
  .ctx-item {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    width: 100%;
    text-align: left;
    padding: 0.3rem 0.5rem;
    background: none;
    border: none;
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
    border-radius: var(--r-sm);
    color: var(--text);
  }
  .ctx-item :global(svg) { color: var(--muted); flex: none; }
  .ctx-item:hover,
  .ctx-item:focus-visible { background: color-mix(in srgb, var(--accent) 12%, transparent); }
</style>
