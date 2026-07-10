//! `billz-core` — the driver-owning spine of the billz SQL Server client.
//!
//! Pure Rust, no Tauri, headless-testable. `mssql-client` is a *private* dependency of this crate
//! and must never leak past its boundary: the `app` crate and the Svelte UI see only `core`'s own
//! plain, serializable types (`QueryResult` / `ColumnMeta` / `CellValue`). See `PLAN.md` §3.
//!
//! This is the Phase-0 scaffold (`billz-ce1.1`): the crate is intentionally empty. Modules land in
//! later beads — `result`/`error` (ce1.2), `types` (ce1.3), `connection` (ce1.4), `context` (ce1.5),
//! then `executor`/`schema`/`query_store`. The two probes in `examples/` (`typed_probe`,
//! `dynamic_dump`) are the working proof of the exact `mssql-client` calls those modules will use.
