<script lang="ts">
  import { tick } from "svelte";
  import type { SavedQuery } from "./api";
  import { Pencil, Search, Trash2 } from "./icons";
  import NameDialog from "./NameDialog.svelte";
  import { library, remove, save } from "./savedQueries.svelte";
  import { filterQueries, planRename, renameSeed } from "./savedQueriesLogic";
  import { newTabWithContent } from "./tabs.svelte";
  import { pushToast } from "./toasts.svelte";

  // Search is component-local $state (avoids the cross-module $derived caveat noted
  // in tabs.svelte.ts): the filtered view derives from it + the shared library list.
  let search = $state("");
  let searchInput = $state<HTMLInputElement>();
  const filtered = $derived(filterQueries(library.list, search));

  // Open = SQL into a fresh tab LINKED to this saved query (d28.3: savedQueryId
  // drives the param bar). Passes the query's target database too.
  function openSavedQuery(q: SavedQuery) {
    newTabWithContent(q.sql, q.targetDatabase, q.id);
  }

  async function onDelete(q: SavedQuery) {
    if (confirm(`Delete saved query "${q.name}"?`)) {
      await remove(q.id);
    }
  }

  // billz-1kn: rename. Reuses NameDialog (built generic in billz-he0 for exactly
  // this) rather than a second name UI — window.prompt is unreliable in the Tauri
  // v2 WKWebView, and two name flows drift apart.
  //
  // Only the ID is captured when the dialog opens; the row itself is re-read from
  // the live list at submit time (planRename), so a rename can't write back stale
  // SQL. `trigger` is the row's Rename button — the dialog steals focus and must
  // hand it back.
  type RenameTarget = { id: string; seed: string; trigger: HTMLElement | null };
  let renaming = $state<RenameTarget | null>(null);

  function openRename(q: SavedQuery, e: MouseEvent) {
    renaming = { id: q.id, seed: renameSeed(q), trigger: e.currentTarget as HTMLElement };
  }

  async function restoreFocus(trigger: HTMLElement | null) {
    // `await tick()` is load-bearing: after a rename the row can drop out of
    // `filtered` (the search box matched only the OLD name), which removes the
    // keyed <li> and this trigger. That removal lands a microtask AFTER save()
    // resolves, so testing isConnected any earlier reads a node that's about to
    // vanish — and focus falls to <body>, the exact thing this guards against.
    await tick();
    if (trigger?.isConnected) trigger.focus();
    else searchInput?.focus();
  }

  async function cancelRename() {
    const trigger = renaming?.trigger ?? null;
    renaming = null;
    await restoreFocus(trigger);
  }

  async function submitRename(name: string) {
    const target = renaming;
    renaming = null; // unmount now; focus is restored once the write settles
    if (!target) return;
    const plan = planRename(library.list, target.id, name);
    if (plan.kind === "missing") {
      pushToast("error", "That saved query no longer exists.");
    } else if (plan.kind === "write") {
      // Both toasts name the NEW name, never the old one: on the entries this bead
      // exists for, the old name IS the query — a truncated SQL fragment, or a
      // whole pasted statement — and error toasts are sticky, so quoting it would
      // park unreadable SQL in the corner of the app until it's dismissed by hand.
      try {
        await save(plan.query);
        pushToast("success", `Renamed to "${plan.query.name}".`);
      } catch (e) {
        // TODO(billz-sjn): when save() gains a distinguishable write-ok/refresh-failed
        // signal, this needs a second branch — a throw will no longer imply "nothing
        // was written". No de-duplication here; billz-667 coalesces repeats.
        pushToast("error", `Couldn't rename to "${plan.query.name}": ${String(e)}`);
      }
    }
    await restoreFocus(target.trigger);
  }

  // First non-empty line of the SQL, for a muted list preview.
  function preview(sql: string): string {
    return sql.split("\n").map((l) => l.trim()).find((l) => l.length > 0) ?? "";
  }
</script>

