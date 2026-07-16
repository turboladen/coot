# Changelog

All notable changes to coot are documented here. Format loosely follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-15

First release. A personal macOS SQL Server client (Tauri + Svelte, Rust core) —
SQL-auth only, single-user, for use on DEV boxes.

### Query editor & execution
- CodeMirror 6 SQL editor with syntax highlighting and comment toggle.
- Multiple query tabs; each tab's scratch SQL is autosaved and restored across
  restarts, with a per-tab dirty indicator dot when a saved query has unsaved edits.
- Parameter binding: declare typed, scoped parameters and run them via a param bar.
- Saved query library: save, update, and reopen named queries.
- Per-tab target-database picker — the database is execution context, chosen per run.

### Results grid
- Virtualized results grid (TanStack Table + TanStack Virtual) for large result sets.
- Drag-to-resize columns; double-click a resize handle to autofit to content.
- Column widths persist across sessions (LRU-capped so the store can't grow unbounded).
- Header stays synced with the grid on horizontal scroll.
- Decimal, money, and bigint values cross the boundary as strings — no float precision loss.

### Object explorer
- Lazy-loading tree of Databases → Tables / Views → Columns.
- Row selection highlight, right-click to select, and a schema Refresh action.
- Keyboard/`aria-expanded` accessibility on expandable nodes.

### Connections & security
- SQL-auth connections with connection metadata stored locally (never the password).
- Passwords stored in the macOS Keychain via `keyring`; optional session-only
  passwords held in memory for the app session and never written to disk.
- Per-session Keychain password caching — no re-prompt on every query.

### Cross-tenant fan-out
- Run one query across many databases in parallel, with a multi-select database picker.
- Combined results grid when shapes match, plus a per-database status strip.

### Theming & platform
- Light / dark / system theme toggle with a CVD-safe dark palette.
- macOS desktop app built on Tauri v2; signed with a local self-signed identity
  (see `SIGNING.md`). Not Apple-notarized — see `RELEASING.md` for the download/install
  step recipients must run.

[0.1.0]: https://github.com/turboladen/coot/releases/tag/v0.1.0
