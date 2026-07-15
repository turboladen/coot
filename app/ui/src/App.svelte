<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { type ConnectionConfig, type DbRunOutcome, type ParamScope, type QueryResult, type SqlType, runFanout, runParams, runSql, setSessionPassword } from "./lib/api";
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
  import FanoutPicker from "./lib/FanoutPicker.svelte";
  import FanoutStatusBar from "./lib/FanoutStatusBar.svelte";
  import { combineFanoutResults, effectiveFanoutDatabases } from "./lib/fanoutLogic";
  import { type Message, summarize } from "./lib/resultSummary";
  import { deriveParams, nextParamValues, persistDeclared, resolve, toResolvedParams, valueSource } from "./lib/paramBarLogic";
  import { clearSessionParam, sessionParams, setSessionParams } from "./lib/sessionParams.svelte";
  import { clearGlobalParam, globalParams, setGlobalParams } from "./lib/globalParams.svelte";
  import { conns, refresh } from "./lib/connections.svelte";
  import { library, refresh as refreshLibrary, save as saveQuery } from "./lib/savedQueries.svelte";
  import { activeContent, flushSave, restore, setActiveContent, setActiveDatabase, setFanout, tabsState } from "./lib/tabs.svelte";
  import { isTabDirty } from "./lib/tabsLogic";
  import { treeRefresh } from "./lib/tree/refresh.svelte";
  import { dbStore, load as loadDatabases } from "./lib/databases.svelte";
  import { setTheme, theme } from "./lib/theme.svelte";
  import { Database, Monitor, Moon, Network, Play, Save, Sun } from "./lib/icons";

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

  // Cross-tenant fan-out run state (billz-0gh.1.3). `fanoutResults` (null = not a
  // fan-out run) holds one outcome per database, in input order; when present the
  // results area renders the fan-out surfaces instead of the plain ResultTabs.
  // `selectedFanoutDb` focuses one DB's grid in the per-DB fallback mode.
  let fanoutResults = $state<DbRunOutcome[] | null>(null);
  let selectedFanoutDb = $state(0);

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

  // ids that are session-only and not yet unlocked this session → "locked"
  // (xhv.2: surfaces existing unlocked/rememberPassword state as the
  // ConnectionList row status dot — no new state, no behavior change.)
  const lockedIds = $derived(
    new Set(conns.list.filter((c) => !c.rememberPassword && !unlocked.has(c.id)).map((c) => c.id)),
  );

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

  // Fan-out (billz-0gh.1.3). The stored selection intersected with the CURRENT
  // connection's ONLINE databases (the effectiveDb invariant — never fan out to a
  // DB absent/offline on this connection; a persisted selection can carry stale
  // names). run() sends this, not the raw stored list.
  const effFanoutDbs = $derived(
    curTab ? effectiveFanoutDatabases(curTab.fanoutDatabases, dbStore.list) : [],
  );
  // Whether the last fan-out run's ok DBs stack into one grid (identical column
  // shapes, one result set each) + the synthesized combined QueryResult. Null when
  // not a fan-out run.
  const fanoutCombined = $derived(fanoutResults ? combineFanoutResults(fanoutResults) : null);
  // Fan-out doesn't support parameter binding (it runs the raw batch text, the
  // run_sql analog). Disable the toggle on a param'd saved-query tab rather than
  // silently fanning out unbound SQL. Parameterized fan-out is deferred (billz-0gh.1.4).
  const fanoutDisabled = $derived(curParams.length > 0);

  // d28.10: the active saved-query tab has unsaved SQL edits (tab content differs
  // from the stored definition). Gates the "Update saved query" button and doubles
  // as the unsaved-edits signal. Exact-string compare — a trailing-newline-only
  // diff reads as dirty (honest + simplest for a single-user tool).
  const dirty = $derived(!!curTab && isTabDirty(curTab, library.list));

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

  // Messages-pane content for a whole fan-out run: a header line, then one error
  // line per failed DB (successes are summarized by the status strip + the grid).
  // Keeps the Messages tab honest and consistent with the single-DB path.
  function fanoutMessages(outcomes: DbRunOutcome[]): Message[] {
    const okCount = outcomes.filter((o) => o.error == null).length;
    const header: Message = {
      kind: okCount === outcomes.length ? "info" : "error",
      text: `Fan-out across ${outcomes.length} database${outcomes.length === 1 ? "" : "s"} — ${okCount} ok, ${outcomes.length - okCount} failed.`,
    };
    const errs = outcomes
      .filter((o) => o.error != null)
      .map((o): Message => ({ kind: "error", text: `${o.database}: ${o.error}` }));
    return [header, ...errs];
  }

  // Messages-pane content for ONE database's slice, shown next to that DB's grid
  // in the per-DB fallback: its own success summary, or its error line.
  function perDbMessages(outcome: DbRunOutcome): Message[] {
    if (outcome.error != null) return [{ kind: "error", text: outcome.error }];
    return summarize(outcome.results);
  }

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
      // Fan-out path (billz-0gh.1.3): run the raw batch across many DBs in
      // parallel. Takes priority over the param/run_sql paths; the toggle is
      // disabled on a param'd tab (fanoutDisabled), so a fan-out tab never carries
      // @params. Uses effFanoutDbs (stored ∩ current ONLINE) — an empty set (no
      // selection, or all stale) nudges instead of calling the backend.
      if (curTab?.fanout) {
        if (effFanoutDbs.length === 0) {
          results = null;
          fanoutResults = null;
          messages = [{ kind: "error", text: "Select at least one database to fan out to." }];
          activeTab = "messages";
          return;
        }
        const t = editor?.getRunTarget();
        if (!t) return;
        const out = await runFanout(id, effFanoutDbs, t.text, t.selection || null, t.line);
        // An empty/whitespace/comment-only batch runs but yields no outcomes
        // (getRunTarget() returns a truthy {text:""} so the `!t` guard above
        // doesn't catch it). Nudge cleanly instead of rendering an empty fan-out
        // surface — mirrors the single-DB path's "nothing to run" handling.
        if (out.length === 0) {
          fanoutResults = null;
          results = null;
          messages = [{ kind: "info", text: "Nothing to run." }];
          activeTab = "messages";
          return;
        }
        fanoutResults = out;
        results = null;
        selectedFanoutDb = 0;
        messages = fanoutMessages(out);
        // Land on the grid when there's one to show, else the Messages pane —
        // mirrors the single-DB path's `out.length > 0 ? 0 : "messages"`. Combined
        // mode always has a grid (≥1 column); fallback keys off the first DB's own
        // result sets, so an errored/DML first DB opens on Messages (its error),
        // not a misleading "Run a query to see results." placeholder.
        const combo = combineFanoutResults(out);
        activeTab = combo.canCombine || out[0].results.length > 0 ? 0 : "messages";
        return;
      }
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
      fanoutResults = null;
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
    curTab?.fanout; // and whenever fan-out is toggled on/off — a mode switch, so
    // the pane must not keep the other mode's stale grid (toggling the box
    // selection keeps `fanout` true, so mid-selection edits don't clear results).
    results = null;
    fanoutResults = null;
    selectedFanoutDb = 0;
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
    <div class="brand">
      <Database size={16} /> <span>billz</span>
      <!-- Color-theme control (billz-xhv.6): System / Light / Dark. `system`
           follows the OS; the others pin `data-theme` on <html>. -->
      <div class="theme-toggle" role="group" aria-label="Color theme">
        <button
          class:active={theme.choice === "system"}
          onclick={() => setTheme("system")}
          title="Follow system theme"
          aria-label="Follow system theme"
          aria-pressed={theme.choice === "system"}
        ><Monitor size={14} /></button>
        <button
          class:active={theme.choice === "light"}
          onclick={() => setTheme("light")}
          title="Light theme"
          aria-label="Light theme"
          aria-pressed={theme.choice === "light"}
        ><Sun size={14} /></button>
        <button
          class:active={theme.choice === "dark"}
          onclick={() => setTheme("dark")}
          title="Dark theme"
          aria-label="Dark theme"
          aria-pressed={theme.choice === "dark"}
        ><Moon size={14} /></button>
      </div>
    </div>
    <ConnectionList lockedIds={lockedIds} onnew={openNew} onedit={openEdit} />
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
             its value feeds runSql, where the executor issues USE [db]. The
             fan-out toggle (billz-0gh.1.3) swaps the single picker for a
             multi-select checklist and routes Run through runFanout. -->
        <div class="toolbar" class:fanout={curTab?.fanout}>
          <Database size={14} />
          <!-- Fan-out toggle. Disabled on a param'd tab (fan-out runs raw batch
               text — no param binding this wave). -->
          <button
            class="fanout-toggle"
            class:active={curTab?.fanout}
            disabled={!curTab || fanoutDisabled}
            aria-pressed={!!curTab?.fanout}
            title={fanoutDisabled
              ? "Fan-out doesn't support parameters yet."
              : "Fan-out: run this query across many databases in parallel"}
            onclick={() => curTab && setFanout(!curTab.fanout, curTab.fanoutDatabases)}
          >
            <Network size={14} /> Fan-out
          </button>
          {#if curTab?.fanout}
            <FanoutPicker selected={effFanoutDbs} onchange={(dbs) => setFanout(true, dbs)} />
          {:else}
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
          {/if}
          <button class="primary" onclick={run} disabled={running}><Play size={14} /> {running ? "Running…" : "Run"}</button>
          {#if curSavedQuery}
            <button
              onclick={updateSavedQuery}
              disabled={!dirty}
              title="Save the tab's edited SQL back to this saved query"
            >
              <Save size={14} /> Update saved query
            </button>
          {/if}
        </div>
        <div class="grid-pane">
          {#if fanoutResults}
            <!-- Fan-out results (billz-0gh.1.3): an always-on per-DB status strip
                 over EITHER one combined grid (uniform shapes) or a per-DB tab
                 strip (differing shapes / multi-result-set). Both reuse
                 ResultTabs/ResultsGrid unchanged — the combined grid is a normal
                 synthesized QueryResult with a leading `database` column. -->
            <div class="fanout-results">
              {#if fanoutCombined?.canCombine && fanoutCombined.combined}
                <FanoutStatusBar outcomes={fanoutResults} />
                <div class="fanout-grid">
                  <ResultTabs results={[fanoutCombined.combined]} {messages} bind:activeTab />
                </div>
              {:else}
                <!-- Per-DB fallback: clicking a status chip focuses that DB's grid
                     (reset activeTab so a stale index can't show an empty pane). -->
                <FanoutStatusBar
                  outcomes={fanoutResults}
                  selectable
                  selectedIndex={selectedFanoutDb}
                  onselect={(i) => {
                    selectedFanoutDb = i;
                    // Focus the DB's grid, or its Messages pane when it has no
                    // result sets (errored/DML) — else ResultTabs would show the
                    // "Run a query" placeholder next to a selected-but-empty DB.
                    activeTab = (fanoutResults?.[i]?.results.length ?? 0) > 0 ? 0 : "messages";
                  }}
                />
                <div class="fanout-grid">
                  <ResultTabs
                    results={fanoutResults[selectedFanoutDb]?.results ?? []}
                    messages={fanoutResults[selectedFanoutDb]
                      ? perDbMessages(fanoutResults[selectedFanoutDb])
                      : []}
                    bind:activeTab
                  />
                </div>
              {/if}
            </div>
          {:else}
            <ResultTabs {results} {messages} bind:activeTab />
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
  }
  /* Two-row sidebar: ConnectionList at natural height, the tree scrolling below
     it. min-height:0 on the tree region lets it shrink so it scrolls internally
     under a long connection list. */
  aside {
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    background: var(--panel);
    overflow: hidden;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-3) var(--sp-3);
    font-weight: 700;
    letter-spacing: -0.02em;
    border-bottom: 1px solid var(--border);
    color: var(--text);
  }
  .brand :global(svg) { color: var(--brand); }
  /* Color-theme control (billz-xhv.6). Its own class — NOT `.mode-toggle`, which
     owns the Objects/Library selector — sharing would couple unrelated controls.
     Lives INSIDE the `.brand` header (pushed right via margin-auto), so there's
     no border-top bar to double up against the brand's border-bottom. Icon-only
     ghost buttons; the active state reuses the accent-tint visual language. */
  .theme-toggle {
    display: flex;
    gap: 0.15rem;
    margin-left: auto;
  }
  .theme-toggle button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.2rem 0.3rem;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--faint);
    cursor: pointer;
    transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
  }
  .theme-toggle button:hover:not(.active) { color: var(--muted); }
  .theme-toggle button.active {
    background: color-mix(in srgb, var(--accent) 14%, var(--raised));
    color: var(--accent-press);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
  }
  /* Icons follow the button's own colour (currentColor), overriding the
     brand-purple `.brand :global(svg)` rule above via later source order. */
  .theme-toggle :global(svg) { color: inherit; }
  /* Segmented [Objects | Library] toggle between the connection list and the
     scrolling lower region. */
  .mode-toggle {
    display: flex;
    gap: 0.25rem;
    padding: 0.3rem 0.5rem;
    border-top: 1px solid var(--border);
  }
  .mode-toggle button {
    flex: 1;
    font: inherit;
    font-size: var(--fs-sm);
    padding: var(--sp-1) var(--sp-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--raised);
    color: var(--muted);
    cursor: pointer;
    transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
  }
  .mode-toggle button.active {
    background: color-mix(in srgb, var(--accent) 14%, var(--raised));
    color: var(--accent-press);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
    font-weight: 600;
  }
  .lower-pane {
    flex: 1;
    min-height: 0;
    overflow: auto;
    border-top: 1px solid var(--border);
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
    color: var(--warn);
    background: color-mix(in srgb, var(--warn) 12%, var(--raised));
    border-bottom: 1px solid color-mix(in srgb, var(--warn) 30%, var(--border));
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
    border-bottom: 1px solid var(--border);
    min-height: 0;
    overflow: hidden;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.4rem 0.6rem;
    border-bottom: 1px solid var(--border);
  }
  .toolbar :global(svg) { color: var(--muted); flex: none; }
  /* The primary Run button's Play icon must read on the teal fill, not muted. */
  .toolbar button.primary :global(svg) { color: var(--accent-fg); }
  .toolbar button {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-1);
  }
  .db-picker {
    font: inherit;
    font-size: 0.85rem;
    max-width: 16rem;
    padding: 0.15rem 0.3rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--raised);
    color: var(--text);
  }
  .db-picker:disabled { color: var(--faint); }
  /* Fan-out toggle: a ghost button that reuses the theme-toggle's accent-tint
     active language. When on, the toolbar wraps so the checklist gets its own row. */
  .toolbar.fanout {
    flex-wrap: wrap;
    align-items: flex-start;
  }
  .fanout-toggle {
    flex: none;
    padding: 0.2rem 0.5rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--raised);
    color: var(--muted);
    cursor: pointer;
    transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
  }
  .fanout-toggle:hover:not(:disabled):not(.active) { color: var(--text); }
  .fanout-toggle.active {
    background: color-mix(in srgb, var(--accent) 14%, var(--raised));
    color: var(--accent-press);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
    font-weight: 600;
  }
  .fanout-toggle:disabled { color: var(--faint); cursor: default; }
  .fanout-toggle.active :global(svg) { color: inherit; }
  .grid-pane {
    min-height: 0;
    overflow: hidden;
  }
  /* Fan-out results: status strip (natural height) over the grid area (fills). */
  .fanout-results {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .fanout-grid {
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
  }
</style>
