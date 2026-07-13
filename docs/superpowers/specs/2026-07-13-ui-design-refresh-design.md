# billz UI Design & UX Refresh — Design Spec

**Date:** 2026-07-13
**Epic:** `billz-xhv` · Phases `billz-xhv.1`…`billz-xhv.5` (+ deferred `billz-xhv.6`)
**Status:** Approved design direction (brainstormed with visual companion); ready for planning.

## 1. Goal

The UI works but looks vanilla — bare `system-ui`, hardcoded `#ccc` borders, no design
system. Make it a **modern dev-tool**: crisp, tinted, iconographed, and quietly polished —
"business-y but fun" — without changing a single behavior. Purely a presentation-layer pass.

North star: TablePlus / Linear-class polish, applied to billz's existing layout.

## 2. Non-goals (scope discipline)

- **No new features.** No sorting/filtering/resizing, no new tree nodes, no new panes.
- **No component restructuring** beyond what styling strictly needs (e.g. splitting a grid
  header string into name + type spans is in; refactoring data flow is out).
- **No manual light/dark toggle in v1** — OS-follow only. The toggle UI is deferred
  (`billz-xhv.6`); Phase A still lands the `data-theme` attribute hook it will use.
- No SvelteKit, no CSS framework (Tailwind etc.), no runtime theming lib. Plain CSS custom
  properties + Svelte scoped styles, per the locked stack.

## 3. Locked decisions (from brainstorming)

| Dimension | Decision | Rationale |
|---|---|---|
| Vibe | Modern dev-tool | User pick; TablePlus cousin |
| Theme | Light-primary, dark secondary (OS-follow) | User works mostly in light |
| Fonts | IBM Plex Sans (UI) + IBM Plex Mono (editor/grid) | Distinctive, business-y, dodges JetBrains-Mono fatigue |
| Canvas | Violet-tinted neutrals, layered (canvas→panel→raised) | "More color in the whites/greys," echoes user's preferred Slack CVD theme |
| Accent | Teal (complementary pop against violet canvas) | Primary action must not blend into chrome |
| Icons | Lucide via `lucide-svelte`, ~12 icons | Single consistent stroke family |
| Motion | Subtle (120–150ms), `prefers-reduced-motion`-gated | Fun through polish, not animation |

### 3.1 Colorblind-safety (hard requirement — user has red/green deficiency)

Non-negotiable rules, enforced in every phase:

- **State is never carried by hue alone.** Pair every status with an icon or shape.
  - Connected = **filled teal dot + check**; locked/offline = **hollow ring** (shape change,
    not a red/green swap).
- **Primary action = teal, never green.**
- **Type tags = blue, never red** (red on a type tag also falsely reads as "error").
- **Semantic axis is blue/teal ↔ amber/orange** (separates cleanly for all vision types).
  Red is reserved *only* for genuinely destructive actions, and *always* paired with an icon.
- Param tiers (Local/Session/Global) differ in **hue + text label** — hue is never the only cue.

## 4. Design tokens

Single source of truth in `app/ui/src/app.css`. Light on `:root`; dark via
`@media (prefers-color-scheme: dark)` **and** `:root[data-theme="dark"]` (attribute wins, so the
deferred toggle can force a mode). Components reference `var(--…)` only — no raw hex anywhere.

### 4.1 Color — Light (`:root`)

```
/* surfaces (violet-tinted neutrals) */
--canvas:#e7e6f4;  --panel:#f6f5fc;  --raised:#ffffff;
--border:#e0ddef;  --border-strong:#cfcbe4;
/* text */
--text:#252032;  --muted:#726c82;  --faint:#a09aae;
/* brand + action */
--brand:#7c3aed;        /* violet — chrome/identity */
--accent:#0d9488;       /* teal — primary action, active, focus */
--accent-press:#0f766e; --accent-fg:#ffffff;
/* syntax */
--syn-kw:#7c3aed;  --syn-fn:#2563eb;  --syn-str:#c2410c;  --syn-num:#0f766e;  --syn-comment:#8b86a0;  --syn-var:#7c3aed;
/* data semantics */
--type-tag:#1d4ed8;                  /* blue, never red */
--tier-local:#64748b;  --tier-session:#0d9488;  --tier-global:#7c3aed;
--num-cell:#0f766e;    --null-cell:#a09aae;
/* status + feedback (CVD-safe, icon-paired) */
--ok:#0d9488;  --warn:#c2410c;  --danger:#dc2626;   /* danger = destructive only */
```

