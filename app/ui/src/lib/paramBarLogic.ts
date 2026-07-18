// Pure, rune-free param logic for the saved-query param bar (d28.3). Unit-tested
// via paramBarLogic.test.ts; ParamBar.svelte / App.svelte are the runes wrappers.
import type { ColumnInfo, Param, ResolvedParam, SavedQuery, SqlType } from "./api";

// Ordered, first-appearance, deduped @param names found in NORMAL SQL context only
// (billz-7c9). A small T-SQL lexer skips text where an @word is NOT a parameter:
// single-quote strings (incl. N'…' and the '' escape), [..] / "..." quoted
// identifiers (]] / "" escapes), -- line comments, /* */ block comments (T-SQL
// NESTS these), and @@ system vars. A lone @ / @ before a non-ident is not a param.
//
// This is the SHARED lexical contract with core/src/param_bind.rs::splice_raw_text —
// the two implementations are kept in lockstep by a mirrored edge-case corpus (the
// "billz-7c9 corpus" in both test files). NOT handled (deferred, semantic not
// lexical): a DECLARE'd @local still surfaces as a harmless unset param field.
export function scanParamNames(sql: string): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  const isStart = (c: string) => /[A-Za-z_]/.test(c);
  const isCont = (c: string) => /[A-Za-z0-9_]/.test(c);
  // n=normal s=single-quote br=bracket dq=double-quote lc=line-comment bc=block-comment
  let state: "n" | "s" | "br" | "dq" | "lc" | "bc" = "n";
  let depth = 0; // block-comment nesting depth (only meaningful in "bc")
  let i = 0;
  const n = sql.length;
  while (i < n) {
    const c = sql[i];
    const d = sql[i + 1];
    switch (state) {
      case "n":
        if (c === "'") {
          state = "s";
          i++;
        } else if (c === "[") {
          state = "br";
          i++;
        } else if (c === '"') {
          state = "dq";
          i++;
        } else if (c === "-" && d === "-") {
          state = "lc";
          i += 2;
        } else if (c === "/" && d === "*") {
          state = "bc";
          depth = 1;
          i += 2;
        } else if (c === "@" && d === "@") {
          i += 2; // @@ROWCOUNT etc. — a system var, never a param
        } else if (c === "@" && d !== undefined && isStart(d)) {
          let j = i + 1;
          while (j < n && isCont(sql[j])) j++;
          const name = sql.slice(i, j);
          if (!seen.has(name)) {
            seen.add(name);
            out.push(name);
          }
          i = j;
        } else {
          i++;
        }
        break;
      case "s": // '…' — the '' doubled-quote is an escape, not a close
        if (c === "'" && d === "'") i += 2;
        else if (c === "'") {
          state = "n";
          i++;
        } else i++;
        break;
      case "br": // […] — ]] is an escaped bracket
        if (c === "]" && d === "]") i += 2;
        else if (c === "]") {
          state = "n";
          i++;
        } else i++;
        break;
      case "dq": // "…" — skipped whether QUOTED_IDENTIFIER is ON (ident) or OFF (string)
        if (c === '"' && d === '"') i += 2;
        else if (c === '"') {
          state = "n";
          i++;
        } else i++;
        break;
      case "lc": // -- … to end of line
        if (c === "\n") state = "n";
        i++;
        break;
      case "bc": // /* … */ with nesting
        if (c === "/" && d === "*") {
          depth++;
          i += 2;
        } else if (c === "*" && d === "/") {
          depth--;
          if (depth === 0) state = "n";
          i += 2;
        } else i++;
        break;
    }
  }
  return out;
}

// Ordered param list for a query: scan @names in `sql` (dedup, first-appearance
// order) and merge with `stored` — an existing name keeps its sqlType/scope/
// lastValue; a new name defaults to raw-text (null) / local / unset.
export function deriveParams(sql: string, stored: Param[]): Param[] {
  const byName = new Map(stored.map((p) => [p.name, p]));
  return scanParamNames(sql).map(
    (name) => byName.get(name) ?? { name, sqlType: null, lastValue: null, scope: "local" },
  );
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

// d28.8: persist-back for an opened saved query under the "stable template" model.
// The saved query's param set is defined by its STORED sql (not the edited tab
// content): only DECLARED params are remembered; params edited into the tab are
// scratch (used for the run, discarded). A declared param not currently visible
// (edited out of the tab) keeps its stored value — editing is non-destructive.
// Reuses routeWrites for tier routing; the caller merges session/global + saves
// params exactly as before. See the d28.8 design spec for the full rationale.
export function persistDeclared(
  stored: SavedQuery,
  values: Record<string, string>,
): { params: Param[]; session: Record<string, string>; global: Record<string, string> } {
  const declared = deriveParams(stored.sql, stored.params);
  const toWrite = declared.filter((p) => p.name in values);
  const routed = routeWrites(toWrite, values);
  const written = new Map(routed.params.map((p) => [p.name, p]));
  const params = declared.map((p) => written.get(p.name) ?? p);
  return { params, session: routed.session, global: routed.global };
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
// right-click can scope, d28.7). Uses scanParamNames so @table inside a string/
// comment doesn't count, and @table2 / @tablename / @@table never match (each scans
// to its own full name). @table is the case-sensitive convention openScopedQuery fills.
export function queriesReferencingTable(queries: SavedQuery[]): SavedQuery[] {
  return queries.filter((q) => scanParamNames(q.sql).includes("@table"));
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
