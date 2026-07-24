// `bun test` — tabsLogic.ts is rune-free plain TS, so it imports cleanly here
// with no Svelte compiler (unlike tabs.svelte.ts). Excluded from svelte-check via
// tsconfig `exclude`, same as renderCell.test.ts / resultSummary.test.ts.
import { describe, expect, test } from "bun:test";
import type { SavedQuery } from "./api";
import { deriveTitle, deserialize, isTabDirty, pickNeighbourId, type QueryTab, serialize, type TabsState } from "./tabsLogic";

function tab(
  id: string,
  content = "",
  database: string | null = null,
  savedQueryId: string | null = null,
  connectionId: string | null = null,
): QueryTab {
  return { id, title: deriveTitle(content), content, database, savedQueryId, connectionId, fanout: false, fanoutDatabases: [] };
}

describe("deriveTitle", () => {
  test("empty content → Untitled", () => {
    expect(deriveTitle("")).toBe("Untitled");
  });

  test("whitespace-only content → Untitled", () => {
    expect(deriveTitle("   \n\t\n  ")).toBe("Untitled");
  });

  test("first non-empty line wins, skipping leading blanks", () => {
    expect(deriveTitle("\n\n  SELECT 1  \nSELECT 2")).toBe("SELECT 1");
  });

  test("truncates long lines to ~24 chars with an ellipsis", () => {
    const title = deriveTitle("SELECT * FROM a_very_long_table_name_here");
    expect(title).toBe("SELECT * FROM a_very_lo…");
    expect(title.length).toBe(24);
  });

  test("short line kept verbatim (no ellipsis)", () => {
    expect(deriveTitle("SELECT 1")).toBe("SELECT 1");
  });
});

describe("pickNeighbourId", () => {
  const tabs = [tab("a"), tab("b"), tab("c")];

  test("close active middle → left neighbour", () => {
    expect(pickNeighbourId(tabs, "b")).toBe("a");
  });

  test("close first → new first (the old second)", () => {
    expect(pickNeighbourId(tabs, "a")).toBe("b");
  });

  test("close last → left neighbour", () => {
    expect(pickNeighbourId(tabs, "c")).toBe("b");
  });

  test("close the only remaining tab → null (caller reseeds)", () => {
    expect(pickNeighbourId([tab("a")], "a")).toBe(null);
  });

  test("unknown id → current first tab's id", () => {
    expect(pickNeighbourId(tabs, "zzz")).toBe("a");
  });
});

describe("isTabDirty", () => {
  function saved(id: string, sql: string): SavedQuery {
    return { id, name: id, sql, targetDatabase: null, params: [] };
  }
  const lib = [saved("q-1", "SELECT 1")];

  test("linked, content differs → dirty", () => {
    expect(isTabDirty(tab("a", "SELECT 2", null, "q-1"), lib)).toBe(true);
  });

  test("linked, content matches → clean", () => {
    expect(isTabDirty(tab("a", "SELECT 1", null, "q-1"), lib)).toBe(false);
  });

  test("scratch tab (no savedQueryId) → never dirty", () => {
    expect(isTabDirty(tab("a", "SELECT 2", null, null), lib)).toBe(false);
  });

  test("linked query missing from library → not dirty", () => {
    expect(isTabDirty(tab("a", "SELECT 2", null, "gone"), lib)).toBe(false);
  });

  test("empty library with a linked tab → not dirty", () => {
    expect(isTabDirty(tab("a", "SELECT 2", null, "q-1"), [])).toBe(false);
  });

  test("trailing-newline-only diff reads dirty (exact-compare intent)", () => {
    expect(isTabDirty(tab("a", "SELECT 1\n", null, "q-1"), lib)).toBe(true);
  });
});

