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
