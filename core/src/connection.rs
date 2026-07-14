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
    /// `false` ⇒ session-only password (prompted at connect, held in memory,
    /// never written to the Keychain). Default `true` for back-compat with
    /// configs written before billz-85b. Metadata only — not a secret.
    #[serde(default = "default_true")]
    pub remember_password: bool,
}

fn default_true() -> bool {
    true
}

impl ConnectionConfig {
    /// The `host:port` to TCP-probe before dialing the driver, or `None` when no
    /// STATIC target can be derived — in which case the reachability preflight is
    /// SKIPPED and `connect` behaves exactly as before (conservative: never block
    /// a working connection on a guess).
    ///
    /// `server` is ADO.NET `Data Source` syntax: `host`, `host,port` (COMMA, not
    /// colon), `host\INSTANCE` (named instance), optionally a protocol prefix
    /// (`tcp:`/`np:`/`lpc:`). We probe only the unambiguous `host[,port]` shape:
    ///   - no comma → default port 1433;
    ///   - a comma whose right side isn't a valid `u16` → `None` (don't
    ///     false-negative on a malformed port);
    ///   - a named instance (`\INSTANCE`) with no explicit comma-port → `None`
    ///     (its port is dynamic — assigned by the SQL Browser over UDP 1434 — so
    ///     there is no static TCP port to probe); with an explicit port we probe
    ///     the host before the `\`;
    ///   - a protocol prefix (`tcp:host`) or any other stray `:` in the host →
    ///     `None`; otherwise the host would become e.g. `"tcp:host"`, fail DNS, and
    ///     wrongly report the VPN down — blocking a working connection;
    ///   - a bracketed IPv6 literal (`[::1],1433`) → brackets stripped;
    ///   - empty/whitespace → `None`.
    pub(crate) fn preflight_target(&self) -> Option<(String, u16)> {
        let s = self.server.trim();
        if s.is_empty() {
            return None;
        }
        // Split the host segment from an explicit `,port`. A malformed port
        // (non-`u16`) yields `None` for the whole fn — skip rather than misfire.
        let (addr, explicit_port) = match s.split_once(',') {
            Some((a, p)) => (a.trim(), Some(p.trim().parse::<u16>().ok()?)),
            None => (s, None),
        };
        // Named instance: the TCP host is the part before `\`; the instance's port
        // is dynamic unless one was given explicitly.
        let host_seg = match addr.split_once('\\') {
            Some((h, _instance)) => {
                explicit_port?; // dynamic port (SQL Browser) → skip preflight
                h.trim()
            }
            None => addr,
        };
        // Strip a bracketed IPv6 literal (`[::1]` → `::1`). After this the host
        // must not contain a `:` — a leftover colon is a protocol prefix
        // (`tcp:`/`np:`/`lpc:`) or an unbracketed oddity we won't guess at.
        let host = match host_seg.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            Some(ipv6) => ipv6,
            None if host_seg.contains(':') => return None,
            None => host_seg,
        };
        if host.is_empty() {
            return None;
        }
        Some((host.to_string(), explicit_port.unwrap_or(1433)))
    }
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

/// A [`SecretStore`] decorator that caches passwords in memory for the process
/// lifetime, so the prompt-inducing macOS Keychain is read **at most once per
/// connection per session**. Every subsequent `get_password` (a new query, a new
/// tab, each `GO` batch) is served from the cache without touching the OS
/// keychain — no repeated authorization prompts.
///
/// Wraps any inner store. `set_password` writes through to the inner store *and*
/// refreshes the cache (so editing a connection's password takes effect without
/// a restart); `delete_password` evicts. The plaintext lives only in memory —
/// consistent with the "secrets never on disk in plaintext" invariant
/// (`CLAUDE.md`); the connection metadata and the durable secret still live in
/// the config file and the Keychain respectively.
pub struct CachingSecretStore<S: SecretStore> {
    inner: S,
    cache: Mutex<HashMap<String, String>>,
}

impl<S: SecretStore> CachingSecretStore<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            cache: Mutex::new(HashMap::new()),
        }
    }
}

impl<S: SecretStore> SecretStore for CachingSecretStore<S> {
    fn set_password(&self, id: &ConnectionId, password: &str) -> Result<()> {
        self.inner.set_password(id, password)?;
        self.cache
            .lock()
            .unwrap()
            .insert(id.0.clone(), password.to_string());
        Ok(())
    }

