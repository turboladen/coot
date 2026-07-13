# billz UI Design & UX Refresh — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle the billz UI into a modern dev-tool look (tinted violet canvas, teal accent, IBM Plex fonts, Lucide icons, CVD-safe semantics) via a CSS design-token system, changing zero behavior.

**Architecture:** Introduce CSS custom properties in `app/ui/src/app.css` as the single palette source (light on `:root`, dark via `prefers-color-scheme` + a `data-theme` override hook). Every component swaps its hardcoded hex for `var(--…)`. The CodeMirror editor gets one CSS-var-driven theme so it flips with the app. Work ships as 5 disjoint-component phases (Tasks 1–5), each its own PR.

**Tech Stack:** Svelte 5 (runes) + Vite SPA, plain scoped CSS + custom properties, `@fontsource/ibm-plex-sans`/`-mono` (bundled woff2, no CDN), `lucide-svelte`, CodeMirror 6 (`@codemirror/language` `HighlightStyle`). Package manager: **bun**. Task runner: **just**.

## Global Constraints

- **No behavior change.** All existing tests + `svelte-check` must stay green: `just verify` gates every task. Verbatim.
- **No CDN.** Fonts self-hosted via `@fontsource`; icons via `lucide-svelte` (JS dep, no native toolchain). Offline is required (Tauri app).
- **Colorblind-safety is a hard requirement** (user has red/green deficiency): state never carried by hue alone (pair with icon/shape); primary action teal never green; type tags blue never red; semantic axis blue/teal ↔ amber/orange; red reserved for destructive + always icon-paired; param tiers differ in hue **and** text label.
- **Tokens only in components** — no raw hex in `.svelte` `<style>` after a component's phase; reference `var(--…)`.
- Stack is locked: Svelte SPA (not SvelteKit), plain CSS (no Tailwind), bun (never npm/pnpm/yarn), CodeMirror + TanStack. Deviate only with a stated reason.
- Commit trailer: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`. Avoid backticks in `git commit -m` (fish command-substitution eats them).
- Verification for presentational work = `just verify` green + the per-task manual visual checklist in **both** themes (toggle macOS System Settings → Appearance). There are no meaningful unit tests for CSS; do not fabricate them. The one real automated test is the Phase A token-contract guard.

---

## File Structure

**Created:**
- `app/ui/src/lib/theme/highlight.ts` — CodeMirror `HighlightStyle` mapping Lezer tags → `var(--syn-*)` (Task 3).
- `app/ui/src/lib/icons.ts` — re-exports the used Lucide icons from `lucide-svelte`, so imports are centralized and the used-set is auditable. **Created in Task 1** (shared by Tasks 2/4/5 — hoisting it into the foundation is what makes those tasks truly order-independent).
- `app/ui/src/app.css.test.ts` — token-contract test (Task 1).

**Modified (by phase):**
- Task 1: `app/ui/src/app.css`, `app/ui/package.json` (+ `bun.lock`), `app/ui/src/main.ts` (font imports). Also creates `lib/icons.ts` (see above) and adds the `lucide-svelte` dependency.
- Task 2: `App.svelte`, `lib/ConnectionList.svelte`, `lib/TabBar.svelte`, `lib/tree/*.svelte` (consumes `lib/icons.ts` from Task 1).
- Task 3: `lib/SqlEditor.svelte`, `lib/theme/highlight.ts`.
- Task 4: `lib/ResultsGrid.svelte`, `lib/ResultTabs.svelte`, `lib/ParamBar.svelte`, `lib/SavedQueryLibrary.svelte`, `lib/tree/ColumnLeaf.svelte`.
- Task 5: `lib/ConnectionForm.svelte`, `lib/PasswordPrompt.svelte`, `lib/tree/LoadingNote.svelte`, empty-state markup in `ResultsGrid`/`ObjectTree`/`SavedQueryLibrary`.

Bead map: Task 1 = `billz-xhv.1`, Task 2 = `.2`, Task 3 = `.3`, Task 4 = `.4`, Task 5 = `.5`. Each task = one branch off `main` → PR → squash-merge. Tasks 2–5 depend only on Task 1; after it merges they may run in any order (disjoint components).

---

## Task 1: Design-token foundation + fonts (`billz-xhv.1`)

**Files:**
- Modify: `app/ui/package.json` (+ `bun.lock`)
- Modify: `app/ui/src/main.ts`
- Modify: `app/ui/src/app.css`
- Create: `app/ui/src/lib/icons.ts`
- Create: `app/ui/src/app.css.test.ts`

**Interfaces:**
- Produces: the full CSS custom-property set (names below) available globally; the `--font-ui`/`--font-mono` families; a `data-theme="dark"` override hook on `:root`; and the `lib/icons.ts` re-export module (consumed by Tasks 2/4/5). Every later task consumes these token names and the icon module.

- [ ] **Step 1: Add dependencies + the shared icon module**

Run (from `app/ui`):
```bash
bun add @fontsource/ibm-plex-sans @fontsource/ibm-plex-mono lucide-svelte
```
Expected: all three added to `package.json` dependencies; `bun.lock` updated. (`lucide-svelte@1.x` peers `svelte ^5` — verified compatible.)

Then create `app/ui/src/lib/icons.ts` — the single audit point for every Lucide icon the app uses, so Tasks 2/4/5 all import from one place and the used-set stays visible. Include the full set now (extras like `Check`/`Clock`/`Globe` are used by later tasks; declaring them here keeps 2–5 order-independent):
```ts
// Central re-export of the ONLY Lucide icons the app uses. Tree-shaken by Vite
// to just these. Add here first if a component needs a new one.
export {
  Database, Table2, Columns3, Eye, Play, Save, RefreshCw,
  Plus, Lock, ChevronDown, ChevronRight, Search, X,
  Check, Clock, Globe,
} from "lucide-svelte";
```

- [ ] **Step 2: Import only the used weights in `main.ts`**

Add near the top of `app/ui/src/main.ts` (above the app mount):
```ts
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
import "@fontsource/ibm-plex-sans/600.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "./app.css";
```
(If `./app.css` is already imported elsewhere, keep a single import — do not duplicate.)

- [ ] **Step 3: Write `app.css` — tokens, reset, base**

Replace the entire contents of `app/ui/src/app.css` with:
```css
/* ============ Design tokens ============ */
:root {
  color-scheme: light;

  /* surfaces (violet-tinted neutrals) */
  --canvas:#e7e6f4; --panel:#f6f5fc; --raised:#ffffff;
  --border:#e0ddef; --border-strong:#cfcbe4;
  /* text */
  --text:#252032; --muted:#726c82; --faint:#a09aae;
  /* brand + action */
  --brand:#7c3aed; --accent:#0d9488; --accent-press:#0f766e; --accent-fg:#ffffff;
  /* syntax */
  --syn-kw:#7c3aed; --syn-fn:#2563eb; --syn-str:#c2410c; --syn-num:#0f766e;
  --syn-comment:#8b86a0; --syn-var:#7c3aed;
  /* data semantics */
  --type-tag:#1d4ed8;
  --tier-local:#64748b; --tier-session:#0d9488; --tier-global:#7c3aed;
  --num-cell:#0f766e; --null-cell:#a09aae;
  /* status + feedback (CVD-safe, icon-paired) */
  --ok:#0d9488; --warn:#c2410c; --danger:#dc2626;

  /* scale */
  --sp-1:.25rem; --sp-2:.5rem; --sp-3:.75rem; --sp-4:1rem; --sp-5:1.5rem;
  --r-sm:6px; --r-md:8px; --r-lg:10px; --r-xl:14px; --r-pill:999px;
  --fs-xs:.72rem; --fs-sm:.8rem; --fs-base:.86rem; --fs-md:.95rem; --fs-lg:1.15rem;
  --font-ui:'IBM Plex Sans', system-ui, sans-serif;
  --font-mono:'IBM Plex Mono', ui-monospace, SFMono-Regular, Menlo, monospace;
  --dur-fast:120ms; --dur:150ms; --ease:cubic-bezier(.2,.6,.2,1);
  --shadow-sm:0 1px 2px rgba(40,30,80,.08); --shadow-md:0 6px 20px rgba(40,30,80,.12);
}

