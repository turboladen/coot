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
