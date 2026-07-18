<script lang="ts">
  import type { SqlType } from "./api";
  import { Plus, Search } from "./icons";
  import { removeVariable, upsertVariable, variables } from "./variables.svelte";
  import { buildInsertToken, isValidVariableName, type Variable } from "./variablesLogic";

  // Reusable named values (V2). Insert drops @name into the active editor (App wires
  // onInsert → editor.insertAtCursor). Edits persist immediately via the store. Name is
  // identity: editing a name is delete-old + add-new (references in SQL are the user's
  // to update, like renaming a column).
  let { onInsert }: { onInsert: (token: string) => void } = $props();

  const TYPES: (SqlType | "")[] = [
    "", "int", "bigint", "nvarchar", "bit", "date", "datetime2", "decimal", "uniqueidentifier", "money",
  ];

  // Editing state. `editingName` is "" while adding a brand-new variable; otherwise the
  // name of the row being edited (the original, so we can delete-then-upsert on rename).
  let open = $state(false);
  let editingName = $state("");
  let fName = $state("");
  let fValue = $state("");
  let fType = $state<SqlType | "">("");
  let fNote = $state("");
  let error = $state("");

  function startAdd() {
    editingName = "";
    fName = "";
    fValue = "";
    fType = "nvarchar"; // safe-bind default (no inference)
    fNote = "";
    error = "";
    open = true;
  }

  function startEdit(v: Variable) {
    editingName = v.name;
    fName = v.name;
    fValue = v.value;
    fType = v.sqlType ?? "";
    fNote = v.note;
    error = "";
    open = true;
  }

  function cancel() {
    open = false;
    error = "";
  }

  function submit() {
    const name = fName.trim();
    if (!isValidVariableName(name)) {
      error = "Name must be a bare identifier (letters, digits, _; no leading digit or @).";
      return;
    }
    // Reject a name collision with a DIFFERENT existing variable.
    const clash = variables.list.some((v) => v.name === name && v.name !== editingName);
    if (clash) {
      error = `A variable named "${name}" already exists.`;
      return;
    }
    if (editingName && editingName !== name) removeVariable(editingName); // rename
    upsertVariable({ name, value: fValue, sqlType: fType === "" ? null : fType, note: fNote.trim() });
    open = false;
    error = "";
  }

  function onDelete(v: Variable) {
    if (confirm(`Delete variable "@${v.name}"?`)) removeVariable(v.name);
  }
</script>

<div class="list">
  <div class="header">
    <h2>Variables</h2>
    <button onclick={startAdd}><Plus size={14} /> New variable</button>
  </div>

  {#if open}
    <div class="form">
      <input class="mono" placeholder="name (e.g. benchmark_user)" bind:value={fName} />
      <input placeholder="value" bind:value={fValue} />
      <div class="row">
        <select bind:value={fType} title="Bind type — raw-text splices literally (injectable)">
          {#each TYPES as t (t)}
            <option value={t}>{t === "" ? "raw-text" : t}</option>
          {/each}
        </select>
        <input class="note" placeholder="note (optional)" bind:value={fNote} />
      </div>
      {#if error}<p class="error">{error}</p>{/if}
      <div class="actions">
        <button onclick={submit}>Save</button>
        <button onclick={cancel}>Cancel</button>
      </div>
    </div>
  {/if}

  {#if variables.list.length === 0}
    <div class="empty">
      <Search size={20} />
      <p>No variables yet. Add one to reuse it as <code>@name</code> in any query.</p>
    </div>
  {:else}
    <ul>
      {#each variables.list as v (v.name)}
        <li>
          <div class="meta">
            <strong class="mono">@{v.name}</strong>
            <span class="val">{v.value}{#if !v.sqlType} <span class="raw" title="raw-text — spliced literally (injectable)">raw!</span>{/if}</span>
            {#if v.note}<span class="note-line">{v.note}</span>{/if}
          </div>
          <div class="actions">
            <button onclick={() => onInsert(buildInsertToken(v))}>Insert</button>
            <button onclick={() => startEdit(v)}>Edit</button>
            <button onclick={() => onDelete(v)}>Delete</button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .list { padding: 0.5rem; }
  .header { display: flex; align-items: center; justify-content: space-between; }
  h2 { font-size: 1rem; margin: 0.5rem 0; color: var(--text); }
  .mono { font-family: var(--font-mono); }
  .form { display: flex; flex-direction: column; gap: 0.3rem; margin-bottom: 0.5rem; }
  .form .row { display: flex; gap: 0.3rem; }
  .form .note { flex: 1; }
  .error { color: var(--danger); font-size: 0.75rem; margin: 0.1rem 0 0; }
  .empty {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: var(--sp-2); padding: var(--sp-5) var(--sp-2); color: var(--muted);
    font-size: 0.9rem; text-align: center;
  }
  .empty :global(svg) { color: var(--faint); }
  .empty p { margin: 0; }
  .empty code { font-family: var(--font-mono); }
  input, select {
    font-size: 0.85rem; padding: 0.2rem 0.3rem;
    border: 1px solid var(--border-strong); border-radius: var(--r-sm);
    background: var(--raised); color: var(--text);
  }
  ul { list-style: none; margin: 0; padding: 0; }
  li {
    padding: 0.5rem; border: 1px solid var(--border); border-radius: var(--r-md);
    margin-bottom: 0.4rem; transition: background var(--dur-fast) var(--ease);
  }
  li:hover { background: color-mix(in srgb, var(--brand) 8%, transparent); }
  .meta { display: flex; flex-direction: column; gap: 0.1rem; }
  .val {
    color: var(--muted); font-size: 0.8rem;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .raw {
    font-size: 0.6rem; padding: 0.02rem 0.28rem; border-radius: var(--r-pill);
    background: color-mix(in srgb, var(--danger) 12%, var(--raised));
    color: var(--danger); font-weight: 700;
  }
  .note-line { color: var(--faint); font-size: 0.72rem; font-style: italic; }
  .actions { display: flex; gap: 0.3rem; margin-top: 0.3rem; }
  button { font-size: 0.8rem; cursor: pointer; }
  .header button { display: inline-flex; align-items: center; gap: 0.2rem; }
</style>
