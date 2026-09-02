//! Saved-query + parameter data model.
//!
//! These are `core`-owned, UI-facing serde types — no `mssql_client` import (the
//! driver boundary, `CLAUDE.md`). Mirrors `connection.rs`: a newtype id, camelCase
//! structs, closed enums, and `#[serde(default)]` on defaulted fields so a
//! hand-edited JSON file loads cleanly. Persistence lives in
//! [`crate::query_store`], exactly as `connection_store` splits from `connection`.
//!
//! This module defines the shapes only, and not every field is interpreted
//! inside `core`. Bind-versus-raw-text substitution lives in
//! [`crate::param_bind`] and [`crate::executor`], and [`crate::query_store`]
//! persists whatever it is handed. But `scope` and `last_value` are carried
//! through `core` untouched: the frontend decides which tier a value belongs to
//! and writes it back into the `SavedQuery` it sends. `sql_type` is likewise
//! filled in there, from a column's catalog type. So a change to what a tier
//! MEANS is a frontend change, even though the tiers are named here.

// The bead wave these shapes come from: d28.1 defines them here; d28.2
// substitutes, d28.3 remembers last values, d28.4 resolves scope, d28.5
// auto-types from the catalog, d28.6 builds the library UI.

use serde::{Deserialize, Serialize};

/// Stable identifier for a saved query. Newtype (like [`crate::ConnectionId`]) so
/// the library UI and store are type-safe about *which* saved query.
/// `transparent` → serializes as a bare JSON string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SavedQueryId(pub String);

/// The small set of SQL types a **bind** param can declare. NOT the
/// full wire-token surface ([`crate::friendly_type_name`]) — deliberately capped to
/// "the things you filter by." A bind param carries `Some(SqlType)`; a raw-text
/// fragment carries `None`.
///
/// **Closed enum (NOT `#[non_exhaustive]`)**: adding a variant is a *compile
/// error* at every `match` on it, which is the point.
///
/// Serde: `rename_all = "lowercase"` yields the exact SQL keyword strings
/// (`"int"`, `"nvarchar"`, `"datetime2"`, `"uniqueidentifier"`…), which equal
/// [`crate::friendly_type_name`]'s output.
///
// The parallel with `CellValue` / the driver's `SqlValue` — both
// `#[non_exhaustive]` — is a false one: those mirror the driver's evolving value
// space, this is coot's own capped set. `core` has no external consumers
// (`CLAUDE.md`: not built for distribution), so a compile error at every `match`
// (d28.2's `sp_executesql` decl map, d28.5's catalog map) beats a silent runtime
// fallthrough into a broken binding. The serde strings matching
// `friendly_type_name` lets d28.5 map a catalog type name straight to a
// `SqlType`.

// Note (d28.2, resolved): a bare tag carries no precision/length, and that turns
// out not to matter for the bind path. The driver (`mssql-client` 0.20.2) does NOT
// build the `sp_executesql` declaration from this tag — it derives it from the
// bound `SqlValue` variant at runtime: `nvarchar` is auto-sized to the actual
// string, `decimal(38, scale)` takes the scale from the parsed value, `money` is
// `money`. So `param_bind`'s only job is producing the right `SqlValue`; no
// precision/length metadata on `SqlType` is needed. (If a future driver ever
// required an explicit decl, THAT is where width would re-enter — not here.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SqlType {
    Int,
    BigInt,
    NVarChar,
    Bit,
    Date,
    DateTime2,
    Decimal,
    UniqueIdentifier,
    Money,
}

/// Resolution tier for a param value. Narrowest wins: a per-query `Local` value
/// beats a `Session` value, which beats a `Global` default. Unit variants — the
/// *value* lives in [`Param::last_value`] or in the session and global stores;
/// the scope is only the tier discriminator.
///
/// Closed set (the model names exactly three tiers) → not `#[non_exhaustive]`, so
/// every resolution `match` stays exhaustive. `Default = Local` (a saved query's
/// own params are per-query unless promoted).
// This enum is the shape only; d28.4 implements the resolution over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamScope {
    Global,
    Session,
    #[default]
    Local,
}

/// A query parameter. The `sql_type` discriminator decides the
/// substitution mechanism, which is implemented in `param_bind`, NOT here:
///   `Some(_)` → **bind** param, real `sp_executesql` (typed, safe).
///   `None`    → **raw-text** fragment, string-spliced (unsafe, render LOUD).
// Every field is defined here but read and written elsewhere: `sql_type` by
// d28.2's substitution, `last_value` by d28.3, `scope` by d28.4's resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    /// The placeholder, e.g. `"@cust"`.
    pub name: String,
    /// `Some` → bind (typed widget); `None` → raw-text fragment.
    pub sql_type: Option<SqlType>,
    /// The remembered last value. Persisted — a query input, not a credential
    /// (see `query_store` module docs).
    pub last_value: Option<String>,
    /// Resolution tier. `serde(default)` → JSON omitting it loads as
    /// [`ParamScope::Local`].
    #[serde(default)]
    pub scope: ParamScope,
}

