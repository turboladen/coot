# Variables Library Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the hidden `@param` scope-tier system into a browsable, named **Variables Library** whose values resolve into `@name` references in *any* editor tab.

**Architecture:** Frontend-only + localStorage. A new `coot.variables.v1` store holds named `Variable`s (name = token minus `@`). At run time, any `@name` that matches a library variable binds to that variable's value+type; every other `@name` is a per-run query input. The `Local/Session/Global` scope dropdown and Session tier are removed — scope is *implied by name*, library always wins. No Rust/Tauri/driver changes: `run_params` already accepts `ResolvedParam[]`.

**Tech Stack:** Svelte 5 (runes), TypeScript (strict), `bun:test`, CodeMirror 6, Tauri v2. Pure logic lives in rune-free `*.ts` (unit-tested); `*.svelte`/`*.svelte.ts` are the live wrappers (verified by `bun run check` + visual).

## Global Constraints

- **bun only** for install/scripts/tests — never npm/pnpm/yarn/node.
- `cargo fmt` + `cargo clippy` + `bun run check` clean before "done". **Warnings are errors.**
- `SqlType` is the capped 9-tag set: `int, bigint, nvarchar, bit, date, datetime2, decimal, uniqueidentifier, money`. Do not add tags.
- Decimals/money cross the JSON boundary as **strings**.
- Pure logic in rune-free `.ts` files so it's `bun test`-able; runes only in `.svelte`/`.svelte.ts`.
- Secrets never touch disk in plaintext — N/A here (variables are query inputs, not secrets), but never store a value that looks like a credential to Keychain-only paths; localStorage is fine for query inputs.
- Follow existing patterns: store wrappers mirror `globalParams.svelte.ts`; panels mirror `SavedQueryLibrary.svelte`; tolerant parsers mirror `parseStringMap`.
- Commit after every green step. Fish shell: emit fish-compatible commands.
- Run frontend tests with `just ui-test` (`cd app/ui && bun test`); typecheck with `just ui-check` (`cd app/ui && bun run check`).

---

## File Structure

**New:**
- `app/ui/src/lib/variablesLogic.ts` — pure, rune-free: `Variable` type, name validation, index, `variableFor`, `buildInsertToken`, tolerant parse/serialize, `migrateGlobalParams`, `resolveRun`, `persistInputs`.
- `app/ui/src/lib/variablesLogic.test.ts` — unit tests (the `paramBarLogic.test.ts` pattern).
- `app/ui/src/lib/variables.svelte.ts` — runes store for `coot.variables.v1` (load + one-time migration, `upsertVariable`, `removeVariable`). Thin wrapper, no unit test (mirrors `globalParams.svelte.ts` precedent).
- `app/ui/src/lib/VariablesLibrary.svelte` — the panel (mirrors `SavedQueryLibrary.svelte`), rendered inside the Library sidebar mode's `Variables` sub-view.

**Modified:**
- `app/ui/src/lib/SqlEditor.svelte` — add `insertAtCursor(text)` to the `bind:this` public API.
- `app/ui/src/lib/ParamBar.svelte` — chip (library hit) vs input (query input) rendering; drop the scope `<select>`, tier badges, and clear-tier buttons.
- `app/ui/src/App.svelte` — remove the `curSavedQuery` gate on `curParams`; add `varsByName`/`libraryHits` deriveds; add the `Saved queries | Variables` Library sub-view toggle + `VariablesLibrary`; rewrite the run() param branch to use `resolveRun`/`persistInputs`; drop tier writes and now-unused imports.

**Untouched (by design):** `paramBarLogic.ts`, `globalParams.svelte.ts`, `sessionParams.svelte.ts`, `param_bind.rs`, `run_params`. The old tier functions become unused but stay (cleanup is a filed follow-on bead, so their tests keep passing).

---

## Task 1: `variablesLogic.ts` — type, validation, indexing, insert token

**Files:**
- Create: `app/ui/src/lib/variablesLogic.ts`
- Test: `app/ui/src/lib/variablesLogic.test.ts`

**Interfaces:**
- Consumes: `SqlType`, `Param`, `ResolvedParam`, `SavedQuery` from `./api`.
- Produces:
  - `type Variable = { name: string; value: string; sqlType: SqlType | null; note: string }`
  - `isValidVariableName(name: string): boolean`
  - `indexByName(vars: Variable[]): Map<string, Variable>`
  - `variableFor(paramName: string, byName: Map<string, Variable>): Variable | null`
  - `buildInsertToken(v: Variable): string`

- [ ] **Step 1: Write the failing test**