/* Dark: OS-follow AND an explicit override hook (attribute wins for the
   deferred toggle, billz-xhv.6). */
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) { color-scheme: dark; }
  :root:not([data-theme="light"]) {
    --canvas:#17151f; --panel:#1e1b28; --raised:#24202f;
    --border:#302b3d; --border-strong:#3b3550;
    --text:#e9e6f2; --muted:#9d97ac; --faint:#6f6982;
    --brand:#a78bfa; --accent:#2dd4bf; --accent-press:#14b8a6; --accent-fg:#0b0f0e;
    --syn-kw:#c4b5fd; --syn-fn:#7dd3fc; --syn-str:#fdba74; --syn-num:#5eead4;
    --syn-comment:#6f6982; --syn-var:#c4b5fd;
    --type-tag:#7dd3fc;
    --tier-local:#94a3b8; --tier-session:#2dd4bf; --tier-global:#a78bfa;
    --num-cell:#5eead4; --null-cell:#6f6982;
    --ok:#2dd4bf; --warn:#fdba74; --danger:#f87171;
  }
}
:root[data-theme="dark"] {
  color-scheme: dark;
  --canvas:#17151f; --panel:#1e1b28; --raised:#24202f;
  --border:#302b3d; --border-strong:#3b3550;
  --text:#e9e6f2; --muted:#9d97ac; --faint:#6f6982;
  --brand:#a78bfa; --accent:#2dd4bf; --accent-press:#14b8a6; --accent-fg:#0b0f0e;
  --syn-kw:#c4b5fd; --syn-fn:#7dd3fc; --syn-str:#fdba74; --syn-num:#5eead4;
  --syn-comment:#6f6982; --syn-var:#c4b5fd;
  --type-tag:#7dd3fc;
  --tier-local:#94a3b8; --tier-session:#2dd4bf; --tier-global:#a78bfa;
  --num-cell:#5eead4; --null-cell:#6f6982;
  --ok:#2dd4bf; --warn:#fdba74; --danger:#f87171;
}

