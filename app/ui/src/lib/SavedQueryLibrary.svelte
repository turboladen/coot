<script lang="ts">
  import type { SavedQuery } from "./api";
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

  // Open = SQL into a fresh scratch tab. Named one-liner so d28.3 can grow a
  // prompt+run sibling. Does NOT run or bind params.
  function openSavedQuery(q: SavedQuery) {
    newTabWithContent(q.sql);
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
  <div class="header">
    <h2>Saved queries</h2>
    <button onclick={startPromote}>Promote current tab</button>
  </div>

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
    <p class="empty">No saved queries yet.</p>
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
  .list { padding: 0.5rem; }
  .header { display: flex; align-items: center; justify-content: space-between; }
  h2 { font-size: 1rem; margin: 0.5rem 0; }
  .empty { color: #888; font-size: 0.9rem; }
  .promote { display: flex; flex-direction: column; gap: 0.3rem; margin-bottom: 0.5rem; }
  .search { width: 100%; margin-bottom: 0.5rem; box-sizing: border-box; }
  input { font-size: 0.85rem; padding: 0.2rem 0.3rem; }
  ul { list-style: none; margin: 0; padding: 0; }
  li { padding: 0.5rem; border: 1px solid #ccc; border-radius: 4px; margin-bottom: 0.4rem; }
  .meta { display: flex; flex-direction: column; }
  .sql { color: #888; font-size: 0.8rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .actions { display: flex; gap: 0.3rem; margin-top: 0.3rem; }
  button { font-size: 0.8rem; cursor: pointer; }
</style>
