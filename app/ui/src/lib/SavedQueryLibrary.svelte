<script lang="ts">
  import type { SavedQuery } from "./api";
  import { Search } from "./icons";
  import { library, remove, save } from "./savedQueries.svelte";
  import { filterQueries, promoteToSavedQuery } from "./savedQueriesLogic";
  import { activeContent, newTabWithContent } from "./tabs.svelte";
  import { deriveTitle } from "./tabsLogic";

  // Search is component-local $state (avoids the cross-module $derived caveat noted
  // in tabs.svelte.ts): the filtered view derives from it + the shared library list.
  let search = $state("");
  const filtered = $derived(filterQueries(library.list, search));

  // Promote-current-tab: the inline name row is revealed by the button, pre-filled
  // with the active tab's derived title (mirrors ConnectionForm's field pattern —
  // window.prompt is unreliable in the Tauri v2 WKWebView).
  let promoting = $state(false);
  let name = $state("");

  function startPromote() {
    name = deriveTitle(activeContent());
    promoting = true;
  }
  function cancelPromote() {
    promoting = false;
  }
  async function confirmPromote() {
    const sql = activeContent();
    // Guard: nothing to promote (empty SQL) or no name.
    if (sql.trim() === "" || name.trim() === "") return;
    await save(promoteToSavedQuery(crypto.randomUUID(), name, sql, null));
    promoting = false;
  }

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

  // First non-empty line of the SQL, for a muted list preview.
  function preview(sql: string): string {
    return sql.split("\n").map((l) => l.trim()).find((l) => l.length > 0) ?? "";
  }
</script>

<div class="list">
  <!-- billz-a5y.8 nit#1: the panel's own header ("Library" in LibraryPanel) is the
       single header now — this component's redundant "Saved queries" h2 is gone.
       Promote-current-tab is the panel's primary action, given a full-width CTA. -->
  <button class="promote-btn" onclick={startPromote}>Promote current tab</button>

  {#if promoting}
    <div class="promote">
      <input placeholder="Query name" bind:value={name} />
      <div class="actions">
        <button onclick={confirmPromote}>Save</button>
        <button onclick={cancelPromote}>Cancel</button>
      </div>
    </div>
  {/if}

  <input class="search" placeholder="Search queries" bind:value={search} />

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
          <div class="actions">
            <button onclick={() => openSavedQuery(q)}>Open</button>
            <button onclick={() => onDelete(q)}>Delete</button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .list { padding: var(--sp-2); }
  /* Full-width promote CTA at the body top (billz-a5y.8 nit#1) — inherits the
     app.css outline-button system; kept secondary (not teal) so it doesn't compete
     with Run. */
  .promote-btn { width: 100%; margin-bottom: var(--sp-2); }
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
  .promote { display: flex; flex-direction: column; gap: 0.3rem; margin-bottom: 0.5rem; }
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
  .meta { display: flex; flex-direction: column; }
  .sql {
    color: var(--muted);
    font-size: 0.8rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions { display: flex; gap: 0.3rem; margin-top: 0.3rem; }
  button { font-size: 0.8rem; cursor: pointer; }
</style>
