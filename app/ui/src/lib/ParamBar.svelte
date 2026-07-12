<script lang="ts">
  import type { Param, ParamScope } from "./api";

  // Fields-only param bar (d28.3/d28.4). Parent owns `values` (a $state record);
  // we mutate it in place on input. Scope changes bubble via `onScopeChange` (App
  // persists). `sources` drives the inherited-value badge. No Run button — the
  // toolbar Run is param-aware (App.svelte). `sources`/`onScopeChange` default so
  // the component stands alone; App always supplies both.
  let {
    params,
    values,
    sources = {},
    onScopeChange = () => {},
  }: {
    params: Param[];
    values: Record<string, string>;
    sources?: Record<string, "local" | "session" | "global" | null>;
    onScopeChange?: (name: string, scope: ParamScope) => void;
  } = $props();
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
      <select
        class="scope"
        value={p.scope}
        onchange={(e) => onScopeChange(p.name, e.currentTarget.value as ParamScope)}
        title="Where this query remembers {p.name}'s value"
      >
        <option value="local">Local</option>
        <option value="session">Session</option>
        <option value="global">Global</option>
      </select>
      {#if sources[p.name] === "session"}
        <span class="inh session" title="value inherited from the Session tier">↳ session</span>
      {:else if sources[p.name] === "global"}
        <span class="inh global" title="value inherited from the Global tier">↳ global</span>
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
  .scope {
    font: 0.7rem system-ui, sans-serif;
    padding: 0.1rem 0.25rem;
    border: 1px solid #c8c8c8;
    border-radius: 4px;
    background: #fff;
  }
  .inh {
    font-size: 0.6rem;
    padding: 0.06rem 0.34rem;
    border-radius: 999px;
    border: 1px solid transparent;
  }
  .inh.session {
    background: #eef7ee;
    color: #2f7d32;
    border-color: #bfe0c0;
  }
  .inh.global {
    background: #eef2fb;
    color: #3557b7;
    border-color: #c3d0f0;
  }
</style>
