import { describe, expect, test } from "bun:test";
import { parseWidthStore, signatureOf } from "./columnWidthsLogic";

describe("signatureOf", () => {
  test("is deterministic — same names give the same key", () => {
    expect(signatureOf(["id", "name", "created"])).toBe(signatureOf(["id", "name", "created"]));
  });

  test("is order-sensitive", () => {
    expect(signatureOf(["a", "b"])).not.toBe(signatureOf(["b", "a"]));
  });

  test("distinguishes different names", () => {
    expect(signatureOf(["a", "b"])).not.toBe(signatureOf(["a", "c"]));
  });

  test("preserves duplicate names (differs from the deduped list)", () => {
    expect(signatureOf(["x", "x"])).not.toBe(signatureOf(["x"]));
  });

  test("empty column list", () => {
    expect(signatureOf([])).toBe("[]");
  });

  test("empty-string column name is handled", () => {
    expect(signatureOf([""])).toBe(signatureOf([""]));
    expect(signatureOf([""])).not.toBe(signatureOf([]));
  });
});

describe("parseWidthStore", () => {
  test("null → {}", () => {
    expect(parseWidthStore(null)).toEqual({});
  });

  test("invalid JSON → {}", () => {
    expect(parseWidthStore("{not json")).toEqual({});
  });

  test("non-object shapes → {}", () => {
    expect(parseWidthStore("[1,2]")).toEqual({});
    expect(parseWidthStore("42")).toEqual({});
    expect(parseWidthStore('"hi"')).toEqual({});
    expect(parseWidthStore("null")).toEqual({});
  });

  test("valid nested map round-trips", () => {
    const raw = JSON.stringify({ '["id","name"]': { id: 80, name: 220 } });
    expect(parseWidthStore(raw)).toEqual({ '["id","name"]': { id: 80, name: 220 } });
  });

  test("drops non-number widths", () => {
    const raw = JSON.stringify({ sig: { a: "120", b: null, c: true, d: 150 } });
    expect(parseWidthStore(raw)).toEqual({ sig: { d: 150 } });
  });

  test("drops NaN / Infinity / non-positive widths", () => {
    // NaN and Infinity aren't representable in JSON literals, so build the raw
    // string directly to exercise the numeric guard.
    const raw = '{"sig":{"a":NaN,"b":Infinity,"c":0,"d":-40,"e":90}}';
    expect(parseWidthStore(raw)).toEqual({}); // NaN/Infinity make the whole blob invalid JSON
    const clean = JSON.stringify({ sig: { c: 0, d: -40, e: 90 } });
    expect(parseWidthStore(clean)).toEqual({ sig: { e: 90 } });
  });

  test("drops non-object inner values, keeps the rest", () => {
    const raw = JSON.stringify({ good: { a: 100 }, bad: 5, alsoBad: [1] });
    expect(parseWidthStore(raw)).toEqual({ good: { a: 100 } });
  });

  test("round-trips with a signatureOf key", () => {
    const sig = signatureOf(["id", "name"]);
    const raw = JSON.stringify({ [sig]: { id: 100 } });
    expect(parseWidthStore(raw)).toEqual({ [sig]: { id: 100 } });
  });
});