describe("serialize / deserialize", () => {
  const state: TabsState = {
    tabs: [tab("a", "SELECT 1"), tab("b", "SELECT 2")],
    activeId: "b",
  };

  test("round-trips a valid state", () => {
    expect(deserialize(serialize(state))).toEqual(state);
  });

  test("null input → null", () => {
    expect(deserialize(null)).toBe(null);
  });

  test("malformed JSON → null", () => {
    expect(deserialize("{not json")).toBe(null);
  });

  test("empty tabs array → null (caller seeds default)", () => {
    expect(deserialize(JSON.stringify({ tabs: [], activeId: "a" }))).toBe(null);
  });

  test("a malformed tab poisons the blob → null", () => {
    expect(deserialize(JSON.stringify({ tabs: [{ id: "a" }], activeId: "a" }))).toBe(null);
  });

  test("round-trips a tab's target database (billz-cwt.9)", () => {
    const withDb: TabsState = {
      tabs: [tab("a", "SELECT 1", "ESP_Arnotts_Group_DEV"), tab("b", "SELECT 2", null)],
      activeId: "a",
    };
    expect(deserialize(serialize(withDb))).toEqual(withDb);
  });

  test("a v1 blob without a database field defaults to null (backward compat)", () => {
    // Tabs persisted before cwt.9 have no `database` key — must load, not poison.
    const legacy = JSON.stringify({
      tabs: [{ id: "a", title: "SELECT 1", content: "SELECT 1" }],
      activeId: "a",
    });
    expect(deserialize(legacy)?.tabs[0].database).toBe(null);
  });

  test("round-trips savedQueryId (d28.3)", () => {
    const withSq: TabsState = {
      tabs: [tab("a", "SELECT 1", null, "q-42")],
      activeId: "a",
    };
    expect(deserialize(serialize(withSq))).toEqual(withSq);
  });

  test("a pre-d28.3 blob without savedQueryId defaults to null", () => {
    const legacy = JSON.stringify({
      tabs: [{ id: "a", title: "t", content: "SELECT 1", database: null }],
      activeId: "a",
    });
    expect(deserialize(legacy)?.tabs[0].savedQueryId).toBe(null);
  });

  test("a non-string, non-null database coerces to null", () => {
    const bad = JSON.stringify({
      tabs: [{ id: "a", title: "t", content: "SELECT 1", database: 42 }],
      activeId: "a",
    });
    expect(deserialize(bad)?.tabs[0].database).toBe(null);
  });

  test("an empty-string database normalizes to null (no USE [] at run time)", () => {
    const empty = JSON.stringify({
      tabs: [{ id: "a", title: "t", content: "SELECT 1", database: "" }],
      activeId: "a",
    });
    expect(deserialize(empty)?.tabs[0].database).toBe(null);
  });

  test("round-trips fan-out state (billz-0gh.1.3)", () => {
    const t = tab("a", "SELECT 1");
    t.fanout = true;
    t.fanoutDatabases = ["ESP_Nomad_SE_DEV", "ESP_Nomad_US_DEV"];
    const withFanout: TabsState = { tabs: [t], activeId: "a" };
    expect(deserialize(serialize(withFanout))).toEqual(withFanout);
  });

  test("a pre-fanout blob without fanout keys defaults to false/[]", () => {
    const legacy = JSON.stringify({
      tabs: [{ id: "a", title: "t", content: "SELECT 1", database: null, savedQueryId: null }],
      activeId: "a",
    });
    const t = deserialize(legacy)?.tabs[0];
    expect(t?.fanout).toBe(false);
    expect(t?.fanoutDatabases).toEqual([]);
  });

  test("garbled fanout fields default rather than poison the blob", () => {
    const bad = JSON.stringify({
      tabs: [{ id: "a", title: "t", content: "SELECT 1", fanout: "yes", fanoutDatabases: "nope" }],
      activeId: "a",
    });
    const t = deserialize(bad)?.tabs[0];
    expect(t?.fanout).toBe(false);
    expect(t?.fanoutDatabases).toEqual([]);
  });

  test("non-string entries in fanoutDatabases are dropped", () => {
    const mixed = JSON.stringify({
      tabs: [{ id: "a", title: "t", content: "SELECT 1", fanoutDatabases: ["DB_A", 42, null, "DB_B"] }],
      activeId: "a",
    });
    expect(deserialize(mixed)?.tabs[0].fanoutDatabases).toEqual(["DB_A", "DB_B"]);
  });

  test("round-trips a tab's connectionId (billz-a5y.1)", () => {
    const withConn: TabsState = {
      tabs: [tab("a", "SELECT 1", null, null, "conn-x"), tab("b", "SELECT 2", null, null, null)],
      activeId: "a",
    };
    expect(deserialize(serialize(withConn))).toEqual(withConn);
  });

  test("a pre-a5y.1 blob without a connectionId field defaults to null", () => {
    // Tabs persisted before a5y.1 have no `connectionId` key — must load, not poison,
    // and default to null (= no connection → empty state), not inherit a stale id.
    const legacy = JSON.stringify({
      tabs: [{ id: "a", title: "t", content: "SELECT 1", database: null, savedQueryId: null }],
      activeId: "a",
    });
    expect(deserialize(legacy)?.tabs[0].connectionId).toBe(null);
  });

  test("a non-string, non-null connectionId coerces to null", () => {
    const bad = JSON.stringify({
      tabs: [{ id: "a", title: "t", content: "SELECT 1", connectionId: 42 }],
      activeId: "a",
    });
    expect(deserialize(bad)?.tabs[0].connectionId).toBe(null);
  });

  test("an empty-string connectionId normalizes to null (no dead-id target)", () => {
    const empty = JSON.stringify({
      tabs: [{ id: "a", title: "t", content: "SELECT 1", connectionId: "" }],
      activeId: "a",
    });
    expect(deserialize(empty)?.tabs[0].connectionId).toBe(null);
  });

  test("dangling activeId is repaired to the first tab", () => {
    const repaired = deserialize(JSON.stringify({ tabs: [tab("a"), tab("b")], activeId: "gone" }));
    expect(repaired?.activeId).toBe("a");
  });

  test("missing activeId is repaired to the first tab", () => {
    const repaired = deserialize(JSON.stringify({ tabs: [tab("a")] }));
    expect(repaired?.activeId).toBe("a");
  });
});
