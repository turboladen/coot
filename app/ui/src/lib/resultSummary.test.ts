// `bun test` — resultSummary.ts imports only `import type { QueryResult }`
// (fully erased), so api.ts's `@tauri-apps/api/core` import is never evaluated
// here. Excluded from svelte-check via tsconfig `exclude`, same as
// renderCell.test.ts.
import { describe, expect, test } from "bun:test";
import type { ColumnMeta, QueryResult } from "./api";
import { summarize, tabLabel } from "./resultSummary";

// Build a QueryResult with `nRows` rows over `nCols` columns. Cell contents are
// irrelevant to the counts these formatters report, so they stay empty.
function make(nRows: number, nCols: number): QueryResult {
  const columns: ColumnMeta[] = Array.from({ length: nCols }, (_, i) => ({
    name: `c${i}`,
    sqlType: "int",
    nullable: false,
    precision: null,
    scale: null,
  }));
  const rows = Array.from({ length: nRows }, () => Array.from({ length: nCols }, () => ({ kind: "Null" as const })));
  return { columns, rows, rowsAffected: null };
}

describe("tabLabel", () => {
  test("1-based index and pluralizes >1 rows", () => {
    expect(tabLabel(make(42, 3), 0)).toBe("Result 1 · 42 rows");
  });

  test("singular '1 row' (no trailing s)", () => {
    expect(tabLabel(make(1, 2), 1)).toBe("Result 2 · 1 row");
  });

  test("0 rows pluralizes to 'rows'", () => {
    expect(tabLabel(make(0, 1), 2)).toBe("Result 3 · 0 rows");
  });
});

describe("summarize", () => {
  test("0 result sets → honest no-result info line", () => {
    expect(summarize([])).toEqual([{ kind: "info", text: "Query ran. No result set returned." }]);
  });

  test("1 result set → singular header + per-set counts", () => {
    expect(summarize([make(1, 1)])).toEqual([
      { kind: "info", text: "Ran successfully — 1 result set." },
      { kind: "info", text: "Result 1: 1 row, 1 column" },
    ]);
  });

  test("N result sets → plural header + one line per set", () => {
    expect(summarize([make(42, 3), make(0, 2)])).toEqual([
      { kind: "info", text: "Ran successfully — 2 result sets." },
      { kind: "info", text: "Result 1: 42 rows, 3 columns" },
      { kind: "info", text: "Result 2: 0 rows, 2 columns" },
    ]);
  });
});
