// Pure, rune-free param logic for the saved-query param bar (d28.3). Unit-tested
// via paramBarLogic.test.ts; ParamBar.svelte / App.svelte are the runes wrappers.
import type { ColumnInfo, Param, ResolvedParam, SavedQuery, SqlType } from "./api";

// Matches T-SQL param placeholders. `(?<!@)` skips server globals like
// @@ROWCOUNT / @@IDENTITY (the doubled @). NOTE (billz-u2t / PLAN §6): a regex
// still matches @x inside a string literal/comment, or a DECLARE'd @local → a
// harmless extra field (the actual substitution is still d28.2's typed bind /
// flagged raw-text splice). A proper T-SQL lexer would tighten this; deferred
// with u2t.
const PARAM_RE = /(?<!@)@[A-Za-z_]\w*/g;

// Ordered param list for a query: scan @names in `sql` (dedup, first-appearance
// order) and merge with `stored` — an existing name keeps its sqlType/scope/
// lastValue; a new name defaults to raw-text (null) / local / unset.
export function deriveParams(sql: string, stored: Param[]): Param[] {
  const byName = new Map(stored.map((p) => [p.name, p]));
  const seen = new Set<string>();
  const out: Param[] = [];
  for (const m of sql.matchAll(PARAM_RE)) {
    const name = m[0];
    if (seen.has(name)) continue;
    seen.add(name);
    out.push(byName.get(name) ?? { name, sqlType: null, lastValue: null, scope: "local" });
  }
  return out;
}

// Execute-time params from the derived params + the bar's current field values
// (keyed by param name). Missing value → "" (an unset field).
export function toResolvedParams(params: Param[], values: Record<string, string>): ResolvedParam[] {
  return params.map((p) => ({ name: p.name, sqlType: p.sqlType, value: values[p.name] ?? "" }));
}

// Resolve a param's value across tiers (PLAN §5): Local ?? Session ?? Global.
// A missing store key is `undefined` at runtime (Record index), so `??` falls
// through correctly even though TS types the index as `string`.
export function resolve(
  param: Param,
  session: Record<string, string>,
  global: Record<string, string>,
): string | null {
  return param.lastValue ?? session[param.name] ?? global[param.name] ?? null;
}

// Which tier the resolved value comes from (drives the inherited badge, d28.4
// option B). Computed from STORED state, so a field typed-but-not-yet-run still
// reads as inherited until the Run persists it — acceptable ("no Local value
// stored yet; inheriting"). `name in store` mirrors resolve's `??` for our
// Record<string,string> data (no undefined values are ever stored).
export function valueSource(
  param: Param,
  session: Record<string, string>,
  global: Record<string, string>,
): "local" | "session" | "global" | null {
  if (param.lastValue !== null) return "local";
  if (param.name in session) return "session";
  if (param.name in global) return "global";
  return null;
}

// On Run, route each param's field value to ITS scope. Local → lastValue set;
// Session/Global → the returned store map gets the value AND lastValue is cleared
// (so `resolve` falls through to the shared tier). Scope is preserved; the caller
// merges `session`/`global` into the live stores and saves `params`.
//
// BY DESIGN: routing to a tier does NOT clear the other shared tiers. Setting a
// Global value while a Session value for that name is active won't visibly take
// effect until the session ends — Session > Global is the §5 precedence, not a
// bug. Clearing a tier value is a separate feature (deferred, bead).
export function routeWrites(
  params: Param[],
  values: Record<string, string>,
): { params: Param[]; session: Record<string, string>; global: Record<string, string> } {
  const session: Record<string, string> = {};
  const global: Record<string, string> = {};
  const outParams = params.map((p) => {
    const value = values[p.name] ?? "";
    if (p.scope === "session") {
      session[p.name] = value;
      return { ...p, lastValue: null };
    }
    if (p.scope === "global") {
      global[p.name] = value;
      return { ...p, lastValue: null };
    }
    return { ...p, lastValue: value };
  });
  return { params: outParams, session, global };
}

