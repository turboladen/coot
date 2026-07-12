// Pure, rune-free param logic for the saved-query param bar (d28.3). Unit-tested
// via paramBarLogic.test.ts; ParamBar.svelte / App.svelte are the runes wrappers.
import type { Param, ResolvedParam } from "./api";

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

// Remember (Local, d28.3): copy of `params` with each lastValue set to the field
// value; a param with no field value keeps its prior lastValue. d28.4 will route
// writes by scope.
export function rememberValues(params: Param[], values: Record<string, string>): Param[] {
  return params.map((p) => (p.name in values ? { ...p, lastValue: values[p.name] } : p));
}

// The next param-bar field values, keyed by name. On a tab `switched`, each field
// resets fresh from its `lastValue` (never bleed one query's values into another).
// Otherwise (same-tab recompute — an SQL edit added/removed a param, or the
// library loaded late) a value already in `prev` (typed but not yet run) is
// PRESERVED, and a newly-appeared param seeds from its `lastValue`. Note: `??`
// falls through only on null/undefined, so a user-cleared "" is preserved.
export function nextParamValues(
  switched: boolean,
  params: Param[],
  prev: Record<string, string>,
): Record<string, string> {
  const next: Record<string, string> = {};
  for (const p of params) {
    next[p.name] = switched ? (p.lastValue ?? "") : (prev[p.name] ?? p.lastValue ?? "");
  }
  return next;
}
