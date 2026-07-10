//! `billz-app` — the thin Tauri shell. Real `#[tauri::command]`s delegate into
//! `billz-core`; this crate stays driver-free. **No `mssql_client` type appears
//! in any signature here** — that's guaranteed structurally: `app` depends on
//! `billz-core`, never on `mssql-client` (`CLAUDE.md`, `PLAN.md` §3). The UI sees
//! only `core`'s own serde types (`ConnectionConfig` / `QueryResult` / …).

use billz_core::{
    ConnectionConfig, ConnectionId, ConnectionStore, CoreError, ExecutionContext,
    KeychainSecretStore, QueryResult, SecretStore,
};
use tauri::{Manager, State};

/// Command-boundary error. `CoreError` isn't `Serialize`, so we wrap it and emit
/// a plain string to the frontend (which sees it in `invoke(...).catch`). Only
/// the `Core` variant exists: no command in this wave produces a `tauri::Error`
/// (`app_config_dir()?` is in `.setup`, not a command), and a never-constructed
/// private variant would trip clippy's `dead_code` under warnings-as-errors.
#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error(transparent)]
    Core(#[from] CoreError),
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

type AppResult<T> = Result<T, AppError>;

/// Managed state: the connection-metadata store (holds the `connections.json`
/// path) and the Keychain secret store. Both `Send + Sync + 'static`.
struct AppState {
    connections: ConnectionStore,
    secrets: KeychainSecretStore,
}

/// Trivial bridge command: proves the Svelte -> Rust `invoke` path is wired.
#[tauri::command]
fn app_name() -> &'static str {
    "billz"
}

#[tauri::command]
async fn list_connections(state: State<'_, AppState>) -> AppResult<Vec<ConnectionConfig>> {
    Ok(state.connections.list()?)
}

/// Metadata → `connections.json`; password → Keychain iff `Some`. On edit with
/// no new password, the UI passes `None` to leave the Keychain entry untouched.
#[tauri::command]
async fn save_connection(
    cfg: ConnectionConfig,
    password: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    // Set the secret first: an orphan secret is a smaller problem than metadata
    // pointing at a missing password.
    if let Some(pw) = password {
        state.secrets.set_password(&cfg.id, &pw)?;
    }
    state.connections.upsert(&cfg)?;
    Ok(())
}

#[tauri::command]
async fn delete_connection(id: ConnectionId, state: State<'_, AppState>) -> AppResult<()> {
    state.connections.delete(&id)?;
    state.secrets.delete_password(&id)?; // idempotent
    Ok(())
}

/// Connect + trivial `SELECT 1` via `core::run`. Proves creds + reachability.
/// Errors with `Config("no stored password…")` if the connection was saved with
/// remember-password off — the UI surfaces that string.
#[tauri::command]
async fn test_connection(id: ConnectionId, state: State<'_, AppState>) -> AppResult<()> {
    let cfg = state
        .connections
        .get(&id)?
        .ok_or_else(|| CoreError::Config(format!("no connection {}", id.0)))?;
    let ctx = ExecutionContext::new(cfg.id.clone());
    billz_core::run(&cfg, &state.secrets, &ctx, "SELECT 1").await?;
    Ok(())
}

/// The real query path. `database = None` ⇒ connection default; `Some` ⇒ `USE [db]`.
#[tauri::command]
async fn run_sql(
    id: ConnectionId,
    database: Option<String>,
    sql: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<QueryResult>> {
    let cfg = state
        .connections
        .get(&id)?
        .ok_or_else(|| CoreError::Config(format!("no connection {}", id.0)))?;
    let mut ctx = ExecutionContext::new(cfg.id.clone());
    if let Some(db) = database {
        ctx = ctx.with_database(db);
    }
    Ok(billz_core::run(&cfg, &state.secrets, &ctx, &sql).await?)
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // `app_config_dir()` returns the path but does NOT create it — the
            // store `create_dir_all`s on first write.
            let dir = app.path().app_config_dir()?;
            let store = ConnectionStore::new(dir.join("connections.json"));
            app.manage(AppState {
                connections: store,
                secrets: KeychainSecretStore,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_name,
            list_connections,
            save_connection,
            delete_connection,
            test_connection,
            run_sql
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use billz_core::{
        ConnectionConfig, ConnectionId, ExecutionContext, InMemorySecretStore, SecretStore,
    };

    /// Pins Tauri's *actual* async runtime: `block_on` uses the SAME global
    /// runtime as a `#[tauri::command] async fn`'s `spawn`, so a green run here
    /// proves `core::run`'s tokio driver works on it. Env-gated on `MSSQL_*` —
    /// skips cleanly when unset (box is unreachable from CI; the user runs it
    /// on-network). Mirrors core's `env_connection()`.
    fn env_connection() -> Option<(ConnectionConfig, InMemorySecretStore)> {
        let server = std::env::var("MSSQL_SERVER").ok()?;
        let username = std::env::var("MSSQL_USER").ok()?;
        let password = std::env::var("MSSQL_PASSWORD").ok()?;
        let database = std::env::var("MSSQL_DATABASE").ok()?;

        let cfg = ConnectionConfig {
            id: ConnectionId("app-smoke".into()),
            name: "app-smoke".into(),
            server,
            username,
            default_database: Some(database),
            encrypt: false,
            trust_server_certificate: true,
        };
        let store = InMemorySecretStore::default();
        store.set_password(&cfg.id, &password).unwrap();
        Some((cfg, store))
    }

    #[test]
    fn tauri_runtime_runs_core_query() {
        let Some((cfg, store)) = env_connection() else {
            eprintln!("skipping tauri_runtime_runs_core_query: MSSQL_* env not set");
            return;
        };
        let ctx = ExecutionContext::new(cfg.id.clone());
        let results = tauri::async_runtime::block_on(async {
            billz_core::run(&cfg, &store, &ctx, "SELECT 1").await
        })
        .unwrap();
        assert_eq!(results.len(), 1);
    }
}
