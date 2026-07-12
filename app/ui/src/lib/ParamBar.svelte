<script lang="ts">
  import type { Param, ParamScope, SqlType } from "./api";

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
    onTypeChange = () => {},
    onClearTier = () => {},
  }: {
    params: Param[];
    values: Record<string, string>;
    sources?: Record<string, "local" | "session" | "global" | null>;
    onScopeChange?: (name: string, scope: ParamScope) => void;
    onTypeChange?: (name: string, sqlType: SqlType | null) => void;
    onClearTier?: (name: string, tier: "session" | "global") => void;
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
      <select
        class="type"
        value={p.sqlType ?? ""}
        onchange={(e) =>
          onTypeChange(p.name, e.currentTarget.value === "" ? null : (e.currentTarget.value as SqlType))}
        title="Bind type — raw-text is spliced literally (injectable); a typed value binds via sp_executesql"
      >
        <option value="">raw-text</option>
        <option value="int">int</option>
        <option value="bigint">bigint</option>
        <option value="nvarchar">nvarchar</option>
        <option value="bit">bit</option>
        <option value="date">date</option>
        <option value="datetime2">datetime2</option>
        <option value="decimal">decimal</option>
        <option value="uniqueidentifier">uniqueidentifier</option>
        <option value="money">money</option>
      </select>
      {#if !p.sqlType}
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
        <button
          type="button"
          class="clear-tier"
          aria-label="Clear Session value for {p.name}"
          title="Clear this Session value"
          onclick={() => onClearTier(p.name, "session")}
        >×</button>
      {:else if sources[p.name] === "global"}
        <span class="inh global" title="value inherited from the Global tier">↳ global</span>
        <button
          type="button"
          class="clear-tier"
          aria-label="Clear Global value for {p.name}"
          title="Clear this Global value"
          onclick={() => onClearTier(p.name, "global")}
        >×</button>
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
  .scope,
  .type {
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
  .clear-tier {
    font-size: 0.7rem;
    line-height: 1;
    padding: 0.05rem 0.28rem;
    border: 1px solid #d5d5d5;
    border-radius: 999px;
    background: #fff;
    color: #999;
    cursor: pointer;
  }
  .clear-tier:hover {
    color: #b91c1c;
    border-color: #f3b4b4;
    background: #fde8e8;
  }
</style>
