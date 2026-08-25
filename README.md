# Coot

Coot is a personal SQL Server client for macOS: a SQL editor, a virtualized results grid, an object
browser, and a library of saved, parameterized queries. It exists to replace Azure Data Studio for
one person's day-to-day work against on-prem SQL Server development servers, and it is built for
exactly that. There is one user, one machine, SQL authentication only, and no distribution story
beyond handing a `.dmg` to a few trusted people.

Two crates in a Cargo workspace:

- **`coot-core`** (`core/`) — pure Rust, no Tauri. Connections, secrets, the SQL executor, `sys.*`
  schema introspection, the saved-query store, and query-plan capture and analysis. Every driver
  call lives here.
- **`coot-app`** (`app/`) — a thin Tauri v2 shell whose `#[tauri::command]`s delegate into
  `coot-core`, plus the Svelte 5 frontend under `app/ui/`.

Throughout the docs, a **DEV box** means one of those on-prem SQL Server development servers: real
data, reachable over VPN, safe to run DDL against. Coot has never been pointed at production and is
not written as though it will be.

## What you need

- **macOS.** The app bundles and code-signs for macOS only, and passwords live in the macOS
  Keychain. The Rust workspace itself compiles and tests on Linux, which is what CI does.
- **Rust**, current stable. The workspace is edition 2024 and the `mssql-client` driver requires
  1.88 or newer.
- **[bun](https://bun.sh)** for all frontend tooling — install, scripts, tests, and the Tauri CLI.
  Do not substitute npm, pnpm, yarn, or node.
- **[just](https://github.com/casey/just)** (`brew install just`) — the `justfile` at the repo root
  is the task interface for everything below.

## Running it

A fresh clone has no `app/ui/node_modules`, and every frontend recipe fails without it. Install the
JS dependencies first:

```fish
just install     # cd app/ui && bun install
```

Then launch the desktop app:

```fish
just dev         # Tauri + the Vite dev server; this is the one you want
```

Run `just` on its own to list every recipe. The two you will use most often after `just dev`:

```fish
just verify      # the full gate: fmt + clippy + Rust tests + svelte-check + ui-test + ui-build
just test        # Rust tests only
```

`just verify` is what CI runs and what any change has to pass.

The Rust integration tests that talk to a real server are gated behind `MSSQL_SERVER`, `MSSQL_USER`,
`MSSQL_PASSWORD`, and `MSSQL_DATABASE`. They skip cleanly when those are unset, so `just verify`
passes with no server in reach. Set them to exercise the real driver paths:

```fish
set -x MSSQL_SERVER   your-dev-box
set -x MSSQL_USER     your-login
set -x MSSQL_PASSWORD (op read "op://vault/item/password")
set -x MSSQL_DATABASE some_database
```

Never hardcode a server name or a credential in the repo.

## What you must not break

Four rules hold the design together. Each has a failure mode that is expensive to undo, so treat a
change to any of them as a design decision rather than a refactor.

- **The driver stays behind `coot-core`.** `mssql-client` is a private dependency of `core`. No
  `mssql_client::` type appears in a public API, in the `app` crate, or in the frontend — the UI
  sees only `QueryResult`, `ColumnMeta`, and `CellValue`, which `core` owns. The driver is
  fast-moving and has a single maintainer, and this boundary is what keeps a bad driver day a
  `core`-only change. Three modules are permitted to drive a live client: `executor`, `session`, and
  `plan::capture` ([ADR-0002](docs/adr/0002-connection-reuse-for-schema-introspection.md)).
- **`core` is pure Rust and headless-testable.** If something needs Tauri to test, it belongs in
  `app`.
- **Secrets never touch disk in plaintext.** Passwords go to the macOS Keychain via `keyring`, or
  are held in memory for one app session and never written at all
  ([ADR-0003](docs/adr/0003-session-only-passwords.md)). Connection metadata may sit in local
  config; the password may not.
- **The database is execution context, not a value spliced into SQL.** You cannot bind a database
  name, so the executor takes it as an input and issues `USE [database]` itself. Running the same
  query across many tenant databases is then a loop over contexts.

## Where to look next

| Document | What it answers |
| --- | --- |
| [`docs/adr/`](docs/adr/README.md) | Why the code is the way it is. The ADRs are the durable decision record; start with the index. |
| [`CHANGELOG.md`](CHANGELOG.md) | What the app can currently do, release by release. |
| [`CLAUDE.md`](CLAUDE.md) | Standing conventions for working in this repo — the tech stack, the lint bar, the traps. |
| [`RELEASING.md`](RELEASING.md) | Cutting a signed `.dmg` and publishing it to GitHub Releases. |
| [`SIGNING.md`](SIGNING.md) | The one-time local code-signing setup that stops the Keychain re-prompting. |
| [`PLAN.md`](PLAN.md) | The original design plan, kept as a historical record. The ADRs and the changelog are current truth. |

Work is tracked with [beads](https://github.com/gastownhall/beads) (`bd`), an issue tracker that
keeps its data in a local database and syncs it through the git remote. Individual issues are called
**beads**, and their IDs (`billz-a8a`, `billz-xi6.1`) appear in commit messages and code comments.
`just ready` — or `bd ready` — lists what is actionable now.

## License

MIT. See [`LICENSE`](LICENSE).
