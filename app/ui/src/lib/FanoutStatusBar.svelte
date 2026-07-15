<script lang="ts">
  // billz-0gh.1.3 — the always-on per-database status strip for a fan-out run.
  // One chip per database: name · rows · ok/error · elapsed. Presentational; App
  // owns the outcomes. In per-DB fallback mode (`selectable`) the chips are
  // buttons that focus a DB's grid; in combined mode they're informational.
  import type { DbRunOutcome } from "./api";
  import { fanoutStatus } from "./fanoutLogic";

  let {
    outcomes,
    selectable = false,
    selectedIndex = 0,
    onselect,
  }: {
    outcomes: DbRunOutcome[];
    selectable?: boolean;
    selectedIndex?: number;
    onselect?: (index: number) => void;
  } = $props();

  const rows = $derived(fanoutStatus(outcomes));
  const okCount = $derived(rows.filter((r) => r.ok).length);
</script>

<div class="status-bar">
  <span class="summary">{okCount}/{rows.length} ok</span>
  <div class="chips">
    {#each rows as s, i}
      <!-- Chip content is identical whether selectable or not; only the element
           (button vs span) and the active highlight differ. Error is never
           color-only — the ⚠ glyph + the error text in the title carry it too
           (CVD-safe, matching the Messages pane pattern). -->
      {#if selectable}
        <button
          type="button"
          class="chip"
          class:err={!s.ok}
          class:active={i === selectedIndex}
          aria-pressed={i === selectedIndex}
          title={s.error ?? `${s.database}: ${s.rows} rows`}
          onclick={() => onselect?.(i)}
        >
          <span class="name">{s.database}</span>
          {#if s.ok}
            <span class="meta">{s.rows} rows</span>
          {:else}
            <span class="meta err-text"><span aria-hidden="true">⚠</span> error</span>
          {/if}
          <span class="ms">{s.elapsedMs} ms</span>
        </button>
      {:else}
        <span class="chip" class:err={!s.ok} title={s.error ?? `${s.database}: ${s.rows} rows`}>
          <span class="name">{s.database}</span>
          {#if s.ok}
            <span class="meta">{s.rows} rows</span>
          {:else}
            <span class="meta err-text"><span aria-hidden="true">⚠</span> error</span>
          {/if}
          <span class="ms">{s.elapsedMs} ms</span>
        </span>
      {/if}
    {/each}
  </div>
</div>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    flex: none;
    padding: 0.35rem 0.5rem;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    overflow-x: auto;
  }
  .summary {
    flex: none;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--muted);
    white-space: nowrap;
  }
  .chips {
    display: flex;
    gap: 0.3rem;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-1);
    padding: 0.15rem 0.5rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-pill);
    background: var(--raised);
    color: var(--text);
    font: inherit;
    font-size: 0.78rem;
    white-space: nowrap;
  }
  button.chip {
    cursor: pointer;
    transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease);
  }
  button.chip:hover:not(.active) {
    background: color-mix(in srgb, var(--accent) 8%, var(--raised));
  }
  button.chip.active {
    background: color-mix(in srgb, var(--accent) 16%, var(--raised));
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
    color: var(--accent-press);
    font-weight: 600;
  }
  .chip.err {
    border-color: color-mix(in srgb, var(--danger) 40%, var(--border-strong));
    background: color-mix(in srgb, var(--danger) 10%, var(--raised));
  }
  .name {
    font-weight: 600;
  }
  .meta {
    color: var(--muted);
  }
  .meta.err-text {
    color: var(--danger);
  }
  .ms {
    color: var(--faint);
    font-size: var(--fs-xs);
  }
  button.chip.active .meta,
  button.chip.active .ms {
    color: inherit;
  }
</style>