Create `app/ui/src/lib/variablesLogic.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import type { Variable } from "./variablesLogic";
import {
  buildInsertToken,
  indexByName,
  isValidVariableName,
  variableFor,
} from "./variablesLogic";

const v = (name: string, value = "x"): Variable => ({ name, value, sqlType: "nvarchar", note: "" });

describe("isValidVariableName", () => {
  test("accepts identifier-safe names, rejects spaces/leading digit/@/empty", () => {
    expect(isValidVariableName("benchmark_user")).toBe(true);
    expect(isValidVariableName("_x1")).toBe(true);
    expect(isValidVariableName("my user")).toBe(false);
    expect(isValidVariableName("1abc")).toBe(false);
    expect(isValidVariableName("@x")).toBe(false);
    expect(isValidVariableName("")).toBe(false);
  });
});

describe("variableFor", () => {
  test("strips a leading @ and matches by bare name; miss → null", () => {
    const byName = indexByName([v("vendor", "ACME"), v("user_id", "12")]);
    expect(variableFor("@vendor", byName)?.value).toBe("ACME");
    expect(variableFor("vendor", byName)?.value).toBe("ACME");
    expect(variableFor("@nope", byName)).toBeNull();
  });
});

describe("buildInsertToken", () => {
  test("prefixes @ to the variable name", () => {
    expect(buildInsertToken(v("vendor"))).toBe("@vendor");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd app/ui && bun test variablesLogic`
Expected: FAIL — cannot find module `./variablesLogic`.

- [ ] **Step 3: Write minimal implementation**

Create `app/ui/src/lib/variablesLogic.ts`:

```ts
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd app/ui && bun test variablesLogic`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```fish
git add app/ui/src/lib/variablesLogic.ts app/ui/src/lib/variablesLogic.test.ts
git commit -m "feat(ui): Variable type + name validation, indexing, insert token"
```

---

## Task 2: `variablesLogic.ts` — parse/serialize + legacy migration

**Files:**
- Modify: `app/ui/src/lib/variablesLogic.ts`
- Test: `app/ui/src/lib/variablesLogic.test.ts`

**Interfaces:**
- Consumes: `Variable`, `isValidVariableName` (Task 1); `SqlType` from `./api`.
- Produces:
  - `parseVariables(raw: string | null): Variable[]`
  - `serializeVariables(vars: Variable[]): string`
  - `migrateGlobalParams(global: Record<string, string>): Variable[]`

- [ ] **Step 1: Write the failing test**

Append to `app/ui/src/lib/variablesLogic.test.ts`:

```ts
import { migrateGlobalParams, parseVariables, serializeVariables } from "./variablesLogic";

describe("parseVariables", () => {
  test("round-trips a valid array", () => {
    const vars = [{ name: "vendor", value: "ACME", sqlType: "nvarchar" as const, note: "n" }];
    expect(parseVariables(serializeVariables(vars))).toEqual(vars);
  });

  test("degrades to [] on null / non-array / bad JSON", () => {
    expect(parseVariables(null)).toEqual([]);
    expect(parseVariables("{}")).toEqual([]);
    expect(parseVariables("not json")).toEqual([]);
  });

  test("drops malformed entries: bad name, missing value, unknown sqlType→null, missing note→''", () => {
    const raw = JSON.stringify([
      { name: "ok", value: "1", sqlType: "int", note: "hi" },
      { name: "bad name", value: "1", sqlType: "int", note: "" }, // invalid name → dropped
      { name: "novalue", sqlType: "int", note: "" }, // missing value → dropped
      { name: "weird", value: "2", sqlType: "float", note: "" }, // unknown type → nvarchar? no: null
      { name: "nonote", value: "3", sqlType: "bit" }, // missing note → ""
    ]);
    expect(parseVariables(raw)).toEqual([
      { name: "ok", value: "1", sqlType: "int", note: "hi" },
      { name: "weird", value: "2", sqlType: null, note: "" },
      { name: "nonote", value: "3", sqlType: "bit", note: "" },
    ]);
  });
});

describe("migrateGlobalParams", () => {
  test("strips @, defaults nvarchar/empty note, skips non-identifier names", () => {
    const migrated = migrateGlobalParams({ "@today": "2026-07-17", "@user_id": "12", "@bad name": "x" });
    expect(migrated).toEqual([
      { name: "today", value: "2026-07-17", sqlType: "nvarchar", note: "" },
      { name: "user_id", value: "12", sqlType: "nvarchar", note: "" },
    ]);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd app/ui && bun test variablesLogic`
Expected: FAIL — `parseVariables`/`serializeVariables`/`migrateGlobalParams` not exported.

- [ ] **Step 3: Write minimal implementation**

Append to `app/ui/src/lib/variablesLogic.ts`:

```ts
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd app/ui && bun test variablesLogic`
Expected: PASS (all Task 1 + Task 2 tests).

- [ ] **Step 5: Commit**

```fish
git add app/ui/src/lib/variablesLogic.ts app/ui/src/lib/variablesLogic.test.ts
git commit -m "feat(ui): variables parse/serialize + legacy globalParams migration"
```

---

## Task 3: `variablesLogic.ts` — run-time resolution + saved-query persistence

**Files:**
- Modify: `app/ui/src/lib/variablesLogic.ts`
- Test: `app/ui/src/lib/variablesLogic.test.ts`

**Interfaces:**
- Consumes: `Variable`, `variableFor`, `indexByName` (Task 1); `deriveParams` from `./paramBarLogic`; `Param`, `ResolvedParam`, `SavedQuery` from `./api`.
- Produces:
  - `resolveRun(params: Param[], values: Record<string, string>, byName: Map<string, Variable>): ResolvedParam[]`
  - `persistInputs(stored: SavedQuery, values: Record<string, string>, byName: Map<string, Variable>): Param[]`

