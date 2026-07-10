<script lang="ts">
  import { onMount } from "svelte";
  import { type ConnectionConfig, type QueryResult, runSql } from "./lib/api";
  import ConnectionForm from "./lib/ConnectionForm.svelte";
  import ConnectionList from "./lib/ConnectionList.svelte";
  import SqlEditor from "./lib/SqlEditor.svelte";
  import ResultsGrid from "./lib/ResultsGrid.svelte";
  import { conns, refresh } from "./lib/connections.svelte";

  // The form pane: `undefined` = closed, `null` = new, a config = editing it.
  // `{#key}` on the form remounts it when the target changes so fields re-init.
  let editing = $state<ConnectionConfig | null | undefined>(undefined);

  // The editor-over-grid workspace (shown when no connection form is open).
  let editor = $state<SqlEditor>(); // bind:this — run() calls editor.getRunTarget()
  let sqlText = $state("SELECT TOP 100 * FROM sys.objects;");

  // Live run state. `results` is every flattened result set from the last run
  // (null = never run); `runStatus` is the toolbar line.
  let results = $state<QueryResult[] | null>(null);
  let running = $state(false);
  let runStatus = $state<{ kind: "ok" | "error"; text: string } | null>(null);

  // MVP single-result display: first result set that has columns, else the
  // first, else nothing. Multi-result-set TABS are cwt.7.
  const displayResult = $derived(
    results == null ? null : (results.find((r) => r.columns.length > 0) ?? results[0] ?? null),
  );

  async function run() {
    if (running) return;
    const id = conns.activeId;
    if (!id) {
      runStatus = { kind: "error", text: "Select a connection first." };
      return;
    }
    const t = editor?.getRunTarget();
    if (!t) return;
    running = true;
    runStatus = null;
    try {
      // database:null → the connection's default DB (no DB picker this wave).
      results = await runSql(id, null, t.text, t.selection || null, t.line);
      // length 0 = the batch(es) ran but returned no result set (e.g. a DML
      // INSERT/UPDATE), OR the input was empty. It is NOT an error and must not
      // imply the query didn't execute (rows-affected display is bead billz-38l).
      if (results.length === 0) runStatus = { kind: "ok", text: "No result set returned." };
      else if (results.length > 1) {
        runStatus = { kind: "ok", text: `${results.length} result sets — showing 1 (tabs: cwt.7).` };
      }
    } catch (e) {
      results = null;
      runStatus = { kind: "error", text: String(e) };
    } finally {
      running = false;
    }
  }

  onMount(() => {
    // Falls back silently outside a Tauri webview (plain `vite` in a browser).
    refresh().catch(() => {});
  });

  function openNew() {
    editing = null;
  }
  function openEdit(cfg: ConnectionConfig) {
    editing = cfg;
  }
  function closeForm() {
    editing = undefined;
  }
</script>

<main>
  <aside>
    <ConnectionList onnew={openNew} onedit={openEdit} />
  </aside>
  <section>
    {#if editing !== undefined}
      {#key editing}
        <ConnectionForm editing={editing} onclose={closeForm} />
      {/key}
    {:else}
      <div class="workspace">
        <div class="editor-pane">
          <SqlEditor bind:this={editor} bind:value={sqlText} onrun={run} />
        </div>
        <!-- Run button + Cmd/Ctrl-Enter → getRunTarget() → runSql → displayResult. -->
        <div class="toolbar">
          <button onclick={run} disabled={running}>{running ? "Running…" : "Run"}</button>
          {#if runStatus}<span class="status {runStatus.kind}">{runStatus.text}</span>{/if}
        </div>
        <div class="grid-pane">
          {#if displayResult}
            {#key displayResult}
              <ResultsGrid result={displayResult} />
            {/key}
          {:else}
            <div class="grid-empty">Run a query to see results.</div>
          {/if}
        </div>
      </div>
    {/if}
  </section>
</main>

<style>
  main {
    display: grid;
    grid-template-columns: 20rem 1fr;
    height: 100vh;
    font-family: system-ui, sans-serif;
  }
  aside {
    border-right: 1px solid #ccc;
    overflow-y: auto;
  }
  /* min-height:0 lets the section's children shrink so they scroll internally. */
  section {
    min-height: 0;
    overflow: hidden;
  }
  .workspace {
    display: grid;
    grid-template-rows: minmax(8rem, 40%) auto 1fr;
    height: 100%;
    min-height: 0;
  }
  .editor-pane {
    border-bottom: 1px solid #ccc;
    min-height: 0;
    overflow: hidden;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.4rem 0.6rem;
    border-bottom: 1px solid #ccc;
  }
  .grid-pane {
    min-height: 0;
    overflow: hidden;
  }
  .grid-empty {
    padding: 1rem;
    color: #6b7280;
    font-size: 0.9rem;
  }
  /* Mirrors ConnectionForm's status styling. */
  .status {
    font-size: 0.85rem;
  }
  .status.ok {
    color: #16a34a;
  }
  .status.error {
    color: #dc2626;
    white-space: pre-wrap;
  }
</style>
