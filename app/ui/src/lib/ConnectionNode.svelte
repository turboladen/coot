<script lang="ts">
  import type { ConnectionConfig } from "./api";
  import { conns, remove } from "./connections.svelte";
  import { databasesFor } from "./databases.svelte";
  import { ChevronDown, ChevronRight } from "./icons";
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
    <button class="name" title="Use this connection for the current tab" onclick={() => { setActiveConnection(conn.id); expandRoot(conn.id); }}>
      <span class="dot {status}" title={STATUS_TITLE[status]}></span>
      <span class="meta">
        <strong>{conn.name}</strong>
        <span class="server">{conn.server}</span>
      </span>
    </button>
    <div class="actions">
      <button onclick={() => onedit(conn)}>Edit</button>
      <button onclick={onDelete}>Delete</button>
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
  .actions {
    display: flex;
    gap: 0.3rem;
    flex: none;
  }
  .actions button { font-size: 0.78rem; cursor: pointer; }
</style>
