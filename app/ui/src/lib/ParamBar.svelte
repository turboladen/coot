<script lang="ts">
  import type { Param, ParamScope, SqlType } from "./api";
  import { Clock, Globe, X } from "./icons";

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
        <span class="badge session" title="value inherited from the Session tier"><Clock size={11} /> session</span>
        <button
          type="button"
          class="clear-tier"
          aria-label="Clear Session value for {p.name}"
          title="Clear this Session value"
          onclick={() => onClearTier(p.name, "session")}
        ><X size={12} /></button>
      {:else if sources[p.name] === "global"}
        <span class="badge global" title="value inherited from the Global tier"><Globe size={11} /> global</span>
        <button
          type="button"
          class="clear-tier"
          aria-label="Clear Global value for {p.name}"
          title="Clear this Global value"
          onclick={() => onClearTier(p.name, "global")}
        ><X size={12} /></button>
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
    border-bottom: 1px solid var(--border);
    background: var(--panel);
  }
  .param { display: flex; align-items: center; gap: 0.3rem; }
  .pname { font: 600 0.8rem var(--font-mono); color: var(--text); }
  .param input {
    font: 0.8rem var(--font-mono);
    padding: 0.15rem 0.4rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--raised);
    color: var(--text);
  }
  .param input:focus-visible {
    border: 1.5px solid var(--accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 15%, transparent);
    outline: none;
  }
  .chip {
    font-size: 0.65rem;
    padding: 0.05rem 0.35rem;
    border-radius: var(--r-pill);
    background: color-mix(in srgb, var(--muted) 12%, var(--raised));
    color: var(--muted);
  }
  .chip.raw {
    background: color-mix(in srgb, var(--danger) 12%, var(--raised));
    color: var(--danger);
    font-weight: 700;
    border: 1px solid color-mix(in srgb, var(--danger) 35%, transparent);
  }
  .scope,
  .type {
    font: 0.7rem var(--font-ui);
    padding: 0.1rem 0.25rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--raised);
    color: var(--text);
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    font-size: 0.6rem;
    padding: 0.06rem 0.34rem;
    border-radius: var(--r-pill);
  }
  .badge.session {
    background: color-mix(in srgb, var(--tier-session) 16%, var(--raised));
    color: var(--tier-session);
  }
  .badge.global {
    background: color-mix(in srgb, var(--tier-global) 16%, var(--raised));
    color: var(--tier-global);
  }
  .clear-tier {
    display: inline-flex;
    align-items: center;
    line-height: 1;
    padding: 0.05rem 0.28rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-pill);
    background: var(--raised);
    color: var(--muted);
    cursor: pointer;
  }
  .clear-tier:hover {
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 35%, transparent);
    background: color-mix(in srgb, var(--danger) 12%, var(--raised));
  }
</style>
