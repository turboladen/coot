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
//! # Placeholders
//!
//! A query logged before its parameters were substituted still carries them —
//! `WHERE UC.User_Idx = :user_idx` — and SQL Server will not compile that, so
//! there is no plan to capture. `--bind user_idx=1` puts a T-SQL literal in its
//! place. Every placeholder must be bound; the run says which are not and stops
//! before connecting.
//!
//! A literal rather than a `DECLARE`d variable, deliberately. `DECLARE` makes the
//! optimizer estimate from average density because the value is unknown at
//! compile time, while a parameterized query is SNIFFED on first compile using a
//! real value. The literal reproduces the plan the application runs.
//!
//! # Explaining a multi-tenant corpus against one database
//!
//! Traces from a per-tenant product name a different database on nearly every
//! line, and most of them do not exist on any one server. `--database <name>`
//! explains the whole corpus against one that does. Each result line then
//! carries `requestedDatabase` alongside `database`, so a reader can never
//! mistake the plan for one captured against the tenant the query was written
//! for.
//!
//! This is sound only where the tenants share a schema, and even then the
//! CARDINALITY ESTIMATES come from the substitute's statistics. Findings that
//! follow from the schema — an implicit conversion, a join with no predicate, a
//! column with no usable index — hold. Findings that follow from row counts, and
//! the operator choices the optimizer makes because of them, describe the
//! substitute.
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
//!    Failing to REACH the server is the opposite and stops the run — see
//!    [`classify`], and the progress line every query prints to stderr.
//! 3. **Nothing here executes.** Capture goes through `core`'s
//!    `capture_plan_xml`, so every query is compiled and none is run — which is
//!    what makes it safe to point at SQL nobody has vetted.

use std::io::{BufRead, BufWriter, Write};

use coot_core::{
    ConnectionConfig, ConnectionId, CoreError, ExecutionContext, InMemorySecretStore, PlanNode,
    PlanWarning, SecretStore, parse_plan,
};
use serde_json::{Value, json};

