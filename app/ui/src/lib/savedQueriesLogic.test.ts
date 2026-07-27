// `bun test` — savedQueriesLogic.ts is rune-free plain TS, so it imports cleanly
// here with no Svelte compiler (unlike savedQueries.svelte.ts). Excluded from
// svelte-check via tsconfig `exclude`, same as tabsLogic.test.ts.
import { describe, expect, test } from "bun:test";
import type { SavedQuery } from "./api";
import {
  filterQueries,
  planRename,
  promoteToSavedQuery,
  renameSavedQuery,
  renameSeed,
} from "./savedQueriesLogic";

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

// billz-1kn's motivating input, in its two real shapes.
const LEGACY_SQL = "SELECT TOP 100 * FROM orders WHERE customer_id = @cust";

// SHAPE 1 (the common one): the old promote flow (0032f67) seeded its input with
// deriveTitle(sql), so an untouched pre-he0 name is a ≤24-char fragment ending in
// the truncation ellipsis. Built the same way deriveTitle builds it, so this stays
// honest if TITLE_MAX ever moves.
const TRUNCATED_NAME = LEGACY_SQL.slice(0, 23) + "…";
const truncated = (): SavedQuery => sq("legacy", TRUNCATED_NAME, LEGACY_SQL);

// SHAPE 2: that input was editable, so a name can also be the whole statement
// someone typed or pasted over the suggestion.
const pasted = (): SavedQuery => sq("legacy", LEGACY_SQL, LEGACY_SQL);

describe("renameSavedQuery", () => {
  test("changes the name", () => {
    expect(renameSavedQuery(sq("a", "Old", "SELECT 1"), "New").name).toBe("New");
  });

  // The acceptance criterion, asserted field by field: a rename must not lose the
  // id (tabs link by savedQueryId), the sql, the target database, or the params.
  test("preserves id, sql, targetDatabase and params", () => {
    const before: SavedQuery = {
      id: "a",
      name: "Old",
      sql: "SELECT * FROM t WHERE x = @x",
      targetDatabase: "ESP_DEV",
      params: [{ name: "@x", sqlType: null, lastValue: "7", scope: "global" }],
    };
    const after = renameSavedQuery(before, "New");
    expect(after.id).toBe("a");
    expect(after.sql).toBe("SELECT * FROM t WHERE x = @x");
    expect(after.targetDatabase).toBe("ESP_DEV");
    expect(after.params).toEqual(before.params);
    // Reference-equal, not merely deep-equal: the spread must carry the array
    // through, never clone or rebuild it.
    expect(after.params).toBe(before.params);
  });

  test("trims the new name", () => {
    expect(renameSavedQuery(sq("a", "Old", "SELECT 1"), "  New  ").name).toBe("New");
  });

  test("does not mutate its input", () => {
    const before = sq("a", "Old", "SELECT 1");
    renameSavedQuery(before, "New");
    expect(before.name).toBe("Old");
  });

  // Documents the split: the pure fn does NOT guard an empty name, planRename does.
  test("does not guard an empty name — the caller does", () => {
    expect(renameSavedQuery(sq("a", "Old", "SELECT 1"), "   ").name).toBe("");
  });

  test("a legacy whole-SELECT name renames with the sql intact", () => {
    const after = renameSavedQuery(pasted(), "Recent orders");
    expect(after.name).toBe("Recent orders");
    expect(after.sql).toBe(LEGACY_SQL);
  });
});

