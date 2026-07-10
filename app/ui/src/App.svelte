<script lang="ts">
  import { onMount } from "svelte";
  import type { ConnectionConfig } from "./lib/api";
  import ConnectionForm from "./lib/ConnectionForm.svelte";
  import ConnectionList from "./lib/ConnectionList.svelte";
  import SqlEditor from "./lib/SqlEditor.svelte";
  import ResultsGrid from "./lib/ResultsGrid.svelte";
  import { sampleResult } from "./lib/sampleResult";
  import { refresh } from "./lib/connections.svelte";

  // The form pane: `undefined` = closed, `null` = new, a config = editing it.
  // `{#key}` on the form remounts it when the target changes so fields re-init.
  let editing = $state<ConnectionConfig | null | undefined>(undefined);

  // The editor-over-grid workspace (shown when no connection form is open).
  let editor = $state<SqlEditor>(); // bind:this — cwt.5 calls editor.getRunText()
  let sqlText = $state("SELECT TOP 100 * FROM sys.objects;");
  let result = $state(sampleResult); // TODO(cwt.5): replace with run_sql output

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
          <SqlEditor bind:this={editor} bind:value={sqlText} />
        </div>
        <!-- TODO(cwt.5): add Run button → editor.getRunText() → runSql(id, db, sql) → result -->
        <div class="grid-pane">
          {#key result}
            <ResultsGrid {result} />
          {/key}
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
    grid-template-rows: minmax(8rem, 40%) 1fr;
    height: 100%;
    min-height: 0;
  }
  .editor-pane {
    border-bottom: 1px solid #ccc;
    min-height: 0;
    overflow: hidden;
  }
  .grid-pane {
    min-height: 0;
    overflow: hidden;
  }
</style>
