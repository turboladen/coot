import { describe, expect, test } from "bun:test";
import type { ColumnInfo, Param, SavedQuery } from "./api";
import {
  autoTypeParams,
  catalogTypeToSqlType,
  deriveParams,
  nextParamValues,
  parseStringMap,
  persistDeclared,
  queriesReferencingTable,
  resolve,
  routeWrites,
  scanParamNames,
  toResolvedParams,
  valueSource,
} from "./paramBarLogic";

const bind = (name: string, lastValue: string | null = null): Param => ({
  name,
  sqlType: "int",
  lastValue,
  scope: "local",
});

describe("deriveParams", () => {
  test("new @names get raw-text/local/unset defaults, first-appearance order", () => {
    const got = deriveParams("SELECT * FROM t WHERE a=@b AND c=@a", []);
    expect(got.map((p) => p.name)).toEqual(["@b", "@a"]);
    expect(got[0]).toEqual({ name: "@b", sqlType: null, lastValue: null, scope: "local" });
  });

  test("existing params keep their sqlType/scope/lastValue", () => {
    const stored = [bind("@cust", "12345")];
    const got = deriveParams("WHERE cust=@cust", stored);
    expect(got).toEqual(stored);
  });

  test("duplicate @name collapses to one entry", () => {
    expect(deriveParams("@x=@x", []).map((p) => p.name)).toEqual(["@x"]);
  });

  test("no @params → empty", () => {
    expect(deriveParams("SELECT 1", [])).toEqual([]);
  });

  test("skips @@globals (doubled @), keeps real @params", () => {
    expect(deriveParams("SELECT @@ROWCOUNT, @x", []).map((p) => p.name)).toEqual(["@x"]);
  });

  test("billz-7c9: a @word inside a string literal is NOT a phantom param", () => {
    expect(deriveParams("SELECT * FROM u WHERE email = 'sales@vendor.com'", [])).toEqual([]);
  });
});

describe("scanParamNames — lexer-safe (billz-7c9 corpus)", () => {
  // The `P` column of the shared corpus mirrored in core/src/param_bind.rs splice tests.
  test("#1 plain params, first-appearance order", () => {
    expect(scanParamNames("WHERE a=@b AND c=@a")).toEqual(["@b", "@a"]);
  });
  test("#2 single-quote string literal is skipped", () => {
    expect(scanParamNames("WHERE note = '@dir'")).toEqual([]);
  });
  test("#3 N'...' unicode string literal is skipped", () => {
    expect(scanParamNames("WHERE n = N'@dir'")).toEqual([]);
  });
  test("#4 '' escape keeps one string; trailing param seen", () => {
    expect(scanParamNames("SELECT '@a''@b', @c")).toEqual(["@c"]);
  });
  test("#5 -- line comment skipped; next line param seen", () => {
    expect(scanParamNames("SELECT 1 -- @x\nWHERE y=@z")).toEqual(["@z"]);
  });
  test("#6 /* */ block comment skipped", () => {
    expect(scanParamNames("SELECT /* @a */ @b")).toEqual(["@b"]);
  });
  test("#7 nested block comment — single close does not exit", () => {
    expect(scanParamNames("SELECT /* @a /* @b */ @c */ @d")).toEqual(["@d"]);
  });
  test("#8 bracketed identifier skipped", () => {
    expect(scanParamNames("SELECT [@col], @real")).toEqual(["@real"]);
  });
  test("#9 ]] escaped bracket keeps identifier closed correctly", () => {
    expect(scanParamNames("SELECT [we]]ird @x], @y")).toEqual(["@y"]);
  });
  test("#10 @@ system var skipped", () => {
    expect(scanParamNames("SELECT @@ROWCOUNT, @x")).toEqual(["@x"]);
  });
  test("#11 param immediately after a string closes", () => {
    expect(scanParamNames("WHERE a='x'@y")).toEqual(["@y"]);
  });
  test("#12 lone @ and @, are not params", () => {
    expect(scanParamNames("SELECT @ , @, @dir")).toEqual(["@dir"]);
  });
  test("#13 param in normal context, same name echoed in a comment", () => {
    expect(scanParamNames("ORDER BY @dir -- keep @dir")).toEqual(["@dir"]);
  });
  test("#14 real param also appearing inside a string literal (deduped to one)", () => {
    expect(scanParamNames("WHERE note='@dir' ORDER BY @dir")).toEqual(["@dir"]);
  });
  test("#15 double-quoted identifier skipped", () => {
    expect(scanParamNames('SELECT "@col", @x')).toEqual(["@x"]);
  });
  test("#16 unterminated string does not hang and yields no param", () => {
    expect(scanParamNames("WHERE a='@x")).toEqual([]);
  });
});