- [ ] **Step 1: Write the failing test**

Append to `app/ui/src/lib/variablesLogic.test.ts`:

```ts
import type { Param, SavedQuery } from "./api";
import { persistInputs, resolveRun } from "./variablesLogic";

const param = (name: string, sqlType: Param["sqlType"] = null, lastValue: string | null = null): Param => ({
  name, sqlType, lastValue, scope: "local",
});

describe("resolveRun", () => {
  test("library hit binds to the VARIABLE's value+type; miss uses the input field + param type", () => {
    const byName = indexByName([{ name: "vendor", value: "ACME", sqlType: "nvarchar", note: "" }]);
    const params = [param("@vendor", "int"), param("@user_id", "int")]; // @vendor's own type is ignored
    const resolved = resolveRun(params, { "@user_id": "42" }, byName);
    expect(resolved).toEqual([
      { name: "@vendor", sqlType: "nvarchar", value: "ACME" },
      { name: "@user_id", sqlType: "int", value: "42" },
    ]);
  });

  test("a missing input value → empty string", () => {
    const resolved = resolveRun([param("@x", "int")], {}, new Map());
    expect(resolved).toEqual([{ name: "@x", sqlType: "int", value: "" }]);
  });
});

describe("persistInputs", () => {
  const stored: SavedQuery = {
    id: "q1", name: "q", targetDatabase: null,
    sql: "select * from t where a=@user_id and v=@vendor",
    params: [param("@user_id", "int"), param("@vendor", "nvarchar")],
  };

  test("writes lastValue for declared inputs, skips library-owned names, ignores edited-in params", () => {
    const byName = indexByName([{ name: "vendor", value: "ACME", sqlType: "nvarchar", note: "" }]);
    const out = persistInputs(stored, { "@user_id": "42", "@vendor": "typed-but-ignored", "@scratch": "9" }, byName);
    expect(out).toEqual([
      { name: "@user_id", sqlType: "int", lastValue: "42", scope: "local" },
      { name: "@vendor", sqlType: "nvarchar", lastValue: null, scope: "local" }, // library owns it → untouched
    ]);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd app/ui && bun test variablesLogic`
Expected: FAIL — `resolveRun`/`persistInputs` not exported.

- [ ] **Step 3: Write minimal implementation**

Add the import at the top of `app/ui/src/lib/variablesLogic.ts` (next to the existing `./api` import):

```ts
import { deriveParams } from "./paramBarLogic";
```

Append to `app/ui/src/lib/variablesLogic.ts`:

```ts
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd app/ui && bun test variablesLogic`
Expected: PASS (all tests across Tasks 1–3).

- [ ] **Step 5: Commit**

```fish
git add app/ui/src/lib/variablesLogic.ts app/ui/src/lib/variablesLogic.test.ts
git commit -m "feat(ui): resolveRun (library wins) + persistInputs for query inputs"
```

---

## Task 4: `variables.svelte.ts` — persisted store + one-time migration

**Files:**
- Create: `app/ui/src/lib/variables.svelte.ts`

**Interfaces:**
- Consumes: `parseStringMap` from `./paramBarLogic`; `migrateGlobalParams`, `parseVariables`, `serializeVariables`, `Variable` from `./variablesLogic`.
- Produces:
  - `variables: { list: Variable[] }` (exported `$state`)
  - `upsertVariable(v: Variable): void`
  - `removeVariable(name: string): void`

