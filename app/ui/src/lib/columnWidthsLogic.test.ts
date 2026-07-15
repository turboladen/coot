import { describe, expect, test } from "bun:test";
import { MAX_WIDTH_SIGNATURES, evictSignatures, parseWidthStore, signatureOf } from "./columnWidthsLogic";

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

describe("evictSignatures", () => {
  const keys = (n: number): string[] => Array.from({ length: n }, (_, i) => `k${i}`);

  test("under cap → no eviction", () => {
    expect(evictSignatures(keys(3), 5)).toEqual([]);
  });

  test("exactly at cap → no eviction", () => {
    expect(evictSignatures(keys(5), 5)).toEqual([]);
  });

  test("over cap → drops the oldest-first prefix, count = len - cap", () => {
    // keys are oldest-first (insertion order); the tail is most-recent and kept.
    expect(evictSignatures(["a", "b", "c", "d"], 2)).toEqual(["a", "b"]);
  });

  test("one over cap → drops the single oldest", () => {
    expect(evictSignatures(["a", "b", "c"], 2)).toEqual(["a"]);
  });

  test("empty list → no eviction", () => {
    expect(evictSignatures([], 5)).toEqual([]);
  });

  test("cap of 0 → evicts all", () => {
    expect(evictSignatures(["a", "b"], 0)).toEqual(["a", "b"]);
  });

  test("MAX_WIDTH_SIGNATURES is a positive bound", () => {
    expect(MAX_WIDTH_SIGNATURES).toBeGreaterThan(0);
    // one past the cap evicts exactly the single oldest entry
    expect(evictSignatures(keys(MAX_WIDTH_SIGNATURES + 1), MAX_WIDTH_SIGNATURES)).toEqual(["k0"]);
  });
});
