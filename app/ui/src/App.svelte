<script lang="ts">
  import { onMount } from "svelte";
  import { type ConnectionConfig, type QueryResult, runSql } from "./lib/api";
  import ConnectionForm from "./lib/ConnectionForm.svelte";
  import ConnectionList from "./lib/ConnectionList.svelte";
  import SqlEditor from "./lib/SqlEditor.svelte";
  import ResultTabs from "./lib/ResultTabs.svelte";
  import { type Message, summarize } from "./lib/resultSummary";
  import { conns, refresh } from "./lib/connections.svelte";

  // The form pane: `undefined` = closed, `null` = new, a config = editing it.
  // `{#key}` on the form remounts it when the target changes so fields re-init.
  let editing = $state<ConnectionConfig | null | undefined>(undefined);

  // The editor-over-grid workspace (shown when no connection form is open).
  let editor = $state<SqlEditor>(); // bind:this — run() calls editor.getRunTarget()
  let sqlText = $state("SELECT TOP 100 * FROM sys.objects;");

  // Live run state. `results` is every flattened result set from the last run
  // (null = never run); `messages` feeds the Messages tab (run summary or the
  // error string); `activeTab` selects a result set by index or the Messages
  // pane. Every run reassigns all three (cwt.7).
  let results = $state<QueryResult[] | null>(null);
  let running = $state(false);
  let messages = $state<Message[]>([]);
  let activeTab = $state<number | "messages">(0);

  async function run() {
    if (running) return;
    const id = conns.activeId;
    if (!id) {
      // Route the "pick a connection" nudge through the one output surface too,
      // and clear stale Result tabs from a prior run (implementer note A).
      results = null;
      messages = [{ kind: "error", text: "Select a connection first." }];
      activeTab = "messages";
      return;
    }
    const t = editor?.getRunTarget();
    if (!t) return;
    running = true;
    try {
      // database:null → the connection's default DB (no DB picker this wave).
      const out = await runSql(id, null, t.text, t.selection || null, t.line);
      results = out;
      messages = summarize(out);
      // 0 result sets (e.g. a DML batch — billz-38l) → land on Messages, which
      // carries the honest "No result set returned." line; else the first tab.
      activeTab = out.length > 0 ? 0 : "messages";
    } catch (e) {
      results = null;
      messages = [{ kind: "error", text: String(e) }];
      activeTab = "messages";
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
        <!-- Run button + Cmd/Ctrl-Enter → getRunTarget() → runSql → result tabs. -->
        <div class="toolbar">
          <button onclick={run} disabled={running}>{running ? "Running…" : "Run"}</button>
        </div>
        <div class="grid-pane">
          <ResultTabs {results} {messages} bind:activeTab />
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
</style>