(Thin runes wrapper — no unit test, mirroring the untested `globalParams.svelte.ts`. Verified by `bun run check` here and by the panel's visual test in Task 6.)

- [ ] **Step 1: Create the store**

Create `app/ui/src/lib/variables.svelte.ts`:

```ts
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
```

- [ ] **Step 2: Typecheck**

Run: `cd app/ui && bun run check`
Expected: no new errors from `variables.svelte.ts`.

- [ ] **Step 3: Commit**

```fish
git add app/ui/src/lib/variables.svelte.ts
git commit -m "feat(ui): persisted Variables store with one-time globalParams migration"
```

---

## Task 5: `SqlEditor.svelte` — `insertAtCursor` public method

**Files:**
- Modify: `app/ui/src/lib/SqlEditor.svelte` (add to the `// --- Public API (via bind:this)` block, after `focus()` near line 93)

**Interfaces:**
- Produces: `insertAtCursor(text: string): void` on the `SqlEditor` component instance (reached via `bind:this`).

- [ ] **Step 1: Add the method**

In `app/ui/src/lib/SqlEditor.svelte`, immediately after the existing `focus()` export:

```ts
  // Insert text at the cursor (replacing any selection) and place the caret after it,
  // then focus. Used by the Variables panel's click-to-insert. The updateListener
  // fires onchange, so the tab's stored content stays in sync (no manual value write).
  export function insertAtCursor(text: string): void {
    if (!view) return;
    const { from, to } = view.state.selection.main;
    view.dispatch({ changes: { from, to, insert: text }, selection: { anchor: from + text.length } });
    view.focus();
  }
```

- [ ] **Step 2: Typecheck**

Run: `cd app/ui && bun run check`
Expected: no new errors.

- [ ] **Step 3: Commit**

```fish
git add app/ui/src/lib/SqlEditor.svelte
git commit -m "feat(ui): SqlEditor.insertAtCursor for click-to-insert"
```

---

## Task 6: `VariablesLibrary.svelte` panel + Library sub-view toggle

**Files:**
- Create: `app/ui/src/lib/VariablesLibrary.svelte`
- Modify: `app/ui/src/App.svelte` (Library branch of the sidebar, ~lines 442–452; add `libraryTab` state + import)

**Interfaces:**
- Consumes: `variables`, `upsertVariable`, `removeVariable` from `./variables.svelte`; `Variable`, `isValidVariableName`, `buildInsertToken` from `./variablesLogic`; `SqlType` from `./api`.
- Produces: `VariablesLibrary` component with prop `onInsert: (token: string) => void`.

- [ ] **Step 1: Create the panel**

Create `app/ui/src/lib/VariablesLibrary.svelte`:

```svelte
<script lang="ts">
  import type { SqlType } from "./api";
  import { Plus, Search } from "./icons";
  import { removeVariable, upsertVariable, variables } from "./variables.svelte";
  import { buildInsertToken, isValidVariableName, type Variable } from "./variablesLogic";

  // Reusable named values (V2). Insert drops @name into the active editor (App wires
  // onInsert → editor.insertAtCursor). Edits persist immediately via the store. Name is
  // identity: editing a name is delete-old + add-new (references in SQL are the user's
  // to update, like renaming a column).
  let { onInsert }: { onInsert: (token: string) => void } = $props();

  const TYPES: (SqlType | "")[] = [
    "", "int", "bigint", "nvarchar", "bit", "date", "datetime2", "decimal", "uniqueidentifier", "money",
  ];

  // Editing state. `editingName` is "" while adding a brand-new variable; otherwise the
  // name of the row being edited (the original, so we can delete-then-upsert on rename).
  let open = $state(false);
  let editingName = $state("");
  let fName = $state("");
  let fValue = $state("");
  let fType = $state<SqlType | "">("");
  let fNote = $state("");
  let error = $state("");

  function startAdd() {
    editingName = "";
    fName = "";
    fValue = "";
    fType = "nvarchar"; // safe-bind default (no inference)
    fNote = "";
    error = "";
    open = true;
  }

  function startEdit(v: Variable) {
    editingName = v.name;
    fName = v.name;
    fValue = v.value;
    fType = v.sqlType ?? "";
    fNote = v.note;
    error = "";
    open = true;
  }

  function cancel() {
    open = false;
    error = "";
  }

  function submit() {
    const name = fName.trim();
    if (!isValidVariableName(name)) {
      error = "Name must be a bare identifier (letters, digits, _; no leading digit or @).";
      return;
    }
    // Reject a name collision with a DIFFERENT existing variable.
    const clash = variables.list.some((v) => v.name === name && v.name !== editingName);
    if (clash) {
      error = `A variable named "${name}" already exists.`;
      return;
    }
    if (editingName && editingName !== name) removeVariable(editingName); // rename
    upsertVariable({ name, value: fValue, sqlType: fType === "" ? null : fType, note: fNote.trim() });
    open = false;
    error = "";
  }

  function onDelete(v: Variable) {
    if (confirm(`Delete variable "@${v.name}"?`)) removeVariable(v.name);
  }
</script>

<div class="list">
  <div class="header">
    <h2>Variables</h2>
    <button onclick={startAdd}><Plus size={14} /> New variable</button>
  </div>

  {#if open}
    <div class="form">
      <input class="mono" placeholder="name (e.g. benchmark_user)" bind:value={fName} />
      <input placeholder="value" bind:value={fValue} />
      <div class="row">
        <select bind:value={fType} title="Bind type — raw-text splices literally (injectable)">
          {#each TYPES as t (t)}
            <option value={t}>{t === "" ? "raw-text" : t}</option>
          {/each}
        </select>
        <input class="note" placeholder="note (optional)" bind:value={fNote} />
      </div>
      {#if error}<p class="error">{error}</p>{/if}
      <div class="actions">
        <button onclick={submit}>Save</button>
        <button onclick={cancel}>Cancel</button>
      </div>
    </div>
  {/if}

  {#if variables.list.length === 0}
    <div class="empty">
      <Search size={20} />
      <p>No variables yet. Add one to reuse it as <code>@name</code> in any query.</p>
    </div>
  {:else}
    <ul>
      {#each variables.list as v (v.name)}
        <li>
          <div class="meta">
            <strong class="mono">@{v.name}</strong>
            <span class="val">{v.value}{#if !v.sqlType} <span class="raw" title="raw-text — spliced literally (injectable)">raw!</span>{/if}</span>
            {#if v.note}<span class="note-line">{v.note}</span>{/if}
          </div>
          <div class="actions">
            <button onclick={() => onInsert(buildInsertToken(v))}>Insert</button>
            <button onclick={() => startEdit(v)}>Edit</button>
            <button onclick={() => onDelete(v)}>Delete</button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .list { padding: 0.5rem; }
  .header { display: flex; align-items: center; justify-content: space-between; }
  h2 { font-size: 1rem; margin: 0.5rem 0; color: var(--text); }
  .mono { font-family: var(--font-mono); }
  .form { display: flex; flex-direction: column; gap: 0.3rem; margin-bottom: 0.5rem; }
  .form .row { display: flex; gap: 0.3rem; }
  .form .note { flex: 1; }
  .error { color: var(--danger); font-size: 0.75rem; margin: 0.1rem 0 0; }
  .empty {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: var(--sp-2); padding: var(--sp-5) var(--sp-2); color: var(--muted);
    font-size: 0.9rem; text-align: center;
  }
  .empty :global(svg) { color: var(--faint); }
  .empty p { margin: 0; }
  .empty code { font-family: var(--font-mono); }
  input, select {
    font-size: 0.85rem; padding: 0.2rem 0.3rem;
    border: 1px solid var(--border-strong); border-radius: var(--r-sm);
    background: var(--raised); color: var(--text);
  }
  ul { list-style: none; margin: 0; padding: 0; }
  li {
    padding: 0.5rem; border: 1px solid var(--border); border-radius: var(--r-md);
    margin-bottom: 0.4rem; transition: background var(--dur-fast) var(--ease);
  }
  li:hover { background: color-mix(in srgb, var(--brand) 8%, transparent); }
  .meta { display: flex; flex-direction: column; gap: 0.1rem; }
  .val {
    color: var(--muted); font-size: 0.8rem;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .raw {
    font-size: 0.6rem; padding: 0.02rem 0.28rem; border-radius: var(--r-pill);
    background: color-mix(in srgb, var(--danger) 12%, var(--raised));
    color: var(--danger); font-weight: 700;
  }
  .note-line { color: var(--faint); font-size: 0.72rem; font-style: italic; }
  .actions { display: flex; gap: 0.3rem; margin-top: 0.3rem; }
  button { font-size: 0.8rem; cursor: pointer; }
  .header button { display: inline-flex; align-items: center; gap: 0.2rem; }
</style>
```

- [ ] **Step 2: Wire the Library sub-view toggle in App.svelte**

Add the import near the other sidebar imports (by `import SavedQueryLibrary ...`):

```ts
  import VariablesLibrary from "./lib/VariablesLibrary.svelte";
```

Add sub-view state near `sidebarMode` (App.svelte:33):

```ts
  let libraryTab = $state<"queries" | "variables">("queries");
```

Replace the Library branch (App.svelte ~lines 450–452, the `{:else}` that renders `<SavedQueryLibrary />`) with:

```svelte
      {:else}
        <div class="lib-subtabs">
          <button class:active={libraryTab === "queries"} onclick={() => (libraryTab = "queries")}>
            Saved queries
          </button>
          <button class:active={libraryTab === "variables"} onclick={() => (libraryTab = "variables")}>
            Variables
          </button>
        </div>
        {#if libraryTab === "queries"}
          <SavedQueryLibrary />
        {:else}
          <VariablesLibrary onInsert={(t) => editor?.insertAtCursor(t)} />
        {/if}
      {/if}
```

Add sub-tab styling to App.svelte's `<style>` (near the `.mode-toggle` rule):

```css
  .lib-subtabs { display: flex; gap: 0.25rem; padding: 0.4rem 0.5rem 0; }
  .lib-subtabs button {
    flex: 1; font-size: 0.78rem; padding: 0.2rem 0.3rem; cursor: pointer;
    border: 1px solid var(--border); border-radius: var(--r-sm);
    background: var(--raised); color: var(--muted);
  }
  .lib-subtabs button.active { color: var(--text); border-color: var(--accent); background: var(--panel); }
```

- [ ] **Step 3: Typecheck**

Run: `cd app/ui && bun run check`
Expected: no errors. (`editor` is already an in-scope `$state<SqlEditor>` at App.svelte:56.)

- [ ] **Step 4: Visual verify (light + dark)**

Run the app (`just dev`) or vite dev (`cd app/ui && bun run dev`, port 1420). With Playwright: open the sidebar **Library → Variables** sub-view; add a variable `benchmark_user = 12345` (int) and `vendor = The best vendor (nvarchar)`; screenshot the panel in **light and dark**. Confirm: New-variable form validates a bad name; Insert drops `@benchmark_user` at the editor cursor; the `raw!` chip shows only for raw-text variables.

- [ ] **Step 5: Commit**

```fish
git add app/ui/src/lib/VariablesLibrary.svelte app/ui/src/App.svelte
git commit -m "feat(ui): Variables panel + Library sub-view toggle + click-to-insert"
```

---

## Task 7: `ParamBar.svelte` — library-chip vs query-input reframe

**Files:**
- Modify: `app/ui/src/lib/ParamBar.svelte` (replace `<script>` props + template; drop scope/tier UI)

**Interfaces:**
- Consumes: `Param`, `SqlType` from `./api`; `Variable` from `./variablesLogic`.
- Produces: `ParamBar` with props `{ params: Param[]; values: Record<string, string>; libraryHits: Record<string, Variable>; savedTab: boolean; onTypeChange?: (name: string, sqlType: SqlType | null) => void }`.

Behavior: a `@name` in `libraryHits` renders a read-only **chip** (`@name → value  LIB`); otherwise it's an editable **query input** (value field + a bind-type selector **only when `savedTab`** — scratch-tab inputs are raw-text, flagged, since there's no saved query to persist a type to).

