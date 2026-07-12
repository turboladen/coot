<script lang="ts">
  import type { Param } from "./api";

  // Fields-only param bar (d28.3, Option A). Parent owns `values` (a $state
  // record); we mutate it in place on input. No Run button here — the toolbar
  // Run is param-aware (App.svelte).
  let { params, values }: { params: Param[]; values: Record<string, string> } = $props();
</script>

<div class="parambar">
  {#each params as p (p.name)}
    <label class="param">
      <span class="pname">{p.name}</span>
      <input
        value={values[p.name] ?? ""}
        oninput={(e) => (values[p.name] = e.currentTarget.value)}
      />
      {#if p.sqlType}
        <span class="chip">{p.sqlType}</span>
      {:else}
        <span class="chip raw" title="raw-text — spliced literally into the SQL (injectable)">raw!</span>
      {/if}
    </label>
  {/each}
</div>

<style>
  .parambar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.6rem;
    border-bottom: 1px solid #ccc;
    background: #f7f9fc;
  }
  .param { display: flex; align-items: center; gap: 0.3rem; }
  .pname { font: 600 0.8rem ui-monospace, monospace; color: #333; }
  .param input {
    font: 0.8rem ui-monospace, monospace;
    padding: 0.15rem 0.4rem;
    border: 1px solid #c8c8c8;
    border-radius: 4px;
  }
  .chip {
    font-size: 0.65rem;
    padding: 0.05rem 0.35rem;
    border-radius: 999px;
    background: #eef2f7;
    color: #556;
  }
  .chip.raw {
    background: #fde8e8;
    color: #b91c1c;
    font-weight: 700;
    border: 1px solid #f3b4b4;
  }
</style>
