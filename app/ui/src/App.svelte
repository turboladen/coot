<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { type ConnectionConfig, type ParamScope, type QueryResult, type SqlType, runParams, runSql, setSessionPassword } from "./lib/api";
  import { SvelteSet } from "svelte/reactivity";
  import ConnectionForm from "./lib/ConnectionForm.svelte";
  import PasswordPrompt from "./lib/PasswordPrompt.svelte";
  import ConnectionList from "./lib/ConnectionList.svelte";
  import ObjectTree from "./lib/tree/ObjectTree.svelte";
  import SavedQueryLibrary from "./lib/SavedQueryLibrary.svelte";
  import ParamBar from "./lib/ParamBar.svelte";
  import SqlEditor from "./lib/SqlEditor.svelte";
  import TabBar from "./lib/TabBar.svelte";
  import ResultTabs from "./lib/ResultTabs.svelte";
  import { type Message, summarize } from "./lib/resultSummary";
  import { deriveParams, nextParamValues, persistDeclared, resolve, toResolvedParams, valueSource } from "./lib/paramBarLogic";
  import { clearSessionParam, sessionParams, setSessionParams } from "./lib/sessionParams.svelte";
  import { clearGlobalParam, globalParams, setGlobalParams } from "./lib/globalParams.svelte";
  import { conns, refresh } from "./lib/connections.svelte";
  import { library, refresh as refreshLibrary, save as saveQuery } from "./lib/savedQueries.svelte";
  import { activeContent, flushSave, restore, setActiveContent, setActiveDatabase, tabsState } from "./lib/tabs.svelte";
  import { treeRefresh } from "./lib/tree/refresh.svelte";
  import { dbStore, load as loadDatabases } from "./lib/databases.svelte";

  // Sidebar lower region toggles between the object tree and the saved-query
  // library (d28.6) — the library gets its own full-height home per PLAN §5.
  let sidebarMode = $state<"objects" | "library">("objects");

  // The form pane: `undefined` = closed, `null` = new, a config = editing it.
  // `{#key}` on the form remounts it when the target changes so fields re-init.
  let editing = $state<ConnectionConfig | null | undefined>(undefined);

  // If the connection being edited is deleted from the list (dsq), close the now
  // stale form — otherwise clicking Save would re-create the deleted connection
  // via upsert. Reacts to the removal wherever it originates (the list deletes
  // directly through the store). `editing` truthy ⇒ it's a config with an `id`;
  // `null` (new) / `undefined` (closed) are left alone.
  $effect(() => {
    // Capture in a const so the narrowing survives into the `.some()` closure
    // (TS widens the mutable `editing` back to its union otherwise).
    const cfg = editing;
    if (cfg && !conns.list.some((c) => c.id === cfg.id)) {
      editing = undefined;
    }
  });

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

  // Single trigger for the shared databases store (cwt.10): App is always
  // mounted, so it owns the load; the tree's root and the DB picker both just
  // read `dbStore`. Reloads on connection switch and on a schema Refresh
  // (treeRefresh.nonce); the store drops out-of-order responses.
  // billz-85b: session-only password unlock. `unlocked` = connection ids with a
  // session password set this app-session (same lifetime as the backend session
  // map — both empty on restart). `dismissed` = ids whose prompt the user closed
  // (modal hides, connection stays locked, a 🔒 banner offers to reopen).
  const unlocked = new SvelteSet<string>();
  const dismissed = new SvelteSet<string>();
  // The active connection if it's session-only and not yet unlocked → locked.
  const lockedConn = $derived.by(() => {
    const c = conns.list.find((c) => c.id === conns.activeId);
    return c && !c.rememberPassword && !unlocked.has(c.id) ? c : null;
  });
  const showPrompt = $derived(!!lockedConn && !dismissed.has(lockedConn.id));

  async function unlock(id: string, password: string) {
    await setSessionPassword(id, password); // await BEFORE marking unlocked (no desync)
    dismissed.delete(id);
    unlocked.add(id); // re-derives lockedConn → null → the load effect fires
  }

  $effect(() => {
    treeRefresh.nonce; // track: a Refresh re-issues the load
    if (lockedConn) return; // billz-85b: wait for unlock before hitting the DB
    loadDatabases(conns.activeId);
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
    activeDb !== null && dbStore.list.some((d) => d.name === activeDb) ? activeDb : null,
  );

  // The active editor tab + the saved query it was opened from (d28.3).
  const curTab = $derived(tabsState.tabs.find((t) => t.id === tabsState.activeId));
  const curSavedQuery = $derived(
    curTab?.savedQueryId ? library.list.find((q) => q.id === curTab.savedQueryId) ?? null : null,
  );
  // Params derived from the tab's live SQL merged with the saved query's declared
  // params. Empty ⇒ no param bar, plain run_sql path.
  const curParams = $derived(
    curSavedQuery ? deriveParams(curTab?.content ?? "", curSavedQuery.params) : [],
  );

  // d28.10: the active saved-query tab has unsaved SQL edits (tab content differs
  // from the stored definition). Gates the "Update saved query" button and doubles
  // as the unsaved-edits signal. Exact-string compare — a trailing-newline-only
  // diff reads as dirty (honest + simplest for a single-user tool).
  const dirty = $derived(!!curSavedQuery && !!curTab && curTab.content !== curSavedQuery.sql);

  // d28.10: explicit "redefine" counterpart to Run (d28.8: Run never redefines).
  // Persist the tab's edited SQL + reconciled param declarations back to the linked
  // saved query. deriveParams reconciles: surviving params keep type/scope/value,
  // edited-in params become declared (later Runs remember them), removed drop.
  // targetDatabase + param values untouched. Re-checks dirty (stale-click no-op).
  async function updateSavedQuery() {
    if (!curSavedQuery || !curTab || !dirty) return;
    await saveQuery({
      ...curSavedQuery,
      sql: curTab.content,
      params: deriveParams(curTab.content, curSavedQuery.params),
    });
  }

  // Which tier each param's displayed value resolves from (drives the badge).
  // Reads the stores reactively so a badge updates live when a store changes.
  const paramSources = $derived(
    Object.fromEntries(curParams.map((p) => [p.name, valueSource(p, sessionParams, globalParams)])),
  );

  // Persist a scope change immediately (a rare, deliberate config choice — avoids
  // a separate scope state map). Declares a newly-derived param in the process.
  async function onScopeChange(name: string, scope: ParamScope) {
    if (!curSavedQuery) return;
    const params = curParams.map((p) => (p.name === name ? { ...p, scope } : p));
    await saveQuery({ ...curSavedQuery, params });
  }

  // Persist a type change immediately (mirrors onScopeChange). A typed param
  // routes through d28.2's sp_executesql bind path on the next Run; raw-text
  // (null) splices. Declares a newly-derived param in the process.
  async function onTypeChange(name: string, sqlType: SqlType | null) {
    if (!curSavedQuery) return;
    const params = curParams.map((p) => (p.name === name ? { ...p, sqlType } : p));
    await saveQuery({ ...curSavedQuery, params });
  }

  // d28.9: clear the SURFACED tier value for a param (the one the inherited badge
  // shows), then refresh the field to the new resolved value (a lower tier or
  // empty). Reads the stores AFTER the clear. Chaining clears both tiers; a value
  // shadowed by a Local value has no badge, so it isn't reachable here (rare).
  function onClearTier(name: string, tier: "session" | "global") {
    if (tier === "session") clearSessionParam(name);
    else clearGlobalParam(name);
    const param = curParams.find((p) => p.name === name);
    if (param) paramValues[name] = resolve(param, sessionParams, globalParams) ?? "";
  }

  // Bar field values, keyed by param name. On a TAB SWITCH rebuild fresh from each
  // param's resolved value (never bleed one query's values into another). On a
  // same-tab recompute (an SQL keystroke changes curTab.content → curParams, or the
  // library loads async after the tab opened) PRESERVE values the user typed but
  // hasn't run yet, and seed any newly-appeared param from resolve. `valuesTabId`
  // is effect-local bookkeeping; `untrack` reads state without subscribing (else
  // writing paramValues below would re-trigger this effect).
  let paramValues = $state<Record<string, string>>({});
  let valuesTabId = "";
  $effect(() => {
    const id = tabsState.activeId; // tab switch → full reset
    const params = curParams; // track: late library load + new @names on edit
    // Snapshot prior values + the stores via untrack (plain copies — reading the
    // proxy keys outside untrack would subscribe and re-arm this effect). Session/
    // Global changes are thus picked up on the next switch/rebuild, not mid-edit.
    const prev = untrack(() => ({ ...paramValues }));
    const session = untrack(() => ({ ...sessionParams }));
    const global = untrack(() => ({ ...globalParams }));
    paramValues = nextParamValues(id !== valuesTabId, params, prev, session, global);
    valuesTabId = id;
  });

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
    running = true;
    try {
      // `effectiveDb` (not the raw stored value) so we never USE a DB absent from
      // the active connection (cwt.9). Param-aware (d28.3): a saved-query tab with
      // derived @params runs via run_params (bind/splice); everything else keeps
      // the plain run_sql path (selection/GO-splitting).
      let out: QueryResult[];
      if (curParams.length > 0 && curSavedQuery && curTab) {
        // Persist values to their scope tiers (d28.4), but ONLY for params declared
        // in the saved query's STORED sql (d28.8): a saved query is a stable
        // template, so SQL edits and edited-in @params are scratch — run this
        // session, remembered nowhere; editing a declared param out is
        // non-destructive. (Explicit save-back = billz-d28.10.) The RUN below still
        // uses curParams + curTab.content, so edited-in params execute fine now.
        const routed = persistDeclared(curSavedQuery, paramValues);
        await saveQuery({ ...curSavedQuery, params: routed.params });
        setSessionParams(routed.session);
        setGlobalParams(routed.global);
        const resolved = toResolvedParams(curParams, paramValues);
        out = await runParams(id, effectiveDb, curTab.content, resolved);
      } else {
        const t = editor?.getRunTarget();
        if (!t) return;
        out = await runSql(id, effectiveDb, t.text, t.selection || null, t.line);
      }
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
    <!-- billz-85b: unlock prompt / 🔒 banner for a session-only active connection.
         Rendered at section top (outside .workspace's 5-track grid) — the modal is
         position:fixed; the banner is a thin block above the content. -->
    {#if showPrompt && lockedConn}
      <PasswordPrompt
        name={lockedConn.name}
        onsubmit={(pw) => unlock(lockedConn.id, pw)}
        oncancel={() => dismissed.add(lockedConn.id)}
      />
    {:else if lockedConn}
      <div class="locked-note">
        🔒 {lockedConn.name} is locked (session-only password not entered).
        <button type="button" onclick={() => dismissed.delete(lockedConn.id)}>Enter password</button>
      </div>
    {/if}
    {#if editing !== undefined}
      {#key editing}
        <ConnectionForm editing={editing} onclose={closeForm} onSessionUnlock={(id) => unlocked.add(id)} />
      {/key}
    {:else}
      <div class="workspace">
        <TabBar />
        <!-- Always a grid child (empty placeholder when no params) so the 5-track
             .workspace template stays aligned (d28.3). -->
        {#if curParams.length > 0}
          <ParamBar params={curParams} values={paramValues} sources={paramSources} onScopeChange={onScopeChange} onTypeChange={onTypeChange} onClearTier={onClearTier} />
        {:else}
          <div class="param-bar-slot"></div>
        {/if}
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
            {#each dbStore.list as db (db.databaseId)}
              <option value={db.name} disabled={db.stateDesc !== "ONLINE"}>
                {db.name}{db.stateDesc !== "ONLINE" ? ` (${db.stateDesc.toLowerCase()})` : ""}
              </option>
            {/each}
          </select>
          <button onclick={run} disabled={running}>{running ? "Running…" : "Run"}</button>
          {#if curSavedQuery}
            <button
              onclick={updateSavedQuery}
              disabled={!dirty}
              title="Save the tab's edited SQL back to this saved query"
            >
              Update saved query
            </button>
          {/if}
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
  .locked-note {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.3rem 0.6rem;
    font-size: 0.8rem;
    color: #92400e;
    background: #fef3c7;
    border-bottom: 1px solid #fcd34d;
  }
  .locked-note button {
    font-size: 0.75rem;
    cursor: pointer;
  }
  .workspace {
    display: grid;
    /* tab bar · param bar · editor · toolbar · grid */
    grid-template-rows: auto auto minmax(8rem, 40%) auto 1fr;
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
