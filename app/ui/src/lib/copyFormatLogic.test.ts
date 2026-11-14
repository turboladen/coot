// `bun test`. copyFormatLogic.ts imports only types plus renderCell.ts, so no
// Tauri or DOM is loaded here. Excluded from svelte-check via tsconfig `exclude`.
import { describe, expect, test } from "bun:test";
import type { CellValue, QueryResult } from "./api";
import { canCopy, copiedMessage, formatResult, toCsv, toMarkdown } from "./copyFormatLogic";

function res(columns: string[], rows: CellValue[][]): QueryResult {
  return {
    columns: columns.map((name) => ({ name, sqlType: "nvarchar", nullable: true, precision: null, scale: null })),
    rows,
    rowsAffected: null,
  };
}
const T = (value: string): CellValue => ({ kind: "Text", value });
const I = (value: number): CellValue => ({ kind: "Int", value });
const N: CellValue = { kind: "Null" };

describe("toCsv", () => {
  test("header + rows, \\n-joined, no trailing newline", () => {
    expect(toCsv(res(["id", "name"], [[I(1), T("a")], [I(2), T("b")]]))).toBe("id,name\n1,a\n2,b");
  });
  test("NULL is literal NULL", () => {
    expect(toCsv(res(["x"], [[N]]))).toBe("x\nNULL");
  });
  test("quotes commas, quotes, CR/LF and edge whitespace", () => {
    expect(toCsv(res(["v"], [[T("a,b")], [T('say "hi"')], [T("l1\r\nl2")], [T(" pad")]]))).toBe(
      'v\n"a,b"\n"say ""hi"""\n"l1\r\nl2"\n" pad"',
    );
  });
  test("header names are escaped like cells", () => {
    expect(toCsv(res(["a,b", 'q"'], []))).toBe('"a,b","q"""');
  });
  test("a lone empty field is quoted so the record is not a blank line", () => {
    expect(toCsv(res(["s"], [[T("")], [T("x")]]))).toBe('s\n""\nx');
    expect(toCsv(res(["a", "b"], [[T(""), T("")]]))).toBe("a,b\n,");
  });
  test("decimal and bigint copied verbatim", () => {
    const r = res(
      ["d", "b"],
      [[{ kind: "Decimal", value: "12345678901234567890.123456" }, { kind: "BigInt", value: "9223372036854775807" }]],
    );
    expect(toCsv(r)).toBe("d,b\n12345678901234567890.123456,9223372036854775807");
  });
});

describe("toMarkdown", () => {
  test("GFM table; numeric column right-aligned", () => {
    expect(toMarkdown(res(["id", "name"], [[I(1), T("a")]]))).toBe("| id | name |\n| ---: | --- |\n| 1 | a |");
  });
  test("all-NULL column stays left-aligned; NULL is literal", () => {
    expect(toMarkdown(res(["n"], [[N]]))).toBe("| n |\n| --- |\n| NULL |");
  });
  test("numeric column with some NULLs is still right-aligned", () => {
    expect(toMarkdown(res(["n"], [[I(1)], [N]]))).toBe("| n |\n| ---: |\n| 1 |\n| NULL |");
  });
  test("escapes pipes and turns CRLF / CR / LF into <br>", () => {
    expect(toMarkdown(res(["a|b"], [[T("x|y")], [T("1\r\n2\r3\n4")]]))).toBe(
      "| a\\|b |\n| --- |\n| x\\|y |\n| 1<br>2<br>3<br>4 |",
    );
  });
  test("backslashes before a pipe are doubled so the pipe stays escaped", () => {
    expect(toMarkdown(res(["p"], [[T("C:\\|x")], [T("a\\b")]]))).toBe("| p |\n| --- |\n| C:\\\\\\|x |\n| a\\b |");
  });
  test("empty string cell renders empty", () => {
    expect(toMarkdown(res(["s"], [[T("")]]))).toBe("| s |\n| --- |\n|  |");
  });
  test("zero rows → header + separator only", () => {
    expect(toMarkdown(res(["a"], []))).toBe("| a |\n| --- |");
  });
});

describe("canCopy / copiedMessage / formatResult", () => {
  test("canCopy needs a result with columns", () => {
    expect(canCopy(null)).toBe(false);
    expect(canCopy(res([], []))).toBe(false);
    expect(canCopy(res(["a"], []))).toBe(true);
  });
  test("copiedMessage pluralizes", () => {
    expect(copiedMessage(1, "markdown")).toBe("Copied 1 row as Markdown");
    expect(copiedMessage(42, "csv")).toBe("Copied 42 rows as CSV");
    expect(copiedMessage(0, "csv")).toBe("Copied 0 rows as CSV");
  });
  test("formatResult dispatches", () => {
    const r = res(["a"], [[I(1)]]);
    expect(formatResult(r, "csv")).toBe(toCsv(r));
    expect(formatResult(r, "markdown")).toBe(toMarkdown(r));
  });
});
