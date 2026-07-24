import { expect, test } from "bun:test";
import {
  DEFAULT_LIBRARY_WIDTH,
  MAX_LIBRARY_WIDTH,
  MIN_LIBRARY_WIDTH,
  clampWidth,
  parseLayout,
} from "./layoutLogic";

test("clampWidth: an in-range value passes through unchanged", () => {
  expect(clampWidth(320)).toBe(320);
  expect(clampWidth(MIN_LIBRARY_WIDTH)).toBe(MIN_LIBRARY_WIDTH);
  expect(clampWidth(MAX_LIBRARY_WIDTH)).toBe(MAX_LIBRARY_WIDTH);
});

test("clampWidth: below MIN floors to MIN, above MAX ceils to MAX", () => {
  expect(clampWidth(MIN_LIBRARY_WIDTH - 100)).toBe(MIN_LIBRARY_WIDTH);
  expect(clampWidth(0)).toBe(MIN_LIBRARY_WIDTH);
  expect(clampWidth(-50)).toBe(MIN_LIBRARY_WIDTH);
  expect(clampWidth(MAX_LIBRARY_WIDTH + 500)).toBe(MAX_LIBRARY_WIDTH);
});

test("clampWidth: NaN / Infinity / non-finite fall back to DEFAULT", () => {
  expect(clampWidth(NaN)).toBe(DEFAULT_LIBRARY_WIDTH);
  expect(clampWidth(Infinity)).toBe(DEFAULT_LIBRARY_WIDTH);
  expect(clampWidth(-Infinity)).toBe(DEFAULT_LIBRARY_WIDTH);
});

test("clampWidth: a non-number value falls back to DEFAULT", () => {
  // Callers include parseLayout, which hands untrusted JSON fields straight in.
  expect(clampWidth("320" as unknown as number)).toBe(DEFAULT_LIBRARY_WIDTH);
  expect(clampWidth(null as unknown as number)).toBe(DEFAULT_LIBRARY_WIDTH);
  expect(clampWidth(undefined as unknown as number)).toBe(DEFAULT_LIBRARY_WIDTH);
  expect(clampWidth({} as unknown as number)).toBe(DEFAULT_LIBRARY_WIDTH);
});

test("parseLayout: a valid blob round-trips (width clamped)", () => {
  expect(parseLayout(JSON.stringify({ libraryOpen: true, libraryWidth: 400 }))).toEqual({
    libraryOpen: true,
    libraryWidth: 400,
  });
});

test("parseLayout: null (first launch) yields the collapsed default", () => {
  expect(parseLayout(null)).toEqual({
    libraryOpen: false,
    libraryWidth: DEFAULT_LIBRARY_WIDTH,
  });
});

test("parseLayout: corrupt JSON falls back to defaults (throw caught internally)", () => {
  expect(parseLayout("{not json")).toEqual({
    libraryOpen: false,
    libraryWidth: DEFAULT_LIBRARY_WIDTH,
  });
  expect(parseLayout("")).toEqual({
    libraryOpen: false,
    libraryWidth: DEFAULT_LIBRARY_WIDTH,
  });
});

test("parseLayout: a partial blob fills the missing field with its default", () => {
  expect(parseLayout(JSON.stringify({ libraryOpen: true }))).toEqual({
    libraryOpen: true,
    libraryWidth: DEFAULT_LIBRARY_WIDTH,
  });
  expect(parseLayout(JSON.stringify({ libraryWidth: 500 }))).toEqual({
    libraryOpen: false,
    libraryWidth: 500,
  });
});

test("parseLayout: wrong-type fields coerce to defaults (open strict ===true, width via clampWidth)", () => {
  expect(parseLayout(JSON.stringify({ libraryOpen: "yes", libraryWidth: "wide" }))).toEqual({
    libraryOpen: false,
    libraryWidth: DEFAULT_LIBRARY_WIDTH,
  });
  // truthy-but-not-true must NOT open (strict === true)
  expect(parseLayout(JSON.stringify({ libraryOpen: 1, libraryWidth: 9999 }))).toEqual({
    libraryOpen: false,
    libraryWidth: MAX_LIBRARY_WIDTH,
  });
});

test("parseLayout: a non-object JSON value (array / number / string) falls back to defaults", () => {
  expect(parseLayout("[1,2,3]")).toEqual({
    libraryOpen: false,
    libraryWidth: DEFAULT_LIBRARY_WIDTH,
  });
  expect(parseLayout("42")).toEqual({
    libraryOpen: false,
    libraryWidth: DEFAULT_LIBRARY_WIDTH,
  });
  expect(parseLayout('"a string"')).toEqual({
    libraryOpen: false,
    libraryWidth: DEFAULT_LIBRARY_WIDTH,
  });
  expect(parseLayout("null")).toEqual({
    libraryOpen: false,
    libraryWidth: DEFAULT_LIBRARY_WIDTH,
  });
});