@media (prefers-reduced-motion: reduce) {
  :root { --dur:0ms; --dur-fast:0ms; }
}

/* ============ Base ============ */
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: var(--font-ui);
  font-size: var(--fs-base);
  color: var(--text);
  background: var(--canvas);
  -webkit-font-smoothing: antialiased;
}
:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
```

- [ ] **Step 4: Write the token-contract test**

Create `app/ui/src/app.css.test.ts`:
```ts
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
```

- [ ] **Step 5: Run the token test — verify it passes**

Run (from `app/ui`): `bun test src/app.css.test.ts`
Expected: 2 pass.

- [ ] **Step 6: Full gate**

Run (repo root): `just verify`
Expected: Rust tests + `svelte-check` green (no behavior touched). The app now renders on IBM Plex + violet canvas even before components are tokenized, because `body` sets the base.

- [ ] **Step 7: Visual check**

Run `just dev`. Confirm: font is IBM Plex (not system), background is the violet-tinted canvas, no console errors. Toggle macOS appearance → background flips to dark `#17151f`. Text stays legible in both.

- [ ] **Step 8: Commit**

```bash
git add app/ui/package.json app/ui/bun.lock app/ui/src/main.ts app/ui/src/app.css app/ui/src/app.css.test.ts app/ui/src/lib/icons.ts
git commit -m "xhv.1: design-token foundation + IBM Plex fonts + icon module

CSS custom-property palette (light + OS-follow dark + data-theme hook),
scale tokens, base reset, self-hosted IBM Plex Sans/Mono, shared lucide-svelte
icon re-export (icons.ts). Token-contract test.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Shell chrome restyle + icons (`billz-xhv.2`)

**Files:**
- Modify: `app/ui/src/App.svelte` (styles in the `<style>` block, lines ~391–479; markup for the sidebar header + segmented toggle + toolbar icons; **also** compute + pass a `lockedIds` prop to `<ConnectionList>` — see Step 5)
- Modify: `app/ui/src/lib/ConnectionList.svelte`, `app/ui/src/lib/TabBar.svelte`
- Modify: `app/ui/src/lib/tree/DatabaseNode.svelte`, `TableNode.svelte`, `ViewNode.svelte`, `ObjectTree.svelte`

**Interfaces:**
- Consumes: all Task 1 tokens + `lib/icons.ts` (both created in Task 1).

- [ ] **Step 1: Confirm the Task 1 foundation is present**

`lucide-svelte` and `lib/icons.ts` were added in Task 1. Verify: `rg "from \"lucide-svelte\"" app/ui/src/lib/icons.ts` returns the export block. If a component below needs an icon not in that list, add it to `icons.ts` first (don't import from `lucide-svelte` directly elsewhere).

- [ ] **Step 2: (folded into Task 1 — icon module already exists)**

No action; proceed to Step 3.

- [ ] **Step 3: Tokenize `App.svelte` styles**

In `App.svelte`'s `<style>` block, replace every hardcoded value with a token (there are ~10 hex literals). Concretely:
- `main { font-family: system-ui, sans-serif; }` → remove the `font-family` line (inherits `--font-ui` from body).
- `aside { border-right: 1px solid #ccc; }` → `border-right: 1px solid var(--border); background: var(--panel);`
- `.mode-toggle { border-top: 1px solid #ccc; }` → `var(--border)`; make its buttons a real segmented control:
```css
.mode-toggle button {
  flex: 1; font: inherit; font-size: var(--fs-sm);
  padding: var(--sp-1) var(--sp-2); border: 1px solid var(--border);
  border-radius: var(--r-sm); background: var(--raised); color: var(--muted);
  cursor: pointer; transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}
.mode-toggle button.active {
  background: color-mix(in srgb, var(--accent) 14%, var(--raised));
  color: var(--accent-press); border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
  font-weight: 600;
}
```
- `.lower-pane`, `.editor-pane`, `.toolbar` borders `#ccc` → `var(--border)`.
- `.locked-note { color:#92400e; background:#fef3c7; border-bottom:1px solid #fcd34d; }` → `color: var(--warn); background: color-mix(in srgb, var(--warn) 12%, var(--raised)); border-bottom: 1px solid color-mix(in srgb, var(--warn) 30%, var(--border));`
- `.db-picker:disabled { color:#ccc; }` → `var(--faint)`; style `.db-picker` with `border:1px solid var(--border-strong); border-radius:var(--r-sm); background:var(--raised); color:var(--text);`

- [ ] **Step 4: Add a branded sidebar header + toolbar/tab icons (markup)**

In `App.svelte`, at the top of `<aside>` (before `<ConnectionList>`), add:
```svelte
<div class="brand"><Database size={16} /> <span>billz</span></div>
```
and in `<style>`:
```css
.brand { display:flex; align-items:center; gap:var(--sp-2); padding:var(--sp-3) var(--sp-3);
  font-weight:700; letter-spacing:-.02em; border-bottom:1px solid var(--border); color:var(--text); }
.brand :global(svg) { color: var(--brand); }
```
In the toolbar, import and add icons: `Run` button gets `<Play size={14} />`, `Update saved query` gets `<Save size={14} />`, DB picker precede with `<Database size={14} />`. Import at top of script:
```ts
import { Database, Play, Save } from "./lib/icons";
```
(Adjust the relative path: from `App.svelte` it is `./lib/icons`.)

- [ ] **Step 5: Tokenize `ConnectionList.svelte` + CVD-safe lock status**

`ConnectionList` currently only knows `active` (`ConnectionList.svelte:26`, `class:active={conns.activeId === cfg.id}`). The real per-row status the app tracks is **session-lock**: a connection needs a password when it's session-only and not yet unlocked. That state lives in `App.svelte` (`unlocked` SvelteSet, `App.svelte:68`), NOT in `ConnectionList`. So thread it down as a prop — surfacing existing state, inventing none. The dot means **ready (filled teal) vs. locked/needs-password (hollow ring)** — an honest label, and CVD-safe (fill *and* shape differ).

First, in `App.svelte`, add a derived set and pass it in:
```ts
// ids that are session-only and not yet unlocked this session → "locked"
const lockedIds = $derived(
  new Set(conns.list.filter((c) => !c.rememberPassword && !unlocked.has(c.id)).map((c) => c.id)),
);
```
```svelte
<ConnectionList lockedIds={lockedIds} onnew={openNew} onedit={openEdit} />
```
Then in `ConnectionList.svelte`, accept the prop (default empty so it still stands alone) and render the dot per row:
```svelte
<!-- script: add to the $props() destructure -->
let { lockedIds = new Set<string>(), onnew, onedit } = $props();
```
```svelte
<!-- markup: dot before each connection name -->
{#if lockedIds.has(cfg.id)}
  <span class="dot off" title="Session password needed"></span>
{:else}
  <span class="dot on" title="Ready"><Check size={8} /></span>
{/if}
```
```css
.dot { display:inline-flex; align-items:center; justify-content:center; width:.7rem; height:.7rem; border-radius:var(--r-pill); flex:none; }
.dot.on { background: var(--ok); box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 22%, transparent); }
.dot.on :global(svg) { color: var(--accent-fg); width:7px; height:7px; }
.dot.off { background: transparent; border: 1.5px solid var(--faint); }
```
Also swap `ConnectionList`'s ~4 existing hex literals to tokens, and the active row → `background: color-mix(in srgb, var(--accent) 12%, transparent); font-weight:600;`. Import `Check` from `./icons`.

Note: `lockedIds` recomputes from existing reactive state — no behavior change, just a display of state App already owns. Verify in Step 8 that the dot flips to filled after entering a session password.

- [ ] **Step 6: Tokenize `TabBar.svelte`**

Swap its ~9 hex literals. Active tab = `background: var(--raised); box-shadow: var(--shadow-sm);`. Dirty indicator dot = `background: var(--accent);`. Replace any text `×`/`+` with `<X size={13} />` / `<Plus size={13} />` from `./icons`.

- [ ] **Step 7: Tokenize the tree nodes + add icons**

For `DatabaseNode.svelte` (7 hex), `TableNode.svelte` (9), `ViewNode.svelte` (1), `ObjectTree.svelte` (4): swap hex → tokens; row hover `background: color-mix(in srgb, var(--brand) 8%, transparent);`, selected row `background: color-mix(in srgb, var(--accent) 12%, transparent); color: var(--accent-press);`. Prefix each node with its Lucide icon (`Database`/`Table2`/`Eye`) and the disclosure `ChevronRight`/`ChevronDown` (rotate via CSS on expand). Icons use `color: var(--muted)` idle, `var(--accent-press)` when selected.

- [ ] **Step 8: Gate + visual check**

Run `just verify` (expect green). Run `just dev`: sidebar has brand header, segmented toggle looks like a control, connection status uses filled-vs-hollow dot, tree rows have icons + chevrons, Run/Update/tabs have icons. Check light **and** dark. Grep for stragglers: `rg --glob '*.svelte' '#[0-9a-fA-F]{3,6}\b' App.svelte lib/ConnectionList.svelte lib/TabBar.svelte lib/tree/` — expect no matches.

- [ ] **Step 9: Commit**

```bash
git add -A app/ui/src
git commit -m "xhv.2: shell chrome on tokens + Lucide icons

Sidebar brand header, segmented Objects/Library toggle, CVD-safe connection
status (filled dot vs hollow ring), tokenized tabs and tree with per-node
icons + chevrons, toolbar Run/Save/Database icons.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: CodeMirror editor theme (`billz-xhv.3`)

**Files:**
- Create: `app/ui/src/lib/theme/highlight.ts`
- Modify: `app/ui/src/lib/SqlEditor.svelte` (extensions array, lines ~23–45)

**Interfaces:**
- Consumes: Task 1 `--syn-*` tokens, `--font-mono`.
- Produces: `billzHighlight` extension + `editorTheme` extension.

- [ ] **Step 1: Declare the CodeMirror deps, then write the highlight + editor theme module**

`highlight.ts` imports directly from `@codemirror/language` and `@lezer/highlight`. Both resolve today only as *hoisted transitives* of `@codemirror/lang-sql`/`codemirror` — fragile for a direct import. Declare them:
```bash
bun add @codemirror/language @lezer/highlight
```
Expected: both added to `package.json`; `bun.lock` updated. Commit the lockfile with this task.

Create `app/ui/src/lib/theme/highlight.ts`:
```ts
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

// One theme, driven by the app's CSS variables — so the editor flips with
// light/dark automatically (EditorView.theme values are plain CSS strings, so
// `var(--x)` resolves against the document like any other CSS).
const style = HighlightStyle.define([
  { tag: [t.keyword, t.modifier, t.operatorKeyword], color: "var(--syn-kw)", fontWeight: "500" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "var(--syn-fn)" },
  { tag: [t.string, t.special(t.string)], color: "var(--syn-str)" },
  { tag: [t.number, t.bool, t.null], color: "var(--syn-num)" },
  { tag: [t.lineComment, t.blockComment], color: "var(--syn-comment)", fontStyle: "italic" },
  { tag: [t.variableName, t.propertyName], color: "var(--syn-var)" },
]);

export const billzHighlight = syntaxHighlighting(style);

export const editorTheme = EditorView.theme({
  "&": { height: "100%", fontSize: "13px", backgroundColor: "var(--raised)", color: "var(--text)" },
  ".cm-scroller": { overflow: "auto", fontFamily: "var(--font-mono)", lineHeight: "1.6" },
  ".cm-gutters": { backgroundColor: "var(--panel)", color: "var(--faint)", border: "none", borderRight: "1px solid var(--border)" },
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--brand) 6%, transparent)" },
  ".cm-activeLineGutter": { backgroundColor: "color-mix(in srgb, var(--brand) 8%, transparent)", color: "var(--muted)" },
  "&.cm-focused .cm-cursor": { borderLeftColor: "var(--accent)" },
  "&.cm-focused .cm-selectionBackground, ::selection": { backgroundColor: "color-mix(in srgb, var(--accent) 22%, transparent)" },
  ".cm-selectionBackground": { backgroundColor: "color-mix(in srgb, var(--accent) 14%, transparent)" },
});
```

- [ ] **Step 2: Wire it into `SqlEditor.svelte`, replacing the inline theme**

In `SqlEditor.svelte`, add the import:
```ts
import { billzHighlight, editorTheme } from "./theme/highlight";
```
In the `extensions` array, **remove** the existing inline `EditorView.theme({...})` object (the `"&": { height... }` one, ~lines 36–39) and add `editorTheme` and `billzHighlight` in its place:
```ts
    basicSetup,
    sql({ dialect: MSSQL }),
    editorTheme,
    billzHighlight,