### 4.2 Color — Dark (`@media prefers-color-scheme: dark`, `:root[data-theme="dark"]`)

```
--canvas:#17151f;  --panel:#1e1b28;  --raised:#24202f;
--border:#302b3d;  --border-strong:#3b3550;
--text:#e9e6f2;  --muted:#9d97ac;  --faint:#6f6982;
--brand:#a78bfa;  --accent:#2dd4bf;  --accent-press:#14b8a6;  --accent-fg:#0b0f0e;
--syn-kw:#c4b5fd;  --syn-fn:#7dd3fc;  --syn-str:#fdba74;  --syn-num:#5eead4;  --syn-comment:#6f6982;  --syn-var:#c4b5fd;
--type-tag:#7dd3fc;
--tier-local:#94a3b8;  --tier-session:#2dd4bf;  --tier-global:#a78bfa;
--num-cell:#5eead4;  --null-cell:#6f6982;
--ok:#2dd4bf;  --warn:#fdba74;  --danger:#f87171;
```

### 4.3 Scale tokens

```
/* spacing */   --sp-1:.25rem; --sp-2:.5rem; --sp-3:.75rem; --sp-4:1rem; --sp-5:1.5rem;
/* radius */    --r-sm:6px; --r-md:8px; --r-lg:10px; --r-xl:14px; --r-pill:999px;
/* type scale */--fs-xs:.72rem; --fs-sm:.8rem; --fs-base:.86rem; --fs-md:.95rem; --fs-lg:1.15rem;
/* fonts */     --font-ui:'IBM Plex Sans',system-ui,sans-serif; --font-mono:'IBM Plex Mono',ui-monospace,SFMono-Regular,Menlo,monospace;
/* motion */    --dur-fast:120ms; --dur:150ms; --ease:cubic-bezier(.2,.6,.2,1);
/* elevation */ --shadow-sm:0 1px 2px rgba(40,30,80,.08); --shadow-md:0 6px 20px rgba(40,30,80,.12);
```

`@media (prefers-reduced-motion: reduce)` zeroes `--dur`/`--dur-fast`.

## 5. Typography

- Self-host **IBM Plex Sans** (400/500/600) and **IBM Plex Mono** (400/500) via
  `@fontsource/ibm-plex-sans` + `@fontsource/ibm-plex-mono` (bundled `.woff2`, **no CDN** —
  offline requirement for the Tauri app). Import only the used weights.
- UI text → `--font-ui`. Editor, data grid cells, param fields, type tags' numeric context → `--font-mono`.
- Grid/editor use `font-variant-numeric: tabular-nums` so numbers align.

## 6. Iconography

`lucide-svelte`, importing only: `Database`, `Table2`, `Columns3`, `Eye` (views), `Play` (run),
`Save`, `RefreshCw`, `Plus`, `Lock`, `ChevronDown`/`ChevronRight`, `Search`, `X`. One stroke
weight (2px) at 14–16px. Icons inherit `currentColor` so they theme for free.

## 7. Component-by-component changes

Grounded in the current components. Behavior unchanged throughout.

- **`app.css`** — tokens, base reset, font imports, body/`color-scheme`. (Phase A)
- **`App.svelte`** — swap the `main`/`aside`/`.workspace`/`.toolbar`/`.locked-note` styles onto
  tokens; add branded sidebar header; `.mode-toggle` → real segmented control. (Phase B)
- **`ConnectionList.svelte`** — token styling; CVD-safe status dot (filled teal + check vs hollow
  ring); active row = tinted accent background. (Phase B)
- **`TabBar.svelte`** — active tab = raised surface + shadow; dirty indicator = accent dot;
  `+`/`×` as Lucide icons. (Phase B)
- **Toolbar (in `App.svelte`)** — DB picker as a styled control with a Database icon; `Run` =
  teal filled with Play icon; `Update saved query` = Save icon, outline. (Phase B/C)
- **`SqlEditor.svelte`** — extend the existing `EditorView.theme` and add a `HighlightStyle`
  (`@codemirror/language` `syntaxHighlighting` + `HighlightStyle.define`) mapping Lezer `tags.*`
  to `var(--syn-*)`; theme gutters/active-line/selection/cursor. **One theme, CSS-var-driven,
  flips with the app.** Font → `--font-mono`. (Phase C)
