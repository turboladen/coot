//! Connection metadata, the connection-string builder, and secret storage.
//!
//! Invariants (`CLAUDE.md`): SQL-auth only; **secrets never on disk in
//! plaintext** — `ConnectionConfig` has no password field, so it *cannot* be
//! serialized with one; the password lives only in the macOS Keychain and is
//! fetched at connect time. The connection-string builder graduates the spike
//! probes (`typed_probe.rs` / `dynamic_dump.rs`), and secret storage is behind a
//! trait so unit tests never touch the OS keychain.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Stable identifier for a saved connection. Newtype so [`ExecutionContext`]
/// (`crate::context`) is type-safe about *which* connection it runs against.
///
/// [`ExecutionContext`]: crate::context::ExecutionContext
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectionId(pub String);

/// Saved connection metadata. **No password field by construction** — it can
/// never be written to disk. SQL-auth only (no auth-mode enum): no Entra/AAD/
/// Windows auth (`CLAUDE.md` scope). `strict`/`no_tls` encrypt modes are
/// deferred (`PLAN.md` §2) and intentionally not modeled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    pub id: ConnectionId,
    /// Display name in the connection manager.
    pub name: String,
    /// `"host,1433"`.
    pub server: String,
    pub username: String,
    pub default_database: Option<String>,
    /// `false` ⇒ `Encrypt=false` ("optional") — the locked default (`PLAN.md` §2).
    #[serde(default)]
    pub encrypt: bool,
    /// `true` ⇒ `TrustServerCertificate=true` — the locked default (`PLAN.md` §2).
    #[serde(default = "default_true")]
    pub trust_server_certificate: bool,
}

fn default_true() -> bool {
    true
}

/// Escape a value for an ADO.NET connection string. `mssql-client`'s
/// `split_connection_string` (verified in `config.rs`) parses ADO.NET style: a
/// value may be wrapped in `"` with an embedded quote **doubled** (`""` → `"`)
/// to escape; braces are literal. So we wrap in double quotes and double any
/// embedded double-quote — this survives `;`, quotes, and leading whitespace.
fn ado_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// Build the `mssql-client` connection string, in the spike's shape. The
/// password is passed in (fetched from the keychain at connect time) so it never
/// lives in [`ConnectionConfig`]. Consumed by the executor (bead ce1.6) via
/// `Config::from_connection_string`.
///
/// # Warning
/// The returned string carries the **plaintext password** — the one place
/// plaintext exists, in memory, by design. It must NEVER be logged or persisted.
pub fn build_connection_string(cfg: &ConnectionConfig, password: &str) -> String {
    // `username`, `default_database`, and the password are user-entered free
    // text, so ADO-quote each — a stray `;`/`"`/leading space would otherwise
    // corrupt the string. `server` is a structured `host,port` the user types
    // in that shape; left unquoted so the driver's host/port split is untouched.
    let mut s = format!(
        "Server={server};User Id={user};Password={password};Encrypt={encrypt};\
         TrustServerCertificate={trust};Application Name=billz",
        server = cfg.server,
        user = ado_quote(&cfg.username),
        password = ado_quote(password),
        encrypt = cfg.encrypt,
        trust = cfg.trust_server_certificate,
    );
    if let Some(db) = &cfg.default_database {
        s.push_str(&format!(";Database={}", ado_quote(db)));
    }
    s
}

/// Password storage, abstracted so unit tests never hit the OS keychain (no
/// prompts, works headless). Production uses [`KeychainSecretStore`]; tests use
/// [`InMemorySecretStore`].
///
/// `Send + Sync` so `&dyn SecretStore` can be held across an `.await` inside a
/// Tauri async command (whose future must be `Send`). Both implementors already
/// satisfy it (unit struct / `Mutex`-guarded map).
pub trait SecretStore: Send + Sync {
    fn set_password(&self, id: &ConnectionId, password: &str) -> Result<()>;
    /// `Ok(None)` when nothing is stored for `id`.
    fn get_password(&self, id: &ConnectionId) -> Result<Option<String>>;
    fn delete_password(&self, id: &ConnectionId) -> Result<()>;
}

/// Production secret store: the macOS Keychain via `keyring::v1::Entry`,
/// service = `"billz"`, account = the connection id. `keyring`'s `NoEntry`
/// (nothing stored) maps to `Ok(None)` on read and `Ok(())` on delete
/// (idempotent); every other keyring error is stringified into
/// [`CoreError::Secret`] so the driver-agnostic error surface (`PLAN.md` §3) is
/// preserved — no `#[from] keyring::Error`.
pub struct KeychainSecretStore;

impl KeychainSecretStore {
    const SERVICE: &'static str = "billz";

    fn entry(id: &ConnectionId) -> Result<keyring::v1::Entry> {
        keyring::v1::Entry::new(Self::SERVICE, &id.0).map_err(|e| CoreError::Secret(e.to_string()))
    }
}

impl SecretStore for KeychainSecretStore {
    fn set_password(&self, id: &ConnectionId, password: &str) -> Result<()> {
        Self::entry(id)?
            .set_password(password)
            .map_err(|e| CoreError::Secret(e.to_string()))
    }

    fn get_password(&self, id: &ConnectionId) -> Result<Option<String>> {
        match Self::entry(id)?.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::v1::Error::NoEntry) => Ok(None),
            Err(e) => Err(CoreError::Secret(e.to_string())),
        }
    }

    fn delete_password(&self, id: &ConnectionId) -> Result<()> {
        match Self::entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::v1::Error::NoEntry) => Ok(()),
            Err(e) => Err(CoreError::Secret(e.to_string())),
        }
    }
}

