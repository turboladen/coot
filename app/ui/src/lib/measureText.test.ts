import { describe, expect, test } from "bun:test";
import { fitColumnWidth } from "./measureText";

// Chrome added by fitColumnWidth: 16px padding + 1px border = 17.
describe("fitColumnWidth", () => {
  test("clamps up to minSize when content is narrow", () => {
    expect(fitColumnWidth([10], 56, 800)).toBe(56); // ceil(27) < 56
  });

  test("adds cell chrome (padding + border) to the content width", () => {
    expect(fitColumnWidth([100], 56, 800)).toBe(117); // ceil(100 + 17)
  });

  test("uses the widest content (header vs cells)", () => {
    expect(fitColumnWidth([40, 200, 90], 56, 800)).toBe(217); // ceil(200 + 17)
  });

  test("ceils fractional canvas measurements", () => {
    expect(fitColumnWidth([100.2], 56, 800)).toBe(118); // ceil(117.2)
  });

  test("empty content falls back to minSize", () => {
    expect(fitColumnWidth([], 56, 800)).toBe(56);
  });

  test("honors a custom minSize", () => {
    expect(fitColumnWidth([10], 120, 800)).toBe(120);
  });

  test("clamps down to maxCap for very wide content", () => {
    expect(fitColumnWidth([5000], 56, 800)).toBe(800); // ceil(5017) capped
  });
});
