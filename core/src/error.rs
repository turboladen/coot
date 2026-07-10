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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
