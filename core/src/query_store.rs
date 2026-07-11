//! On-disk saved-query library — a JSON file of `Vec<SavedQuery>`.
//!
//! Pure Rust, headless-testable (`PLAN.md` §3): `app` supplies only the path
//! (Tauri's app-config dir, e.g. `saved_queries.json`) and this module owns all
//! persistence. Mirrors [`crate::connection_store`] semantics exactly.
//!
//! **Param values DO persist in plaintext** — deliberately, and the inverse of
//! `connection_store`'s no-password invariant. The "secrets never on disk"
//! invariant (`CLAUDE.md`) is about connection *passwords* → macOS Keychain. Param
//! values are query inputs (a customer id, a date, an `ORDER BY` clause), not
//! credentials, and remember-last-value (`PLAN.md` §5, d28.3) *requires* them on
//! disk. The `last_value_persists_to_disk` test asserts this on purpose.
//!
//! No in-memory cache, no interior mutability: single user, single process ⇒ every
//! op reads the whole file, mutates, writes the whole file. That keeps
//! [`QueryStore`] a bare `PathBuf` — trivially `Send + Sync` for Tauri managed
//! state.

use std::fs;
use std::path::PathBuf;

#[cfg(test)]
use std::path::Path;

use crate::error::{CoreError, Result};
use crate::query::{SavedQuery, SavedQueryId};

/// Reads/writes the saved-query JSON file. Just a path — no state.
pub struct QueryStore {
    path: PathBuf,
}