- [ ] **Step 1: Replace the `<script>` block**

Replace the entire `<script lang="ts"> … </script>` at the top of `app/ui/src/lib/ParamBar.svelte` with:

```svelte
<script lang="ts">
  import type { Param, SqlType } from "./api";
  import type { Variable } from "./variablesLogic";

  // Reframed param bar (V2). Each derived @name is EITHER a library hit (resolved from
  // the Variables Library — read-only chip) OR a query input (editable value; bind-type
  // selector only on a saved-query tab, where the type persists). No scope dropdown / no
  // tiers — scope is implied by name and the library always wins. Parent owns `values`
  // ($state record) and mutates it in place on input.
  let {
    params,
    values,
    libraryHits = {},
    savedTab = false,
    onTypeChange = () => {},
  }: {
    params: Param[];
    values: Record<string, string>;
    libraryHits?: Record<string, Variable>;
    savedTab?: boolean;
    onTypeChange?: (name: string, sqlType: SqlType | null) => void;
  } = $props();
</script>
```

- [ ] **Step 2: Replace the template `<div class="parambar"> … </div>`**

Replace the whole `.parambar` block with:

```svelte
<div class="parambar">
  {#each params as p (p.name)}
    {#if libraryHits[p.name]}
      <span class="param lib" title="Bound from the Variables Library (@{libraryHits[p.name].name})">
        <span class="pname">{p.name}</span>
        <span class="arrow">→</span>
        <span class="libval">{libraryHits[p.name].value}</span>
        <span class="badge">LIB</span>
      </span>
    {:else}
      <label class="param">
        <span class="pname">{p.name}</span>
        <input
          value={values[p.name] ?? ""}
          oninput={(e) => (values[p.name] = e.currentTarget.value)}
        />
        {#if savedTab}
          <select
            class="type"
            value={p.sqlType ?? ""}
            onchange={(e) =>
              onTypeChange(p.name, e.currentTarget.value === "" ? null : (e.currentTarget.value as SqlType))}
            title="Bind type — raw-text is spliced literally (injectable); a typed value binds via sp_executesql"
          >
            <option value="">raw-text</option>
            <option value="int">int</option>
            <option value="bigint">bigint</option>
            <option value="nvarchar">nvarchar</option>
            <option value="bit">bit</option>
            <option value="date">date</option>
            <option value="datetime2">datetime2</option>
            <option value="decimal">decimal</option>
            <option value="uniqueidentifier">uniqueidentifier</option>
            <option value="money">money</option>
          </select>
        {/if}
        {#if !p.sqlType || !savedTab}
          <span class="chip raw" title="raw-text — spliced literally into the SQL (injectable). Add it to the Variables Library for a safe typed bind.">raw!</span>
        {/if}
      </label>
    {/if}
  {/each}
</div>
```

