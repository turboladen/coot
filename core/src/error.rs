//! Core error type — the backend-agnostic error surface for the whole crate.
//!
//! `CoreError` deliberately carries **no** `#[from]` for any backend error
//! (`mssql_client::Error`, `keyring::Error`, …). Backends are stringified at
//! their adapter — the executor for the driver, `KeychainSecretStore` for
//! keyring — so the public error API never names a backend type. That is what
//! keeps a bad-driver-day a `core`-only change (`PLAN.md` §3, `CLAUDE.md`).

use thiserror::Error;

/// Crate-wide `Result` alias.
pub type Result<T> = std::result::Result<T, CoreError>;

/// The one error type crossing `core`'s public boundary.
///
/// `#[non_exhaustive]` so later beads (e.g. the executor's `Query(String)`)
/// can add variants without a breaking change. The executor MUST stringify
/// `mssql_client::Error` into its own variant — never `#[from]` — or the driver
/// type leaks into this public enum and breaks the §3 invariant.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// A connection could not be built or configured (bad server, etc.).
    #[error("connection configuration error: {0}")]
    Config(String),
    /// The secret store (keychain) failed. `keyring::Error` is stringified here
    /// by `KeychainSecretStore` so this variant stays backend-agnostic.
    #[error("secret store error: {0}")]
    Secret(String),
    /// A query or statement failed on the server or in the driver. The driver's
    /// `mssql_client::Error` is stringified here by the executor — never
    /// `#[from]` — so the public error surface never names a driver type
    /// (`PLAN.md` §3, `CLAUDE.md`).
    #[error("query error: {0}")]
    Query(String),
    /// A **transport-level** failure — a dropped/closed socket, TLS, or a TDS
    /// protocol/codec desync — as opposed to a deterministic server/query error.
    /// The executor classifies the driver's transport-ish `mssql_client::Error`
    /// variants here (never `#[from]`). Retryable: a reused connection that hits
    /// this may just have a stale socket, so the caller can drop it and reconnect
    /// once (`billz-lpb.1`). See [`CoreError::is_transport`].
    #[error("connection transport error: {0}")]
    Transport(String),
    /// Reading or writing the on-disk connection metadata failed. `io::Error` /
    /// `serde_json::Error` are stringified here by `ConnectionStore` so this
    /// variant stays backend-agnostic (no `#[from]`), matching `Secret`/`Query`.
    #[error("connection store error: {0}")]
    Store(String),
    /// The server's `host:port` could not be reached during the pre-connect
    /// reachability probe (`preflight_reachable`) — the TCP dial was refused,
    /// unresolvable, or timed out. On this DEV setup the overwhelming cause is a
    /// down Azure VPN tunnel, so we fail fast with a friendly, human-facing
    /// message (the whole sentence — `#[error("{0}")]` — is rendered verbatim in
    /// the UI) instead of the driver's cryptic ~15s login timeout. NOT a
    /// `Transport` (that classifies *driver* failures on an established path);
    /// this fires before the driver is ever dialed.
    #[error("{0}")]
    Unreachable(String),
    /// A bind param's value could not be parsed into its declared type — a
    /// **pre-flight** user error (it happens in `parse_bind_value` before any
    /// server contact), distinct from a driver/`Query` failure. Note: Money
    /// range/scale errors are NOT caught here; they surface as `Query` at send
    /// time (`param_bind` / `PLAN.md` §5).
    #[error("parameter error: {0}")]
    Param(String),
}

impl CoreError {
    /// Whether this is a transport-level failure worth retrying on a fresh
    /// connection (`billz-lpb.1`). Only [`CoreError::Transport`] qualifies — a
    /// `Query`/server error is deterministic, so re-running it would just repeat
    /// the failure and cost a wasted login.
    pub fn is_transport(&self) -> bool {
        matches!(self, CoreError::Transport(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_transport_only_true_for_transport() {
        assert!(CoreError::Transport("socket closed".into()).is_transport());
        assert!(!CoreError::Query("permission denied".into()).is_transport());
        assert!(!CoreError::Config("no server".into()).is_transport());
        assert!(!CoreError::Secret("boom".into()).is_transport());
        assert!(!CoreError::Store("bad json".into()).is_transport());
        assert!(!CoreError::Param("bad int".into()).is_transport());
        assert!(!CoreError::Unreachable("vpn down".into()).is_transport());
    }

    #[test]
    fn unreachable_display_is_the_message_verbatim() {
        // No "... error:" prefix — the preflight supplies the whole user-facing
        // sentence and the UI renders it as-is.
        let e = CoreError::Unreachable("Can't reach SQL Server at h:1433.".into());
        assert_eq!(e.to_string(), "Can't reach SQL Server at h:1433.");
    }

    #[test]
    fn transport_display_mentions_transport() {
        let e = CoreError::Transport("connection closed".into());
        assert_eq!(
            e.to_string(),
            "connection transport error: connection closed"
        );
    }

    #[test]
    fn secret_display_mentions_secret_store() {
        let e = CoreError::Secret("boom".into());
        assert_eq!(e.to_string(), "secret store error: boom");
    }

    #[test]
    fn config_display_mentions_configuration() {
        let e = CoreError::Config("no server".into());
        assert!(e.to_string().contains("connection configuration error"));
        assert!(e.to_string().contains("no server"));
    }

    #[test]
    fn store_display_mentions_connection_store() {
        let e = CoreError::Store("bad json".into());
        assert_eq!(e.to_string(), "connection store error: bad json");
    }
}