describe("renameSeed", () => {
  test("keeps a normal name", () => {
    expect(renameSeed(sq("a", "Orders by customer", "SELECT 1"))).toBe("Orders by customer");
  });

  // billz-1kn plan review: a length threshold would throw away a real name on a
  // typo-fix rename — the same bug as always re-deriving. Length is not the signal.
  test("keeps a long human name (no length threshold)", () => {
    const longName = "Quarterly revenue reconciliation across all regional ledgers.";
    expect(longName.length).toBeGreaterThan(60);
    expect(renameSeed(sq("a", longName, "SELECT 1"))).toBe(longName);
  });

  test("trims a normal name", () => {
    expect(renameSeed(sq("a", "  Orders  ", "SELECT 1"))).toBe("Orders");
  });

  // SHAPE 1 — the shape the old promote flow actually minted, and the one the
  // first version of this function missed: the "…" made the prefix test fail, so
  // the name came back verbatim and Enter persisted the ellipsis forever.
  test("a deriveTitle-truncated legacy name → derives (ellipsis-aware prefix test)", () => {
    expect(TRUNCATED_NAME).toEndWith("…");
    expect(renameSeed(truncated())).toBe("SELECT TOP 100 * FROM o");
  });

  // SHAPE 2 — someone typed/pasted the whole statement over the suggestion.
  test("name IS the sql → derives from the sql", () => {
    expect(renameSeed(pasted())).toBe("SELECT TOP 100 * FROM o");
  });

  // A half-pasted legacy entry: the name is a truncated PREFIX of the statement.
  test("name is a prefix of the sql → derives", () => {
    expect(renameSeed(sq("a", "SELECT TOP 100 * FROM", LEGACY_SQL))).toBe("SELECT TOP 100 * FROM o");
  });

  test("multi-line name → derives", () => {
    expect(renameSeed(sq("a", "SELECT *\nFROM orders", "SELECT 1 FROM dual"))).toBe(
      "SELECT 1 FROM dual",
    );
  });

  test("empty name → derives", () => {
    expect(renameSeed(sq("a", "", "SELECT 1"))).toBe("SELECT 1");
  });

  test("whitespace-only name → derives", () => {
    expect(renameSeed(sq("a", "   \t ", "SELECT 1"))).toBe("SELECT 1");
  });

  // A DERIVED seed is machine-made and gets PERSISTED verbatim when you just hit
  // Enter, so it must never carry deriveTitle's truncation ellipsis. Asserted on
  // both legacy shapes — the truncated one is the case that regressed.
  // Scoped to `endsWith`, matching what tidy actually promises: it strips a
  // TRAILING ellipsis, and says nothing about one in the middle of a name.
  test("a derived seed never ends in the truncation ellipsis", () => {
    expect(renameSeed(truncated()).endsWith("…")).toBe(false);
    expect(renameSeed(pasted()).endsWith("…")).toBe(false);
  });

  // The deliberate asymmetry: only DERIVED seeds are cleaned. A name the user
  // typed comes back verbatim, so an intentional trailing ellipsis survives.
  test("keeps a human name that intentionally ends in an ellipsis", () => {
    expect(renameSeed(sq("a", "Work in progress…", "SELECT 1"))).toBe("Work in progress…");
  });

  test("unusable name + empty sql → Untitled", () => {
    expect(renameSeed(sq("a", "", "   "))).toBe("Untitled");
  });

  // tidy's "" fallback, which keeps the seed non-empty so the dialog's submit is
  // never disabled on open. Reachable: a first line of ≥25 ellipsis characters
  // truncates to 24 of them, and stripping the trailing run leaves nothing.
  test("sql that tidies away to nothing → Untitled (never an empty seed)", () => {
    expect(renameSeed(sq("a", "", "…".repeat(30)))).toBe("Untitled");
  });

  // Accepted, documented false positive — see the renameSeed comment. A 1-char
  // name that prefixes the SQL derives rather than being kept.
  test("a 1-char name that prefixes the sql derives (accepted false positive)", () => {
    expect(renameSeed(sq("a", "S", "SELECT 1"))).toBe("SELECT 1");
  });
});

describe("planRename", () => {
  const list = [sq("a", "Orders", "SELECT 1"), sq("b", "Logins", "SELECT 2")];

  function expectWrite(plan: ReturnType<typeof planRename>): SavedQuery {
    if (plan.kind !== "write") throw new Error(`expected a write plan, got "${plan.kind}"`);
    return plan.query;
  }

  test("whitespace-only name → noop", () => {
    expect(planRename(list, "a", "   ")).toEqual({ kind: "noop" });
  });

  // Ordering guard: an empty submit on an already-deleted row is a silent no-op,
  // not a "no longer exists" error toast.
  test("whitespace-only name on a missing id → noop, not missing", () => {
    expect(planRename(list, "gone", "   ")).toEqual({ kind: "noop" });
  });

  test("unchanged name → noop (no pointless write)", () => {
    expect(planRename(list, "a", "Orders")).toEqual({ kind: "noop" });
  });

  test("unchanged name after trimming → noop", () => {
    expect(planRename(list, "a", "  Orders  ")).toEqual({ kind: "noop" });
  });

  test("unknown id → missing", () => {
    expect(planRename(list, "gone", "New name")).toEqual({ kind: "missing" });
  });

  test("a real change → write, with the fields preserved", () => {
    const q = expectWrite(planRename(list, "a", "Recent orders"));
    expect(q.id).toBe("a");
    expect(q.name).toBe("Recent orders");
    expect(q.sql).toBe("SELECT 1");
  });

  test("write trims the raw name", () => {
    expect(expectWrite(planRename(list, "a", "  Recent orders  ")).name).toBe("Recent orders");
  });

  // Reads the row from the passed list, never from a snapshot the caller captured
  // when the dialog opened — otherwise a rename would silently revert a concurrent
  // SQL edit, since it writes the WHOLE row back.
  test("takes the row from the list, not a caller snapshot", () => {
    const fresh = [sq("a", "Orders", "SELECT 2 -- edited while the dialog was open")];
    expect(expectWrite(planRename(fresh, "a", "Recent orders")).sql).toBe(
      "SELECT 2 -- edited while the dialog was open",
    );
  });
});
