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

  test("no @params → params is []", () => {
    expect(promoteToSavedQuery("id1", "n", "SELECT 1", null).params).toEqual([]);
  });

  // billz-he0: promote used to hardcode `params: []`, while the sibling save paths
  // (openScopedQuery, App.updateSavedQuery) both ran deriveParams — so a promoted
  // query with @params came back with an empty param list and no param bar.
  test("derives @params from the sql", () => {
    const q = promoteToSavedQuery("id1", "n", "SELECT * FROM t WHERE x = @x AND y = @y", null);
    // The sigil is part of the name throughout the param model (paramBarLogic).
    expect(q.params.map((p) => p.name)).toEqual(["@x", "@y"]);
  });

  test("derived params start unconfigured and local-scoped", () => {
    const [p] = promoteToSavedQuery("id1", "n", "SELECT @a", null).params;
    expect(p).toEqual({ name: "@a", sqlType: null, lastValue: null, scope: "local" });
  });

  test("first-appearance order, deduped", () => {
    const q = promoteToSavedQuery("id1", "n", "SELECT @b, @a, @b", null);
    expect(q.params.map((p) => p.name)).toEqual(["@b", "@a"]);
  });

  // A literal/comment @word is not a param — deriveParams is lexer-aware (billz-7c9),
  // and promote inherits that rather than re-implementing a naive scan.
  test("ignores @words inside string literals", () => {
    expect(promoteToSavedQuery("id1", "n", "SELECT '@notaparam'", null).params).toEqual([]);
  });

  test("passes through sql and targetDatabase", () => {
    const q = promoteToSavedQuery("id1", "n", "SELECT 1", "ESP_DEV");
    expect(q.sql).toBe("SELECT 1");
    expect(q.targetDatabase).toBe("ESP_DEV");
  });
});
