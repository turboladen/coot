import { describe, expect, test } from "bun:test";
import { childKey } from "./treeKey";

describe("childKey", () => {
  test("composes parent + kind + name", () => {
    expect(childKey("conn1", "db", "Sales")).toBe("conn1/db:Sales");
  });

  test("nests hierarchically", () => {
    const db = childKey("conn1", "db", "Sales");
    const tbl = childKey(db, "table", "dbo.Orders");
    expect(tbl).toBe("conn1/db:Sales/table:dbo.Orders");
  });

  test("distinguishes kinds at the same level", () => {
    const db = childKey("conn1", "db", "Sales");
    expect(childKey(db, "view", "dbo.X")).not.toBe(childKey(db, "table", "dbo.X"));
  });

  test("same table name under different DBs yields distinct keys", () => {
    const a = childKey(childKey("c", "db", "A"), "table", "dbo.T");
    const b = childKey(childKey("c", "db", "B"), "table", "dbo.T");
    expect(a).not.toBe(b);
  });

  test("same node name under different connections yields distinct keys", () => {
    expect(childKey("conn1", "db", "Sales")).not.toBe(childKey("conn2", "db", "Sales"));
  });

  test("is deterministic", () => {
    expect(childKey("c", "col", "id")).toBe(childKey("c", "col", "id"));
  });
});
