# Changelog

All notable changes to Coot are documented here. Format loosely follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Connections & sidebar
- Every saved connection is its own collapsible root in the object tree, carrying a
  passive status dot that reflects only what that connection has done this session.
- Per-connection object state, so expanding one connection leaves the others alone.
- Connection form splits host and port, reveals the typed password on request, and
  offers a dropdown of the server's databases for the default.
- Connection rows carry compact hover icons and a right-click context menu.

### Query editor & library
- A tab owns its own (connection, database) target rather than following a global one.
- The saved-query library lives in a collapsible right-hand panel with a persisted
  width and open/closed state.
- Save the current editor contents to the library without leaving the editor, and
  rename a saved query from the library panel.

### Notifications
- Toasts carry success, failure, and progress for every feedback surface in the app.
- Repeated identical toasts coalesce into one slot with a count.

### Query plans
- `coot-core` can capture a query's estimated plan, parse the ShowPlanXML into a typed
  model, reduce it to a shape fingerprint that groups tenant databases into
  equivalence classes, and judge it into a list of findings. No UI reaches this yet.
- Plan capture strips server measurements — build number, statistics timestamps, memory
  grant, row counts, costs — before anything is written to disk, so a captured `.sqlplan`
  fixture is safe to commit.
- Capturing a plan for a query passed as a command-line argument is refused. A plan
  embeds the query verbatim across seven attributes, and the secret scan matches only
  the configured server, user, and database — not a customer id in a `WHERE` clause.

### Fixed
- `@param` scanning runs through a T-SQL lexer in both the frontend and `core`, so an
  `@name` inside a string literal, a bracketed identifier, or a comment is not mistaken
  for a parameter — `'sales@vendor.com'` yields no `@vendor`.
- Saving a library query fails only when the write itself fails. The list is updated from
  the write, and the reconciling refresh behind it is logged rather than thrown, so a
  successful save is never reported as an error and retried into a duplicate row.
- Long object-tree names ellipsize inside the sidebar rather than overflowing it, and a
  row shows a `title` tooltip only when its text is actually elided.

### Documentation
- Architecture Decision Records in `docs/adr/` are the durable decision record; the
  brainstorming specs and plans they were distilled from are transient and untracked.
- A `README.md` orients a newcomer, and `PLAN.md` carries a banner marking it as a
  historical record rather than a description of the current app.

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
- Light / dark / system theme toggle with a colorblind-safe dark palette — no status is
  carried by hue alone (see ADR-0007).
- macOS desktop app built on Tauri v2; signed with a local self-signed identity
  (see `SIGNING.md`). Not Apple-notarized — see `RELEASING.md` for the download/install
  step recipients must run.

[Unreleased]: https://github.com/turboladen/coot/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/turboladen/coot/releases/tag/v0.1.0
