import { describe, expect, test } from "bun:test";
import type { Param } from "./api";
import { deriveParams, nextParamValues, rememberValues, toResolvedParams } from "./paramBarLogic";

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
});

describe("toResolvedParams / rememberValues", () => {
  const params: Param[] = [
    bind("@cust", "old"),
    { name: "@col", sqlType: null, lastValue: null, scope: "local" },
  ];

  test("toResolvedParams pulls current field values, '' when unset", () => {
    const got = toResolvedParams(params, { "@cust": "42" });
    expect(got).toEqual([
      { name: "@cust", sqlType: "int", value: "42" },
      { name: "@col", sqlType: null, value: "" },
    ]);
  });

  test("rememberValues updates lastValue from fields, keeps prior when absent", () => {
    const got = rememberValues(params, { "@cust": "42" });
    expect(got[0].lastValue).toBe("42");
    expect(got[1].lastValue).toBe(null);
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
    expect(nextParamValues(true, [p("@a", "av"), p("@b")], { "@a": "typed" })).toEqual({
      "@a": "av",
      "@b": "",
    });
  });

  test("same-tab preserves typed values, seeds newly-appeared params from lastValue", () => {
    expect(nextParamValues(false, [p("@a", "av"), p("@new", "nv")], { "@a": "typed" })).toEqual({
      "@a": "typed",
      "@new": "nv",
    });
  });

  test("same-tab preserves a user-cleared empty value ('' is not replaced)", () => {
    expect(nextParamValues(false, [p("@a", "av")], { "@a": "" })).toEqual({ "@a": "" });
  });
});
