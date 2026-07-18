// Pure, rune-free logic for the Variables Library (V2). Unit-tested via
// variablesLogic.test.ts; variables.svelte.ts / VariablesLibrary.svelte / App.svelte
// are the runes wrappers. A Variable's `name` is its identity == the SQL token minus
// the leading '@' (so "benchmark_user" ↔ @benchmark_user). Values resolve into any
// @name reference; the library always wins over a query input of the same name.
import type { Param, ResolvedParam, SavedQuery, SqlType } from "./api";
import { deriveParams } from "./paramBarLogic";

export type Variable = {
  name: string; // identity == token minus '@'; must match /^[A-Za-z_]\w*$/
  value: string;
  sqlType: SqlType | null; // null → raw-text (unsafe literal splice)
  note: string; // optional memory aid; decoration only, never identity
};

const NAME_RE = /^[A-Za-z_]\w*$/;

export function isValidVariableName(name: string): boolean {
  return NAME_RE.test(name);
}

export function indexByName(vars: Variable[]): Map<string, Variable> {
  return new Map(vars.map((v) => [v.name, v]));
}

// Strip a leading '@' from a param name and look up its library variable (or null).
export function variableFor(paramName: string, byName: Map<string, Variable>): Variable | null {
  return byName.get(paramName.replace(/^@/, "")) ?? null;
}

export function buildInsertToken(v: Variable): string {
  return `@${v.name}`;
}

const SQL_TYPES: readonly SqlType[] = [
  "int", "bigint", "nvarchar", "bit", "date", "datetime2", "decimal", "uniqueidentifier", "money",
];

function asSqlType(v: unknown): SqlType | null {
  return typeof v === "string" && (SQL_TYPES as readonly string[]).includes(v) ? (v as SqlType) : null;
}

// Tolerant parse of coot.variables.v1 (a JSON array of Variable). Drops malformed
// entries (bad/absent name or value); unknown sqlType → null (raw-text); absent note
// → "". Degrades to [] on null / non-array / bad JSON. Mirrors parseStringMap.
export function parseVariables(raw: string | null): Variable[] {
  if (raw === null) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const out: Variable[] = [];
  for (const e of parsed) {
    if (typeof e !== "object" || e === null) continue;
    const o = e as Record<string, unknown>;
    if (typeof o.name !== "string" || !isValidVariableName(o.name)) continue;
    if (typeof o.value !== "string") continue;
    out.push({
      name: o.name,
      value: o.value,
      sqlType: asSqlType(o.sqlType),
      note: typeof o.note === "string" ? o.note : "",
    });
  }
  return out;
}

export function serializeVariables(vars: Variable[]): string {
  return JSON.stringify(vars);
}

// One-time migration: legacy coot.globalParams.v1 (Record<'@name', value>) → Variable[].
// Strips the leading '@'; defaults to nvarchar (safe bind — the old map stored no type)
// and an empty note; skips keys that aren't identifier-safe after stripping.
export function migrateGlobalParams(global: Record<string, string>): Variable[] {
  const out: Variable[] = [];
  for (const [key, value] of Object.entries(global)) {
    const name = key.replace(/^@/, "");
    if (!isValidVariableName(name)) continue;
    out.push({ name, value, sqlType: "nvarchar", note: "" });
  }
  return out;
}

// Execute-time params. A library-matched @name binds to the VARIABLE's value+type
// (the library always wins — its own param sqlType is ignored). Every other @name is a
// query input taking its bar field value + the param's own type. Feeds run_params.
export function resolveRun(
  params: Param[],
  values: Record<string, string>,
  byName: Map<string, Variable>,
): ResolvedParam[] {
  return params.map((p) => {
    const v = variableFor(p.name, byName);
    if (v) return { name: p.name, sqlType: v.sqlType, value: v.value };
    return { name: p.name, sqlType: p.sqlType, value: values[p.name] ?? "" };
  });
}

// Persist a saved query's INPUT values back as lastValue (per-query memory). Only
// DECLARED params (from the stored SQL) are remembered — edited-in @params are scratch
// (mirrors the old persistDeclared "stable template" rule). Library-matched params are
// skipped: the library owns their value, not the query.
export function persistInputs(
  stored: SavedQuery,
  values: Record<string, string>,
  byName: Map<string, Variable>,
): Param[] {
  const declared = deriveParams(stored.sql, stored.params);
  return declared.map((p) => {
    if (variableFor(p.name, byName)) return p; // library owns it
    if (!(p.name in values)) return p; // not surfaced this run
    return { ...p, lastValue: values[p.name] ?? "" };
  });
}
