//! `billz-core` — the driver-owning spine of the billz SQL Server client.
//!
//! Pure Rust, no Tauri, headless-testable. `mssql-client` is a *private* dependency of this crate
//! and must never leak past its boundary: the `app` crate and the Svelte UI see only `core`'s own
//! plain, serializable types (`QueryResult` / `ColumnMeta` / `CellValue`). See `PLAN.md` §3.
//!
//! Wave-1 modules (`result`/`error`, `types`, `connection`, `context`) land the
//! foundational, driver-free types. The two probes in `examples/`
//! (`typed_probe`, `dynamic_dump`) are the working proof of the exact
//! `mssql-client` calls the driver-touching modules use.

pub mod batch;
pub mod connection;
pub mod connection_store;
pub mod context;
pub mod error;
pub mod executor;
pub mod query;
pub mod query_store;
pub mod result;
pub mod schema;
pub mod types;

pub use batch::{batch_at_line, split_batches};
pub use connection::{
    CachingSecretStore, ConnectionConfig, ConnectionId, InMemorySecretStore, KeychainSecretStore,
    SecretStore, build_connection_string,
};
pub use connection_store::ConnectionStore;
pub use context::ExecutionContext;
pub use error::{CoreError, Result};
pub use executor::run;
pub use query::{Param, ParamScope, SavedQuery, SavedQueryId, SqlType};
pub use query_store::QueryStore;
pub use result::{CellValue, ColumnMeta, QueryResult};
pub use schema::{
    ColumnInfo, DatabaseInfo, SchemaCache, TableInfo, ViewInfo, list_columns, list_databases,
    list_tables, list_views,
};
pub use types::friendly_type_name;