describe("toResolvedParams", () => {
  const params: Param[] = [
    bind("@cust", "old"),
    { name: "@col", sqlType: null, lastValue: null, scope: "local" },
  ];

  test("pulls current field values, '' when unset", () => {
    const got = toResolvedParams(params, { "@cust": "42" });
    expect(got).toEqual([
      { name: "@cust", sqlType: "int", value: "42" },
      { name: "@col", sqlType: null, value: "" },
    ]);
  });
});

describe("resolve / valueSource (precedence Local ?? Session ?? Global)", () => {
  const p = (name: string, lastValue: string | null): Param => ({
    name,
    sqlType: null,
    lastValue,
    scope: "local",
  });

  test("Local wins", () => {
    expect(resolve(p("@a", "L"), { "@a": "S" }, { "@a": "G" })).toBe("L");
    expect(valueSource(p("@a", "L"), { "@a": "S" }, {})).toBe("local");
  });
  test("Session when no Local", () => {
    expect(resolve(p("@a", null), { "@a": "S" }, { "@a": "G" })).toBe("S");
    expect(valueSource(p("@a", null), { "@a": "S" }, { "@a": "G" })).toBe("session");
  });
  test("Global when no Local/Session", () => {
    expect(resolve(p("@a", null), {}, { "@a": "G" })).toBe("G");
    expect(valueSource(p("@a", null), {}, { "@a": "G" })).toBe("global");
  });
  test("null when nowhere", () => {
    expect(resolve(p("@a", null), {}, {})).toBe(null);
    expect(valueSource(p("@a", null), {}, {})).toBe(null);
  });
});

describe("routeWrites", () => {
  const p = (name: string, scope: "local" | "session" | "global"): Param => ({
    name,
    sqlType: null,
    lastValue: "old",
    scope,
  });

  test("Local → lastValue set; Session/Global → store write + lastValue cleared", () => {
    const params = [p("@l", "local"), p("@s", "session"), p("@g", "global")];
    const got = routeWrites(params, { "@l": "lv", "@s": "sv", "@g": "gv" });
    expect(got.params.find((q) => q.name === "@l")!.lastValue).toBe("lv");
    expect(got.params.find((q) => q.name === "@s")!.lastValue).toBe(null);
    expect(got.params.find((q) => q.name === "@g")!.lastValue).toBe(null);
    expect(got.session).toEqual({ "@s": "sv" });
    expect(got.global).toEqual({ "@g": "gv" });
    expect(got.params.map((q) => q.scope)).toEqual(["local", "session", "global"]);
  });
});

