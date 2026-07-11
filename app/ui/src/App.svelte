<script lang="ts">
  import { onMount } from "svelte";
  import { type ConnectionConfig, type QueryResult, runSql } from "./lib/api";
  import ConnectionForm from "./lib/ConnectionForm.svelte";
  import ConnectionList from "./lib/ConnectionList.svelte";
  import ObjectTree from "./lib/tree/ObjectTree.svelte";
  import SqlEditor from "./lib/SqlEditor.svelte";
  import TabBar from "./lib/TabBar.svelte";
  import ResultTabs from "./lib/ResultTabs.svelte";
  import { type Message, summarize } from "./lib/resultSummary";
  import { conns, refresh } from "./lib/connections.svelte";
  import { activeContent, flushSave, restore, setActiveContent, tabsState } from "./lib/tabs.svelte";

  // The form pane: `undefined` = closed, `null` = new, a config = editing it.
  // `{#key}` on the form remounts it when the target changes so fields re-init.
  let editing = $state<ConnectionConfig | null | undefined>(undefined);

  // The editor-over-grid workspace (shown when no connection form is open). The
  // editor text lives in the tabsState module (one scratch tab each, autosaved);
  // `editor` bind:this points at the active tab's remounted CM (run() reads it).
  let editor = $state<SqlEditor>(); // bind:this — run() calls editor.getRunTarget()

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

  // Clear the transient results/Messages pane when the active tab changes so the
  // grid never shows one tab's results next to another tab's SQL (that mismatch
  // is the confusing state). Re-run (Cmd-Enter) to repopulate. Results are never
  // persisted — switching or relaunching starts the pane empty.
  $effect(() => {
    tabsState.activeId; // track: re-run whenever the active tab changes
    results = null;
    messages = [];
    activeTab = 0;
  });

  onMount(() => {
    // Load persisted tabs (or seed the default) before the editor mounts.
    restore();
    // Falls back silently outside a Tauri webview (plain `vite` in a browser).
    refresh().catch(() => {});
    // Flush any pending debounced save on app quit (Cmd-Q). Tauri's WKWebView
    // doesn't reliably deliver `beforeunload` at termination, so hook the window
    // close event instead; the localStorage write is synchronous. try/catch so
    // plain `vite` browser dev (no Tauri window) degrades silently.
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        unlisten = await getCurrentWindow().onCloseRequested(() => {
          flushSave();
        });
      } catch {
        // Not running under Tauri — the debounce + structural-op flushes still bound loss.
      }
    })();
    return () => unlisten?.();
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
    <!-- Tree for the active connection. {#key conns.activeId} remounts it on a
         connection switch so every node resets to idle and reloads. -->
    <div class="tree-pane">
      {#key conns.activeId}
        <ObjectTree />
      {/key}
    </div>
  </aside>
  <section>
    {#if editing !== undefined}
      {#key editing}
        <ConnectionForm editing={editing} onclose={closeForm} />
      {/key}
    {:else}
      <div class="workspace">
        <TabBar />
        <div class="editor-pane">
          <!-- Remount the editor per active tab: a fresh CM instance gives each
               tab its own doc/undo/cursor (no bleed across tabs). `value` is
               init-only; edits flow back via onchange → tabsState → autosave. -->
          {#key tabsState.activeId}
            <SqlEditor bind:this={editor} value={activeContent()} onchange={setActiveContent} onrun={run} />
          {/key}
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
  /* Two-row sidebar: ConnectionList at natural height, the tree scrolling below
     it. min-height:0 on the tree region lets it shrink so it scrolls internally
     under a long connection list. */
  aside {
    display: flex;
    flex-direction: column;
    border-right: 1px solid #ccc;
    overflow: hidden;
  }
  .tree-pane {
    flex: 1;
    min-height: 0;
    overflow: auto;
    border-top: 1px solid #ccc;
  }
  /* min-height:0 lets the section's children shrink so they scroll internally. */
  section {
    min-height: 0;
    overflow: hidden;
  }
  .workspace {
    display: grid;
    /* tab bar (auto) · editor · toolbar (auto) · grid */
    grid-template-rows: auto minmax(8rem, 40%) auto 1fr;
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
