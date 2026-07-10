//! `billz-core` — the driver-owning spine of the billz SQL Server client.
//!
//! Pure Rust, no Tauri, headless-testable. `mssql-client` is a *private* dependency of this crate
//! and must never leak past its boundary: the `app` crate and the Svelte UI see only `core`'s own
//! plain, serializable types (`QueryResult` / `ColumnMeta` / `CellValue`). See `PLAN.md` §3.
//!
//! Wave-1 modules (`result`/`error`, `types`, `connection`, `context`) land the
//! foundational, driver-free types. Still to come in later beads:
//! `executor`/`schema`/`query_store`. The two probes in `examples/`
//! (`typed_probe`, `dynamic_dump`) are the working proof of the exact
//! `mssql-client` calls those modules will use.

pub mod connection;
pub mod context;
pub mod error;
pub mod result;
pub mod types;

pub use connection::{
    ConnectionConfig, ConnectionId, InMemorySecretStore, KeychainSecretStore, SecretStore,
    build_connection_string,
};
pub use context::ExecutionContext;
pub use error::{CoreError, Result};
pub use result::{CellValue, ColumnMeta, QueryResult};
pub use types::friendly_type_name;