/// In-memory secret store for tests — no OS calls, no prompts.
#[derive(Default)]
pub struct InMemorySecretStore(Mutex<HashMap<String, String>>);

impl SecretStore for InMemorySecretStore {
    fn set_password(&self, id: &ConnectionId, password: &str) -> Result<()> {
        self.0
            .lock()
            .unwrap()
            .insert(id.0.clone(), password.to_string());
        Ok(())
    }

    fn get_password(&self, id: &ConnectionId) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(&id.0).cloned())
    }

    fn delete_password(&self, id: &ConnectionId) -> Result<()> {
        self.0.lock().unwrap().remove(&id.0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> ConnectionConfig {
        ConnectionConfig {
            id: ConnectionId("dev-box".into()),
            name: "Dev box".into(),
            server: "myhost,1433".into(),
            username: "sa".into(),
            default_database: None,
            encrypt: false,
            trust_server_certificate: true,
        }
    }

    #[test]
    fn in_memory_store_round_trips() {
        let store = InMemorySecretStore::default();
        let id = ConnectionId("c1".into());
        assert_eq!(store.get_password(&id).unwrap(), None);
        store.set_password(&id, "hunter2").unwrap();
        assert_eq!(store.get_password(&id).unwrap(), Some("hunter2".into()));
        store.delete_password(&id).unwrap();
        assert_eq!(store.get_password(&id).unwrap(), None);
    }

    #[test]
    fn in_memory_delete_of_absent_id_is_ok() {
        let store = InMemorySecretStore::default();
        store.delete_password(&ConnectionId("nope".into())).unwrap();
    }

    #[test]
    fn config_serde_round_trips_and_holds_no_password() {
        let cfg = sample_config();
        let s = serde_json::to_string(&cfg).unwrap();
        // The "no plaintext on disk" invariant, checked structurally.
        assert!(!s.to_lowercase().contains("password"), "serialized: {s}");
        assert!(s.contains(r#""defaultDatabase":null"#), "camelCase: {s}");
        let back: ConnectionConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn config_defaults_apply_when_absent() {
        // encrypt defaults false, trust_server_certificate defaults true.
        let json =
            r#"{"id":"x","name":"X","server":"h,1433","username":"u","defaultDatabase":null}"#;
        let cfg: ConnectionConfig = serde_json::from_str(json).unwrap();
        assert!(!cfg.encrypt);
        assert!(cfg.trust_server_certificate);
    }

    #[test]
    fn connection_string_has_expected_pieces() {
        let s = build_connection_string(&sample_config(), "pw");
        assert!(s.contains("Server=myhost,1433"));
        assert!(s.contains(r#"User Id="sa""#));
        assert!(s.contains("Password=\"pw\""));
        assert!(s.contains("Encrypt=false"));
        assert!(s.contains("TrustServerCertificate=true"));
        assert!(s.contains("Application Name=billz"));
        // No default database → no ;Database= clause.
        assert!(!s.contains(";Database="));
    }

    #[test]
    fn connection_string_appends_database_when_present() {
        let mut cfg = sample_config();
        cfg.default_database = Some("ESP_Nomad_SE_DEV".into());
        let s = build_connection_string(&cfg, "pw");
        assert!(s.contains(r#";Database="ESP_Nomad_SE_DEV""#));
    }

    #[test]
    fn connection_string_reflects_encrypt_toggle() {
        let mut cfg = sample_config();
        cfg.encrypt = true;
        cfg.trust_server_certificate = false;
        let s = build_connection_string(&cfg, "pw");
        assert!(s.contains("Encrypt=true"));
        assert!(s.contains("TrustServerCertificate=false"));
    }

    #[test]
    fn special_char_password_is_ado_quoted() {
        // A password with `;`, a double-quote, and a leading space. ADO.NET
        // rule (verified in mssql-client's split_connection_string): wrap in
        // double quotes, double embedded double-quotes. We assert the exact
        // quoted form, then that the driver parses the whole string without
        // error (its password field is private, so we prove well-formedness,
        // not byte-equality).
        let pw = r#" my;"weird"pass"#;
        let s = build_connection_string(&sample_config(), pw);
        assert!(s.contains(r#"Password=" my;""weird""pass""#), "got: {s}");
        let cfg = mssql_client::Config::from_connection_string(&s)
            .expect("driver must parse the quoted password");
        let _ = cfg; // parse success is the assertion; password field is private.
    }

    #[test]
    fn special_chars_in_user_and_database_are_quoted_and_parse() {
        // username and database are user-entered free text too; a `;` or `"`
        // in either must be quoted, not corrupt the string. Round-trip through
        // the real driver parser to prove the whole string is well-formed.
        let mut cfg = sample_config();
        cfg.username = r#"weird;"user"#.into();
        cfg.default_database = Some(r#"db;"name"#.into());
        let s = build_connection_string(&cfg, "pw");
        assert!(s.contains(r#"User Id="weird;""user""#), "got: {s}");
        assert!(s.contains(r#";Database="db;""name""#), "got: {s}");
        mssql_client::Config::from_connection_string(&s)
            .expect("driver must parse quoted user + database");
    }

    // The one real-keychain test. `#[ignore]`d so default `cargo test` triggers
    // NO OS prompt; run manually with `cargo test -- --ignored`.
    #[test]
    #[ignore = "hits the real macOS Keychain; run with --ignored"]
    fn keychain_store_real_round_trip() {
        let store = KeychainSecretStore;
        let id = ConnectionId("billz-test-ephemeral".into());
        store.set_password(&id, "secret").unwrap();
        assert_eq!(store.get_password(&id).unwrap(), Some("secret".into()));
        store.delete_password(&id).unwrap();
        assert_eq!(store.get_password(&id).unwrap(), None);
    }
}
