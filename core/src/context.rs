//! Execution context — the database is a first-class **execution input**, not
//! spliced SQL and not baked into the connection (`PLAN.md` §4).
//!
//! Shaped so future cross-tenant fan-out is a loop, not a rewrite:
//! `for db in dbs { run(ctx.clone().with_database(db), …) }`.

use crate::connection::ConnectionId;

/// Which connection to run against, and (optionally) which database on it.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionContext {
    pub connection_id: ConnectionId,
    /// `None` = use the connection's default database.
    pub database: Option<String>,
}

impl ExecutionContext {
    /// A context on the connection's default database.
    pub fn new(connection_id: ConnectionId) -> Self {
        Self {
            connection_id,
            database: None,
        }
    }

    /// Target a specific database. Returns `Self` by value — the fan-out seam
    /// (`PLAN.md` §4): clone a base context and rebind the database per tenant.
    pub fn with_database(mut self, database: impl Into<String>) -> Self {
        self.database = Some(database.into());
        self
    }

    /// The target database, if pinned.
    pub fn database(&self) -> Option<&str> {
        self.database.as_deref()
    }

    /// The `USE [database];` the executor (bead ce1.6) issues before a batch,
    /// with the identifier safely bracket-quoted (`]` → `]]`). `None` when no
    /// database is pinned (stay on the connection default).
    ///
    /// Identifier quoting lives here, in context, because it is the cleanest
    /// headless-testable proof that "database is context, not string-spliced
    /// SQL." The executor must call this — never hand-splice `USE`.
    pub fn use_statement(&self) -> Option<String> {
        self.database
            .as_deref()
            .map(|db| format!("USE [{}];", db.replace(']', "]]")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ExecutionContext {
        ExecutionContext::new(ConnectionId("dev-box".into()))
    }

    #[test]
    fn new_has_no_database_and_no_use_statement() {
        let c = ctx();
        assert_eq!(c.database(), None);
        assert_eq!(c.use_statement(), None);
    }

    #[test]
    fn with_database_sets_database_and_use_statement() {
        let c = ctx().with_database("ESP_Nomad_SE_DEV");
        assert_eq!(c.database(), Some("ESP_Nomad_SE_DEV"));
        assert_eq!(
            c.use_statement().as_deref(),
            Some("USE [ESP_Nomad_SE_DEV];")
        );
    }

    #[test]
    fn fan_out_produces_distinct_contexts_from_one_base() {
        let base = ctx();
        let dbs = ["db_a", "db_b", "db_c"];
        let contexts: Vec<_> = dbs
            .iter()
            .map(|db| base.clone().with_database(*db))
            .collect();
        assert_eq!(contexts.len(), 3);
        assert_eq!(contexts[0].database(), Some("db_a"));
        assert_eq!(contexts[2].database(), Some("db_c"));
        // Base is untouched by the fan-out.
        assert_eq!(base.database(), None);
    }

    #[test]
    fn use_statement_bracket_escapes_closing_bracket() {
        let c = ctx().with_database("weird]name");
        assert_eq!(c.use_statement().as_deref(), Some("USE [weird]]name];"));
    }
}
