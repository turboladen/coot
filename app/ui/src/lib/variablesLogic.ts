// Pure, rune-free logic for the Variables Library (V2). Unit-tested via
// variablesLogic.test.ts; variables.svelte.ts / VariablesLibrary.svelte / App.svelte
// are the runes wrappers. A Variable's `name` is its identity == the SQL token minus
// the leading '@' (so "benchmark_user" ↔ @benchmark_user). Values resolve into any
// @name reference; the library always wins over a query input of the same name.
import type { Param, ResolvedParam, SavedQuery, SqlType } from "./api";

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