- **`ResultsGrid.svelte`** — token surfaces; header split into `name` + styled blue `type` span
  (currently one `"name : type"` string); zebra striping via `vi.index % 2` class (NOT
  `:nth-child` — rows are absolutely-positioned/virtualized); numeric cells → `--num-cell`;
  `.nullish` → `--null-cell` italic; header 2px accent-tinted border. (Phase D)
- **`ResultTabs.svelte`** — result-set tabs + Messages as pill tabs; row-count pill on results;
  Messages error lines use `--danger` **with an icon**. (Phase D)
- **`ParamBar.svelte`** — tier badges (slate/teal/violet + label), Session/Global carry a small
  clock/globe icon; the tier-clear `×` is a Lucide icon; type/scope selects styled. (Phase D)
- **`SavedQueryLibrary.svelte`** — list rows on tokens, hover/active states, icons. (Phase D)
- **`ConnectionForm.svelte`** — inputs/labels/buttons on tokens; focus rings = accent. (Phase E)
- **`PasswordPrompt.svelte`** — modal card styling, Lock icon, keep existing focus-trap/ESC a11y
  behavior intact. (Phase E)
- **`tree/*`** — chevrons + per-node Lucide icons (Database/Table2/Columns3/Eye); inline blue
  type tag on `ColumnLeaf`; `LoadingNote` spinner on tokens; hover/selected states. (Phase B/D)
- **Empty/loading states** — grid "No rows."/"No result set.", empty tree/library get muted,
  centered, slightly friendlier treatment (still terse). (Phase E)

## 8. Motion

Hover/active/focus color + background transitions at `--dur-fast`; tree-expand and tab-switch at
`--dur`; focus rings appear instantly (a11y). All via `transition` on tokened properties; all
disabled under `prefers-reduced-motion`. No entrance animations, no spinners beyond the existing
tree-load note.

## 9. Phasing → beads

| Bead | Phase | Deliverable | Depends on |
|---|---|---|---|
| `billz-xhv.1` | A | Tokens + fonts + reset in `app.css`; light/dark; `data-theme` hook | — |
| `billz-xhv.2` | B | Shell: App layout, sidebar, ConnectionList, TabBar, toolbar, tree icons; wire Lucide | A |
| `billz-xhv.3` | C | CodeMirror theme + HighlightStyle (CSS-var-driven) | A |
| `billz-xhv.4` | D | ResultsGrid, ResultTabs, ParamBar/badges, Library, column type tags | A |
| `billz-xhv.5` | E | ConnectionForm, PasswordPrompt, locked banner, empty/loading states | A |
| `billz-xhv.6` | — | *(Deferred)* manual light/dark toggle UI | A |

Each phase is its own branch → PR → squash-merge, per the pipeline. B–E may proceed in any order
once A lands (they touch disjoint components).

## 10. Testing & verification

- **Behavior is unchanged**, so all existing tests must stay green: `just verify` (Rust + `svelte-check`)
  gates every phase.
- No new behavior tests (this is presentational). Optional: a trivial assertion that the core
  token names exist in `app.css` (contract guard), only if cheap.
- **Manual visual verification** via `just dev` after each phase, in both light and dark (toggle
  macOS appearance), and a quick keyboard-only pass on the modal (focus trap unbroken).
- `cargo fmt` + `cargo clippy` clean (unchanged Rust, but the gate runs).

## 11. Risks & mitigations

- **CM theme + CSS vars** — if a Lezer tag isn't mapped, that token falls back to default text
  color (safe, just unstyled). Verify against the MSSQL dialect's actual tags in Phase C.
- **Font bundle size** — limit to the 5 weights listed; `@fontsource` ships per-weight files.
- **Dark mode drift** — every token has a dark value; the risk is components that hardcoded hex.
  Phase A + a grep for `#` in `.svelte` `<style>` blocks catches stragglers each phase.
- **Grid striping/virtualization** — must key off virtual index; called out in Phase D.
- **New dependency (`lucide-svelte`)** — light JS-only dep, no native toolchain; flagged and
  approved. Tree-shaken to the used icons.

## 12. Definition of done

All five phases merged; app looks like the approved capstone mockup in both themes; `just verify`
green; no behavior regressions; deferred toggle filed. Non-goals untouched.
