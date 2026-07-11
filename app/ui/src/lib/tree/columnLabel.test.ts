// `bun test` — columnLabel.ts imports only `import type { ColumnInfo }` (fully
// erased), so api.ts's `@tauri-apps/api/core` import is never evaluated here.
// Excluded from svelte-check via tsconfig `exclude`.
import { describe, expect, test } from "bun:test";
import { columnLabel } from "./columnLabel";

// A base row; each case overrides only the fields it exercises.
const base = {
  name: "col",
  dataType: "int",
  nullable: false,
  isPrimaryKey: false,
  isForeignKey: false,
  ordinal: 1,
};

describe("columnLabel", () => {
  test("plain column → NOT NULL, no badges", () => {
    expect(columnLabel({ ...base, name: "id", dataType: "int" })).toEqual({
      name: "id",
      dataType: "int",
      nullText: "NOT NULL",
      isPrimaryKey: false,
      isForeignKey: false,
    });
  });

  test("nullable column → NULL", () => {
    const r = columnLabel({ ...base, name: "note", dataType: "nvarchar(50)", nullable: true });
    expect(r.dataType).toBe("nvarchar(50)");
    expect(r.nullText).toBe("NULL");
    expect(r.isPrimaryKey).toBe(false);
    expect(r.isForeignKey).toBe(false);
  });

  test("primary key → PK flag set", () => {
    const r = columnLabel({ ...base, name: "id", isPrimaryKey: true });
    expect(r.isPrimaryKey).toBe(true);
    expect(r.isForeignKey).toBe(false);
    expect(r.nullText).toBe("NOT NULL");
  });

  test("foreign key → FK flag set", () => {
    const r = columnLabel({ ...base, name: "customer_id", isForeignKey: true });
    expect(r.isForeignKey).toBe(true);
    expect(r.isPrimaryKey).toBe(false);
  });

  test("primary + foreign key → both flags set", () => {
    const r = columnLabel({ ...base, name: "id", isPrimaryKey: true, isForeignKey: true });
    expect(r.isPrimaryKey).toBe(true);
    expect(r.isForeignKey).toBe(true);
  });

  test("decimal(19,4) type passes through verbatim", () => {
    expect(columnLabel({ ...base, name: "amount", dataType: "decimal(19,4)" }).dataType).toBe(
      "decimal(19,4)",
    );
  });
});
