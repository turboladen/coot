import { expect, test } from "bun:test";
import { type Choice, parseChoice, resolveAttr } from "./themeLogic";

test("resolveAttr: system removes the attribute, light/dark pin it", () => {
  expect(resolveAttr("system")).toBe(null);
  expect(resolveAttr("light")).toBe("light");
  expect(resolveAttr("dark")).toBe("dark");
});

test("parseChoice: the three valid values pass through unchanged", () => {
  for (const c of ["system", "light", "dark"] as Choice[]) {
    expect(parseChoice(c)).toBe(c);
  }
});

test("parseChoice: null / empty / garbage / wrong-case fall back to system", () => {
  expect(parseChoice(null)).toBe("system");
  expect(parseChoice("")).toBe("system");
  expect(parseChoice("garbage")).toBe("system");
  expect(parseChoice("Light")).toBe("system");
  expect(parseChoice("SYSTEM")).toBe("system");
});