describe("catalogTypeToSqlType", () => {
  test("maps exact families", () => {
    expect(catalogTypeToSqlType("int")).toBe("int");
    expect(catalogTypeToSqlType("bigint")).toBe("bigint");
    expect(catalogTypeToSqlType("bit")).toBe("bit");
    expect(catalogTypeToSqlType("date")).toBe("date");
    expect(catalogTypeToSqlType("uniqueidentifier")).toBe("uniqueidentifier");
  });
  test("strips width/precision suffixes", () => {
    expect(catalogTypeToSqlType("nvarchar(50)")).toBe("nvarchar");
    expect(catalogTypeToSqlType("decimal(19,4)")).toBe("decimal");
    expect(catalogTypeToSqlType("datetime2(7)")).toBe("datetime2");
    expect(catalogTypeToSqlType("nvarchar(MAX)")).toBe("nvarchar");
  });
  test("widens into the capped set", () => {
    expect(catalogTypeToSqlType("smallint")).toBe("int");
    expect(catalogTypeToSqlType("tinyint")).toBe("int");
    expect(catalogTypeToSqlType("varchar(10)")).toBe("nvarchar");
    expect(catalogTypeToSqlType("char(2)")).toBe("nvarchar");
    expect(catalogTypeToSqlType("datetime")).toBe("datetime2");
    expect(catalogTypeToSqlType("smalldatetime")).toBe("datetime2");
    expect(catalogTypeToSqlType("numeric(10,0)")).toBe("decimal");
    expect(catalogTypeToSqlType("smallmoney")).toBe("money");
    expect(catalogTypeToSqlType("money")).toBe("money");
  });
  test("unmappable types → null (raw-text)", () => {
    expect(catalogTypeToSqlType("float")).toBe(null);
    expect(catalogTypeToSqlType("real")).toBe(null);
    expect(catalogTypeToSqlType("time(7)")).toBe(null);
    expect(catalogTypeToSqlType("varbinary(MAX)")).toBe(null);
    expect(catalogTypeToSqlType("xml")).toBe(null);
  });
  test("case-insensitive", () => {
    expect(catalogTypeToSqlType("NVarChar(50)")).toBe("nvarchar");
    expect(catalogTypeToSqlType("INT")).toBe("int");
  });
});

describe("parseStringMap", () => {
  test("null / malformed → {}", () => {
    expect(parseStringMap(null)).toEqual({});
    expect(parseStringMap("{not json")).toEqual({});
    expect(parseStringMap("[1,2]")).toEqual({});
  });
  test("keeps string entries, drops non-string", () => {
    expect(parseStringMap(JSON.stringify({ "@a": "x", "@b": 5, "@c": "y" }))).toEqual({
      "@a": "x",
      "@c": "y",
    });
  });
});

describe("nextParamValues", () => {
  const p = (name: string, lastValue: string | null = null): Param => ({
    name,
    sqlType: null,
    lastValue,
    scope: "local",
  });

  test("tab switch resets each field fresh from lastValue (drops typed values)", () => {
    expect(nextParamValues(true, [p("@a", "av"), p("@b")], { "@a": "typed" }, {}, {})).toEqual({
      "@a": "av",
      "@b": "",
    });
  });

  test("same-tab preserves typed values, seeds newly-appeared params from lastValue", () => {
    expect(
      nextParamValues(false, [p("@a", "av"), p("@new", "nv")], { "@a": "typed" }, {}, {}),
    ).toEqual({ "@a": "typed", "@new": "nv" });
  });

  test("same-tab preserves a user-cleared empty value ('' is not replaced)", () => {
    expect(nextParamValues(false, [p("@a", "av")], { "@a": "" }, {}, {})).toEqual({ "@a": "" });
  });

  test("pre-fills from resolve across tiers on tab switch", () => {
    expect(nextParamValues(true, [p("@a"), p("@b", "L")], {}, { "@a": "S" }, {})).toEqual({
      "@a": "S",
      "@b": "L",
    });
  });
});

const savedQuery = (id: string, sql: string): SavedQuery => ({
  id,
  name: id,
  sql,
  targetDatabase: null,
  params: [],
});
const col = (name: string, dataType: string): ColumnInfo => ({
  name,
  dataType,
  nullable: true,
  isPrimaryKey: false,
  isForeignKey: false,
  ordinal: 0,
});

describe("queriesReferencingTable", () => {
  test("keeps queries that reference @table, drops others", () => {
    const qs = [
      savedQuery("a", "SELECT * FROM @table"),
      savedQuery("b", "SELECT 1"),
      savedQuery("c", "SELECT * FROM @table WHERE id=@id"),
    ];
    expect(queriesReferencingTable(qs).map((q) => q.id)).toEqual(["a", "c"]);
  });
  test("word-boundary: @table2 / @tablename do NOT count; @@table does not", () => {
    expect(queriesReferencingTable([savedQuery("a", "FROM @table2")])).toEqual([]);
    expect(queriesReferencingTable([savedQuery("a", "FROM @tablename")])).toEqual([]);
    expect(queriesReferencingTable([savedQuery("a", "PRINT @@table")])).toEqual([]);
  });
});