- [ ] **Step 3: Update the `<style>` block**

In `app/ui/src/lib/ParamBar.svelte`, remove the now-unused `.badge.session`, `.badge.global`, `.clear-tier`, `.clear-tier:hover` rules, and replace the generic `.badge` rule with library-chip styles. Add:

```css
  .param.lib {
    gap: 0.25rem;
    padding: 0.15rem 0.4rem;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: color-mix(in srgb, var(--tier-global) 10%, var(--raised));
  }
  .param.lib .arrow { color: var(--faint); }
  .param.lib .libval {
    font: 0.8rem var(--font-mono); color: var(--text);
    max-width: 16rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .badge {
    font-size: 0.58rem; font-weight: 700; letter-spacing: 0.03em;
    padding: 0.05rem 0.32rem; border-radius: var(--r-pill);
    background: color-mix(in srgb, var(--tier-global) 22%, var(--raised));
    color: var(--tier-global);
  }
```

(Keep the existing `.parambar`, `.param`, `.pname`, `.param input`, `.param input:focus-visible`, `.chip`, `.chip.raw`, `.scope`/`.type` rules.)

- [ ] **Step 4: Typecheck**

Run: `cd app/ui && bun run check`
Expected: PARAMBAR errors only in `App.svelte` (its call site still passes the old props). That's fixed in Task 8 — you may see App.svelte errors here; ParamBar.svelte itself must be clean.

- [ ] **Step 5: Commit**

```fish
git add app/ui/src/lib/ParamBar.svelte
git commit -m "feat(ui): reframe ParamBar into library chips vs query inputs"
```

---

## Task 8: `App.svelte` — remove the gate, wire resolution, drop tiers

**Files:**
- Modify: `app/ui/src/App.svelte` (imports; `curParams`; new deriveds; `paramValues` effect; `run()` param branch; `ParamBar` call site; remove tier handlers/imports)

**Interfaces:**
- Consumes: `variables` from `./variables.svelte`; `indexByName`, `variableFor`, `resolveRun`, `persistInputs`, `Variable` from `./variablesLogic`; existing `deriveParams`, `nextParamValues` from `./paramBarLogic`; `runParams`, `runSql`, `saveQuery` from `./api`.

- [ ] **Step 1: Update imports**

Add:

```ts
  import { variables } from "./lib/variables.svelte";
  import { indexByName, persistInputs, resolveRun, variableFor, type Variable } from "./lib/variablesLogic";
```

Edit the existing `paramBarLogic` import (App.svelte:18) — drop `persistDeclared`, `resolve`, `toResolvedParams`, `valueSource`; keep the two still used:

```ts
  import { deriveParams, nextParamValues } from "./lib/paramBarLogic";
```
Leave the separate `import { isTabDirty } from "./lib/tabsLogic";` (App.svelte:24) untouched — `isTabDirty` is NOT from `paramBarLogic`.

Remove these now-unused imports entirely: `setSessionParams`, `sessionParams` (from `./lib/sessionParams.svelte`); `setGlobalParams`, `clearGlobalParam`, `globalParams` (from `./lib/globalParams.svelte`); `clearSessionParam`. Also drop `type ParamScope` from the `./lib/api` import on App.svelte:3 (no longer referenced once `onScopeChange` is deleted).

- [ ] **Step 2: Remove the saved-query gate on `curParams` + add the library deriveds**

Replace (App.svelte:136–138):

```ts
  const curParams = $derived(
    curSavedQuery ? deriveParams(curTab?.content ?? "", curSavedQuery.params) : [],
  );
```

with:

```ts
  // Params derived from the tab's live SQL in EVERY tab (V2 — no saved-query gate).
  // Merged with the linked saved query's declared params when there is one; a scratch
  // tab starts each param at raw-text/local/unset.
  const curParams = $derived(deriveParams(curTab?.content ?? "", curSavedQuery?.params ?? []));

  // Library index + the subset of curParams that resolve from a library variable (name
  // match). libraryHits drives the ParamBar chips and resolveRun's "library wins".
  const varsByName = $derived(indexByName(variables.list));
  const libraryHits = $derived(
    Object.fromEntries(
      curParams
        .map((p) => [p.name, variableFor(p.name, varsByName)] as const)
        .filter((e): e is readonly [string, Variable] => e[1] !== null),
    ),
  );
```

- [ ] **Step 3: Delete the tier handlers**

Remove `paramSources` (App.svelte ~176–180), `onScopeChange` (~182–188), and `onClearTier` (~199–208) entirely. Keep `onTypeChange` (still used for saved-tab query inputs), but note it already returns early when `!curSavedQuery`.

- [ ] **Step 4: Simplify the `paramValues` effect**

Replace the effect body (App.svelte ~219–230) so it no longer reads the tier stores:

```ts
  let paramValues = $state<Record<string, string>>({});
  let valuesTabId = "";
  $effect(() => {
    const id = tabsState.activeId; // tab switch → full reset
    const params = curParams; // track: late library load + new @names on edit
    const prev = untrack(() => ({ ...paramValues }));
    // No tier stores anymore — inputs seed from each param's own lastValue (or "").
    paramValues = nextParamValues(id !== valuesTabId, params, prev, {}, {});
    valuesTabId = id;
  });
```

- [ ] **Step 5: Rewrite the run() param branch**

Replace the param branch inside `run()` (App.svelte ~311–329, the `let out: QueryResult[]; if (curParams.length > 0 && curSavedQuery && curTab) { … } else { … }`) with:

```ts
      // Param-aware run (V2). Any tab with derived @params runs via run_params
      // (bind/splice); everything else keeps the plain run_sql path (selection/GO
      // splitting). resolveRun binds library names to their variable value+type and
      // leaves query inputs to their bar field. Saved-query INPUT values persist back
      // as lastValue (per-query memory); library names are owned by the library.
      let out: QueryResult[];
      if (curParams.length > 0 && curTab) {
        if (curSavedQuery) {
          const params = persistInputs(curSavedQuery, paramValues, varsByName);
          await saveQuery({ ...curSavedQuery, params });
        }
        const resolved = resolveRun(curParams, paramValues, varsByName);
        out = await runParams(id, effectiveDb, curTab.content, resolved);
      } else {
        const t = editor?.getRunTarget();
        if (!t) return;
        out = await runSql(id, effectiveDb, t.text, t.selection || null, t.line);
      }
```

- [ ] **Step 6: Update the ParamBar call site**

Replace the `<ParamBar ... />` line (App.svelte ~481):

```svelte
          <ParamBar params={curParams} values={paramValues} libraryHits={libraryHits} savedTab={!!curSavedQuery} onTypeChange={onTypeChange} />
```

- [ ] **Step 7: Typecheck + tests**

Run: `cd app/ui && bun run check`
Expected: clean (no unused-import warnings, no missing props).
Run: `cd app/ui && bun test`
Expected: all pass (existing + new `variablesLogic` tests). The old `paramBarLogic.test.ts` still passes — those functions are untouched.

- [ ] **Step 8: Visual verify (light + dark) — the core flow**

Run the app. Verify end-to-end:
1. **Scratch tab, library hit:** with `vendor` in the library, type `select * from o where v = @vendor` in a fresh scratch tab → the param bar appears (no saved query needed) showing a `@vendor → "The best…" LIB` chip. (This is the original "sometimes @foo isn't recognized" bug, now fixed.)
2. **Scratch tab, query input:** type `@nope` → an editable field with a `raw!` chip, no type selector.
3. **Saved-query tab:** open a saved query with `@user_id` (not in library) → editable field **with** the bind-type selector; set it, Run, reopen → value remembered.
4. **Library wins:** add `user_id` to the library → the `@user_id` field becomes a `LIB` chip.
Screenshot the param bar (chip + input states) in **light and dark**.