impl QueryStore {
    /// `path` is the `saved_queries.json` file (its parent dir may not yet exist;
    /// [`Self::upsert`]/[`Self::delete`] `create_dir_all` before writing).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Read + parse the file. A **missing** file ⇒ `Ok(vec![])` (first run, not an
    /// error). A present-but-corrupt file ⇒ `Err(CoreError::Store)` — surface it
    /// rather than silently discarding the user's saved-query library.
    pub fn list(&self) -> Result<Vec<SavedQuery>> {
        let bytes = match fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(CoreError::Store(e.to_string())),
        };
        serde_json::from_slice(&bytes).map_err(|e| CoreError::Store(e.to_string()))
    }

    /// The saved query with `id`, if any.
    pub fn get(&self, id: &SavedQueryId) -> Result<Option<SavedQuery>> {
        Ok(self.list()?.into_iter().find(|q| &q.id == id))
    }

    /// Insert-or-replace by `q.id`, then rewrite the whole file.
    pub fn upsert(&self, q: &SavedQuery) -> Result<()> {
        let mut all = self.list()?;
        match all.iter_mut().find(|existing| existing.id == q.id) {
            Some(existing) => *existing = q.clone(),
            None => all.push(q.clone()),
        }
        self.write_all(&all)
    }

    /// Remove by `id`, then rewrite. Idempotent — an absent id is `Ok(())`.
    pub fn delete(&self, id: &SavedQueryId) -> Result<()> {
        let mut all = self.list()?;
        all.retain(|q| &q.id != id);
        self.write_all(&all)
    }

    /// Serialize + write the whole file. Temp-file + `rename` so a mid-write crash
    /// can't truncate the file and wipe every saved query (the blast radius is
    /// *everything*). Pretty JSON for human-inspectability.
    fn write_all(&self, all: &[SavedQuery]) -> Result<()> {
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
    use crate::query::{Param, ParamScope, SqlType};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A unique temp dir per call — no `tempfile` dep needed for a single-user
    /// tool. Cleaned up at the end of each test via `remove_dir_all`.
    fn temp_store_path() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "billz-query-store-test-{}-{}",
            std::process::id(),
            n
        ));
        dir.join("saved_queries.json")
    }

    fn cleanup(path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    /// Builds a saved query carrying a bind param, a raw-text param with a
    /// remembered `last_value`, and a `target_database` — so the round-trip tests
    /// exercise the whole AC (params + target db persist).
    fn sample_query(id: &str, name: &str) -> SavedQuery {
        SavedQuery {
            id: SavedQueryId(id.into()),
            name: name.into(),
            sql: "SELECT * FROM orders WHERE cust = @cust ORDER BY @col".into(),
            target_database: Some("ESP_Suntory_DEV".into()),
            params: vec![
                Param {
                    name: "@cust".into(),
                    sql_type: Some(SqlType::Int),
                    last_value: Some("12345".into()),
                    scope: ParamScope::Session,
                },
                Param {
                    name: "@col".into(),
                    sql_type: None,
                    last_value: Some("orders".into()),
                    scope: ParamScope::Local,
                },
            ],
        }
    }

    #[test]
    fn list_on_missing_file_is_empty() {
        let path = temp_store_path();
        let store = QueryStore::new(&path);
        assert_eq!(store.list().unwrap(), vec![]);
        // No write happened, so the parent dir shouldn't have been created.
        assert!(!path.parent().unwrap().exists());
        cleanup(&path);
    }

    #[test]
    fn upsert_then_list_round_trips() {
        let path = temp_store_path();
        let store = QueryStore::new(&path);
        store.upsert(&sample_query("a", "Alpha")).unwrap();
        store.upsert(&sample_query("b", "Bravo")).unwrap();
        let all = store.list().unwrap();
        assert_eq!(all.len(), 2);
        // The AC: params + target_database round-trip save→load→equal.
        assert_eq!(all[0], sample_query("a", "Alpha"));
        assert_eq!(all[1], sample_query("b", "Bravo"));
        cleanup(&path);
    }

    #[test]
    fn upsert_replaces_by_id() {
        let path = temp_store_path();
        let store = QueryStore::new(&path);
        store.upsert(&sample_query("a", "Alpha")).unwrap();
        let mut renamed = sample_query("a", "Renamed");
        renamed.sql = "SELECT 2".into();
        renamed.params.clear();
        store.upsert(&renamed).unwrap();
        let all = store.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0], renamed);
        cleanup(&path);
    }

    #[test]
    fn get_returns_none_for_absent_id() {
        let path = temp_store_path();
        let store = QueryStore::new(&path);
        store.upsert(&sample_query("a", "Alpha")).unwrap();
        assert_eq!(store.get(&SavedQueryId("nope".into())).unwrap(), None);
        cleanup(&path);
    }

    #[test]
    fn get_returns_the_saved_query() {
        let path = temp_store_path();
        let store = QueryStore::new(&path);
        store.upsert(&sample_query("a", "Alpha")).unwrap();
        assert_eq!(
            store.get(&SavedQueryId("a".into())).unwrap(),
            Some(sample_query("a", "Alpha"))
        );
        cleanup(&path);
    }

    #[test]
    fn delete_removes_and_is_idempotent() {
        let path = temp_store_path();
        let store = QueryStore::new(&path);
        store.upsert(&sample_query("a", "Alpha")).unwrap();
        store.upsert(&sample_query("b", "Bravo")).unwrap();
        store.delete(&SavedQueryId("a".into())).unwrap();
        let all = store.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, SavedQueryId("b".into()));
        // Deleting an absent id is Ok (idempotent).
        store.delete(&SavedQueryId("a".into())).unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        cleanup(&path);
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let path = temp_store_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "this is not json").unwrap();
        let store = QueryStore::new(&path);
        assert!(matches!(store.list(), Err(CoreError::Store(_))));
        cleanup(&path);
    }

    #[test]
    fn last_value_persists_to_disk() {
        // The deliberate inverse of connection_store's no-password test:
        // remembering the value IS the feature (PLAN §5), so it must land on disk.
        let path = temp_store_path();
        let store = QueryStore::new(&path);
        store.upsert(&sample_query("a", "Alpha")).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("12345"), "persisted file: {raw}");
        cleanup(&path);
    }
}
