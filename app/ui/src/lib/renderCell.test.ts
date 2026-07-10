// `bun test` — zero new dependency (Bun's built-in runner executes TS natively).
// renderCell.ts imports only `import type { CellValue }` (fully erased), so
// api.ts's `@tauri-apps/api/core` import is never evaluated here — no Tauri at
// test time. Excluded from svelte-check via tsconfig `exclude` (bun:test has no
// TS types in this project).
import { describe, expect, test } from "bun:test";
import { renderCell } from "./renderCell";

describe("renderCell", () => {
  test("Int → right-aligned, monospace", () => {
    expect(renderCell({ kind: "Int", value: 42 })).toEqual({
      text: "42",
      align: "right",
      nullish: false,
      mono: true,
    });
  });

  test("Float → right-aligned, monospace", () => {
    expect(renderCell({ kind: "Float", value: 3.5 })).toEqual({
      text: "3.5",
      align: "right",
      nullish: false,
      mono: true,
    });
  });

  test("Decimal → string preserved verbatim (NOT Number()-ed)", () => {
    // A value that would lose precision through an f64 round-trip.
    const precise = "12345678901234567890.1234";
    const r = renderCell({ kind: "Decimal", value: precise });
    expect(r.text).toBe(precise);
    expect(r.align).toBe("right");
    expect(r.mono).toBe(true);
  });

  test("Null → 'NULL' + nullish flag", () => {
    expect(renderCell({ kind: "Null" })).toEqual({
      text: "NULL",
      align: "left",
      nullish: true,
      mono: false,
    });
  });

  test("Bool → 'true' / 'false'", () => {
    expect(renderCell({ kind: "Bool", value: true }).text).toBe("true");
    expect(renderCell({ kind: "Bool", value: false }).text).toBe("false");
  });

  test("Binary → 0x… hex verbatim, monospace", () => {
    const r = renderCell({ kind: "Binary", value: "0xdeadbeef" });
    expect(r.text).toBe("0xdeadbeef");
    expect(r.align).toBe("left");
    expect(r.mono).toBe(true);
  });

  test("Text → left-aligned", () => {
    expect(renderCell({ kind: "Text", value: "hello" })).toEqual({
      text: "hello",
      align: "left",
      nullish: false,
      mono: false,
    });
  });

  test("DateTime → left-aligned string", () => {
    const r = renderCell({ kind: "DateTime", value: "2026-07-10T12:00:00" });
    expect(r.text).toBe("2026-07-10T12:00:00");
    expect(r.align).toBe("left");
  });

  test("Uuid → left-aligned string", () => {
    const r = renderCell({ kind: "Uuid", value: "0d9b296a-0000-0000-0000-000000000000" });
    expect(r.align).toBe("left");
    expect(r.mono).toBe(false);
  });

  test("unknown kind → JSON fallback", () => {
    expect(renderCell({ kind: "SomethingNew", value: { a: 1 } }).text).toBe('{"a":1}');
  });

  test("unknown kind, no value → empty string", () => {
    expect(renderCell({ kind: "SomethingNew" }).text).toBe("");
  });
});
