// Persisted (localStorage) Variables Library — the reusable named values that resolve
// into @name references anywhere (V2). Mirrors globalParams.svelte.ts: load on init,
// persist on write, degrade to [] on a corrupt blob. Mutate the exported $state's
// `list` in place — never reassign the export.
//
// One-time migration: when the new key is ABSENT, seed from the legacy globalParams
// map (coot.globalParams.v1). The legacy key is left intact — rollback-safe.
import { parseStringMap } from "./paramBarLogic";
import { migrateGlobalParams, parseVariables, serializeVariables, type Variable } from "./variablesLogic";

const STORAGE_KEY = "coot.variables.v1";
const LEGACY_GLOBAL_KEY = "coot.globalParams.v1";

function load(): Variable[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw !== null) return parseVariables(raw);
    // No variables yet → one-time migration from the legacy global-params map.
    const migrated = migrateGlobalParams(parseStringMap(localStorage.getItem(LEGACY_GLOBAL_KEY)));
    if (migrated.length > 0) localStorage.setItem(STORAGE_KEY, serializeVariables(migrated));
    return migrated;
  } catch (e) {
    console.warn("coot: failed to load variables from localStorage", e);
    return [];
  }
}

export const variables = $state<{ list: Variable[] }>({ list: load() });

function persist(): void {
  try {
    localStorage.setItem(STORAGE_KEY, serializeVariables(variables.list));
  } catch (e) {
    console.warn("coot: failed to persist variables to localStorage", e);
  }
}

// Add a new variable or replace the existing one of the same name (name is identity).
// The caller validates the name (isValidVariableName) and handles rename before calling.
export function upsertVariable(v: Variable): void {
  const i = variables.list.findIndex((x) => x.name === v.name);
  if (i >= 0) variables.list[i] = v;
  else variables.list.push(v);
  persist();
}

export function removeVariable(name: string): void {
  variables.list = variables.list.filter((v) => v.name !== name);
  persist();
}
