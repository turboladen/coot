<script lang="ts">
  import { onMount } from "svelte";
  import { type ConnectionConfig, type DatabaseInfo, listDatabases, type QueryResult, runSql } from "./lib/api";
  import ConnectionForm from "./lib/ConnectionForm.svelte";
  import ConnectionList from "./lib/ConnectionList.svelte";
  import ObjectTree from "./lib/tree/ObjectTree.svelte";
  import SavedQueryLibrary from "./lib/SavedQueryLibrary.svelte";
  import SqlEditor from "./lib/SqlEditor.svelte";
  import TabBar from "./lib/TabBar.svelte";
  import ResultTabs from "./lib/ResultTabs.svelte";
  import { type Message, summarize } from "./lib/resultSummary";
  import { conns, refresh } from "./lib/connections.svelte";
  import { refresh as refreshLibrary } from "./lib/savedQueries.svelte";
  import { activeContent, flushSave, restore, setActiveContent, setActiveDatabase, tabsState } from "./lib/tabs.svelte";
  import { treeRefresh } from "./lib/tree/refresh.svelte";

  // Sidebar lower region toggles between the object tree and the saved-query
  // library (d28.6) — the library gets its own full-height home per PLAN §5.
  let sidebarMode = $state<"objects" | "library">("objects");

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

  // Databases for the per-tab DB picker (cwt.9). Loaded for the active connection
  // and refreshed alongside the tree (treeRefresh.nonce), swallowing errors like
  // the tree does. Reusing list_databases — no new backend.
  let databases = $state<DatabaseInfo[]>([]);
  $effect(() => {
    const id = conns.activeId;
    treeRefresh.nonce; // track: a schema Refresh also repopulates the picker
    if (!id) {
      databases = [];
      return;
    }
    // Guard against an out-of-order resolve: on a rapid connection switch a
    // slower prior fetch must not overwrite the newer connection's list. The
    // cleanup runs before the next effect run (or on unmount) and drops the
    // stale response.
    let cancelled = false;
    listDatabases(id)
      .then((dbs) => !cancelled && (databases = dbs))
      .catch(() => !cancelled && (databases = []));
    return () => {
      cancelled = true;
    };
  });

  // The active tab's stored target DB (null = connection default). Derived so the
  // picker, which lives outside the per-tab {#key} block, tracks tab switches + edits.
  const activeDb = $derived(
    tabsState.tabs.find((t) => t.id === tabsState.activeId)?.database ?? null,
  );

  // The stored DB validated against the CURRENT connection's databases. If it
  // isn't in the list (e.g. after switching to a connection that lacks it, or
  // before the list has loaded), it resolves to null so the picker's displayed
  // selection and run()'s target ALWAYS agree on the connection default — never a
  // silent USE [db] against the wrong server. The stored value is left untouched,
  // so returning to its own connection restores the selection.
  const effectiveDb = $derived(
    activeDb !== null && databases.some((d) => d.name === activeDb) ? activeDb : null,
  );

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
      // Per-tab target DB (cwt.9): the executor issues USE [db] before the batch;
      // null ⇒ the connection's default DB. `effectiveDb` (not the raw stored
      // value) so we never USE a DB absent from the active connection.
      const out = await runSql(id, effectiveDb, t.text, t.selection || null, t.line);
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
    activeDb; // and whenever the picker retargets the DB — else the grid would
    // show the prior DB's rows next to a changed picker (the same mismatch).
    results = null;
    messages = [];
    activeTab = 0;
  });

  onMount(() => {
    // Load persisted tabs (or seed the default) before the editor mounts.
    restore();
    // Falls back silently outside a Tauri webview (plain `vite` in a browser).
    refresh().catch(() => {});
    refreshLibrary().catch(() => {});
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
    <!-- Segmented toggle: the lower region shows the object tree OR the saved-query
         library (d28.6). Objects need a connection; the library is independent. -->
    <div class="mode-toggle">
      <button class:active={sidebarMode === "objects"} onclick={() => (sidebarMode = "objects")}>
        Objects
      </button>
      <button class:active={sidebarMode === "library"} onclick={() => (sidebarMode = "library")}>
        Library
      </button>
    </div>
    <div class="lower-pane">
      {#if sidebarMode === "objects"}
        <!-- Tree for the active connection. The key remounts it on a connection
             switch (every node resets to idle and reloads) and on a Refresh bump
             (rqb.5 — drops the node-local memos so the invalidated core cache re-queries). -->
        {#key `${conns.activeId}:${treeRefresh.nonce}`}
          <ObjectTree />
        {/key}
      {:else}
        <SavedQueryLibrary />
      {/if}
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
        <!-- DB picker + Run button. Cmd/Ctrl-Enter → getRunTarget() → runSql →
             result tabs. The picker sets the active tab's target DB (cwt.9);
             its value feeds runSql, where the executor issues USE [db]. -->
        <div class="toolbar">
          <select
            class="db-picker"
            title="Target database — the runner issues USE [db] before your batch"
            value={effectiveDb ?? ""}
            disabled={!conns.activeId}
            onchange={(e) => setActiveDatabase(e.currentTarget.value || null)}
          >
            <option value="">(default database)</option>
            {#each databases as db (db.databaseId)}
              <option value={db.name} disabled={db.stateDesc !== "ONLINE"}>
                {db.name}{db.stateDesc !== "ONLINE" ? ` (${db.stateDesc.toLowerCase()})` : ""}
              </option>
            {/each}
          </select>
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
  /* Segmented [Objects | Library] toggle between the connection list and the
     scrolling lower region. */
  .mode-toggle {
    display: flex;
    gap: 0.25rem;
    padding: 0.3rem 0.5rem;
    border-top: 1px solid #ccc;
  }
  .mode-toggle button {
    flex: 1;
    font-size: 0.8rem;
    cursor: pointer;
  }
  .mode-toggle button.active {
    font-weight: 600;
    border-color: #3b82f6;
  }
  .lower-pane {
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
  .db-picker {
    font: inherit;
    font-size: 0.85rem;
    max-width: 16rem;
    padding: 0.15rem 0.3rem;
  }
  .db-picker:disabled { color: #ccc; }
  .grid-pane {
    min-height: 0;
    overflow: hidden;
  }
</style>
