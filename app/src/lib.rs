//! `billz-app` — the thin Tauri shell. Real `#[tauri::command]`s delegate into
//! `billz-core`; this crate stays driver-free. **No `mssql_client` type appears
//! in any signature here** — that's guaranteed structurally: `app` depends on
//! `billz-core`, never on `mssql-client` (`CLAUDE.md`, `PLAN.md` §3). The UI sees
//! only `core`'s own serde types (`ConnectionConfig` / `QueryResult` / …).

use billz_core::{
    CachingSecretStore, ColumnInfo, ConnectionConfig, ConnectionId, ConnectionStore, CoreError,
    DatabaseInfo, ExecutionContext, KeychainSecretStore, QueryResult, QueryStore, SavedQuery,
    SavedQueryId, SchemaCache, SecretStore, TableInfo, ViewInfo, build_connection_string,
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
    schema: SchemaCache, // in-memory introspection cache (rqb.2)
    queries: QueryStore, // saved-query library (d28.6) — saved_queries.json
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
    // Does this save change HOW we connect? Only then must the reused
    // introspection client (+ its cached schema) be dropped so the next use
    // reconnects with fresh config. A metadata-only edit (rename, default-db)
    // keeps the warm client — that amortized login is the whole point of
    // billz-lpb. A new password ⇒ changed; server/user/TLS diff ⇒ changed; a
    // brand-new connection has nothing cached, so this is a harmless no-op.
    let connect_changed = password.is_some()
        || state.connections.get(&cfg.id)?.is_some_and(|old| {
            // Compare via the connection-string builder (the single source of
            // truth for what affects a connection) with a fixed dummy password,
            // so this can't drift as connect-affecting fields are added/changed —
            // only cfg-derived params differ here (password is handled above).
            build_connection_string(&old, "") != build_connection_string(&cfg, "")
        });
    if let Some(pw) = password {
        state.secrets.set_password(&cfg.id, &pw)?;
    }
    state.connections.upsert(&cfg)?;
    if connect_changed {
        state.schema.forget_connection(&cfg.id);
    }
    Ok(())
}

#[tauri::command]
async fn delete_connection(id: ConnectionId, state: State<'_, AppState>) -> AppResult<()> {
    state.connections.delete(&id)?;
    state.secrets.delete_password(&id)?; // idempotent
    state.schema.forget_connection(&id); // drop cached client + schema
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

/// Object-tree data (rqb.2). The four schema commands mirror `test_connection`'s
/// idiom: resolve `cfg` by id (`?` → `AppError::Core`), then delegate to the
/// managed [`SchemaCache`], which dedups + caches per key. Returns are all
/// `core`-owned serde types — no `mssql_client` type crosses the boundary.
///
/// Arg names (`id`/`db`/`schema`/`table`) are load-bearing: Tauri marshals
/// JS→Rust args by name, so `api.ts`'s `invoke` keys must match exactly. These
/// use `db` (terser) where `run_sql` uses `database`.
#[tauri::command]
async fn list_databases(
    id: ConnectionId,
    state: State<'_, AppState>,
) -> AppResult<Vec<DatabaseInfo>> {
    let cfg = state
        .connections
        .get(&id)?
        .ok_or_else(|| CoreError::Config(format!("no connection {}", id.0)))?;
    Ok(state.schema.databases(&cfg, &state.secrets).await?)
}

#[tauri::command]
async fn list_tables(
    id: ConnectionId,
    db: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<TableInfo>> {
    let cfg = state
        .connections
        .get(&id)?
        .ok_or_else(|| CoreError::Config(format!("no connection {}", id.0)))?;
    Ok(state.schema.tables(&cfg, &state.secrets, &db).await?)
}

#[tauri::command]
async fn list_views(
    id: ConnectionId,
    db: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<ViewInfo>> {
    let cfg = state
        .connections
        .get(&id)?
        .ok_or_else(|| CoreError::Config(format!("no connection {}", id.0)))?;
    Ok(state.schema.views(&cfg, &state.secrets, &db).await?)
}

#[tauri::command]
async fn list_columns(
    id: ConnectionId,
    db: String,
    schema: String,
    table: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<ColumnInfo>> {
    let cfg = state
        .connections
        .get(&id)?
        .ok_or_else(|| CoreError::Config(format!("no connection {}", id.0)))?;
    Ok(state
        .schema
        .columns(&cfg, &state.secrets, &db, &schema, &table)
        .await?)
}

/// Refresh (rqb.5): drop the active connection's cached schema so the next
/// tree load re-queries sys.* (I do DDL on DEV and want new objects at once).
#[tauri::command]
async fn refresh_schema(id: ConnectionId, state: State<'_, AppState>) -> AppResult<()> {
    state.schema.invalidate_connection(&id);
    Ok(())
}

/// Saved-query library (d28.6). Three thin passthroughs over the managed
/// [`QueryStore`], mirroring the connection commands: each bottoms out in
/// `CoreError` via `?` (no new `AppError` variant), and returns `core`-owned
/// serde types only. The library is connection-independent — a `SavedQuery` has
/// no connection id.
#[tauri::command]
async fn list_queries(state: State<'_, AppState>) -> AppResult<Vec<SavedQuery>> {
    Ok(state.queries.list()?)
}

/// Insert-or-replace by `query.id`. The UI mints the id (like `ConnectionForm`),
/// builds the whole `SavedQuery`, and passes it here — so promote AND future
/// rename/edit are one path. Mirror of `save_connection`.
#[tauri::command]
async fn save_query(query: SavedQuery, state: State<'_, AppState>) -> AppResult<()> {
    state.queries.upsert(&query)?;
    Ok(())
}

#[tauri::command]
async fn delete_query(id: SavedQueryId, state: State<'_, AppState>) -> AppResult<()> {
    state.queries.delete(&id)?; // idempotent in the store
    Ok(())
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
                schema: SchemaCache::new(),
                queries: QueryStore::new(dir.join("saved_queries.json")),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_name,
            list_connections,
            save_connection,
            delete_connection,
            test_connection,
            run_sql,
            list_databases,
            list_tables,
            list_views,
            list_columns,
            refresh_schema,
            list_queries,
            save_query,
            delete_query
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
