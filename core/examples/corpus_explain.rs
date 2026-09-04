//! Capture and parse an estimated plan for every query in a corpus, so an LLM
//! can be handed facts about a batch of generated SQL rather than impressions.
//!
//! # Using it
//!
//!     just corpus-explain corpus/queries.jsonl > corpus/plans.jsonl
//!
//! Input is JSONL, one query per line:
//!
//! ```json
//! {"id": "trace-abc123", "sql": "SELECT …", "database": "Contoso_DEV"}
//! ```
//!
//! `id` is whatever identifies the query upstream — a Langfuse trace id — and
//! comes back on the matching output line so a finding can be traced to the
//! prompt that produced it. `database` is optional and falls back to
//! `MSSQL_DATABASE`.
//!
//! Output is JSONL, one line per input line, in the same order, plus a tally on
//! stderr. `objects` is every table the plan touched, so the union across the
//! run says exactly which tables to export schema for.
//!
//! # THIS IS NOT A FIXTURE TOOL. Do not make it one.
//!
//! `dump_plan` writes files that go into git, so it forces `master`, refuses
//! anything naming another database, strips every measurement, and scans for
//! secrets. NONE of that applies here: the whole point is to explain real
//! queries against a real database, so the output names real tables and columns
//! and is work data. It stays on the machine that produced it.
//!
//! Consequences, in the order they will bite:
//!
//! 1. **Never commit the output.** `/corpus/` is gitignored for this reason;
//!    keep both the input and the output there. Moving a file out does not make
//!    it safe.
//! 2. **A query that will not compile is a RESULT, not a failure.** Generated
//!    SQL references tables that do not exist and gets syntax wrong, and how
//!    often it does is one of the things the corpus is being read for. Each such
//!    line comes back with `"ok": false` and its error, and the run continues.
//! 3. **Nothing here executes.** Capture goes through `core`'s
//!    `capture_plan_xml`, so every query is compiled and none is run — which is
//!    what makes it safe to point at SQL nobody has vetted.

use std::io::{BufRead, BufWriter, Write};

use coot_core::{
    ConnectionConfig, ConnectionId, ExecutionContext, InMemorySecretStore, PlanNode, PlanWarning,
    SecretStore, parse_plan,
};
use serde_json::{Value, json};