```
Leave `basicSetup` in place. Mechanism (verified against `@codemirror/language`'s `getHighlighters` = `main.length ? main : fallback`): `basicSetup`'s default highlighter is registered as a *fallback* (`{fallback: true}`), while `billzHighlight` is a *main* highlighter — so once billz is present it wins cleanly and basicSetup's default is bypassed entirely. Unmapped Lezer tags therefore render as plain `var(--text)` (NOT basicSetup colors). This is why there's no double-highlighter conflict; it's the intended behavior.

- [ ] **Step 3: Gate**

Run `just verify`. Expected: `svelte-check` + Rust green. (`@codemirror/language` + `@lezer/highlight` are now declared deps from Step 1, so the direct imports resolve robustly.)

- [ ] **Step 4: Visual check**

`just dev`: type a query with keywords, a string, a number, a comment, an `@param`. Confirm violet keywords / blue functions / amber strings / teal numbers / muted-italic comments; gutter matches the panel tint; active line has a faint violet wash; cursor + selection are teal. Toggle macOS dark → the editor recolors to the dark syntax palette with NO code change (proves the CSS-var approach).

- [ ] **Step 5: Commit**

```bash
git add app/ui/src/lib/theme/highlight.ts app/ui/src/lib/SqlEditor.svelte app/ui/package.json app/ui/bun.lock
git commit -m "xhv.3: CSS-var-driven CodeMirror theme