// Map a catalog canonical type (ColumnInfo.data_type, e.g. "nvarchar(50)",
// "decimal(19,4)", "int") into the capped SqlType set, or null (→ raw-text) for
// types outside it. Widens (smallint→int, datetime→datetime2, varchar→nvarchar):
// SqlType is the deliberately capped "things you filter by" set, and the driver
// derives the sp_executesql declaration from the bound value at runtime, so a
// widened tag still binds correctly. Exported for d28.7's right-click auto-fill.
export function catalogTypeToSqlType(dataType: string): SqlType | null {
  const base = dataType.split("(")[0].trim().toLowerCase();
  switch (base) {
    case "int":
    case "tinyint":
    case "smallint":
      return "int";
    case "bigint":
      return "bigint";
    case "nvarchar":
    case "nchar":
    case "varchar":
    case "char":
      return "nvarchar";
    case "bit":
      return "bit";
    case "date":
      return "date";
    case "datetime2":
    case "datetime":
    case "smalldatetime":
      return "datetime2";
    case "decimal":
    case "numeric":
      return "decimal";
    case "money":
    case "smallmoney":
      return "money";
    case "uniqueidentifier":
      return "uniqueidentifier";
    default:
      return null;
  }
}

// Saved queries whose SQL references the literal @table param (the ones a table
// right-click can scope, d28.7). `(?<!@)` skips @@table; `(?![A-Za-z0-9_])` is the
// word boundary so @table2 / @tablename don't count. Case-sensitive: @table is the
// convention that openScopedQuery fills.
export function queriesReferencingTable(queries: SavedQuery[]): SavedQuery[] {
  const re = /(?<!@)@table(?![A-Za-z0-9_])/;
  return queries.filter((q) => re.test(q.sql));
}

// Auto-type params from a table's columns (d28.7, consuming d28.5's mapping): for
// each param that is NOT @table, is UNSET (sqlType null), and whose name (minus @,
// case-insensitive) matches a column, set its sqlType via catalogTypeToSqlType
// (itself null for a float/real/etc. column → stays raw-text). Already-typed params,
// @table, and non-matching params are unchanged (manual overrides win).
export function autoTypeParams(params: Param[], columns: ColumnInfo[]): Param[] {
  const byLowerName = new Map(columns.map((c) => [c.name.toLowerCase(), c]));
  return params.map((p) => {
    if (p.name === "@table" || p.sqlType !== null) return p;
    const c = byLowerName.get(p.name.slice(1).toLowerCase());
    return c ? { ...p, sqlType: catalogTypeToSqlType(c.dataType) } : p;
  });
}

// Tolerant parse of a persisted name→value map (globalParams). null / malformed
// / non-object / non-string entries degrade to {} (mirrors tabsLogic.deserialize).
export function parseStringMap(raw: string | null): Record<string, string> {
  if (raw === null) return {};
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return {};
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return {};
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(parsed)) {
    if (typeof v === "string") out[k] = v;
  }
  return out;
}

// The next param-bar field values, keyed by name. On a tab `switched`, each field
// resets fresh from `resolve` (Local ?? Session ?? Global); otherwise a value the
// user typed (`prev`) is PRESERVED and a newly-appeared param seeds from resolve.
// `??` preserves a user-cleared "". Stores are passed as plain snapshots by the
// caller (read untracked, so a Session change propagates on next switch, not
// disruptively mid-edit).
export function nextParamValues(
  switched: boolean,
  params: Param[],
  prev: Record<string, string>,
  session: Record<string, string>,
  global: Record<string, string>,
): Record<string, string> {
  const next: Record<string, string> = {};
  for (const p of params) {
    const r = resolve(p, session, global) ?? "";
    next[p.name] = switched ? r : (prev[p.name] ?? r);
  }
  return next;
}