#[tokio::main]
async fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}");
            eprintln!("usage: corpus_explain [<corpus.jsonl>] [--database <name>]");
            std::process::exit(1);
        }
    };

    let lines = match read_corpus(args.path.as_deref()) {
        Ok(lines) => lines,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    // Before the env check, for the same reason `read_corpus` runs first: a
    // corpus nobody can compile should not need credentials to discover.
    let bound = match bind_corpus(&lines, &args.bind) {
        Ok(bound) => bound,
        Err(unbound) => {
            eprintln!(
                "the corpus still has parameter placeholders. SQL Server cannot compile a \
                 query that holds one, so no plan exists for any of them."
            );
            eprint!("bind each to a T-SQL literal and rerun:\n  just corpus-explain <file>");
            for name in &unbound {
                eprint!(" --bind {name}=<literal>");
            }
            eprintln!();
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

    if let Some(db) = &args.database {
        eprintln!(
            "explaining all {} queries against {db}, ignoring the database each was \
             generated for. `requestedDatabase` records that on every result line.",
            lines.len()
        );
    }

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut warning_tally: Vec<(String, usize)> = Vec::new();
    let mut error_tally: Vec<(String, usize)> = Vec::new();
    let mut objects_seen: Vec<String> = Vec::new();

    // Sequential, one connection per query. A corpus is hundreds of queries, not
    // millions, and a burst of parallel connects against a shared box buys
    // nothing worth the contention.
    for (n, entry) in lines.iter().enumerate() {
        let (database, substituted_for) = resolve_database(
            args.database.as_deref(),
            entry.database.as_deref(),
            &fallback_db,
        );
        let ctx = ExecutionContext::new(cfg.id.clone()).with_database(database);

        let mut line = match explain_one(&cfg, &store, &ctx, &bound[n]).await {
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
                progress(
                    n + 1,
                    lines.len(),
                    &entry.id,
                    "ok",
                    &format!("{} warning(s)", warnings.len()),
                );
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
            Err(Failure::Verdict(msg)) => {
                failed += 1;
                progress(n + 1, lines.len(), &entry.id, "ERR", &msg);
                match error_tally.iter_mut().find(|(k, _)| *k == msg) {
                    Some((_, c)) => *c += 1,
                    None => error_tally.push((msg.clone(), 1)),
                }
                json!({
                    "id": entry.id,
                    "ok": false,
                    "database": database,
                    "sql": entry.sql,
                    "error": msg,
                })
            }
            Err(Failure::Fatal(msg)) => abort(&mut out, n + 1, lines.len(), &msg),
        };

        if let Some(r) = substituted_for {
            line["requestedDatabase"] = json!(r);
        }
        // `sql` stays the query as generated, because that is what the corpus is
        // being read to judge. This says what was actually compiled.
        if bound[n] != entry.sql {
            line["boundParameters"] = json!(
                args.bind
                    .iter()
                    .filter(|(k, _)| entry.sql.contains(&format!(":{k}")))
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect::<serde_json::Map<String, Value>>()
            );
        }

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
    if !error_tally.is_empty() {
        error_tally.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        eprintln!("why queries did not compile:");
        for (msg, count) in &error_tally {
            eprintln!("  {count:>5}  {}", one_line(msg, 110));
        }
    }
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

/// Every query with its placeholders bound, or every name left unbound across
/// the whole corpus.
///
/// Reports all of them at once — binding one at a time, a rerun per name, would
/// be a connection and a full capture pass each time.
fn bind_corpus(
    lines: &[Entry],
    values: &[(String, String)],
) -> std::result::Result<Vec<String>, Vec<String>> {
    let mut bound = Vec::with_capacity(lines.len());
    let mut unbound: Vec<String> = Vec::new();
    for entry in lines {
        match bind(&entry.sql, values) {
            Ok(sql) => bound.push(sql),
            Err(missing) => {
                for name in missing {
                    if !unbound.contains(&name) {
                        unbound.push(name);
                    }
                }
            }
        }
    }
    if unbound.is_empty() {
        Ok(bound)
    } else {
        Err(unbound)
    }
}

/// Every `:name` placeholder in `sql`, as a byte range and the bare name.
///
/// Skips a colon inside a string literal, a bracketed identifier, or a comment,
/// and skips `::` — so a time literal, a column named `[a:b]`, and a
/// PostgreSQL-style cast are all left alone.
fn placeholders(sql: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let b = sql.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            // A string literal. A doubled quote needs no case of its own: the
            // closing quote is followed immediately by an opening one, which the
            // outer match dispatches straight back here, so the scan is never
            // outside the literal and no colon inside one is ever reached.
            b'\'' => {
                i += 1;
                while i < b.len() {
                    let quote = b[i] == b'\'';
                    i += 1;
                    if quote {
                        break;
                    }
                }
            }
            b'[' => {
                i += 1;
                while i < b.len() && b[i] != b']' {
                    i += 1;
                }
                i += 1;
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                    i += 1;
                }
                i += 2;
            }
            // A cast operator, not a placeholder. Stepping over BOTH colons
            // matters: landing on the second one would read the type as a name.
            b':' if b.get(i + 1) == Some(&b':') => i += 2,
            b':' => {
                let mut j = i + 1;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                if j > i + 1 {
                    out.push((i..j, sql[i + 1..j].to_string()));
                    i = j;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    out
}

/// `sql` with every placeholder replaced by its bound literal, or the names that
/// have no value.
fn bind(sql: &str, values: &[(String, String)]) -> std::result::Result<String, Vec<String>> {
    let found = placeholders(sql);
    if found.is_empty() {
        return Ok(sql.to_string());
    }
    let mut missing: Vec<String> = Vec::new();
    let mut out = String::with_capacity(sql.len());
    let mut last = 0;
    for (range, name) in &found {
        match values.iter().find(|(k, _)| k == name) {
            Some((_, v)) => {
                out.push_str(&sql[last..range.start]);
                out.push_str(v);
                last = range.end;
            }
            None if !missing.contains(name) => missing.push(name.clone()),
            None => {}
        }
    }
    if !missing.is_empty() {
        return Err(missing);
    }
    out.push_str(&sql[last..]);
    Ok(out)
}

/// The database to explain a query against, and the one it asked for when a
/// substitution happened.
///
/// `--database` wins over the corpus, which wins over `MSSQL_DATABASE`. The
/// second element is `Some` only when the corpus named a database and something
/// else was used, so a caller can record the substitution on the result without
/// writing a field that says nothing.
fn resolve_database<'a>(
    override_db: Option<&'a str>,
    requested: Option<&'a str>,
    fallback: &'a str,
) -> (&'a str, Option<&'a str>) {
    let used = override_db.or(requested).unwrap_or(fallback);
    (used, requested.filter(|r| *r != used))
}

/// A corpus path, and a database that overrides the one every entry asks for.
struct Args {
    path: Option<String>,
    database: Option<String>,
    bind: Vec<(String, String)>,
}

/// Parse `[<path>] [--database <name>] [--bind <name>=<literal>]...`. A missing
/// path reads stdin.
fn parse_args() -> std::result::Result<Args, String> {
    let mut path: Option<String> = None;
    let mut database = None;
    let mut bind = Vec::new();
    let mut rest = std::env::args().skip(1);
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--database" | "-d" => {
                database = Some(rest.next().ok_or("--database needs a database name")?);
            }
            "--bind" | "-b" => {
                let pair = rest.next().ok_or("--bind needs <name>=<literal>")?;
                // Split on the FIRST `=`: a bound literal may contain one.
                let (name, value) = pair
                    .split_once('=')
                    .ok_or_else(|| format!("--bind {pair} is not <name>=<literal>"))?;
                bind.push((name.to_string(), value.to_string()));
            }
            // A bare `-` is the conventional name for stdin, so it is a path.
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option {other}"));
            }
            other => {
                if path.replace(other.to_string()).is_some() {
                    return Err("give at most one corpus path".into());
                }
            }
        }
    }
    Ok(Args {
        path,
        database,
        bind,
    })
}