HighlightStyle mapping Lezer tags to --syn-* tokens + gutter/active-line/
cursor/selection theme. One theme, flips with the app light/dark.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Data surfaces — grid, params, badges (`billz-xhv.4`)

**Files:**
- Modify: `app/ui/src/lib/ResultsGrid.svelte` (header markup ~112–118, row markup ~129–147, styles ~155–218)
- Modify: `app/ui/src/lib/ResultTabs.svelte`, `app/ui/src/lib/ParamBar.svelte`, `app/ui/src/lib/SavedQueryLibrary.svelte`, `app/ui/src/lib/tree/ColumnLeaf.svelte`

**Interfaces:**
- Consumes: Task 1 tokens, `./icons` (Task 2).

- [ ] **Step 1: Tokenize `ResultsGrid` surfaces**

Swap its 7 hex literals: `.header-row { border-bottom:2px solid var(--border-strong); background:var(--panel); }`, `.th { border-right:1px solid var(--border); color:var(--muted); font-family:var(--font-ui); }`, `.tr { border-bottom:1px solid var(--border); }`, `.td { border-right:1px solid var(--border); }`, `.td.nullish { color:var(--null-cell); font-style:italic; }`, `.empty,.no-rows { color:var(--muted); }`. Keep `.td.mono` but set `font-family:var(--font-mono)`.