<div class="list">
  <!-- billz-a5y.8 nit#1: the panel's own header ("Library" in LibraryPanel) is the
       single header now — this component's redundant "Saved queries" h2 is gone.
       billz-he0: "Promote current tab" is gone too. Saving is a PUSH from the
       editor toolbar (next to the SQL it saves), so this panel is purely a BROWSER
       of saved queries — one save path, not two to keep in sync. -->
  <input
    class="search"
    placeholder="Search queries"
    bind:value={search}
    bind:this={searchInput}
  />

  {#if library.list.length === 0}
    <div class="empty">
      <Search size={20} />
      <p>No saved queries yet.</p>
    </div>
  {:else}
    <ul>
      {#each filtered as q (q.id)}
        <li>
          <div class="meta">
            <strong>{q.name}</strong>
            <span class="sql">{preview(q.sql)}</span>
          </div>
          <!-- Open stays the one always-visible control (the primary verb, and a
               row with no visible action reads as inert). Rename/Delete are compact
               icons revealed on hover/focus-within — billz-a5y.4's connection-row
               pattern, which exists so a third peer button doesn't turn every row
               into a button bar and so a destructive Delete isn't equal in weight
               to the hot path. No context menu: two icons don't need one, and
               hand-rolling a third copy of the fixed-position menu is what
               TODO(billz-1hz) exists to prevent. -->
          <div class="actions">
            <button onclick={() => openSavedQuery(q)}>Open</button>
            <div class="icons">
              <button
                title="Rename saved query"
                aria-label="Rename saved query"
                onclick={(e) => openRename(q, e)}
              >
                <Pencil size={14} />
              </button>
              <button
                title="Delete saved query"
                aria-label="Delete saved query"
                onclick={() => onDelete(q)}
              >
                <Trash2 size={14} />
              </button>
            </div>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<!-- billz-1kn rename prompt. The seed is the current name, or a cleaned-up
     suggestion derived from the SQL when the "name" is really the whole query
     (pre-billz-he0 entries) — see renameSeed.

     TODO(billz-ppw): App's window-level ⌘S handler doesn't know this dialog
     exists — it guards only App's OWN modal state — so ⌘S while renaming can
     mount a SECOND NameDialog over this one, or silently write back a linked
     tab. The fix belongs in App.svelte. -->
{#if renaming}
  <NameDialog
    title="Rename saved query"
    label="Query name"
    value={renaming.seed}
    submitLabel="Rename"
    onsubmit={submitRename}
    oncancel={cancelRename}
  />
{/if}

<style>
  .list { padding: var(--sp-2); }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--sp-2);
    padding: var(--sp-5) var(--sp-2);
    color: var(--muted);
    font-size: 0.9rem;
    text-align: center;
  }
  .empty :global(svg) {
    color: var(--faint);
  }
  .empty p {
    margin: 0;
  }
  .search { width: 100%; margin-bottom: 0.5rem; box-sizing: border-box; }
  input {
    font-size: 0.85rem;
    padding: 0.2rem 0.3rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--raised);
    color: var(--text);
  }
  ul { list-style: none; margin: 0; padding: 0; }
  li {
    padding: 0.5rem;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    margin-bottom: 0.4rem;
    transition: background var(--dur-fast) var(--ease);
  }
  li:hover {
    background: color-mix(in srgb, var(--brand) 8%, transparent);
  }
  /* min-width:0 so the name's ellipsis can actually engage inside the flex column. */
  .meta { display: flex; flex-direction: column; min-width: 0; }
  /* billz-1kn: the NAME needs the same overflow guard .sql already had — a
     pre-billz-he0 entry's name is the whole statement, which wrapped into a
     multi-line wall. Mirrors billz-6s0's tree-name ellipsis fix. */
  .meta strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sql {
    color: var(--muted);
    font-size: 0.8rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions { display: flex; align-items: center; gap: 0.3rem; margin-top: 0.3rem; }
  button { font-size: 0.8rem; cursor: pointer; }
  /* Icon cluster (billz-1kn), mirroring ConnectionNode's .actions: pushed to the
     row's trailing edge so it reads as secondary rather than as a third peer
     control next to Open. `pointer-events` is paired with `opacity` deliberately —
     an opacity:0 button is still hit-testable, so without it a pointer that lands
     inside Delete without a preceding hover (scrolling under a stationary cursor,
     the panel opening under it) would fire an INVISIBLE Delete. opacity (not
     display/visibility) keeps the buttons focusable, so Tab reveals the cluster
     via :focus-within for keyboard users. */
  .icons {
    display: flex;
    gap: 0.1rem;
    margin-left: auto;
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--dur-fast) var(--ease);
  }
  li:hover .icons,
  li:focus-within .icons {
    opacity: 1;
    pointer-events: auto;
  }
  /* Reset the app.css global button base (border + --raised bg + 0.3rem 0.7rem
     padding) for the icon buttons ONLY — scoped to .icons so Open keeps its
     outline-button chrome. */
  .icons button {
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
  .icons button:hover { background: color-mix(in srgb, var(--brand) 12%, transparent); }
  .icons button :global(svg) { color: inherit; }
</style>
