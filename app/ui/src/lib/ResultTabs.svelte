<script lang="ts">
  // cwt.7 — a single tab strip over the results grid: one tab per result set,
  // then a trailing "Messages" tab (the SSMS/ADS "Results | Messages" pattern).
  // Presentational: App owns `results`/`messages` and the bindable `activeTab`.
  import type { QueryResult } from "./api";
  import type { Message } from "./resultSummary";
  import { tabLabel } from "./resultSummary";
  import ResultsGrid from "./ResultsGrid.svelte";

  let {
    results,
    messages,
    activeTab = $bindable(),
  }: {
    results: QueryResult[] | null;
    messages: Message[];
    activeTab: number | "messages";
  } = $props();

  // The grid to show: the selected result set, or null when the Messages tab is
  // active or the index is out of range. `run()` assigns results + activeTab
  // synchronously, so there's no stale-index window; the `?? null` guards anyway.
  const selectedResult = $derived(
    results && typeof activeTab === "number" ? (results[activeTab] ?? null) : null,
  );
</script>

<div class="result-tabs">
  <div class="tabs">
    {#each results ?? [] as r, i}
      <button class="tab" class:active={activeTab === i} onclick={() => (activeTab = i)}>
        {tabLabel(r, i)}
      </button>
    {/each}
    <button
      class="tab"
      class:active={activeTab === "messages"}
      onclick={() => (activeTab = "messages")}
    >
      Messages
    </button>
  </div>

  <div class="pane">
    {#if activeTab === "messages"}
      <!-- TODO(billz-mfd): PRINT/info output -->
      <div class="messages">
        {#each messages as m}
          <div class="msg {m.kind}">{m.text}</div>
        {/each}
      </div>
    {:else if selectedResult}
      {#key selectedResult}
        <ResultsGrid result={selectedResult} />
      {/key}
    {:else}
      <div class="grid-empty">Run a query to see results.</div>
    {/if}
  </div>
</div>

<style>
  .result-tabs {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .tabs {
    display: flex;
    flex: none;
    gap: 0.25rem;
    padding: 0 0.4rem;
    border-bottom: 1px solid #ccc;
    overflow-x: auto;
  }
  .tab {
    padding: 0.35rem 0.7rem;
    border: none;
    border-bottom: 2px solid transparent;
    background: none;
    font: inherit;
    font-size: 0.85rem;
    color: #555;
    white-space: nowrap;
    cursor: pointer;
  }
  .tab.active {
    color: #111;
    border-bottom-color: #2563eb;
  }
  .pane {
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
  }
  .messages {
    padding: 0.5rem 0.6rem;
    overflow-y: auto;
    height: 100%;
    font-size: 0.85rem;
    line-height: 1.5;
  }
  /* Mirrors App/ConnectionForm status colors: info plain, error red. */
  .msg.error {
    color: #dc2626;
    white-space: pre-wrap;
  }
  .grid-empty {
    padding: 1rem;
    color: #6b7280;
    font-size: 0.9rem;
  }
</style>
