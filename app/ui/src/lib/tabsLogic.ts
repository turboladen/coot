// Pure, rune-free tab logic — the unit-testable substance of the editor tab bar
// (cwt.8). Lives in a plain `.ts` (NOT `.svelte.ts`) so `bun test` can import it
// without a Svelte compiler; `tabs.svelte.ts` is the runes wrapper that holds the
// live `$state` and delegates here. Mirrors the repo's renderCell.ts/resultSummary.ts
// pure-helper pattern.

// `import type` (not a value import) so `bun test` never pulls in api.ts's
// `@tauri-apps/api/core` dependency, and to satisfy verbatimModuleSyntax — matches
// sibling savedQueriesLogic.ts.
import type { SavedQuery } from "./api";

// `database`: the tab's target DB for the runner (billz-cwt.9). null = the
// connection's default DB. Each tab carries its own, so one tab can sit on
// ESP_Arnotts_Group_DEV while another targets ESP_Suntory_DEV (PLAN §4/§5).
export type QueryTab = {
  id: string;
  title: string;
  content: string;
  database: string | null;
  // The connection this tab targets (billz-a5y.1). null = no connection → the
  // empty state (Run nudges "Select a connection first."). Each tab owns its own,
  // so one tab can run against server X while another targets server Y — run() and
  // the toolbar read THIS, not the global active connection (PLAN §4/§5).
  connectionId: string | null;
  // The saved query this tab was opened from (d28.3) — drives the param bar.
  // null = a plain scratch tab.
  savedQueryId: string | null;
  // Cross-tenant fan-out (billz-0gh.1.3): when true, Run fans the batch out across
  // `fanoutDatabases` in parallel instead of the single-DB path. Per-tab so one tab
  // can fan out while another runs a normal single-DB query.
  fanout: boolean;
  fanoutDatabases: string[];
};

export type TabsState = { tabs: QueryTab[]; activeId: string };

// billz-kno: does this tab have unsaved edits against its linked saved query?
// A scratch tab (no savedQueryId) is never dirty. A tab whose linked query is
// gone (deleted from the library) is treated as NOT dirty — there's nothing to be
// dirty against — matching App.svelte's active-tab `dirty` which requires the
// saved query to exist. Exact-string compare: a trailing-newline-only diff reads
// dirty (honest + simplest for a single-user tool).
export function isTabDirty(tab: QueryTab, saved: SavedQuery[]): boolean {
  if (tab.savedQueryId == null) return false;
  const q = saved.find((s) => s.id === tab.savedQueryId);
  return q != null && tab.content !== q.sql;
}

// Derive a tab's display title from its content: the first non-empty (trimmed)
// line, truncated to ~24 chars; "Untitled" when the content is empty/whitespace.
// Titles are derived, not free-typed, this wave (no rename polish — out of scope).
const TITLE_MAX = 24;
export function deriveTitle(content: string): string {
  for (const line of content.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.length > 0) {
      return trimmed.length > TITLE_MAX ? trimmed.slice(0, TITLE_MAX - 1) + "…" : trimmed;
    }
  }
  return "Untitled";
}

// Which tab should become active after `closingId` is removed. Picks the previous
// neighbour (clamped to index 0), so closing a middle/last tab lands on its left
// neighbour and closing the first lands on the new first. Returns null when the
// closing tab is the only one (caller reseeds a fresh empty tab). If `closingId`
// isn't found, returns the current first tab's id (or null if empty).
export function pickNeighbourId(tabs: QueryTab[], closingId: string): string | null {
  const idx = tabs.findIndex((t) => t.id === closingId);
  if (idx === -1) return tabs.length > 0 ? tabs[0].id : null;
  if (tabs.length <= 1) return null;
  const neighbourIdx = idx > 0 ? idx - 1 : 1; // left neighbour, or the new-first
  return tabs[neighbourIdx].id;
}

// Serialize the tab set for persistence. Plain JSON of the whole state shape.
export function serialize(state: TabsState): string {
  return JSON.stringify(state);
}

// Parse a persisted blob back into a valid TabsState, or null when it's
// malformed/empty (caller then seeds the default tab). Repairs a dangling
// activeId (one matching no tab) by falling back to the first tab's id, so a
// partially-valid blob stays usable rather than being discarded.
export function deserialize(json: string | null): TabsState | null {
  if (json === null) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const obj = parsed as Record<string, unknown>;
  if (!Array.isArray(obj.tabs)) return null;
  const tabs: QueryTab[] = [];
  for (const t of obj.tabs) {
    if (
      typeof t === "object" && t !== null
      && typeof (t as Record<string, unknown>).id === "string"
      && typeof (t as Record<string, unknown>).title === "string"
      && typeof (t as Record<string, unknown>).content === "string"
    ) {
      const raw = t as Record<string, unknown>;
      // `database` is read-tolerant: a pre-cwt.9 blob has no key, and a garbled
      // value shouldn't poison the whole set — anything that isn't a non-empty
      // string becomes null (= connection default), matching the field's "unset"
      // meaning. Empty string is normalized too, so a corrupt blob can't produce
      // a `USE []` (invalid T-SQL) at run time.
      const database = typeof raw.database === "string" && raw.database !== "" ? raw.database : null;
      const savedQueryId = typeof raw.savedQueryId === "string" && raw.savedQueryId !== ""
        ? raw.savedQueryId
        : null;
      // `fanout`/`fanoutDatabases` are read-tolerant like `database` above: a
      // pre-fanout blob has neither key, and a garbled value (non-bool / non-
      // string-array) must default rather than poison the whole set. Non-string
      // array entries are dropped so a corrupt blob can't smuggle a bad DB name.
      const fanout = typeof raw.fanout === "boolean" ? raw.fanout : false;
      const fanoutDatabases = Array.isArray(raw.fanoutDatabases)
        ? (raw.fanoutDatabases.filter((x): x is string => typeof x === "string"))
        : [];
      // `connectionId` is read-tolerant like `database` above (billz-a5y.1): a
      // pre-a5y.1 blob has no key, and a garbled value must default rather than
      // poison the whole set. Anything that isn't a non-empty string becomes null
      // (= no connection → empty state); an empty string can't smuggle a dead id.
      const connectionId = typeof raw.connectionId === "string" && raw.connectionId !== ""
        ? raw.connectionId
        : null;
      tabs.push({
        id: raw.id as string,
        title: raw.title as string,
        content: raw.content as string,
        database,
        savedQueryId,
        connectionId,
        fanout,
        fanoutDatabases,
      });
    } else {
      return null; // any malformed tab poisons the blob → reseed default
    }
  }
  if (tabs.length === 0) return null;
  const activeId = typeof obj.activeId === "string" && tabs.some((t) => t.id === obj.activeId)
    ? obj.activeId
    : tabs[0].id; // repair a dangling/absent activeId
  return { tabs, activeId };
}