- [ ] **Step 9: Commit**

```fish
git add app/ui/src/App.svelte
git commit -m "feat(ui): resolve @params from the library in any tab; drop scope tiers"
```

---

## Task 9: Follow-on beads + full verify

**Files:** none (tracker + gates only).

- [ ] **Step 1: File the deferred follow-on beads**

```fish
bd create --title="@-triggered autocomplete for library variables in the editor" --type=feature --priority=2 --description="CodeMirror completion source over Variables Library names (+ in-scope tokens) when the user types @. The biggest discoverability lever; deferred from the Variables Library (V2) build. See docs/superpowers/specs/2026-07-17-variables-library-design.md."
bd create --title="V3: inline local script variables (@x = 123)" --type=feature --priority=3 --description="Parse inline @name = value declarations scoped to one script/tab; resolve them for that run. Deferred from V2. See the Variables Library spec."
bd create --title="V1: context-matrix variables (per-database/server values)" --type=feature --priority=3 --description="One variable label whose value differs per database/server, auto-resolved by execution context. Shelved during brainstorming pending real V2 usage. Keep the Variable model forward-compatible. See the Variables Library spec."
bd create --title="Per-query override of a library variable value" --type=feature --priority=3 --description="Optional 'shadow a library value to a one-off local for this run' affordance. V2 is library-always-wins; add only if missed. See the Variables Library spec."
bd create --title="Cleanup: retire Param.scope + sessionParams module after Variables migration settles" --type=task --priority=3 --description="Remove the vestigial Param.scope field, the unused paramBarLogic tier fns (resolve/valueSource/routeWrites/persistDeclared), sessionParams.svelte.ts, and globalParams write-path once the coot.variables.v1 migration has been in use. See the Variables Library spec."
bd create --title="Sidebar rework: ConnectionList crowds out the Objects/Library toggle" --type=task --priority=2 --description="The left sidebar's ConnectionList grows with each added database and pushes the mode toggle far down. Rethink layout/space allocation. Independent of V2. See the Variables Library spec."
bd create --title="Add time/datetimeoffset/varbinary SqlType support if needed" --type=task --priority=4 --description="core param_bind.rs currently supports a capped 9-type set. Add these only on a real need. Noted while confirming uniqueidentifier IS supported (Variables Library spec)."
```

- [ ] **Step 2: Run the full gate**

Run: `just verify`
Expected: `✓ all checks passed` (fmt-check, clippy/lint, cargo test, ui-check, ui-test, ui-build all green). Rust is untouched but the gate still runs it.

- [ ] **Step 3: Final commit (if any lint/format fixups)**

```fish
git add -A
git commit -m "chore: verify Variables Library feature (fmt/lint/test/build green)"
```

---

## Self-Review

**Spec coverage:**
- Data model (`Variable`, name=identity, `coot.variables.v1`) → Tasks 1, 4. ✓
- Query input concept + implied-scope rule + "library always wins" → Tasks 3 (`resolveRun`), 7, 8. ✓
- Gate removed (params in any tab) → Task 8 Step 2. ✓
- Variables panel inside Library sub-view (no third mode/modal) → Task 6. ✓
- Param-bar reframe (chip vs input, drop scope select) → Task 7. ✓
- Click-to-insert → Tasks 5 (`insertAtCursor`) + 6 (panel wiring). ✓
- Safety/bind types, explicit type, no inference, full 9-type set, `raw!` flag → Tasks 6 (form default nvarchar), 7 (raw! chip), 2 (`asSqlType`). ✓
- Migration (additive, legacy key intact) → Tasks 2 + 4. ✓
- `Param.scope` vestigial / Session retired → Task 8 (stops reading tiers; scope field left in stored JSON), cleanup bead in Task 9. ✓
- Testing (pure-logic units + visual light/dark) → Tasks 1–3 tests, Tasks 6 & 8 visual. ✓
- Follow-on beads (autocomplete, V3, V1, override, cleanup, sidebar, extra types) → Task 9. ✓

**Placeholder scan:** No TBD/TODO; every code step shows full code; every test shows assertions. ✓

**Type consistency:** `Variable` fields (`name/value/sqlType/note`) identical across Tasks 1–8. `variableFor(paramName, byName)`, `resolveRun(params, values, byName)`, `persistInputs(stored, values, byName)`, `indexByName(vars)`, `buildInsertToken(v)`, `upsertVariable(v)`, `removeVariable(name)`, `insertAtCursor(text)` used identically at every call site. ParamBar props (`params/values/libraryHits/savedTab/onTypeChange`) match between Task 7 definition and Task 8 call site. ✓

**Known v1 limitations (intentional, documented):** scratch-tab query inputs are raw-text only (no persisted type — that path is the library or a saved query); a fan-out tab that gains `@params` still routes to the fan-out branch (pre-existing; `fanoutDisabled` discourages it). Both acceptable for v1.
