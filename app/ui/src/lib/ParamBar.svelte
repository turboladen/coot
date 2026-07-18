<script lang="ts">
  import { SQL_TYPES, type Param, type SqlType } from "./api";
  import type { Variable } from "./variablesLogic";

  // Reframed param bar (V2). Each derived @name is EITHER a library hit (resolved from
  // the Variables Library — read-only chip) OR a query input (editable value; bind-type
  // selector only on a saved-query tab, where the type persists). No scope dropdown / no
  // tiers — scope is implied by name and the library always wins. Parent owns `values`
  // ($state record) and mutates it in place on input.
  let {
    params,
    values,
    libraryHits = {},
    savedTab = false,
    onTypeChange = () => {},
  }: {
    params: Param[];
    values: Record<string, string>;
    libraryHits?: Record<string, Variable>;
    savedTab?: boolean;
    onTypeChange?: (name: string, sqlType: SqlType | null) => void;
  } = $props();
</script>

<div class="parambar">
  {#each params as p (p.name)}
    {#if Object.hasOwn(libraryHits, p.name)}
      <!-- Object.hasOwn (not truthiness) guards against a param literally named e.g.
           @constructor/@toString reading as a false hit via Object.prototype. -->
      <span class="param lib" title="Bound from the Variables Library (@{libraryHits[p.name].name})">
        <span class="pname">{p.name}</span>
        <span class="arrow">→</span>
        <span class="libval">{libraryHits[p.name].value}</span>
        <span class="badge">LIB</span>
        {#if !libraryHits[p.name].sqlType}
          <span class="chip raw" title="raw-text — spliced literally into the SQL (injectable)">raw!</span>
        {/if}
      </span>
    {:else}
      <label class="param">
        <span class="pname">{p.name}</span>
        <input
          value={values[p.name] ?? ""}
          oninput={(e) => (values[p.name] = e.currentTarget.value)}
        />
        {#if savedTab}
          <select
            class="type"
            value={p.sqlType ?? ""}
            onchange={(e) =>
              onTypeChange(p.name, e.currentTarget.value === "" ? null : (e.currentTarget.value as SqlType))}
            title="Bind type — raw-text is spliced literally (injectable); a typed value binds via sp_executesql"
          >
            <option value="">raw-text</option>
            {#each SQL_TYPES as t (t)}
              <option value={t}>{t}</option>
            {/each}
          </select>
        {/if}
        {#if !p.sqlType || !savedTab}
          <span class="chip raw" title="raw-text — spliced literally into the SQL (injectable). Add it to the Variables Library for a safe typed bind.">raw!</span>
        {/if}
      </label>
    {/if}
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
  .type {
    font: 0.7rem var(--font-ui);
    padding: 0.1rem 0.25rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--raised);
    color: var(--text);
  }
  .param.lib {
    gap: 0.25rem;
    padding: 0.15rem 0.4rem;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: color-mix(in srgb, var(--tier-global) 10%, var(--raised));
  }
  .param.lib .arrow { color: var(--faint); }
  .param.lib .libval {
    font: 0.8rem var(--font-mono); color: var(--text);
    max-width: 16rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .badge {
    font-size: 0.58rem; font-weight: 700; letter-spacing: 0.03em;
    padding: 0.05rem 0.32rem; border-radius: var(--r-pill);
    background: color-mix(in srgb, var(--tier-global) 22%, var(--raised));
    color: var(--tier-global);
  }
</style>
