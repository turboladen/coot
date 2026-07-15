// Pure, rune-free fan-out logic — the unit-testable substance of the cross-tenant
// fan-out UI (billz-0gh.1.3). Lives in a plain `.ts` (NOT `.svelte.ts`) so `bun test`
// can import it without a Svelte compiler, mirroring tabsLogic.ts / renderCell.ts.
//
// `import type` (not a value import) so `bun test` never pulls in api.ts's
// `@tauri-apps/api/core` dependency, and to satisfy verbatimModuleSyntax — matches
// sibling tabsLogic.ts.
import type { CellValue, ColumnMeta, DatabaseInfo, DbRunOutcome, QueryResult } from "./api";

// The prepended "database" column for a combined fan-out grid. Its shape matches
// the real ColumnMeta (api.ts) so the synthesized QueryResult renders through the
// existing ResultsGrid unchanged. sqlType "nvarchar" = the tag shown in the header.
const DATABASE_COLUMN: ColumnMeta = {
  name: "database",
  sqlType: "nvarchar",
  nullable: false,
  precision: null,
  scale: null,
};

// A glob filter over database names where `*` is the ONLY wildcard. Escapes every
// other regex metachar (so a literal `.` in a name matches a `.`, not any char),
// then maps `*` → `.*`, anchored + case-INSENSITIVE (SQL Server DB names are
// effectively case-insensitive). Returns names of ONLINE databases only (a
// non-ONLINE DB can't be a run target — mirrors the single-DB picker). An empty
// pattern selects NOTHING (a bare `*` selects all ONLINE) — an empty box shouldn't
// silently target every tenant.
export function matchPattern(pattern: string, dbs: DatabaseInfo[]): string[] {
  if (pattern === "") return [];
  // Escape regex metachars EXCEPT `*`, which we translate to `.*` below.
  const escaped = pattern.replace(/[.+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`^${escaped.replace(/\*/g, ".*")}$`, "i");
  return dbs.filter((d) => d.stateDesc === "ONLINE" && re.test(d.name)).map((d) => d.name);
}

// The identity of a result set's column layout: the ordered sequence of
// name + type. Two result sets combine only if these match exactly. JSON-encoded
// so distinct (name, type) pairs can't alias into an equal string — a plain
// separator could collide when a name/type contains it (SQL permits spaces in
// column names: ["a","b c"] vs ["a b","c"]).
function columnSignature(columns: ColumnMeta[]): string {
  return JSON.stringify(columns.map((c) => [c.name, c.sqlType]));
}

// THE key fn. Decide whether every SUCCESSFUL database's result can be stacked
// into one grid, and if so synthesize that grid.
//
// `canCombine` is true iff every `error == null` outcome has EXACTLY ONE result
// set AND all of them share an identical column signature (same ordered
// name+type). When true, build a combined QueryResult: a leading `database`
// column, then each ok DB's columns; rows = every ok DB's rows in input order,
// each prefixed with a Text cell naming its database. `rowsAffected` is null
// (a synthesized read grid). Errored DBs contribute nothing (their `results` is
// empty anyway). When there are no ok DBs, or shapes diverge, or any ok DB
// returned ≠1 result set → canCombine false / combined null (caller falls back to
// per-DB tabs).
export function combineFanoutResults(
  outcomes: DbRunOutcome[],
): { combined: QueryResult | null; canCombine: boolean } {
  const ok = outcomes.filter((o) => o.error == null);
  if (ok.length === 0) return { combined: null, canCombine: false };
  // Every ok DB must have exactly one result set.
  if (!ok.every((o) => o.results.length === 1)) return { combined: null, canCombine: false };
  // …and all must share one column signature.
  const sig = columnSignature(ok[0].results[0].columns);
  if (!ok.every((o) => columnSignature(o.results[0].columns) === sig)) {
    return { combined: null, canCombine: false };
  }

  const columns: ColumnMeta[] = [DATABASE_COLUMN, ...ok[0].results[0].columns];
  const rows: CellValue[][] = [];
  for (const o of ok) {
    const dbCell: CellValue = { kind: "Text", value: o.database };
    for (const row of o.results[0].rows) {
      rows.push([dbCell, ...row]);
    }
  }
  return { combined: { columns, rows, rowsAffected: null }, canCombine: true };
}

// One status row per outcome — drives the always-on status strip. `rows` sums the
// row counts across a DB's result sets (0 for an errored DB); `ok` mirrors
// error == null. Input order preserved (outcomes come back in input-DB order).
export type FanoutStatus = {
  database: string;
  rows: number;
  ok: boolean;
  error: string | null;
  elapsedMs: number;
};

export function fanoutStatus(outcomes: DbRunOutcome[]): FanoutStatus[] {
  return outcomes.map((o) => ({
    database: o.database,
    rows: o.results.reduce((sum, r) => sum + r.rows.length, 0),
    ok: o.error == null,
    error: o.error,
    elapsedMs: o.elapsedMs,
  }));
}

// Intersect a stored selection with the CURRENT connection's ONLINE databases,
// preserving the selection's order. Mirrors App's `effectiveDb` invariant: a
// persisted fan-out selection can carry names from another connection or DBs that
// have since gone offline, and we must never fan out to a DB absent/unavailable
// on the active connection. run() sends THIS, not the raw stored list.
export function effectiveFanoutDatabases(selected: string[], dbs: DatabaseInfo[]): string[] {
  const online = new Set(dbs.filter((d) => d.stateDesc === "ONLINE").map((d) => d.name));
  return selected.filter((name) => online.has(name));
}