    fn get_password(&self, id: &ConnectionId) -> Result<Option<String>> {
        if let Some(pw) = self.cache.lock().unwrap().get(&id.0) {
            return Ok(Some(pw.clone())); // cache hit → no Keychain access, no prompt
        }
        let fetched = self.inner.get_password(id)?;
        if let Some(pw) = &fetched {
            self.cache.lock().unwrap().insert(id.0.clone(), pw.clone());
        }
        Ok(fetched)
    }

    fn delete_password(&self, id: &ConnectionId) -> Result<()> {
        self.inner.delete_password(id)?;
        self.cache.lock().unwrap().remove(&id.0);
        Ok(())
    }
}

/// A [`SecretStore`] decorator that layers an ephemeral, in-memory **session**
/// password map over any inner store. `get_password` prefers a session password
/// (set via [`set_session_password`]); otherwise it falls through to the inner
/// (durable) store. This backs the "don't remember password" path (billz-85b): a
/// session-only password lives ONLY in this map for the process lifetime and is
/// NEVER written to the inner store / Keychain (`CLAUDE.md` disk invariant).
///
/// `set_password` still writes through to the inner store (the remember-on path);
/// `delete_password` clears both layers; [`clear_durable`] clears only the inner
/// (Keychain) side, leaving a live session password intact on a metadata re-save.
///
/// [`set_session_password`]: Self::set_session_password
/// [`clear_durable`]: Self::clear_durable
pub struct SessionOverlaySecretStore<S: SecretStore> {
    session: Mutex<HashMap<String, String>>,
    pub(crate) inner: S,
}

impl<S: SecretStore> SessionOverlaySecretStore<S> {
    pub fn new(inner: S) -> Self {
        Self {
            session: Mutex::new(HashMap::new()),
            inner,
        }
    }

    /// Store a password in the SESSION map only — never the durable inner store.
    pub fn set_session_password(&self, id: &ConnectionId, password: &str) {
        self.session
            .lock()
            .unwrap()
            .insert(id.0.clone(), password.to_string());
    }

    /// Clear ONLY the durable (inner) secret, leaving the session map intact.
    /// Used when a connection is switched to remember-off (drop any stale Keychain
    /// entry) without disturbing a live session password on a plain metadata
    /// re-save (rename/retarget of an already-unlocked session-only connection).
    pub fn clear_durable(&self, id: &ConnectionId) -> Result<()> {
        self.inner.delete_password(id)
    }
}

impl<S: SecretStore> SecretStore for SessionOverlaySecretStore<S> {
    fn set_password(&self, id: &ConnectionId, password: &str) -> Result<()> {
        self.inner.set_password(id, password)
    }

    fn get_password(&self, id: &ConnectionId) -> Result<Option<String>> {
        if let Some(pw) = self.session.lock().unwrap().get(&id.0) {
            return Ok(Some(pw.clone())); // ephemeral session password wins
        }
        self.inner.get_password(id)
    }

