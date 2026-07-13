import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

// Guards the token contract every component depends on: if a name here is
// renamed/removed, the whole restyle breaks silently. Cheap structural check
// (not a visual test).
const css = readFileSync(new URL("./app.css", import.meta.url), "utf8");

const REQUIRED = [
  "--canvas","--panel","--raised","--border","--border-strong",
  "--text","--muted","--faint","--brand","--accent","--accent-press","--accent-fg",
  "--syn-kw","--syn-fn","--syn-str","--syn-num","--syn-comment","--syn-var",
  "--type-tag","--tier-local","--tier-session","--tier-global","--num-cell","--null-cell",
  "--ok","--warn","--danger",
  "--font-ui","--font-mono","--dur","--dur-fast","--ease",
];

test("every required design token is defined", () => {
  for (const name of REQUIRED) {
    expect(css.includes(`${name}:`)).toBe(true);
  }
});

test("dark overrides exist for the surface tokens", () => {
  // dark block must redefine at least the canvas so theme-flip works
  const darkIdx = css.indexOf('[data-theme="dark"]');
  expect(darkIdx).toBeGreaterThan(-1);
  expect(css.slice(darkIdx).includes("--canvas:")).toBe(true);
});
