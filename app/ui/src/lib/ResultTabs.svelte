<script lang="ts">
  // cwt.7 — a single tab strip over the results grid: one tab per result set,
  // then a trailing "Messages" tab (the SSMS/ADS "Results | Messages" pattern).
  // Presentational: App owns `results`/`messages` and the bindable `activeTab`.
  import type { QueryResult } from "./api";
  import type { Message } from "./resultSummary";
  import { tabLabel } from "./resultSummary";
  import ResultsGrid from "./ResultsGrid.svelte";
  import { COPY_FORMATS, canCopy, copiedMessage, formatLabel, formatResult, type CopyFormat } from "./copyFormatLogic";
  import { Copy } from "./icons";
  import { pushToast } from "./toasts.svelte";

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

  const copyable = $derived(canCopy(selectedResult));

  // Copies the active result set, headers included. A rejected clipboard write
  // surfaces as an error toast so a failed copy is never mistaken for a success.
  // Rows go out in query order, which matches the grid only while the grid
  // cannot sort or filter.
  async function copy(format: CopyFormat) {
    const result = selectedResult;
    if (!canCopy(result)) return;
    try {
      await navigator.clipboard.writeText(formatResult(result, format));
      pushToast("success", copiedMessage(result.rows.length, format));
    } catch (e) {
      pushToast("error", `Couldn't copy to the clipboard: ${String(e)}`);
    }
  }
</script>

<div class="result-tabs">
  <div class="tabs">
    {#each results ?? [] as r, i}
      <button class="tab" class:active={activeTab === i} onclick={() => (activeTab = i)}>
        {tabLabel(r, i)}
        <span class="count">{r.rows.length}</span>
      </button>
    {/each}
    <button
      class="tab"
      class:active={activeTab === "messages"}
      onclick={() => (activeTab = "messages")}
    >
      Messages
    </button>
    <div class="copy">
      {#each COPY_FORMATS as f}
        <button
          class="copy-btn"
          aria-label="Copy result as {formatLabel(f)}"
          disabled={!copyable}
          onclick={() => copy(f)}
        >
          <Copy size={14} /> {formatLabel(f)}
        </button>
      {/each}
    </div>
  </div>

  <div class="pane">
    {#if activeTab === "messages"}
      <!-- TODO(billz-mfd): PRINT/info output -->
      <div class="messages">
        {#each messages as m}
          <div class="msg {m.kind}">{#if m.kind === "error"}<span class="msg-icon" aria-hidden="true">⚠</span>{/if}{m.text}</div>
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
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    padding: 0.35rem 0.7rem;
    border: none;
    border-radius: var(--r-sm) var(--r-sm) 0 0;
    background: none;
    font: inherit;
    font-size: 0.85rem;
    color: var(--muted);
    white-space: nowrap;
    cursor: pointer;
  }
  .tab.active {
    color: var(--text);
    background: var(--raised);
    box-shadow: var(--shadow-sm);
  }
  .copy {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    margin-left: auto;
    flex: none;
  }
  /* The global button base supplies font, flex layout, gap, radius, cursor and
     the disabled style; this rule only makes it a quiet, borderless strip control. */
  .copy-btn {
    padding: 0.2rem 0.5rem;
    border: none;
    background: none;
    font-size: var(--fs-xs);
    color: var(--muted);
    white-space: nowrap;
  }
  .copy-btn:hover:not(:disabled) {
    color: var(--text);
    background: var(--raised);
  }
  .count {
    font-size: var(--fs-xs);
    background: color-mix(in srgb, var(--accent) 16%, var(--raised));
    color: var(--accent-press);
    padding: 0 var(--sp-1);
    border-radius: var(--r-pill);
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
  /* Mirrors App/ConnectionForm status colors: info plain, error red + icon
     (never color-only — CVD-safe). */
  .msg.error {
    color: var(--danger);
    white-space: pre-wrap;
  }
  .msg-icon {
    margin-right: var(--sp-1);
  }
  .grid-empty {
    padding: 1rem;
    color: var(--muted);
    font-size: 0.9rem;
  }
</style>
