//! On-disk connection metadata — a JSON file of `Vec<ConnectionConfig>`.
//!
//! Pure Rust, headless-testable (`PLAN.md` §3): `app` supplies only the path
//! (Tauri's app-config dir) and this module owns all persistence. Because
//! [`ConnectionConfig`] has **no password field by construction**, the file
//! structurally cannot contain a password — the Keychain
//! ([`crate::KeychainSecretStore`]) holds those, keyed by connection id. The
//! `persisted_json_contains_no_password` test re-checks that invariant.
//!
//! No in-memory cache, no interior mutability: single user, single process ⇒
//! every op reads the whole file, mutates, writes the whole file. That keeps
//! [`ConnectionStore`] a bare `PathBuf` — trivially `Send + Sync` for Tauri
//! managed state.

use std::fs;
use std::path::PathBuf;

#[cfg(test)]
use std::path::Path;

use crate::connection::{ConnectionConfig, ConnectionId};
use crate::error::{CoreError, Result};

/// Reads/writes the connection-metadata JSON file. Just a path — no state.
pub struct ConnectionStore {
    path: PathBuf,
}

impl ConnectionStore {
    /// `path` is the `connections.json` file (its parent dir may not yet exist;
    /// [`Self::upsert`]/[`Self::delete`] `create_dir_all` before writing).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Read + parse the file. A **missing** file ⇒ `Ok(vec![])` (first run, not
    /// an error). A present-but-corrupt file ⇒ `Err(CoreError::Store)` — surface
    /// it rather than silently discarding the user's saved connections.
    pub fn list(&self) -> Result<Vec<ConnectionConfig>> {
        let bytes = match fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(CoreError::Store(e.to_string())),
        };
        serde_json::from_slice(&bytes).map_err(|e| CoreError::Store(e.to_string()))
    }

    /// The config with `id`, if any.
    pub fn get(&self, id: &ConnectionId) -> Result<Option<ConnectionConfig>> {
        Ok(self.list()?.into_iter().find(|c| &c.id == id))
    }

    /// Insert-or-replace by `cfg.id`, then rewrite the whole file.
    pub fn upsert(&self, cfg: &ConnectionConfig) -> Result<()> {
        let mut all = self.list()?;
        match all.iter_mut().find(|c| c.id == cfg.id) {
            Some(existing) => *existing = cfg.clone(),
            None => all.push(cfg.clone()),
        }
        self.write_all(&all)
    }

    /// Remove by `id`, then rewrite. Idempotent — an absent id is `Ok(())`.
    pub fn delete(&self, id: &ConnectionId) -> Result<()> {
        let mut all = self.list()?;
        all.retain(|c| &c.id != id);
        self.write_all(&all)
    }

    /// Serialize + write the whole file. Temp-file + `rename` so a mid-write
    /// crash can't truncate the file and wipe every saved connection (the blast
    /// radius is *everything*). Pretty JSON for human-inspectability.
    fn write_all(&self, all: &[ConnectionConfig]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| CoreError::Store(e.to_string()))?;
        }
        let json =
            serde_json::to_string_pretty(all).map_err(|e| CoreError::Store(e.to_string()))?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| CoreError::Store(e.to_string()))?;
        fs::rename(&tmp, &self.path).map_err(|e| CoreError::Store(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A unique temp dir per call — no `tempfile` dep needed for a single-user
    /// tool. Cleaned up at the end of each test via `remove_dir_all`.
    fn temp_store_path() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("billz-store-test-{}-{}", std::process::id(), n));
        dir.join("connections.json")
    }

    fn cleanup(path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    fn config(id: &str, name: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: ConnectionId(id.into()),
            name: name.into(),
            server: "myhost,1433".into(),
            username: "sa".into(),
            default_database: None,
            encrypt: false,
            trust_server_certificate: true,
            remember_password: true,
        }
    }

    #[test]
    fn list_on_missing_file_is_empty() {
        let path = temp_store_path();
        let store = ConnectionStore::new(&path);
        assert_eq!(store.list().unwrap(), vec![]);
        // No write happened, so the parent dir shouldn't have been created.
        cleanup(&path);
    }

    #[test]
    fn upsert_then_list_round_trips() {
        let path = temp_store_path();
        let store = ConnectionStore::new(&path);
        store.upsert(&config("a", "Alpha")).unwrap();
        store.upsert(&config("b", "Bravo")).unwrap();
        let all = store.list().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], config("a", "Alpha"));
        assert_eq!(all[1], config("b", "Bravo"));
        cleanup(&path);
    }

    #[test]
    fn upsert_replaces_by_id() {
        let path = temp_store_path();
        let store = ConnectionStore::new(&path);
        store.upsert(&config("a", "Alpha")).unwrap();
        store.upsert(&config("a", "Renamed")).unwrap();
        let all = store.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "Renamed");
        cleanup(&path);
    }

    #[test]
    fn get_returns_none_for_absent_id() {
        let path = temp_store_path();
        let store = ConnectionStore::new(&path);
        store.upsert(&config("a", "Alpha")).unwrap();
        assert_eq!(store.get(&ConnectionId("nope".into())).unwrap(), None);
        cleanup(&path);
    }

    #[test]
    fn get_returns_the_config() {
        let path = temp_store_path();
        let store = ConnectionStore::new(&path);
        store.upsert(&config("a", "Alpha")).unwrap();
        assert_eq!(
            store.get(&ConnectionId("a".into())).unwrap(),
            Some(config("a", "Alpha"))
        );
        cleanup(&path);
    }

    #[test]
    fn delete_removes_and_is_idempotent() {
        let path = temp_store_path();
        let store = ConnectionStore::new(&path);
        store.upsert(&config("a", "Alpha")).unwrap();
        store.upsert(&config("b", "Bravo")).unwrap();
        store.delete(&ConnectionId("a".into())).unwrap();
        let all = store.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, ConnectionId("b".into()));
        // Deleting an absent id is Ok (idempotent).
        store.delete(&ConnectionId("a".into())).unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        cleanup(&path);
    }

    #[test]
    fn persisted_json_contains_no_password() {
        let path = temp_store_path();
        let store = ConnectionStore::new(&path);
        store.upsert(&config("a", "Alpha")).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        // The "no plaintext on disk" invariant, checked on the raw bytes. A real
        // password serializes as the JSON key "password"; check quote-delimited so
        // the metadata key "rememberPassword" (85b) doesn't trip it.
        assert!(
            !raw.to_lowercase().contains("\"password\""),
            "persisted file: {raw}"
        );
        cleanup(&path);
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let path = temp_store_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "this is not json").unwrap();
        let store = ConnectionStore::new(&path);
        assert!(matches!(store.list(), Err(CoreError::Store(_))));
        cleanup(&path);
    }
}
