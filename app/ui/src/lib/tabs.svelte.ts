// UI state for the editor tab bar (Svelte 5 runes module) — cwt.8. The single
// source of truth for the open scratch tabs + which one is active, autosaved to
// localStorage so they survive an app restart. Mirrors connections.svelte.ts:
// mutate the exported `$state` object's fields in place — never reassign the export.
//
// Pure logic (deriveTitle/pickNeighbourId/serialize/deserialize) lives in the
// rune-free tabsLogic.ts so it's `bun test`-able; this module is the live-state
// wrapper + the localStorage adapter + the debounce. These tabs are ephemeral
// SCRATCH editors (distinct from the Phase 3 saved-query library). Results are
// never persisted — only the SQL text, the tab set, and the active id.
import { deriveTitle, deserialize, pickNeighbourId, type QueryTab, serialize, type TabsState } from "./tabsLogic";

export type { QueryTab } from "./tabsLogic";

// Preserve today's opening behaviour: a fresh install seeds one runnable tab.
const DEFAULT_CONTENT = "SELECT TOP 100 * FROM sys.objects;";

// Versioned key so a future shape change can migrate/reset cleanly rather than
// reading a stale blob.
const STORAGE_KEY = "coot.queryTabs.v1";
const SAVE_DEBOUNCE_MS = 500;

export const tabsState = $state<TabsState>({ tabs: [], activeId: "" });

// --- localStorage adapter (the swappable persistence seam) -------------------
// Both sides swallow errors and degrade to null/no-op: a corrupt blob or a
// missing localStorage (defensive — the Tauri webview always has it) must never
// brick the editor. Losing an unsaved scratch on a corrupt blob is acceptable for
// ephemeral state and strictly better than a crash; we console.warn so it's not
// fully silent.
function loadRaw(): TabsState | null {
  try {
    return deserialize(localStorage.getItem(STORAGE_KEY));
  } catch (e) {
    console.warn("coot: failed to load query tabs from localStorage", e);
    return null;
  }
}

function saveRaw(state: TabsState): void {
  try {
    localStorage.setItem(STORAGE_KEY, serialize(state));
  } catch (e) {
    console.warn("coot: failed to save query tabs to localStorage", e);
  }
}

// --- Debounced autosave ------------------------------------------------------
let saveTimer: ReturnType<typeof setTimeout> | undefined;

// Debounced save for content edits (coalesces a typing burst). ~500ms: long
// enough to batch keystrokes, short enough that a quit-right-after-typing loses
// at most half a second (and onCloseRequested/flushSave close even that gap).
function scheduleSave(): void {
  if (saveTimer !== undefined) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = undefined;
    saveRaw(tabsState);
  }, SAVE_DEBOUNCE_MS);
}

// Immediate save: cancels any pending debounce and writes now. Used by structural
// ops (new/close/select) where debouncing only risks loss, and by the app's
// window-close hook to persist the last keystrokes on quit.
export function flushSave(): void {
  if (saveTimer !== undefined) {
    clearTimeout(saveTimer);
    saveTimer = undefined;
  }
  saveRaw(tabsState);
}

// --- Helpers -----------------------------------------------------------------
function newQueryTab(
  content: string,
  database: string | null = null,
  savedQueryId: string | null = null,
): QueryTab {
  return {
    id: crypto.randomUUID(),
    title: deriveTitle(content),
    content,
    database,
    savedQueryId,
    fanout: false,
    fanoutDatabases: [],
  };
}

function seedDefault(): void {
  const tab = newQueryTab(DEFAULT_CONTENT);
  tabsState.tabs = [tab];
  tabsState.activeId = tab.id;
}

export function activeContent(): string {
  // A function (not an exported `$derived` const, which has a Svelte-5
  // cross-module reactivity caveat): correctness doesn't need reactivity here —
  // App re-reads it fresh on each `{#key activeId}` remount.
  return tabsState.tabs.find((t) => t.id === tabsState.activeId)?.content ?? "";
}

// --- Ops ---------------------------------------------------------------------
export function newTab(): void {
  const tab = newQueryTab("");
  tabsState.tabs.push(tab);
  tabsState.activeId = tab.id;
  flushSave();
}

// Open a NEW tab pre-seeded with `content` (and optionally a target `database`)
// and make it active. Used by the tree's double-click-table action (rqb.6),
// which passes the table's DB so the new tab's picker reflects it (cwt.9).
// Structural op → flushSave (no debounce).
export function newTabWithContent(
  content: string,
  database: string | null = null,
  savedQueryId: string | null = null,
): void {
  const tab = newQueryTab(content, database, savedQueryId);
  tabsState.tabs.push(tab);
  tabsState.activeId = tab.id;
  flushSave();
}

// Set the active tab's target database (cwt.9). null = connection default. A
// deliberate structural choice → flushSave immediately (like select/new/close),
// so a quit right after picking a DB never loses it.
export function setActiveDatabase(database: string | null): void {
  const tab = tabsState.tabs.find((t) => t.id === tabsState.activeId);
  if (!tab) return;
  tab.database = database;
  flushSave();
}

// Set the active tab's fan-out enable + database selection (billz-0gh.1.3). Sets
// BOTH fields together (the toggle passes the current selection; the picker passes
// the current enable) so there's no merge ambiguity. A deliberate structural
// choice → flushSave immediately (like setActiveDatabase), so a quit right after
// toggling/selecting never loses it.
export function setFanout(fanout: boolean, databases: string[]): void {
  const tab = tabsState.tabs.find((t) => t.id === tabsState.activeId);
  if (!tab) return;
  tab.fanout = fanout;
  tab.fanoutDatabases = databases;
  flushSave();
}

export function selectTab(id: string): void {
  if (tabsState.activeId === id) return;
  tabsState.activeId = id;
  flushSave();
}

export function closeTab(id: string): void {
  // Pick the survivor BEFORE mutating (the neighbour is relative to the current
  // order). null → we're closing the last tab, so reseed a fresh empty one.
  const neighbour = pickNeighbourId(tabsState.tabs, id);
  tabsState.tabs = tabsState.tabs.filter((t) => t.id !== id);
  if (tabsState.tabs.length === 0) {
    const tab = newQueryTab(""); // never zero tabs
    tabsState.tabs = [tab];
    tabsState.activeId = tab.id;
  } else if (tabsState.activeId === id) {
    tabsState.activeId = neighbour ?? tabsState.tabs[0].id;
  }
  flushSave();
}

export function setActiveContent(text: string): void {
  // The editor's onchange calls this on every doc change. Update the active tab's
  // content + recompute its derived title, then debounce a save.
  const tab = tabsState.tabs.find((t) => t.id === tabsState.activeId);
  if (!tab) return;
  tab.content = text;
  tab.title = deriveTitle(text);
  scheduleSave();
}

// Load the persisted set (or seed the default) — called once from App.onMount.
export function restore(): void {
  const loaded = loadRaw();
  if (loaded) {
    tabsState.tabs = loaded.tabs;
    tabsState.activeId = loaded.activeId;
  } else {
    seedDefault();
  }
}
