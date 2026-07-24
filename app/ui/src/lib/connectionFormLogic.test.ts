// `bun test` — connectionFormLogic.ts is rune-free plain TS, so it imports
// cleanly here with no Svelte compiler. Excluded from svelte-check via tsconfig
// `exclude` (same *Logic.test.ts pattern as tabsLogic.test.ts et al).
//
// Covers the host/port <-> "host,port" wire-format round-trip for the connection
// form overhaul (billz-a5y.7). SQL Server separates host and port with a COMMA
// (not a colon), so the split keys off the LAST comma.
import { describe, expect, test } from "bun:test";
import { formatServer, parseServer } from "./connectionFormLogic";

describe("parseServer", () => {
  test("host,port splits on the comma", () => {
    expect(parseServer("myhost,1433")).toEqual({ host: "myhost", port: "1433" });
  });

  test("host only → empty port", () => {
    expect(parseServer("myhost")).toEqual({ host: "myhost", port: "" });
  });

  test("empty string → empty host and port", () => {
    expect(parseServer("")).toEqual({ host: "", port: "" });
  });

  test("surrounding whitespace is trimmed on both sides", () => {
    expect(parseServer("  myhost , 1433  ")).toEqual({ host: "myhost", port: "1433" });
  });

  test("named instance (backslash, no comma) stays entirely in host", () => {
    expect(parseServer("myhost\\SQLEXPRESS")).toEqual({
      host: "myhost\\SQLEXPRESS",
      port: "",
    });
  });

  test("IPv6 literal has colons not commas → whole thing is host", () => {
    expect(parseServer("2001:db8::1")).toEqual({ host: "2001:db8::1", port: "" });
  });

  test("IPv6 literal with a port splits on the last (only) comma", () => {
    expect(parseServer("2001:db8::1,1433")).toEqual({
      host: "2001:db8::1",
      port: "1433",
    });
  });

  test("multiple commas split on the LAST one", () => {
    expect(parseServer("a,b,c")).toEqual({ host: "a,b", port: "c" });
  });
});

describe("formatServer", () => {
  test("host and port recombine with a comma", () => {
    expect(formatServer("myhost", "1433")).toBe("myhost,1433");
  });

  test("empty port → host only, no trailing comma", () => {
    expect(formatServer("myhost", "")).toBe("myhost");
  });

  test("whitespace-only port → host only", () => {
    expect(formatServer("myhost", "   ")).toBe("myhost");
  });

  test("host and port are trimmed before combining", () => {
    expect(formatServer("  myhost  ", "  1433  ")).toBe("myhost,1433");
  });

  test("both empty → empty string", () => {
    expect(formatServer("", "")).toBe("");
  });
});

describe("round-trip", () => {
  for (const s of ["myhost,1433", "myhost", "", "myhost\\SQLEXPRESS", "2001:db8::1,1433"]) {
    test(`formatServer(parseServer(${JSON.stringify(s)})) reproduces the canonical form`, () => {
      const { host, port } = parseServer(s);
      // The canonical (trimmed) form of the same input.
      const expected = port === "" ? host : `${host},${port}`;
      expect(formatServer(host, port)).toBe(expected);
    });
  }
});
