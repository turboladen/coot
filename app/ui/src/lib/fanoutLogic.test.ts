// `bun test` — fanoutLogic.ts is rune-free plain TS, so it imports cleanly here
// with no Svelte compiler. Excluded from svelte-check via tsconfig `exclude`, same
// as tabsLogic.test.ts / renderCell.test.ts.
import { describe, expect, test } from "bun:test";
import type { CellValue, ColumnMeta, DatabaseInfo, DbRunOutcome, QueryResult } from "./api";
import { combineFanoutResults, effectiveFanoutDatabases, fanoutStatus, matchPattern } from "./fanoutLogic";

function db(name: string, stateDesc = "ONLINE"): DatabaseInfo {
  return { name, databaseId: name.length, stateDesc };
}

function col(name: string, sqlType = "int"): ColumnMeta {
  return { name, sqlType, nullable: true, precision: null, scale: null };
}

function intCell(n: number): CellValue {
  return { kind: "Int", value: n };
}

// A single-result-set outcome with the given columns + rows of Int cells.
function outcome(
  database: string,
  columns: ColumnMeta[],
  rows: number[][],
  opts: { error?: string | null; elapsedMs?: number } = {},
): DbRunOutcome {
  const result: QueryResult = { columns, rows: rows.map((r) => r.map(intCell)), rowsAffected: null };
  return {
    database,
    results: opts.error ? [] : [result],
    error: opts.error ?? null,
    elapsedMs: opts.elapsedMs ?? 1,
  };
}

describe("matchPattern", () => {
  const dbs = [
    db("ESP_Nomad_SE_DEV"),
    db("ESP_Nomad_US_DEV"),
    db("ESP_Suntory_DEV"),
    db("ESP_Nomad_OFF_DEV", "OFFLINE"),
  ];

  test("`*` suffix matches the prefix family, excludes others", () => {
    expect(matchPattern("ESP_Nomad_*", dbs)).toEqual(["ESP_Nomad_SE_DEV", "ESP_Nomad_US_DEV"]);
  });

  test("case-insensitive (DB names)", () => {
    expect(matchPattern("esp_nomad_se_dev", dbs)).toEqual(["ESP_Nomad_SE_DEV"]);
  });

  test("non-ONLINE excluded even when the name matches", () => {
    // ESP_Nomad_OFF_DEV matches the glob but is OFFLINE → not selectable.
    expect(matchPattern("ESP_Nomad_*", dbs)).not.toContain("ESP_Nomad_OFF_DEV");
  });

  test("regex metachars in the pattern are treated literally", () => {
    const withDot = [db("A.B_DEV"), db("AXB_DEV")];
    // The `.` must match a literal dot, not any char → AXB_DEV excluded.
    expect(matchPattern("A.B_*", withDot)).toEqual(["A.B_DEV"]);
  });

  test("a bare `*` selects all ONLINE", () => {
    expect(matchPattern("*", dbs)).toEqual(["ESP_Nomad_SE_DEV", "ESP_Nomad_US_DEV", "ESP_Suntory_DEV"]);
  });

  test("empty pattern selects nothing", () => {
    expect(matchPattern("", dbs)).toEqual([]);
  });

  test("no match → empty", () => {
    expect(matchPattern("ZZZ_*", dbs)).toEqual([]);
  });
});