/// Read the corpus from `path`, or stdin when there is none.
///
/// Reads the whole corpus and rejects a malformed line before anything connects,
/// so a typo in the last line neither surfaces after several minutes of captures
/// nor requires credentials to find.
fn read_corpus(path: Option<&str>) -> std::result::Result<Vec<Entry>, String> {
    let reader: Box<dyn BufRead> = match path {
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

/// The parsed plan for one query, or why there is none.
async fn explain_one(
    cfg: &ConnectionConfig,
    store: &dyn SecretStore,
    ctx: &ExecutionContext,
    sql: &str,
) -> std::result::Result<coot_core::QueryPlan, Failure> {
    let xml = coot_core::capture_plan_xml(cfg, store, ctx, sql)
        .await
        .map_err(classify)?;
    // A document the server produced but `parse_plan` cannot read says something
    // about this query's shape and about coot's parser, so record it and keep
    // going. The run still reached a server, which is what `Fatal` is for.
    parse_plan(&xml).map_err(|e| Failure::Verdict(e.to_string()))
}

/// Why one query produced no plan, split by what it means for the queries after
/// it.
enum Failure {
    /// The server's answer about this query. Ordinary output — hazard 2.
    Verdict(String),
    /// The run cannot produce results at all. Every remaining query would fail
    /// the same way, so it stops.
    Fatal(String),
}

/// Sort a capture failure into a verdict on one query and a condition that ends
/// the run.
///
/// Only [`CoreError::Query`] is a verdict: the server parsed the statement and
/// refused it — invalid object name, syntax error, SHOWPLAN denied. Everything
/// else happened before any answer about the SQL existed.
// The wildcard falls on `Fatal` because `CoreError` is `#[non_exhaustive]` and
// the two mistakes are not symmetric. A new variant treated as fatal stops a run
// that might have continued, and says why. Treated as a verdict it writes
// `"ok": false` under SQL that was never judged, which reads as "the query is
// bad" and is how a down VPN once produced a hundred identical verdicts.
fn classify(e: CoreError) -> Failure {
    match e {
        CoreError::Query(msg) => Failure::Verdict(msg),
        other => Failure::Fatal(other.to_string()),
    }
}

/// One line per query on stderr, so a long run shows its shape while it runs.
///
/// stdout carries the results and is normally redirected to a file, which leaves
/// stderr as the terminal.
fn progress(n: usize, total: usize, id: &str, marker: &str, detail: &str) {
    let width = total.to_string().len();
    eprintln!(
        "[{n:>width$}/{total}] {marker:<3} {id:<24} {}",
        one_line(detail, 96)
    );
}

/// `text` as a single line of at most `max` characters.
///
/// A server message runs to several lines and hundreds of characters, and the
/// whole of it is on the result line in the output file — the terminal gets
/// something it can scan instead.
fn one_line(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &flat[..i]),
        None => flat,
    }
}

