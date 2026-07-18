import { describe, expect, test } from "bun:test";
import type { Variable } from "./variablesLogic";
import {
  buildInsertToken,
  indexByName,
  isValidVariableName,
  variableFor,
} from "./variablesLogic";

const v = (name: string, value = "x"): Variable => ({ name, value, sqlType: "nvarchar", note: "" });

describe("isValidVariableName", () => {
  test("accepts identifier-safe names, rejects spaces/leading digit/@/empty", () => {
    expect(isValidVariableName("benchmark_user")).toBe(true);
    expect(isValidVariableName("_x1")).toBe(true);
    expect(isValidVariableName("my user")).toBe(false);
    expect(isValidVariableName("1abc")).toBe(false);
    expect(isValidVariableName("@x")).toBe(false);
    expect(isValidVariableName("")).toBe(false);
  });
});

describe("variableFor", () => {
  test("strips a leading @ and matches by bare name; miss → null", () => {
    const byName = indexByName([v("vendor", "ACME"), v("user_id", "12")]);
    expect(variableFor("@vendor", byName)?.value).toBe("ACME");
    expect(variableFor("vendor", byName)?.value).toBe("ACME");
    expect(variableFor("@nope", byName)).toBeNull();
  });
});

describe("buildInsertToken", () => {
  test("prefixes @ to the variable name", () => {
    expect(buildInsertToken(v("vendor"))).toBe("@vendor");
  });
});

import { migrateGlobalParams, parseVariables, serializeVariables } from "./variablesLogic";

describe("parseVariables", () => {
  test("round-trips a valid array", () => {
    const vars = [{ name: "vendor", value: "ACME", sqlType: "nvarchar" as const, note: "n" }];
    expect(parseVariables(serializeVariables(vars))).toEqual(vars);
  });

  test("degrades to [] on null / non-array / bad JSON", () => {
    expect(parseVariables(null)).toEqual([]);
    expect(parseVariables("{}")).toEqual([]);
    expect(parseVariables("not json")).toEqual([]);
  });

  test("drops malformed entries: bad name, missing value, unknown sqlType→null, missing note→''", () => {
    const raw = JSON.stringify([
      { name: "ok", value: "1", sqlType: "int", note: "hi" },
      { name: "bad name", value: "1", sqlType: "int", note: "" }, // invalid name → dropped
      { name: "novalue", sqlType: "int", note: "" }, // missing value → dropped
      { name: "weird", value: "2", sqlType: "float", note: "" }, // unknown type → nvarchar? no: null
      { name: "nonote", value: "3", sqlType: "bit" }, // missing note → ""
    ]);
    expect(parseVariables(raw)).toEqual([
      { name: "ok", value: "1", sqlType: "int", note: "hi" },
      { name: "weird", value: "2", sqlType: null, note: "" },
      { name: "nonote", value: "3", sqlType: "bit", note: "" },
    ]);
  });

  test("de-dupes a duplicate name, keeping the LAST occurrence's data", () => {
    const raw = JSON.stringify([
      { name: "dup", value: "first", sqlType: "int", note: "" },
      { name: "dup", value: "second", sqlType: "nvarchar", note: "n" },
    ]);
    expect(parseVariables(raw)).toEqual([{ name: "dup", value: "second", sqlType: "nvarchar", note: "n" }]);
  });
});

describe("migrateGlobalParams", () => {
  test("strips @, defaults nvarchar/empty note, skips non-identifier names", () => {
    const migrated = migrateGlobalParams({ "@today": "2026-07-17", "@user_id": "12", "@bad name": "x" });
    expect(migrated).toEqual([
      { name: "today", value: "2026-07-17", sqlType: "nvarchar", note: "" },
      { name: "user_id", value: "12", sqlType: "nvarchar", note: "" },
    ]);
  });
});

import type { Param, SavedQuery } from "./api";
import { persistInputs, resolveRun } from "./variablesLogic";

const param = (name: string, sqlType: Param["sqlType"] = null, lastValue: string | null = null): Param => ({
  name, sqlType, lastValue, scope: "local",
});

describe("resolveRun", () => {
  test("library hit binds to the VARIABLE's value+type; miss uses the input field + param type", () => {
    const byName = indexByName([{ name: "vendor", value: "ACME", sqlType: "nvarchar", note: "" }]);
    const params = [param("@vendor", "int"), param("@user_id", "int")]; // @vendor's own type is ignored
    const resolved = resolveRun(params, { "@user_id": "42" }, byName);
    expect(resolved).toEqual([
      { name: "@vendor", sqlType: "nvarchar", value: "ACME" },
      { name: "@user_id", sqlType: "int", value: "42" },
    ]);
  });

  test("a missing input value → empty string", () => {
    const resolved = resolveRun([param("@x", "int")], {}, new Map());
    expect(resolved).toEqual([{ name: "@x", sqlType: "int", value: "" }]);
  });
});

describe("persistInputs", () => {
  const stored: SavedQuery = {
    id: "q1", name: "q", targetDatabase: null,
    sql: "select * from t where a=@user_id and v=@vendor",
    params: [param("@user_id", "int"), param("@vendor", "nvarchar")],
  };

  test("writes lastValue for declared inputs, clears library-owned names, ignores edited-in params", () => {
    const byName = indexByName([{ name: "vendor", value: "ACME", sqlType: "nvarchar", note: "" }]);
    const out = persistInputs(stored, { "@user_id": "42", "@vendor": "typed-but-ignored", "@scratch": "9" }, byName);
    expect(out).toEqual([
      { name: "@user_id", sqlType: "int", lastValue: "42", scope: "local" },
      { name: "@vendor", sqlType: "nvarchar", lastValue: null, scope: "local" }, // library owns it → cleared
    ]);
  });

  test("clears a STALE pre-existing lastValue once the library claims the name (no resurfacing old data)", () => {
    const staleStored: SavedQuery = {
      id: "q2", name: "q2", targetDatabase: null,
      sql: "select * from t where v=@vendor",
      params: [param("@vendor", "nvarchar", "OLD_VALUE")], // had a lastValue before "vendor" was a library variable
    };
    const byName = indexByName([{ name: "vendor", value: "ACME", sqlType: "nvarchar", note: "" }]);
    const out = persistInputs(staleStored, {}, byName);
    expect(out).toEqual([{ name: "@vendor", sqlType: "nvarchar", lastValue: null, scope: "local" }]);
  });

  test("leaves a declared param's lastValue untouched when it isn't surfaced this run", () => {
    const storedWithValue: SavedQuery = {
      id: "q3", name: "q3", targetDatabase: null,
      sql: "select * from t where a=@user_id and v=@vendor",
      params: [param("@user_id", "int", "OLD"), param("@vendor", "nvarchar")],
    };
    // Only @vendor is surfaced this run — @user_id's lastValue must survive untouched.
    const out = persistInputs(storedWithValue, { "@vendor": "42" }, new Map());
    expect(out).toEqual([
      { name: "@user_id", sqlType: "int", lastValue: "OLD", scope: "local" },
      { name: "@vendor", sqlType: "nvarchar", lastValue: "42", scope: "local" },
    ]);
  });
});
