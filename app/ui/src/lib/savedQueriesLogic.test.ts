// `bun test` — savedQueriesLogic.ts is rune-free plain TS, so it imports cleanly
// here with no Svelte compiler (unlike savedQueries.svelte.ts). Excluded from
// svelte-check via tsconfig `exclude`, same as tabsLogic.test.ts.
import { describe, expect, test } from "bun:test";
import type { SavedQuery } from "./api";
import { filterQueries, promoteToSavedQuery } from "./savedQueriesLogic";

function sq(id: string, name: string, sql: string): SavedQuery {
  return { id, name, sql, targetDatabase: null, params: [] };
}

describe("filterQueries", () => {
  const list = [
    sq("a", "Orders by customer", "SELECT * FROM orders WHERE cust = @cust"),
    sq("b", "Recent logins", "SELECT TOP 10 * FROM audit_log"),
  ];

  test("matches on name (case-insensitive)", () => {
    expect(filterQueries(list, "ORDERS").map((q) => q.id)).toEqual(["a"]);
  });

  test("matches on sql substring", () => {
    expect(filterQueries(list, "audit_log").map((q) => q.id)).toEqual(["b"]);
  });

  test("case-insensitive on sql too", () => {
    expect(filterQueries(list, "Cust").map((q) => q.id)).toEqual(["a"]);
  });

  test("empty search → all", () => {
    expect(filterQueries(list, "")).toEqual(list);
  });

  test("whitespace-only search → all", () => {
    expect(filterQueries(list, "   ")).toEqual(list);
  });

  test("no match → []", () => {
    expect(filterQueries(list, "zzz")).toEqual([]);
  });
});

describe("promoteToSavedQuery", () => {
  test("trims the name", () => {
    expect(promoteToSavedQuery("id1", "  My query  ", "SELECT 1", null).name).toBe("My query");
  });

  test("uses the injected id", () => {
    expect(promoteToSavedQuery("id1", "n", "SELECT 1", null).id).toBe("id1");
  });

  test("params is always []", () => {
    expect(promoteToSavedQuery("id1", "n", "SELECT * FROM t WHERE x = @x", null).params).toEqual([]);
  });

  test("passes through sql and targetDatabase", () => {
    const q = promoteToSavedQuery("id1", "n", "SELECT 1", "ESP_DEV");
    expect(q.sql).toBe("SELECT 1");
    expect(q.targetDatabase).toBe("ESP_DEV");
  });
});
