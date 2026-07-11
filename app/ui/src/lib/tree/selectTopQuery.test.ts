// `bun test` — selectTopQuery.ts has no api/rune imports, so it loads bare here.
// The one testable unit of rqb.6 (identifier quoting + 3-part query shape).
import { describe, expect, test } from "bun:test";
import { quoteIdent, selectTop1000 } from "./selectTopQuery";

describe("quoteIdent", () => {
  test("wraps a plain identifier", () => {
    expect(quoteIdent("dbo")).toBe("[dbo]");
  });

  test("doubles a closing bracket (mirrors context.rs use_statement)", () => {
    expect(quoteIdent("weird]name")).toBe("[weird]]name]");
  });
});

describe("selectTop1000", () => {
  test("3-part named query", () => {
    expect(selectTop1000("MyDb", "dbo", "Orders")).toBe(
      "SELECT TOP 1000 * FROM [MyDb].[dbo].[Orders];",
    );
  });

  test("a bracket in a part stays quoted", () => {
    expect(selectTop1000("My]Db", "dbo", "Orders")).toBe(
      "SELECT TOP 1000 * FROM [My]]Db].[dbo].[Orders];",
    );
  });
});