describe("combineFanoutResults", () => {
  const cols = [col("id", "int"), col("qty", "int")];

  test("identical shapes → combined with a leading database column + summed rows", () => {
    const outcomes = [
      outcome("DB_A", cols, [[1, 10], [2, 20]]),
      outcome("DB_B", cols, [[3, 30]]),
    ];
    const { combined, canCombine } = combineFanoutResults(outcomes);
    expect(canCombine).toBe(true);
    expect(combined).not.toBeNull();
    // Leading database column, exact ColumnMeta shape.
    expect(combined!.columns[0]).toEqual({
      name: "database",
      sqlType: "nvarchar",
      nullable: false,
      precision: null,
      scale: null,
    });
    expect(combined!.columns.map((c) => c.name)).toEqual(["database", "id", "qty"]);
    expect(combined!.rows.length).toBe(3); // 2 + 1
    expect(combined!.rowsAffected).toBeNull();
  });

  test("each row is prefixed with a Text cell naming its database, in input order", () => {
    const outcomes = [
      outcome("DB_A", cols, [[1, 10], [2, 20]]),
      outcome("DB_B", cols, [[3, 30]]),
    ];
    const { combined } = combineFanoutResults(outcomes);
    expect(combined!.rows.map((r) => r[0])).toEqual([
      { kind: "Text", value: "DB_A" },
      { kind: "Text", value: "DB_A" },
      { kind: "Text", value: "DB_B" },
    ]);
    // Original cells are preserved after the prefix.
    expect(combined!.rows[0]).toEqual([{ kind: "Text", value: "DB_A" }, intCell(1), intCell(10)]);
  });

  test("differing column NAME → not combinable", () => {
    const outcomes = [
      outcome("DB_A", [col("id", "int")], [[1]]),
      outcome("DB_B", [col("code", "int")], [[2]]),
    ];
    expect(combineFanoutResults(outcomes)).toEqual({ combined: null, canCombine: false });
  });

  test("differing column TYPE → not combinable", () => {
    const outcomes = [
      outcome("DB_A", [col("id", "int")], [[1]]),
      outcome("DB_B", [col("id", "bigint")], [[2]]),
    ];
    expect(combineFanoutResults(outcomes)).toEqual({ combined: null, canCombine: false });
  });

  test("column names/types with spaces don't alias into an equal signature", () => {
    // A naive `${name} ${type}` join would collapse both of these to "a b c"; the
    // JSON-encoded signature keeps them distinct → not combinable.
    const outcomes = [
      outcome("DB_A", [col("a", "b c")], [[1]]),
      outcome("DB_B", [col("a b", "c")], [[2]]),
    ];
    expect(combineFanoutResults(outcomes)).toEqual({ combined: null, canCombine: false });
  });

  test("a DB with 2 result sets → not combinable", () => {
    const two: DbRunOutcome = {
      database: "DB_A",
      results: [
        { columns: cols, rows: [], rowsAffected: null },
        { columns: cols, rows: [], rowsAffected: null },
      ],
      error: null,
      elapsedMs: 1,
    };
    expect(combineFanoutResults([two])).toEqual({ combined: null, canCombine: false });
  });

  test("a DB with 0 result sets (DML batch) → not combinable", () => {
    const zero: DbRunOutcome = { database: "DB_A", results: [], error: null, elapsedMs: 1 };
    expect(combineFanoutResults([zero])).toEqual({ combined: null, canCombine: false });
  });

  test("all errored → not combinable", () => {
    const outcomes = [
      outcome("DB_A", cols, [], { error: "boom" }),
      outcome("DB_B", cols, [], { error: "nope" }),
    ];
    expect(combineFanoutResults(outcomes)).toEqual({ combined: null, canCombine: false });
  });

  test("mixed ok/error with matching ok shapes → combines only the ok DBs", () => {
    const outcomes = [
      outcome("DB_A", cols, [[1, 10]]),
      outcome("DB_BROKE", cols, [], { error: "boom" }),
      outcome("DB_C", cols, [[3, 30]]),
    ];
    const { combined, canCombine } = combineFanoutResults(outcomes);
    expect(canCombine).toBe(true);
    // Errored DB excluded; ok DBs in input order.
    expect(combined!.rows.map((r) => r[0])).toEqual([
      { kind: "Text", value: "DB_A" },
      { kind: "Text", value: "DB_C" },
    ]);
  });

  test("single ok DB → combined (degenerate but valid)", () => {
    const { combined, canCombine } = combineFanoutResults([outcome("DB_A", cols, [[1, 10]])]);
    expect(canCombine).toBe(true);
    expect(combined!.rows.length).toBe(1);
  });
});

describe("fanoutStatus", () => {
  test("rows sum across a DB's result sets; ok mirrors error==null; order + ms preserved", () => {
    const multi: DbRunOutcome = {
      database: "DB_MULTI",
      results: [
        { columns: [col("a")], rows: [[intCell(1)], [intCell(2)]], rowsAffected: null },
        { columns: [col("b")], rows: [[intCell(3)]], rowsAffected: null },
      ],
      error: null,
      elapsedMs: 42,
    };
    const errored: DbRunOutcome = { database: "DB_ERR", results: [], error: "boom", elapsedMs: 7 };
    expect(fanoutStatus([multi, errored])).toEqual([
      { database: "DB_MULTI", rows: 3, ok: true, error: null, elapsedMs: 42 },
      { database: "DB_ERR", rows: 0, ok: false, error: "boom", elapsedMs: 7 },
    ]);
  });
});

describe("effectiveFanoutDatabases", () => {
  const dbs = [db("DB_A"), db("DB_B"), db("DB_OFF", "OFFLINE")];

  test("keeps only ONLINE names present on the connection, preserving selection order", () => {
    expect(effectiveFanoutDatabases(["DB_B", "DB_A"], dbs)).toEqual(["DB_B", "DB_A"]);
  });

  test("drops stale/absent names (from another connection)", () => {
    expect(effectiveFanoutDatabases(["DB_A", "DB_GONE"], dbs)).toEqual(["DB_A"]);
  });

  test("drops non-ONLINE selections", () => {
    expect(effectiveFanoutDatabases(["DB_A", "DB_OFF"], dbs)).toEqual(["DB_A"]);
  });

  test("empty selection → empty", () => {
    expect(effectiveFanoutDatabases([], dbs)).toEqual([]);
  });
});
