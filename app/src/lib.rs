//! `billz-app` — the thin Tauri shell. Real `#[tauri::command]`s delegate into
//! `billz-core`; this crate stays driver-free. **No `mssql_client` type appears
//! in any signature here** — that's guaranteed structurally: `app` depends on
//! `billz-core`, never on `mssql-client` (`CLAUDE.md`, `PLAN.md` §3). The UI sees
//! only `core`'s own serde types (`ConnectionConfig` / `QueryResult` / …).

use billz_core::{
    CachingSecretStore, ConnectionConfig, ConnectionId, ConnectionStore, CoreError,
    ExecutionContext, KeychainSecretStore, QueryResult, SecretStore,
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
/// path) and the secret store. The secret store is the Keychain wrapped in a
/// session cache, so the password is read from the (prompt-inducing) macOS
/// Keychain at most once per connection per session — every subsequent query /
/// tab / `GO` batch reuses the in-memory copy. Both `Send + Sync + 'static`.
struct AppState {
    connections: ConnectionStore,
    secrets: CachingSecretStore<KeychainSecretStore>,
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
///
/// Resolves WHAT to run, then GO-splits it (billz-cwt.5). A non-empty selection
/// wins (and still GO-splits, since a selection can span a `GO`); otherwise the
/// batch containing the caret's `line` (1-based, from CodeMirror). The batch
/// logic lives in `core`; this command only orchestrates: resolve text →
/// `split_batches` → loop `core::run` → flatten every result set into one `Vec`
/// (cwt.7 adds tabs). A failing batch aborts the rest (`?`) — SSMS-style
/// continue-on-error is deferred. Empty input → `[]` (the UI shows "nothing to
/// run"). Return shape (`Vec<QueryResult>`) is unchanged from the pre-cwt.5 path.
#[tauri::command]
async fn run_sql(
    id: ConnectionId,
    database: Option<String>,
    sql: String,               // full document text
    selection: Option<String>, // Some(text) when a non-empty selection exists
    line: usize,               // 1-based caret line in `sql` (CodeMirror line number)
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

    let batches: Vec<&str> = match selection.as_deref() {
        Some(sel) if !sel.trim().is_empty() => billz_core::split_batches(sel),
        _ => billz_core::split_batches(billz_core::batch_at_line(&sql, line)),
    };

    // Run each batch and flatten every result set into one Vec.
    let mut out = Vec::new();
    for batch in batches {
        let mut results = billz_core::run(&cfg, &state.secrets, &ctx, batch).await?;
        out.append(&mut results);
    }
    Ok(out)
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
                // Keychain reads are cached for the session (read once per
                // connection, then reused) so a query never re-prompts.
                secrets: CachingSecretStore::new(KeychainSecretStore),
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

    /// Exercises the `run_sql` split+loop against `billz_core` directly (not the
    /// `#[tauri::command]` wrapper, which needs `State<AppState>`): a GO-split
    /// script runs as two batches whose result sets flatten into one Vec. Same
    /// env gate as above — skips cleanly when `MSSQL_*` is unset.
    #[test]
    fn split_and_loop_flattens_two_batches() {
        let Some((cfg, store)) = env_connection() else {
            eprintln!("skipping split_and_loop_flattens_two_batches: MSSQL_* env not set");
            return;
        };
        let ctx = ExecutionContext::new(cfg.id.clone());
        let out = tauri::async_runtime::block_on(async {
            let mut out = Vec::new();
            for batch in billz_core::split_batches("SELECT 1\nGO\nSELECT 2") {
                let mut results = billz_core::run(&cfg, &store, &ctx, batch).await.unwrap();
                out.append(&mut results);
            }
            out
        });
        assert_eq!(out.len(), 2);
    }
}
