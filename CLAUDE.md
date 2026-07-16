# CLAUDE.md

Standing orders for this repo. Read every turn. Terse on purpose. `PLAN.md` is the full spec and the
source of truth for _what_ and _why_; this file is the _rules that must not drift_ across a long
build.

A personal macOS SQL Server client (Tauri + Svelte, Rust core). Single user, me, on DEV boxes. "Good
enough for me" beats "general-purpose." Do not build for scale, multi-user, or distribution.

## Read before doing anything

- `bd ready` — what's actionable now. The build order (`PLAN.md` §9) lives as beads epics
  (`Phase 0…3`) with dependencies, so `bd ready` is the source of truth for _where to start_.
- `PLAN.md` — architecture, data models, phases, non-goals.
- The two spike probes (`core/examples/typed_probe.rs` typed, `core/examples/dynamic_dump.rs`
  untyped dump) — **working proof** of the exact `mssql-client` calls the plan relies on. When in
  doubt about the driver, read/run them (`cargo run -p coot-core --example <name>`), don't guess.
  (`billz-ce1.7` ports them into `core`'s env-gated integration tests.)

## Non-negotiable invariants

- **The driver stays behind `core`.** `mssql-client` is a _private_ dependency of the `core` crate.
  No `mssql_client::` type ever appears in the `app` (Tauri) crate or crosses to Svelte. UI sees
  only `core`'s own `QueryResult` / `ColumnMeta` / `CellValue`. This is the one rule that makes a
  bad-driver-day a `core`-only change. Never break it for convenience.
- **`core` is pure Rust, no Tauri, headless-testable.** If a thing needs Tauri to test, it's in the
  wrong crate.
- **Secrets never touch disk in plaintext.** Passwords go to the macOS Keychain via `keyring`.
  Connection metadata may be config/SQLite; the password may not.
- **Database is execution context, not an in-SQL parameter** (`PLAN.md` §4). Keep it a first-class
  executor input so future cross-tenant fan-out is a loop, not a rewrite.

## Scope discipline

- Build only the current phase (`PLAN.md` §9). Do not gold-plate toward later phases.
- SQL-auth only. No Entra/AAD/Windows auth.
- Do **not** build: cross-DB fan-out, ER diagrams, MCP server, tree nodes beyond
  Databases/Tables/Columns/Views. **Omit** unbuilt tree nodes — don't render disabled stubs.
- If a "nice to have" tempts you and it isn't in the current phase, **file a deferred bead** (or
  leave a `// TODO(phaseN):` pointing at its id) and move on — don't build it.

## Tech stack (locked)

**Rust:** edition 2024, current stable toolchain (mssql-client needs ≥1.88). Cargo workspace, crates
`core` + `app`. TLS = rustls (driver default — do not enable native-tls).