    fn delete_password(&self, id: &ConnectionId) -> Result<()> {
        self.session.lock().unwrap().remove(&id.0);
        self.inner.delete_password(id)
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
            remember_password: true,
        }
    }

    /// Build a config whose `server` is `s`, for `preflight_target` parse tests.
    fn cfg_with_server(s: &str) -> ConnectionConfig {
        let mut c = sample_config();
        c.server = s.into();
        c
    }

    #[test]
    fn preflight_target_parses_host_and_port() {
        assert_eq!(
            cfg_with_server("myhost,1433").preflight_target(),
            Some(("myhost".into(), 1433))
        );
        assert_eq!(
            cfg_with_server("myhost,5000").preflight_target(),
            Some(("myhost".into(), 5000))
        );
    }

    #[test]
    fn preflight_target_defaults_port_when_absent() {
        assert_eq!(
            cfg_with_server("myhost").preflight_target(),
            Some(("myhost".into(), 1433))
        );
    }

    #[test]
    fn preflight_target_skips_empty_or_whitespace() {
        assert_eq!(cfg_with_server("").preflight_target(), None);
        assert_eq!(cfg_with_server("   ").preflight_target(), None);
    }

    #[test]
    fn preflight_target_skips_malformed_port() {
        // A non-`u16` port → skip (don't false-negative a working connection).
        assert_eq!(cfg_with_server("myhost,abc").preflight_target(), None);
        assert_eq!(cfg_with_server("myhost,").preflight_target(), None);
    }

    #[test]
    fn preflight_target_strips_ipv6_brackets() {
        assert_eq!(
            cfg_with_server("[::1],1433").preflight_target(),
            Some(("::1".into(), 1433))
        );
        assert_eq!(
            cfg_with_server("[::1]").preflight_target(),
            Some(("::1".into(), 1433))
        );
    }

    #[test]
    fn preflight_target_skips_named_instance_without_port() {
        // Dynamic port (SQL Browser) — no static TCP port to probe → skip.
        assert_eq!(
            cfg_with_server(r"myhost\SQLEXPRESS").preflight_target(),
            None
        );
    }

    #[test]
    fn preflight_target_probes_named_instance_host_with_explicit_port() {
        assert_eq!(
            cfg_with_server(r"myhost\SQLEXPRESS,1433").preflight_target(),
            Some(("myhost".into(), 1433))
        );
    }

    #[test]
    fn preflight_target_skips_protocol_prefix() {
        // `tcp:host` would otherwise DNS-fail on host "tcp:host" and wrongly
        // report the VPN down, blocking a working connection. Skip instead.
        assert_eq!(cfg_with_server("tcp:myhost,1433").preflight_target(), None);
        assert_eq!(cfg_with_server("tcp:myhost").preflight_target(), None);
        assert_eq!(cfg_with_server("lpc:myhost").preflight_target(), None);
        // Named pipe (`np:\\host\pipe\...`) → skip (no TCP port).
        assert_eq!(
            cfg_with_server(r"np:\\myhost\pipe\sql\query").preflight_target(),
            None
        );
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

    /// A `SecretStore` that counts `get_password` calls — stands in for the
    /// Keychain to prove `CachingSecretStore` reads it at most once per id.
    #[derive(Default)]
    struct CountingStore {
        inner: InMemorySecretStore,
        reads: std::sync::atomic::AtomicUsize,
    }
    impl SecretStore for CountingStore {
        fn set_password(&self, id: &ConnectionId, password: &str) -> Result<()> {
            self.inner.set_password(id, password)
        }
        fn get_password(&self, id: &ConnectionId) -> Result<Option<String>> {
            self.reads
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.inner.get_password(id)
        }
        fn delete_password(&self, id: &ConnectionId) -> Result<()> {
            self.inner.delete_password(id)
        }
    }

    #[test]
    fn caching_store_reads_inner_at_most_once_per_id() {
        use std::sync::atomic::Ordering;
        let id = ConnectionId("c1".into());
        let store = CachingSecretStore::new(CountingStore::default());
        store.set_password(&id, "hunter2").unwrap();

        // Three reads (e.g. three GO batches / three query runs) → the inner
        // (Keychain) store is hit at most once; the rest are cache hits.
        for _ in 0..3 {
            assert_eq!(store.get_password(&id).unwrap(), Some("hunter2".into()));
        }
        assert!(store.inner.reads.load(Ordering::Relaxed) <= 1);
    }

    #[test]
    fn caching_store_refreshes_on_set_and_evicts_on_delete() {
        let id = ConnectionId("c1".into());
        let store = CachingSecretStore::new(InMemorySecretStore::default());
        store.set_password(&id, "old").unwrap();
        assert_eq!(store.get_password(&id).unwrap(), Some("old".into()));
        // A changed password takes effect without a restart (cache refreshed).
        store.set_password(&id, "new").unwrap();
        assert_eq!(store.get_password(&id).unwrap(), Some("new".into()));
        // Delete evicts the cache too.
        store.delete_password(&id).unwrap();
        assert_eq!(store.get_password(&id).unwrap(), None);
    }

    #[test]
    fn session_overlay_prefers_session_over_inner() {
        let inner = InMemorySecretStore::default();
        let id = ConnectionId("c1".into());
        inner.set_password(&id, "durable").unwrap();
        let overlay = SessionOverlaySecretStore::new(inner);
        overlay.set_session_password(&id, "ephemeral");
        assert_eq!(
            overlay.get_password(&id).unwrap().as_deref(),
            Some("ephemeral")
        );
    }

    #[test]
    fn session_overlay_set_session_password_never_reaches_inner() {
        let overlay = SessionOverlaySecretStore::new(InMemorySecretStore::default());
        let id = ConnectionId("c1".into());
        overlay.set_session_password(&id, "ephemeral");
        // Prove nothing was written to the durable inner store — read it directly.
        assert!(overlay.inner.get_password(&id).unwrap().is_none());
    }

    #[test]
    fn session_overlay_set_password_writes_through_to_inner() {
        let overlay = SessionOverlaySecretStore::new(InMemorySecretStore::default());
        let id = ConnectionId("c1".into());
        overlay.set_password(&id, "durable").unwrap();
        assert_eq!(
            overlay.inner.get_password(&id).unwrap().as_deref(),
            Some("durable")
        );
    }

    #[test]
    fn session_overlay_delete_clears_both_layers() {
        let inner = InMemorySecretStore::default();
        let id = ConnectionId("c1".into());
        inner.set_password(&id, "durable").unwrap();
        let overlay = SessionOverlaySecretStore::new(inner);
        overlay.set_session_password(&id, "ephemeral");
        overlay.delete_password(&id).unwrap();
        assert!(overlay.get_password(&id).unwrap().is_none());
        assert!(overlay.inner.get_password(&id).unwrap().is_none());
    }

    #[test]
    fn session_overlay_falls_through_to_inner_when_no_session() {
        let inner = InMemorySecretStore::default();
        let id = ConnectionId("c1".into());
        inner.set_password(&id, "durable").unwrap();
        let overlay = SessionOverlaySecretStore::new(inner);
        assert_eq!(
            overlay.get_password(&id).unwrap().as_deref(),
            Some("durable")
        );
    }

    #[test]
    fn session_overlay_promote_persists_to_inner() {
        // kub: promoting a session-only password to durable (get then set_password)
        // writes it to the inner store, so it survives the session map clearing
        // (the app does this when flipping a connection to remember-on).
        let overlay = SessionOverlaySecretStore::new(InMemorySecretStore::default());
        let id = ConnectionId("c1".into());
        overlay.set_session_password(&id, "pw");
        let known = overlay.get_password(&id).unwrap().unwrap(); // the promote read
        overlay.set_password(&id, &known).unwrap(); // the promote write (→ durable)
        assert_eq!(
            overlay.inner.get_password(&id).unwrap().as_deref(),
            Some("pw")
        );
    }

    #[test]
    fn session_overlay_clear_durable_keeps_session() {
        // Re-saving an already-session-only connection must not wipe the live
        // session password: clear_durable removes only the Keychain side.
        let inner = InMemorySecretStore::default();
        let id = ConnectionId("c1".into());
        inner.set_password(&id, "durable").unwrap();
        let overlay = SessionOverlaySecretStore::new(inner);
        overlay.set_session_password(&id, "ephemeral");
        overlay.clear_durable(&id).unwrap();
        assert!(overlay.inner.get_password(&id).unwrap().is_none()); // durable gone
        assert_eq!(
            overlay.get_password(&id).unwrap().as_deref(),
            Some("ephemeral")
        ); // session kept
    }

    #[test]
    fn config_serde_round_trips_and_holds_no_password() {
        let cfg = sample_config();
        let s = serde_json::to_string(&cfg).unwrap();
        // The "no plaintext on disk" invariant, checked structurally. A real
        // password would serialize as the JSON key "password"; check quote-
        // delimited so the metadata key "rememberPassword" (85b) doesn't trip it.
        assert!(
            !s.to_lowercase().contains("\"password\""),
            "serialized: {s}"
        );
        assert!(s.contains(r#""defaultDatabase":null"#), "camelCase: {s}");
        let back: ConnectionConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn default_database_defaults_none_when_absent() {
        // A hand-edited config omitting defaultDatabase must load (as None), not
        // error with "missing field" (billz-ztr).
        let json = r#"{"id":"c1","name":"n","server":"h,1433","username":"u"}"#;
        let cfg: ConnectionConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.default_database, None);
    }

    #[test]
    fn remember_password_defaults_true_when_absent() {
        // A config written before 85b (no rememberPassword key) must still load.
        let json = r#"{"id":"c1","name":"n","server":"h,1433","username":"u",
            "defaultDatabase":null,"encrypt":false,"trustServerCertificate":true}"#;
        let cfg: ConnectionConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.remember_password);
    }

    #[test]
    fn remember_password_round_trips_false() {
        let mut cfg = sample_config();
        cfg.remember_password = false;
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            json.contains("\"rememberPassword\":false"),
            "serialized: {json}"
        );
        let back: ConnectionConfig = serde_json::from_str(&json).unwrap();
        assert!(!back.remember_password);
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