/// Stop the run, keeping what was already captured.
///
/// Flushes so the results written before the failure survive, then exits
/// non-zero.
fn abort(out: &mut impl Write, n: usize, total: usize, msg: &str) -> ! {
    let _ = out.flush();
    eprintln!(
        "
stopped at query {n} of {total}: {msg}"
    );
    eprintln!(
        "This is the connection, not the corpus. Every remaining query would \
         fail the same way, so nothing more was tried; {} result(s) were written.",
        n - 1
    );
    std::process::exit(1);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_server_verdict_lets_the_run_continue() {
        // `Unreachable` is the specimen that matters. A down tunnel wrote a
        // hundred `"ok": false` lines blaming SQL the server never saw, so the
        // wildcard in `classify` has to land here and not on `Verdict`.
        for e in [
            CoreError::Unreachable("vpn down".into()),
            CoreError::Config("no stored password".into()),
            CoreError::Transport("connection closed".into()),
            CoreError::Secret("keychain denied".into()),
            CoreError::Store("bad json".into()),
            CoreError::Param("bad int".into()),
        ] {
            let rendered = e.to_string();
            assert!(
                matches!(classify(e), Failure::Fatal(_)),
                "{rendered} must stop the run"
            );
        }

        let refused = CoreError::Query("Invalid object name 'dbo.Thing'.".into());
        let Failure::Verdict(msg) = classify(refused) else {
            panic!("a query the server refused must not stop the run");
        };
        // The inner text, not `Display`'s "query error: " wrapper — the result
        // line carries the server's own words.
        assert_eq!(msg, "Invalid object name 'dbo.Thing'.");
    }

    #[test]
    fn a_colon_that_is_not_a_placeholder_is_left_alone() {
        // Each specimen is a colon a naive `:\w+` match WOULD take, and taking
        // any of them rewrites SQL into something the user never wrote.
        for inert in [
            // A time literal: `:30` and `:00` both look like names.
            "SELECT * FROM t WHERE created = '2026-08-01 10:30:00'",
            // `''` escapes a quote, so the literal has not ended at `it''s`.
            "SELECT * FROM t WHERE note = 'it''s 10:30 now'",
            // A bracketed identifier may hold anything at all.
            "SELECT [a:b] FROM t",
            "SELECT * FROM t -- ask :someone about this",
            "SELECT * FROM t /* see :ticket */",
            // A cast, not a placeholder — and the second colon must be stepped
            // over too, or `int` reads as a name.
            "SELECT value::int FROM t",
            // A T-SQL label. Nothing an identifier could start with follows the
            // colon, so there is no name, and a placeholder with an empty name
            // would be reported unbound as `""` and bindable by nothing.
            "retry: SELECT 1 FROM t",
            "SELECT 1 FROM t WHERE x = 1:",
        ] {
            assert_eq!(
                placeholders(inert),
                vec![],
                "found a placeholder in: {inert}"
            );
        }
    }

    #[test]
    fn a_placeholder_is_replaced_in_place() {
        // The real shape out of the corpus, and the colon inside the literal is
        // what discriminates: it must survive untouched while `:user_idx` goes.
        let sql = "SELECT * FROM P WHERE P.At = '10:30:00' AND UC.User_Idx = :user_idx";
        let values = [("user_idx".to_string(), "1".to_string())];
        assert_eq!(
            bind(sql, &values).unwrap(),
            "SELECT * FROM P WHERE P.At = '10:30:00' AND UC.User_Idx = 1"
        );

        // Two occurrences of one name, and a second name, all in one statement.
        let two = "SELECT :a, :b, :a FROM t";
        let both = [
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "'x'".to_string()),
        ];
        assert_eq!(bind(two, &both).unwrap(), "SELECT 1, 'x', 1 FROM t");

        // Unbound names come back so the run can name every one at once.
        assert_eq!(
            bind(two, &[("a".to_string(), "1".to_string())]).unwrap_err(),
            vec!["b".to_string()]
        );
    }

    #[test]
    fn an_override_is_used_and_recorded_as_a_substitution() {
        // The corpus asked for a database that does not exist on this server.
        assert_eq!(
            resolve_database(Some("Local_DEV"), Some("ESP_Arnotts_Group_DEV"), "fallback"),
            ("Local_DEV", Some("ESP_Arnotts_Group_DEV"))
        );
        // An override equal to what the corpus asked for is not a substitution,
        // so nothing is recorded — the discriminating case for the `filter`.
        assert_eq!(
            resolve_database(Some("Same_DEV"), Some("Same_DEV"), "fallback"),
            ("Same_DEV", None)
        );
        // No override: the corpus wins over MSSQL_DATABASE, and honoring a
        // request is not a substitution.
        assert_eq!(
            resolve_database(None, Some("ESP_Arnotts_Group_DEV"), "fallback"),
            ("ESP_Arnotts_Group_DEV", None)
        );
        // Neither: MSSQL_DATABASE, with nothing to record.
        assert_eq!(resolve_database(None, None, "fallback"), ("fallback", None));
        // An override with nothing to substitute for stays quiet.
        assert_eq!(
            resolve_database(Some("Local_DEV"), None, "fallback"),
            ("Local_DEV", None)
        );
    }
}
