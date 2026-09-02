//! `coot-core` — the driver-owning spine of the coot SQL Server client.
//!
//! coot is a single-user desktop client for Microsoft SQL Server: a Tauri shell
//! around a Svelte UI, with this crate underneath. You pick a saved server
//! connection, type SQL in an editor, run it against a chosen database, and read
//! the result sets back in a table. A sidebar lists the server's databases and
//! their tables, views and columns. A library holds named queries with named
//! parameters. This crate implements all of that below the UI; the `app` crate is
//! a thin layer of Tauri commands over the functions re-exported here.
//!
//! Pure Rust, no Tauri, headless-testable. `mssql-client` is a *private*
//! dependency of this crate and must never leak past its boundary: the `app`
//! crate and the Svelte UI see only `core`'s own plain, serializable types
//! ([`QueryResult`], [`ColumnMeta`], [`CellValue`]). That boundary is what keeps
//! a driver replacement a change inside this one crate.
//!
//! # Vocabulary
//!
//! The module docs below use these terms without reintroducing them.
//!
//! - **The runner** is the editor-and-table half of the UI: type SQL, run it,
//!   read result sets. [`executor`] is its data layer.
//! - **The grid** is the table a result set renders into. What it has to display
//!   is why [`CellValue`] has the variants it has.
//! - **The object tree** is the sidebar listing databases, tables, views and
//!   columns. [`schema`] is its data layer and [`session`] supplies its
//!   connection.
//! - **The library** is the saved-query UI. [`query`] defines its shapes,
//!   [`query_store`] persists them, and [`param_bind`] turns a parameter into
//!   something the server accepts.
//! - **A tenant** is one customer's database. The same schema is deployed to
//!   roughly two dozen of them on one server, which is why running a single
//!   query across many databases and comparing the answers is a first-class
//!   operation rather than a loop the caller writes — see [`run_fanout`] and
//!   [`plan::fingerprint`].
//!
//! # Module map
//!
//! - [`connection`] and [`connection_store`]: a server's metadata and the JSON
//!   file holding it. Passwords go to the macOS Keychain through a
//!   [`SecretStore`] and never to that file.
//! - [`context`]: which connection and which database a statement runs against.
//! - [`executor`]: the one place a query is actually sent. [`run`] executes a
//!   batch, [`run_with_params`] executes one with parameters bound, and
//!   [`run_fanout`] executes one across many databases concurrently.
//! - [`session`]: a live connection reused across the object tree's many small
//!   `sys.*` queries, so expanding a node does not pay a login per query.
//! - [`batch`]: client-side splitting on `GO`, which is a separator the client
//!   honors rather than something the server understands.
//! - [`schema`]: the `sys.*` introspection queries, their result shapes, and a
//!   cache with an invalidation seam.
//! - [`query`], [`query_store`], [`param_bind`]: the saved-query model, its file,
//!   and the two ways a parameter value reaches the server.
//! - [`plan`]: estimated execution plans. It captures the XML, parses it, judges
//!   it, and reduces it to a key that groups tenants whose plans agree.
//! - [`result`], [`error`], [`types`]: the values, errors and type names
//!   everything above hands back.
//!
//! # Renamed re-exports
//!
//! Three [`plan`] functions are re-exported here under longer names, so searching
//! `plan/` for the name used at the root finds nothing: [`judge_plan`] is
//! [`plan::judge`], [`plan_shape`] is [`plan::shape`], and [`capture_plan_xml`]
//! is [`plan::capture_xml`].

pub mod batch;
pub mod connection;
pub mod connection_store;
pub mod context;
pub mod error;
pub mod executor;
pub mod param_bind;
pub mod plan;
pub mod query;
pub mod query_store;
pub mod result;
pub mod schema;
pub mod session;
#[cfg(test)]
mod test_support;
pub mod types;

pub use batch::{batch_at_line, split_batches};
pub use connection::{
    CachingSecretStore, ConnectionConfig, ConnectionId, InMemorySecretStore, KeychainSecretStore,
    SecretStore, SessionOverlaySecretStore, build_connection_string,
};
pub use connection_store::ConnectionStore;
pub use context::ExecutionContext;
pub use error::{CoreError, Result};
pub use executor::{run, run_fanout, run_with_params};
pub use param_bind::{BindValue, ResolvedParam};
pub use plan::{
    DbPlanOutcome, Finding, FindingKind, MissingIndex, PlanCapture, PlanNode, PlanStatement,
    PlanVerdict, PlanWarning, QueryPlan, Severity, capture_xml as capture_plan_xml,
    judge as judge_plan, parse_plan, shape as plan_shape,
};
pub use query::{Param, ParamScope, SavedQuery, SavedQueryId, SqlType};
pub use query_store::QueryStore;
pub use result::{CellValue, ColumnMeta, DbRunOutcome, QueryResult};
pub use schema::{
    ColumnInfo, DatabaseInfo, SchemaCache, TableInfo, ViewInfo, list_columns, list_databases,
    list_tables, list_views,
};
pub use session::SessionCache;
pub use types::friendly_type_name;