describe("autoTypeParams", () => {
  const p = (name: string, sqlType: import("./api").SqlType | null): Param => ({
    name,
    sqlType,
    lastValue: null,
    scope: "local",
  });
  const columns = [col("cust", "int"), col("region", "nvarchar(20)"), col("score", "float")];

  test("fills an unset param from a name-matching column (case-insensitive)", () => {
    const got = autoTypeParams([p("@cust", null), p("@Region", null)], columns);
    expect(got.find((q) => q.name === "@cust")!.sqlType).toBe("int");
    expect(got.find((q) => q.name === "@Region")!.sqlType).toBe("nvarchar");
  });
  test("skips @table, already-typed params, and non-matching names", () => {
    const got = autoTypeParams([p("@table", null), p("@cust", "bigint"), p("@nope", null)], columns);
    expect(got.find((q) => q.name === "@table")!.sqlType).toBe(null);
    expect(got.find((q) => q.name === "@cust")!.sqlType).toBe("bigint");
    expect(got.find((q) => q.name === "@nope")!.sqlType).toBe(null);
  });
  test("unmappable column type (float) leaves the param raw-text (null)", () => {
    expect(autoTypeParams([p("@score", null)], columns).find((q) => q.name === "@score")!.sqlType)
      .toBe(null);
  });
});

describe("persistDeclared (d28.8 — saved query is a stable template)", () => {
  const stored = (sql: string, params: Param[]): SavedQuery => ({
    id: "q1",
    name: "q1",
    sql,
    targetDatabase: null,
    params,
  });
  const local = (name: string, lastValue: string | null = null): Param => ({
    name,
    sqlType: null,
    lastValue,
    scope: "local",
  });

  test("orphan: an edited-in param (in values, not in stored sql) is never persisted", () => {
    const q = stored("SELECT * FROM t WHERE a=@a", [local("@a")]);
    const got = persistDeclared(q, { "@a": "1", "@b": "99" });
    expect(got.params.map((p) => p.name)).toEqual(["@a"]);
    expect(got.session).toEqual({});
    expect(got.global).toEqual({});
  });

  test("normal fill: a declared local param's value is remembered (lastValue set)", () => {
    const q = stored("WHERE a=@a", [local("@a")]);
    const got = persistDeclared(q, { "@a": "42" });
    expect(got.params.find((p) => p.name === "@a")!.lastValue).toBe("42");
  });

  test("edit-out is non-destructive: a declared param absent from values keeps its stored value", () => {
    const q = stored("WHERE a=@a AND b=@b", [local("@a", "kept"), local("@b", "old")]);
    // @b edited out of the tab → not in the bar's values.
    const got = persistDeclared(q, { "@a": "new" });
    expect(got.params.find((p) => p.name === "@a")!.lastValue).toBe("new");
    expect(got.params.find((p) => p.name === "@b")!.lastValue).toBe("old");
  });

  test("tier routing: a declared session param routes to the session map, lastValue cleared", () => {
    const q = stored("WHERE a=@a", [{ name: "@a", sqlType: null, lastValue: null, scope: "session" }]);
    const got = persistDeclared(q, { "@a": "sv" });
    expect(got.session).toEqual({ "@a": "sv" });
    expect(got.params.find((p) => p.name === "@a")!.lastValue).toBe(null);
  });

  test("cleared-but-visible: a declared param set to '' persists '' (distinct from edit-out)", () => {
    const q = stored("WHERE a=@a", [local("@a", "old")]);
    const got = persistDeclared(q, { "@a": "" });
    expect(got.params.find((p) => p.name === "@a")!.lastValue).toBe("");
  });

  test("consistency: returned param names are always a subset of the stored sql's params", () => {
    const q = stored("WHERE a=@a", [local("@a")]);
    const got = persistDeclared(q, { "@a": "1", "@b": "2", "@c": "3" });
    expect(got.params.every((p) => q.sql.includes(p.name))).toBe(true);
  });
});