/// A named, searchable library item — distinct from a scratch tab.
/// Minimal by design ("good enough for me"): it carries no description and no
/// timestamps, and either would be a non-breaking `#[serde(default)]` addition
/// if it turned out to be wanted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    pub id: SavedQueryId,
    /// Display name in the library.
    pub name: String,
    /// The SQL text (may contain `@param` placeholders).
    pub sql: String,
    /// The **target database**, separate from the connection, because the
    /// database is execution context. `None` = "current" / the connection's
    /// default. The executor maps this to `ExecutionContext.database`.
    pub target_database: Option<String>,
    /// Declared params. Order preserved (`Vec`, not a map) for stable UI display.
    /// `serde(default)` → a hand-edited file omitting it loads as `[]`, matching
    /// [`crate::ConnectionConfig`]'s defaulting pattern.
    #[serde(default)]
    pub params: Vec<Param>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_query_serde_round_trips_with_params() {
        // One bind param (Some type, promoted to Session) and one raw-text param
        // (None type, remembers a value, default Local scope) — the AC's shape.
        let q = SavedQuery {
            id: SavedQueryId("q1".into()),
            name: "Orders by customer".into(),
            sql: "SELECT * FROM orders WHERE cust = @cust ORDER BY @col".into(),
            target_database: Some("Fabrikam_DEV".into()),
            params: vec![
                Param {
                    name: "@cust".into(),
                    sql_type: Some(SqlType::Int),
                    last_value: None,
                    scope: ParamScope::Session,
                },
                Param {
                    name: "@col".into(),
                    sql_type: None,
                    last_value: Some("orders".into()),
                    scope: ParamScope::Local,
                },
            ],
        };

        let s = serde_json::to_string(&q).unwrap();
        // camelCase keys on the wire.
        assert!(s.contains(r#""targetDatabase":"Fabrikam_DEV""#), "{s}");
        assert!(s.contains(r#""sqlType":"int""#), "{s}");
        assert!(s.contains(r#""sqlType":null"#), "{s}");
        assert!(s.contains(r#""lastValue":"orders""#), "{s}");

        let back: SavedQuery = serde_json::from_str(&s).unwrap();
        assert_eq!(back, q);
    }

    #[test]
    fn sql_type_serde_is_lowercase_sql_keyword() {
        // Locks the wire repr d28.5/d28.2/app depend on: the tag is the SQL keyword.
        for (variant, tag) in [
            (SqlType::Int, "\"int\""),
            (SqlType::BigInt, "\"bigint\""),
            (SqlType::NVarChar, "\"nvarchar\""),
            (SqlType::Bit, "\"bit\""),
            (SqlType::Date, "\"date\""),
            (SqlType::DateTime2, "\"datetime2\""),
            (SqlType::Decimal, "\"decimal\""),
            (SqlType::UniqueIdentifier, "\"uniqueidentifier\""),
            (SqlType::Money, "\"money\""),
        ] {
            let s = serde_json::to_string(&variant).unwrap();
            assert_eq!(s, tag, "{variant:?}");
            let back: SqlType = serde_json::from_str(&s).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn param_scope_serde_and_default() {
        assert_eq!(
            serde_json::to_string(&ParamScope::Global).unwrap(),
            "\"global\""
        );
        assert_eq!(
            serde_json::to_string(&ParamScope::Session).unwrap(),
            "\"session\""
        );
        assert_eq!(
            serde_json::to_string(&ParamScope::Local).unwrap(),
            "\"local\""
        );
        // A Param JSON omitting `scope` deserializes as Local (the serde default).
        let json = r#"{"name":"@x","sqlType":null,"lastValue":null}"#;
        let p: Param = serde_json::from_str(json).unwrap();
        assert_eq!(p.scope, ParamScope::Local);
    }

    #[test]
    fn target_database_none_serializes_null() {
        let q = SavedQuery {
            id: SavedQueryId("q1".into()),
            name: "All".into(),
            sql: "SELECT 1".into(),
            target_database: None,
            params: vec![],
        };
        let s = serde_json::to_string(&q).unwrap();
        assert!(s.contains(r#""targetDatabase":null"#), "{s}");
        let back: SavedQuery = serde_json::from_str(&s).unwrap();
        assert_eq!(back.target_database, None);
    }

    #[test]
    fn optional_fields_default_none_when_absent() {
        // A hand-edited file omitting the Option keys must load (as None), not
        // error with "missing field" (billz-ztr): SavedQuery.targetDatabase and
        // Param.sqlType / Param.lastValue.
        let json = r#"{"id":"q1","name":"N","sql":"SELECT * FROM t WHERE a=@a",
            "params":[{"name":"@a","scope":"local"}]}"#;
        let q: SavedQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.target_database, None);
        assert_eq!(q.params[0].sql_type, None);
        assert_eq!(q.params[0].last_value, None);
    }

    #[test]
    fn saved_query_params_default_when_absent() {
        // A hand-edited file omitting `params` loads as an empty Vec.
        let json = r#"{"id":"q1","name":"N","sql":"SELECT 1","targetDatabase":null}"#;
        let q: SavedQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.params, vec![]);
    }
}
