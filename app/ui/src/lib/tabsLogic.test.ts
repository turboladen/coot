// `bun test` — tabsLogic.ts is rune-free plain TS, so it imports cleanly here
// with no Svelte compiler (unlike tabs.svelte.ts). Excluded from svelte-check via
// tsconfig `exclude`, same as renderCell.test.ts / resultSummary.test.ts.
import { describe, expect, test } from "bun:test";
import { deriveTitle, deserialize, pickNeighbourId, type QueryTab, serialize, type TabsState } from "./tabsLogic";

function tab(id: string, content = ""): QueryTab {
  return { id, title: deriveTitle(content), content };
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

  test("dangling activeId is repaired to the first tab", () => {
    const repaired = deserialize(JSON.stringify({ tabs: [tab("a"), tab("b")], activeId: "gone" }));
    expect(repaired?.activeId).toBe("a");
  });

  test("missing activeId is repaired to the first tab", () => {
    const repaired = deserialize(JSON.stringify({ tabs: [tab("a")] }));
    expect(repaired?.activeId).toBe("a");
  });
});