**JS/TS:** **bun** for everything (install, scripts, running) — never npm/pnpm/yarn/node. **Vite**
as the bundler. **Svelte 5** (runes). Plain Svelte + Vite **SPA** — _not_ SvelteKit (no SSR/routing
needed; SvelteKit's static-adapter dance is pure overhead here).

- Editor: **CodeMirror 6** (lighter than Monaco, clean Svelte integration) with the SQL language +
  comment-toggle. Grid: **TanStack Table** (headless) + **TanStack Virtual** for row virtualization.
  Deviate only with a stated reason.

**Always use latest versions.** Add deps with `cargo add` / `bun add` (they fetch latest) — don't
hand-write stale version pins. **But** commit lockfiles (`Cargo.lock`, `bun.lock`), and after any
`mssql-client` bump, re-run the spike integration tests before trusting it (the driver is
fast-moving and single-maintainer).

## Conventions

- `cargo fmt` + `cargo clippy` clean before considering anything done. Warnings are errors.
- `SqlValue` is `#[non_exhaustive]` → every `match` needs a wildcard arm.
- Decimals cross the JSON boundary as **strings** (no f64 precision loss). Same for money.
- Column headers/types come from result-set/`sys.*` metadata; cell values from `SqlValue`. These are
  two different type sources — see `PLAN.md` §7 (wire tokens vs canonical types).
- **Don't invent driver API.** `mssql-client` moves fast; training data is stale. Verify against
  `docs.rs/mssql-client` (or the installed source) before using a method.
- Compiling the `app` crate (`cargo build`/`test`/`clippy`) does **not** need `app/ui/dist` —
  `tauri::generate_context!` only requires `frontendDist` for `tauri build` bundling.

## Shell & commands

- **Prefer `just` for all tasks** — the `justfile` at the repo root is the task interface. `just`
  lists recipes; the common ones: `just dev` (run the app), `just verify` (the full Rust+frontend
  gate), `just test` / `just lint` / `just fmt`, `just ui-check` / `just ui-test` / `just ui-build`,
  `just app-build` (signed release), `just setup-signing`, `just probe-typed` / `probe-dynamic`. Add
  a recipe there rather than reaching for a raw `cargo`/`bun` invocation.
- I use **fish**. Emit fish-compatible commands (`set -x FOO bar`, not `export FOO=bar`).
- Under the hood: frontend tooling is **bun** (`app/ui`); Rust is **cargo** (workspace root); the
  Tauri CLI runs via bun (`bun run tauri …`, wired with `TAURI_APP_PATH=..`).
- **CI** (`.github/workflows/`): `ci.yml` runs `just verify` on `ubuntu-latest` (Tauri needs apt
  `libwebkit2gtk-4.1-dev`/`libgtk-3-dev`/`librsvg2-dev`/`libayatana-appindicator3-dev`/`libxdo-dev`);
  `audit.yml` runs `cargo deny` weekly + on dep changes. Linux is 10× cheaper than macOS on private repos.

## Secrets & git hygiene

- Integration tests hit the real DEV box, gated behind env vars (`MSSQL_SERVER`, `MSSQL_USER`,
  `MSSQL_PASSWORD`, `MSSQL_DATABASE`). Skip cleanly when unset. Never hardcode a server/cred.
- Password comes from 1Password at runtime: `set -x MSSQL_PASSWORD (op read "op://...")`.
- `.gitignore` secrets, `.env`, local config with connection details, `target/`, `node_modules/`.
  Never commit a credential.

## When to stop and ask me

- Any deviation from a locked decision or an architectural boundary above.
- Any new dependency that pulls a C toolchain / heavy native build, or any auth/network behavior
  change.
- If the spike smoke tests fail after a driver bump — stop, don't paper over it.


<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:6cd5cc61 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.

## Agent Context Profiles

The managed Beads block is task-tracking guidance, not permission to override repository, user, or orchestrator instructions.

- **Conservative (default)**: Use `bd` for task tracking. Do not run git commits, git pushes, or Dolt remote sync unless explicitly asked. At handoff, report changed files, validation, and suggested next commands.
- **Minimal**: Keep tool instruction files as pointers to `bd prime`; use the same conservative git policy unless active instructions say otherwise.
- **Team-maintainer**: Only when the repository explicitly opts in, agents may close beads, run quality gates, commit, and push as part of session close. A current "do not commit" or "do not push" instruction still wins.

## Session Completion

This protocol applies when ending a Beads implementation workflow. It is subordinate to explicit user, repository, and orchestrator instructions.

1. **File issues for remaining work** - Create beads for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Handle git/sync by active profile**:
   ```bash
   # Conservative/minimal/default: report status and proposed commands; wait for approval.
   git status

   # Team-maintainer opt-in only, unless current instructions forbid it:
   git pull --rebase
   git push
   git status
   ```
5. **Hand off** - Summarize changes, validation, issue status, and any blocked sync/commit/push step

**Critical rules:**
- Explicit user or orchestrator instructions override this Beads block.
- Do not commit or push without clear authority from the active profile or the current user request.
- If a required sync or push is blocked, stop and report the exact command and error.
<!-- END BEADS INTEGRATION -->
