<script lang="ts">
  // billz-0gh.1.3 — multi-select database checklist for a fan-out run, over the
  // shared dbStore. ONLINE databases are selectable; non-ONLINE are disabled
  // (mirrors the single-DB picker in App). A glob pattern box bulk-selects
  // matching ONLINE databases (`ESP_Nomad_*`). Presentational: App owns the
  // selection (the active tab's `fanoutDatabases`) and persists via `onchange`.
  import { dbStore } from "./databases.svelte";
  import { matchPattern } from "./fanoutLogic";

  let {
    selected,
    onchange,
  }: {
    selected: string[];
    onchange: (databases: string[]) => void;
  } = $props();

  let pattern = $state("");

  // A Set for O(1) checkbox state; the callback always emits a plain array.
  const selectedSet = $derived(new Set(selected));

  function toggle(name: string, checked: boolean): void {
    const next = new Set(selected);
    if (checked) next.add(name);
    else next.delete(name);
    // Emit in dbStore order so the persisted list is stable/predictable.
    onchange(dbStore.list.filter((d) => next.has(d.name)).map((d) => d.name));
  }

  // Union the pattern matches into the current selection (additive — never
  // deselects). matchPattern already restricts to ONLINE names.
  function selectMatching(): void {
    const next = new Set(selected);
    for (const name of matchPattern(pattern, dbStore.list)) next.add(name);
    onchange(dbStore.list.filter((d) => next.has(d.name)).map((d) => d.name));
  }

  function clearAll(): void {
    onchange([]);
  }
</script>

<div class="fanout-picker">
  <div class="pattern-row">
    <input
      class="pattern"
      type="text"
      placeholder="Pattern e.g. ESP_Nomad_*"
      bind:value={pattern}
      onkeydown={(e) => {
        if (e.key === "Enter") selectMatching();
      }}
    />
    <button type="button" onclick={selectMatching} disabled={pattern === ""}>Select matching</button>
    <button type="button" onclick={clearAll} disabled={selected.length === 0}>Clear</button>
    <span class="count">{selected.length} selected</span>
  </div>
  <div class="list">
    {#each dbStore.list as db (db.databaseId)}
      {@const online = db.stateDesc === "ONLINE"}
      <label class="row" class:disabled={!online}>
        <input
          type="checkbox"
          checked={selectedSet.has(db.name)}
          disabled={!online}
          onchange={(e) => toggle(db.name, e.currentTarget.checked)}
        />
        <span class="db-name">{db.name}</span>
        {#if !online}<span class="state">({db.stateDesc.toLowerCase()})</span>{/if}
      </label>
    {/each}
    {#if dbStore.list.length === 0}
      <div class="empty">No databases loaded.</div>
    {/if}
  </div>
</div>

<style>
  .fanout-picker {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    flex: 1 1 auto;
    min-width: 0;
    max-width: 32rem;
  }
  .pattern-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .pattern {
    flex: 1 1 auto;
    min-width: 0;
    font: inherit;
    font-size: 0.85rem;
    padding: 0.15rem 0.4rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--raised);
    color: var(--text);
  }
  .pattern-row button {
    font-size: 0.8rem;
    white-space: nowrap;
  }
  .count {
    flex: none;
    font-size: var(--fs-xs);
    color: var(--muted);
    white-space: nowrap;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    max-height: 9rem;
    overflow-y: auto;
    padding: 0.25rem;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--raised);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: 0.1rem 0.25rem;
    font-size: 0.82rem;
    color: var(--text);
    border-radius: var(--r-sm);
    cursor: pointer;
  }
  .row:hover:not(.disabled) {
    background: color-mix(in srgb, var(--accent) 8%, var(--raised));
  }
  .row.disabled {
    color: var(--faint);
    cursor: default;
  }
  .state {
    color: var(--faint);
    font-size: var(--fs-xs);
  }
  .empty {
    padding: 0.4rem;
    color: var(--muted);
    font-size: 0.82rem;
  }
</style>