#[tokio::main]
async fn main() {
    let lines = match read_corpus() {
        Ok(lines) => lines,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let Some((cfg, store, fallback_db)) = env_connection() else {
        eprintln!(
            "MSSQL_SERVER / MSSQL_USER / MSSQL_PASSWORD / MSSQL_DATABASE must all be set.\n\
             This example only works on a machine that can reach the box."
        );
        std::process::exit(1);
    };

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut warning_tally: Vec<(String, usize)> = Vec::new();
    let mut objects_seen: Vec<String> = Vec::new();

    // Sequential, one connection per query. A corpus is hundreds of queries, not
    // millions, and a burst of parallel connects against a shared box buys
    // nothing worth the contention.
    for (n, entry) in lines.iter().enumerate() {
        let database = entry.database.as_deref().unwrap_or(&fallback_db);
        let ctx = ExecutionContext::new(cfg.id.clone()).with_database(database);

        let line = match explain_one(&cfg, &store, &ctx, &entry.sql).await {
            Ok(plan) => {
                ok += 1;
                let mut objects = Vec::new();
                let mut warnings = Vec::new();
                for s in &plan.statements {
                    for w in s.all_warnings() {
                        warnings.push(warning_name(w).to_string());
                    }
                    for mi in &s.missing_indexes {
                        warnings.push("missingIndex".to_string());
                        push_unique(&mut objects, mi.table.clone());
                    }
                    if let Some(root) = &s.root {
                        collect_objects(root, &mut objects);
                    }
                }
                for w in &warnings {
                    match warning_tally.iter_mut().find(|(k, _)| k == w) {
                        Some((_, c)) => *c += 1,
                        None => warning_tally.push((w.clone(), 1)),
                    }
                }
                for o in &objects {
                    push_unique(&mut objects_seen, o.clone());
                }
                json!({
                    "id": entry.id,
                    "ok": true,
                    "database": database,
                    "sql": entry.sql,
                    "objects": objects,
                    "warnings": warnings,
                    "plan": plan,
                })
            }
            Err(e) => {
                failed += 1;
                json!({
                    "id": entry.id,
                    "ok": false,
                    "database": database,
                    "sql": entry.sql,
                    "error": e,
                })
            }
        };

        if let Err(e) = writeln!(out, "{line}") {
            eprintln!("writing result {n}: {e}");
            std::process::exit(1);
        }
    }

    if let Err(e) = out.flush() {
        eprintln!("flushing output: {e}");
        std::process::exit(1);
    }

    warning_tally.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    objects_seen.sort();
    eprintln!(
        "\n{ok} explained, {failed} would not compile, {} total",
        lines.len()
    );
    eprintln!("warnings across the corpus:");
    if warning_tally.is_empty() {
        eprintln!("  (none)");
    }
    for (kind, count) in &warning_tally {
        eprintln!("  {count:>5}  {kind}");
    }
    eprintln!("tables touched ({}):", objects_seen.len());
    for o in &objects_seen {
        eprintln!("  {o}");
    }
}

struct Entry {
    id: String,
    sql: String,
    database: Option<String>,
}

/// Read the corpus from the path in `argv[1]`, or stdin when there is none.
///
/// Reads the whole corpus and rejects a malformed line before anything connects,
/// so a typo in the last line neither surfaces after several minutes of captures
/// nor requires credentials to find.
fn read_corpus() -> std::result::Result<Vec<Entry>, String> {
    let arg = std::env::args().nth(1);
    let reader: Box<dyn BufRead> = match arg.as_deref() {
        None | Some("-") => Box::new(std::io::stdin().lock()),
        Some(path) => Box::new(std::io::BufReader::new(
            std::fs::File::open(path).map_err(|e| format!("opening {path}: {e}"))?,
        )),
    };

    let mut out = Vec::new();
    for (i, line) in reader.lines().enumerate() {
        let n = i + 1;
        let line = line.map_err(|e| format!("reading line {n}: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let v: Value =
            serde_json::from_str(&line).map_err(|e| format!("line {n} is not JSON: {e}"))?;
        let sql = v
            .get("sql")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("line {n} has no string `sql`"))?;
        // An id is what ties a finding back to the prompt that produced it, so a
        // corpus without one is analyzable but not actionable. Fall back to the
        // line number rather than refusing, and say so by shape.
        let id = v
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("line-{n}"));
        out.push(Entry {
            id,
            sql: sql.to_string(),
            database: v
                .get("database")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    if out.is_empty() {
        return Err("the corpus is empty".into());
    }
    Ok(out)
}

/// The parsed plan for one query, or the server's complaint about it as a
/// string. `Err` here is ordinary output — see hazard 2 in the module doc.
async fn explain_one(
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
    ctx: &ExecutionContext,
    sql: &str,
) -> std::result::Result<coot_core::QueryPlan, String> {
    let xml = coot_core::capture_plan_xml(cfg, store, ctx, sql)
        .await
        .map_err(|e| e.to_string())?;
    parse_plan(&xml).map_err(|e| e.to_string())
}

/// The `kind` tag serde writes for this warning, without rebuilding the mapping
/// by hand — a second copy would drift from the enum the moment a variant is
/// added.
fn warning_name(w: &PlanWarning) -> String {
    serde_json::to_value(w)
        .ok()
        .and_then(|v| v.get("kind").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| "unknown".into())
}

fn collect_objects(n: &PlanNode, out: &mut Vec<String>) {
    if let Some(o) = &n.object {
        push_unique(out, o.clone());
    }
    for c in &n.children {
        collect_objects(c, out);
    }
}

fn push_unique(out: &mut Vec<String>, value: String) {
    if !out.contains(&value) {
        out.push(value);
    }
}

/// A live `(cfg, store, default database)` from `MSSQL_*`, or `None` when any is
/// unset. Unlike `dump_plan`'s, this keeps `MSSQL_DATABASE` as the context every
/// query without its own runs in — a corpus is explained against the database it
/// was generated for.
fn env_connection() -> Option<(ConnectionConfig, InMemorySecretStore, String)> {
    let server = std::env::var("MSSQL_SERVER").ok()?;
    let username = std::env::var("MSSQL_USER").ok()?;
    let password = std::env::var("MSSQL_PASSWORD").ok()?;
    let database = std::env::var("MSSQL_DATABASE").ok()?;

    let cfg = ConnectionConfig {
        id: ConnectionId("corpus".into()),
        name: "corpus".into(),
        server,
        username,
        default_database: Some(database.clone()),
        encrypt: false,
        trust_server_certificate: true,
        remember_password: true,
    };
    let store = InMemorySecretStore::default();
    store.set_password(&cfg.id, &password).ok()?;
    Some((cfg, store, database))
}