- [ ] **Step 2: Split the header into name + typed tag**

The header currently prints `columnDef.header` = `"name : sqlType"` as one string. Change the column def (line ~23) to keep name/type separate:
```ts
// header carries the display name; sqlType is read from result.columns for the tag
header: c.name,
```
Then in the header markup (~113–117), render both:
```svelte
{#each headerGroup?.headers ?? [] as header, i (header.id)}
  <div class="th" style:width="{header.getSize()}px">
    {header.column.columnDef.header}<span class="htype">{result.columns[i].sqlType}</span>
  </div>
{/each}
```
```css
.htype { margin-left: var(--sp-1); font-size: var(--fs-xs); color: var(--type-tag); font-weight: 400; }
```

- [ ] **Step 3: Add virtualization-safe zebra striping + numeric color**

Rows are absolutely-positioned/virtualized, so `:nth-child` does NOT match data parity. Key the stripe off the virtual index. In the row `{#each ... as vi}` block (~127), add the class:
```svelte
<div
  class="tr"
  class:stripe={vi.index % 2 === 1}
  style:grid-template-columns={gridTemplate}
  ...
>
```
```css
.tr.stripe { background: color-mix(in srgb, var(--brand) 3%, var(--raised)); }
```
For numeric cells, color right-aligned values: the cell already has `style:text-align={r.align}`; add `class:num={r.align === 'right'}` and:
```css
.td.num { color: var(--num-cell); }
```
(`renderCell`'s `align` is already `'right'` for numeric types — confirm in `renderCell.ts`; if it exposes a `numeric` flag prefer that.)

- [ ] **Step 4: Tokenize `ResultTabs` + count pill**

Swap its 6 hex to tokens; result-set + Messages tabs as pills (active = `var(--raised)` + `--shadow-sm`, idle = `var(--muted)`). Add a row-count pill on a results tab:
```css
.count { font-size: var(--fs-xs); background: color-mix(in srgb, var(--accent) 16%, var(--raised));
  color: var(--accent-press); padding: 0 var(--sp-1); border-radius: var(--r-pill); }
```
Messages error lines: `color: var(--danger)` **with** a leading warning icon (import from `./icons` if adding one; otherwise a `⚠` glyph is acceptable) — never color-only.

- [ ] **Step 5: Tokenize `ParamBar` + tier badges**

Swap its 23 hex literals (largest). Fields: `border:1.5px solid var(--accent); box-shadow:0 0 0 3px color-mix(in srgb,var(--accent) 15%,transparent);` on focus; `font-family:var(--font-mono)` for values. Tier badges by source, always with the text label already present:
```css
.badge.local   { background: color-mix(in srgb, var(--tier-local) 15%, var(--raised));   color: var(--tier-local); }
.badge.session { background: color-mix(in srgb, var(--tier-session) 16%, var(--raised)); color: var(--tier-session); }
.badge.global  { background: color-mix(in srgb, var(--tier-global) 16%, var(--raised));  color: var(--tier-global); }
```
Session/Global badges get a small icon (`Clock`/`Globe` — add to `icons.ts` re-export list if used; otherwise reuse existing). The tier-clear `×` → `<X size={12} />`. Type/scope `<select>`s: `border:1px solid var(--border-strong); border-radius:var(--r-sm); background:var(--raised); color:var(--text);`.

- [ ] **Step 6: Tokenize `SavedQueryLibrary` + `ColumnLeaf`**

`SavedQueryLibrary` (3 hex) → tokens; list rows hover/active like the tree. `ColumnLeaf.svelte` (~7 hex) → tokens. It **already renders** a `.type` span (`: {label.dataType}`, `ColumnLeaf.svelte:13,29`), so this is a **retint**, not an add: point that span's `color` at `var(--type-tag)` (blue, never red) and swap the remaining hex (`#333/#888/#aaa/#b8860b/#3b82f6`) to tokens.

- [ ] **Step 7: Gate + visual check**

`just verify` green. `just dev`: run a query with a numeric column, a NULL, and mixed types. Confirm zebra stripes are a faint violet (stable while scrolling — scroll a 100+ row result and verify stripes don't flicker/shift), numbers teal + right-aligned, NULL faint italic, header type tags blue, count pill present, param tier badges are hue+label. Both themes. Grep the touched files for `#` — expect none.

- [ ] **Step 8: Commit**

```bash
git add -A app/ui/src/lib
git commit -m "xhv.4: data surfaces on tokens

Grid: virtualization-safe violet zebra striping, blue header type tags, teal
right-aligned numbers, faint-italic NULL. Result tabs with count pill,
icon-paired error lines. Param tier badges (hue + label) + tokenized fields.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Forms, modals & states (`billz-xhv.5`)

**Files:**
- Modify: `app/ui/src/lib/ConnectionForm.svelte`, `app/ui/src/lib/PasswordPrompt.svelte`, `app/ui/src/lib/tree/LoadingNote.svelte`
- Modify: empty-state markup in `app/ui/src/lib/ResultsGrid.svelte` (`.empty`/`.no-rows`), `app/ui/src/lib/tree/ObjectTree.svelte`, `app/ui/src/lib/SavedQueryLibrary.svelte`

**Interfaces:**
- Consumes: Task 1 tokens, `./icons`.

- [ ] **Step 1: Tokenize `ConnectionForm`**

Swap its 2 hex literals; inputs `border:1px solid var(--border-strong); border-radius:var(--r-sm); background:var(--raised); color:var(--text); padding:var(--sp-1) var(--sp-2);`; labels `color:var(--muted); font-size:var(--fs-sm);`; primary button teal (`background:var(--accent); color:var(--accent-fg);`), secondary outline. Focus rings inherit the global `:focus-visible`.

- [ ] **Step 2: Tokenize `PasswordPrompt` — preserve a11y**

Swap its 3 hex; style the modal card: `background:var(--raised); border:1px solid var(--border-strong); border-radius:var(--r-xl); box-shadow:var(--shadow-md);`; add a `<Lock size={16} />` in the header. **Do not change** the existing focus-trap / ESC / autofocus behavior — only styling. Verify the keyboard flow still works in Step 5.

- [ ] **Step 3: Tokenize `LoadingNote` + empty states**

`LoadingNote` (3 hex) → tokens; spinner uses `border-top-color: var(--accent)`. Empty states (`ResultsGrid` "No rows."/"No result set.", empty tree, empty library) → centered, `color:var(--muted)`, slightly larger padding; a muted icon (`Search`/`Database`) above the text is welcome but keep copy terse.

- [ ] **Step 4: Gate**

Run `just verify`. Expected green.

- [ ] **Step 5: Visual + a11y check**

`just dev`: open the connection form (inputs/buttons tokenized, teal focus rings). Trigger the password prompt for a session-only connection: card is styled with Lock icon; **Tab cycles within the modal, ESC dismisses, first field autofocuses** (unchanged). Empty grid/tree/library look intentional, not broken. Both themes. Grep touched files for `#` — expect none. Final sweep: `rg --glob '*.svelte' '#[0-9a-fA-F]{3,6}\b' app/ui/src` should return only intentional exceptions (ideally nothing).

- [ ] **Step 6: Commit**

```bash
git add -A app/ui/src/lib
git commit -m "xhv.5: forms, modal & empty/loading states on tokens

ConnectionForm inputs/buttons, PasswordPrompt modal card + Lock icon
(a11y behavior unchanged), LoadingNote spinner, friendlier empty states.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- §4 tokens → Task 1 (verbatim). §5 fonts → Task 1 Steps 1–2. §6 icons → Task 2 Steps 1–2 + placements across 2/4/5. §7 component list → Tasks 2–5 (every named component mapped). §8 motion → tokens in Task 1 + transitions applied per component. §3.1 CVD rules → Task 2 Step 5 (status), Task 4 Step 4/5 (type tags, error icons, tier badges). §10 testing → per-task `just verify` + visual checklists + Task 1 contract test. Deferred toggle (§2/`xhv.6`) → `data-theme` hook shipped in Task 1 Step 3; toggle UI not built. **No gaps.**

**Placeholder scan:** No TBD/TODO; every code step shows real code; mechanical swaps enumerate the exact selectors/values. The one soft spot — "confirm in `renderCell.ts`" (Task 4 Step 3) — is a verify-then-use instruction, not a placeholder; the fallback (`r.align === 'right'`) is concrete.

**Type consistency:** Token names identical across Task 1 definitions and Tasks 2–5 usages (`--syn-*`, `--tier-*`, `--type-tag`, `--num-cell`, `--null-cell`, `--accent-*`). `icons.ts` is created in Task 1 with the full 16-icon set (including `Check`/`Clock`/`Globe`), so every later task's `./icons` import resolves regardless of merge order. Module paths (`./theme/highlight`, `./icons`, `./lib/icons`) are relative-correct per file location.

**Plan-review revisions (2026-07-13, adversarial `general-purpose` agent — verdict NEEDS-CHANGES, all addressed):**
1. *BLOCKER* — `icons.ts` + `lucide-svelte` hoisted from Task 2 into **Task 1**, so Tasks 2–5 are genuinely order-independent; full icon set (incl. `Check`/`Clock`/`Globe`) declared at creation.
2. *SHOULD-FIX* — `ConnectionList` has no `connected` signal (only `active`); rewired Task 2 Step 5 to thread a `lockedIds` prop (derived from App's existing `unlocked` state) and reframed the dot as **ready vs. locked**, honest + CVD-safe.
3. *SHOULD-FIX* — `@codemirror/language` + `@lezer/highlight` now explicitly declared in Task 3 Step 1 (were fragile hoisted transitives).
4. *NIT* — corrected the Task 3 fallback comment (billz highlighter wins entirely; unmapped tags → plain `--text`, not basicSetup colors).
5. *NIT* — `ColumnLeaf` corrected to ~7 hex and a **retint** of the existing `.type` span (`label.dataType`), not a new tag.
Reviewer independently *confirmed correct* (read installed source): CM `var()` theming + billz-beats-basicSetup, `lucide-svelte` Svelte-5 support, the three-layer dark cascade specificity, `vi.index % 2` striping, header/`result.columns[i]` index alignment, and `renderCell.align === 'right'` for numerics.
