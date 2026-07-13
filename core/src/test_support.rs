//! Shared test-only helpers (billz-2co). `#[cfg(test)]` in `lib.rs`, so this is
//! compiled only for tests and never ships. Consolidates the `env_connection()`
//! copies that were duplicated across the `executor` / `schema` / `session` test
//! modules — one place to touch when [`ConnectionConfig`] gains a field.

use crate::connection::{ConnectionConfig, ConnectionId, InMemorySecretStore, SecretStore};

/// A live `(cfg, store, database)` built from the `MSSQL_*` env vars, or `None`
/// when any is unset — a clean runtime skip (NOT `#[ignore]`) so headless/CI runs
/// pass. The password is loaded into an in-memory store (never the Keychain).
/// `database` is also in `cfg.default_database`; returned separately for the
/// callers (schema tests) that want it directly.
pub(crate) fn env_connection() -> Option<(ConnectionConfig, InMemorySecretStore, String)> {
    let server = std::env::var("MSSQL_SERVER").ok()?;
    let username = std::env::var("MSSQL_USER").ok()?;
    let password = std::env::var("MSSQL_PASSWORD").ok()?;
    let database = std::env::var("MSSQL_DATABASE").ok()?;

    let cfg = ConnectionConfig {
        id: ConnectionId("smoke".into()),
        name: "smoke".into(),
        server,
        username,
        default_database: Some(database.clone()),
        encrypt: false,
        trust_server_certificate: true,
        remember_password: true,
    };
    let store = InMemorySecretStore::default();
    store.set_password(&cfg.id, &password).unwrap();
    Some((cfg, store, database))
}
