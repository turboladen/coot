//! Capture real ShowPlanXML documents and write them as `.sqlplan` fixtures.
//!
//! # Using it
//!
//! Capture every fixture — this is the one you want:
//!
//!     just dump-plans
//!
//! To add a fixture, put its SQL in [`FIXTURES`] below and run that again.
//!
//! Passing a query as arguments is refused. That form exists only to print the
//! instructions above; see [`permit`] for why.
//!
//! It only works on a machine that can reach the DEV box, and needs the same
//! `MSSQL_*` variables as the probes beside it (fish):
//!
//!     set -x MSSQL_SERVER   …
//!     set -x MSSQL_USER     …
//!     set -x MSSQL_PASSWORD (op read "op://…")
//!     set -x MSSQL_DATABASE …
//!
//! **Read every file it writes before committing it.** The server sends each
//! document as a single line — `scan.sqlplan` is 147KB of it — so
//! `just dump-plans` pretty-prints them afterwards via `just fmt-plans`, which
//! is what makes reading them possible at all.
//!
//! **Nothing it runs executes.** `SET SHOWPLAN_XML ON` makes the server compile
//! each query and hand back the plan without running it.
//!
//! # Why this exists
//!
//! `core::plan`'s parser is tested offline against checked-in fixtures.
//! Hand-authored ShowPlanXML is a trap: the tests all pass while the parser
//! returns empty plans against a real server, because one attribute name
//! differs. So the fixtures must be genuine server output — and only a machine
//! that can reach the DEV box can produce them. Run this there, commit what it
//! writes, and the parser gets developed against reality.
//!
//! # These fixtures go into git. Read this before changing the queries.
//!
//! ShowPlanXML stamps `Database="[…]"` onto **every** `<Object>` element, so a
//! plan captured while connected to a tenant database embeds that database's
//! name throughout — even for a query that only reads `sys.*`. Committing work
//! database, table, or column names to this repo is not acceptable.
//!
//! Four independent defenses, because one is not enough:
//!
//! 1. **No tenant database is ever connected to.** Every query runs against
//!    [`FIXTURE_DB`] (`master`), never `MSSQL_DATABASE`. Together with
//!    `sys.*`-only queries that makes every identifier in the output a
//!    Microsoft-standard name (`master`, `sys`, `dbo`). The sensitive value is
//!    never captured in the first place, which beats scrubbing it out
//!    afterwards.
//! 2. **No measurement of the server survives.** A plan is also a measurement of
//!    the machine that compiled it: `Build` is the exact patch level,
//!    `LastUpdate` timestamps its statistics, and
//!    `EstimatedAvailableMemoryGrant` / `EstimatedPagesCached` /
//!    `EstimatedAvailableDegreeOfParallelism` / `MaxCompileMemory` describe its
//!    memory, buffer pool and CPU, while every row count and cost measures the
//!    catalog it ran against. On a public repo that is server fingerprinting, so
//!    [`sanitize`] strips those attributes and replaces every measurement with a
//!    synthetic round value.
//! 3. **No value from a query nobody reviewed reaches a file.** A plan embeds
//!    the literal values of the SQL it explains — `12345` in a
//!    `WHERE CustomerId = 12345` — across the attributes in
//!    [`LITERAL_ATTRIBUTES`], `StatementText` above all, which is the query
//!    written out in full. The queries in [`FIXTURES`] are in this file and were
//!    read before they were committed, so their values are ours and they are
//!    captured as they are. A query handed in as an argument was read by nobody,
//!    so [`permit`] refuses it — and refuses all of them, because
//!    `StatementText` is on every statement a server returns.
//! 4. **Nothing is written until it passes [`scan_for_secrets`].** Each document
//!    is searched for the configured server, username and database values; a hit
//!    aborts the whole run without touching the filesystem.
//!
//! Cases needing contrived shapes (missing-index suggestions, implicit
//! conversions) need a scratch table. Create it in `master` or `tempdb` with
//! generic column names — never in a tenant database.
//!
//! **None of the four reads the document for you.** [`sanitize`] removes only
//! what it is told to name, so a measurement attribute a future server version
//! invents passes straight through. [`literals`] shows you values and judges
//! none of them — it cannot tell a customer id from a row limit. Read every file
//! before committing it.
//!
//! Unlike the two spike probes beside it (`typed_probe`, `dynamic_dump`), which
//! predate the `core` boundary and drive `mssql-client` directly, this one goes
//! through `core`'s own `capture_plan_xml`, so it exercises the real capture
//! path including the `USE`-before-`SHOWPLAN` ordering.

use std::env;
use std::ops::Range;
use std::path::PathBuf;

use coot_core::{
    ConnectionConfig, ConnectionId, ExecutionContext, InMemorySecretStore, SecretStore,
};
use roxmltree::{Document, Node};

/// Every fixture is captured here, NOT against `MSSQL_DATABASE`. `master` is a
/// standard SQL Server name that reveals nothing, and it carries the same
/// `sys.*` catalog views, so the plans are structurally identical to what a
/// tenant database would produce.
const FIXTURE_DB: &str = "master";

/// Every database a captured fixture may name. [`sanitize`] refuses a document
/// naming any other.
// `mssqlsystemresource` is the hidden database backing `sys.*`, so a plan over a
// catalog view references it whether or not the query mentions it — 103 times
// across the fixtures, against 544 for `master`. An allowlist of FIXTURE_DB
// alone would refuse all five.
const ALLOWED_DATABASES: &[&str] = &[FIXTURE_DB, "mssqlsystemresource"];

/// Every query `just dump-plans` captures, and the fixture filename each one
/// writes. Add a query here to add a fixture.
///
/// Each exercises a different plan shape the parser must handle. Read `sys.*`
/// only, and read the module doc before adding one.
const FIXTURES: &[(&str, &str)] = &[
    // Single operator, no children — the simplest possible tree.
    ("seek", "SELECT name FROM sys.objects WHERE object_id = 1"),
    // A scan over a wide catalog view — exercises larger row estimates.
    ("scan", "SELECT * FROM sys.all_columns"),
    // A join: nested RelOps, so the parser's child-discovery and own-cost
    // arithmetic (subtree cost minus children's subtree costs) get exercised.
    (
        "join",
        "SELECT TOP 10 o.name, c.name FROM sys.objects o \
         JOIN sys.columns c ON c.object_id = o.object_id",
    ),
    // Two statements in ONE batch → two <StmtSimple> elements in one document.
    (
        "two-statements",
        "SELECT COUNT(*) FROM sys.objects; SELECT TOP 1 name FROM sys.schemas;",
    ),
    // An aggregate, so a Stream Aggregate / Hash Match (Aggregate) operator and
    // a deeper tree show up.
    (
        "aggregate",
        "SELECT type_desc, COUNT(*) FROM sys.objects GROUP BY type_desc",
    ),
    // A comma join with no ON predicate, which the optimizer reports as
    // `<Warnings NoJoinPredicate="true">`. This is the accidental-cartesian
    // signal the verdict grades Problem severity, and it is the one warning
    // shape reachable from `sys.*` alone: the others need a table this capture
    // is not permitted to create.
    (
        "no-join-predicate",
        "SELECT TOP 10 o.name, s.name FROM sys.objects o, sys.schemas s",
    ),
];

#[tokio::main]
async fn main() {
    let Some((cfg, store, secrets)) = env_connection() else {
        eprintln!(
            "MSSQL_SERVER / MSSQL_USER / MSSQL_PASSWORD / MSSQL_DATABASE must all be set.\n\
             This example only works on a machine that can reach the DEV box."
        );
        std::process::exit(1);
    };

    // Forced to master — see defense 1 in the module doc.
    let ctx = ExecutionContext::new(cfg.id.clone()).with_database(FIXTURE_DB);

    let args: Vec<String> = env::args().skip(1).collect();
    let Some((work, source)) = plan_work(&args) else {
        eprintln!("usage: dump_plan [<fixture-name> <sql>]");
        std::process::exit(1);
    };

    // Capture EVERYTHING first and secret-scan it before writing a single file.
    // A fixture that leaks must never reach the filesystem, where it could be
    // swept into a commit by `git add -A`.
    let mut captured: Vec<(String, String)> = Vec::with_capacity(work.len());
    for (name, sql) in &work {
        match coot_core::capture_plan_xml(&cfg, &store, &ctx, sql).await {
            Ok(raw) => {
                // Sanitize BEFORE scanning, so the scan sees the exact bytes that
                // would be written rather than an earlier draft of them.
                let (xml, report) = match sanitize(&raw) {
                    Ok(sanitized) => sanitized,
                    Err(e) => {
                        eprintln!("ABORTED: sanitizing '{name}' failed: {e}\nNothing was written.");
                        std::process::exit(1);
                    }
                };
                if let Some(hit) = scan_for_secrets(&xml, &secrets) {
                    eprintln!(
                        "ABORTED: the plan for '{name}' contains {hit}.\n\
                         Nothing was written. Capture against {FIXTURE_DB} with sys.* objects \
                         only — see this example's module doc."
                    );
                    std::process::exit(1);
                }
                // Last gate before the write, and the only one that can refuse a
                // document nothing is wrong with — an ad-hoc query's literals are
                // its own SQL, which only a human can vouch for.
                let found = match literals(&xml) {
                    Ok(found) => found,
                    Err(e) => {
                        eprintln!("ABORTED: reading '{name}' failed: {e}\nNothing was written.");
                        std::process::exit(1);
                    }
                };
                if let Err(refusal) = permit(source, &found) {
                    eprintln!("{refusal}");
                    std::process::exit(1);
                }
                for node in &report.inversions_dropped {
                    eprintln!(
                        "  WARNING: {name} operator {node} inverted, and the inversion could not \
                         be represented. Its own cost is positive in the output."
                    );
                }
                println!(
                    "captured {name} ({} bytes) — {} operators, {} inversions preserved, \
                     {} dropped, {} wasteful reads preserved; clean",
                    xml.len(),
                    report.operators,
                    report.inversions_kept,
                    report.inversions_dropped.len(),
                    report.high_reads,
                );
                captured.push((name.clone(), xml));
            }
            Err(e) => {
                eprintln!("ABORTED: capturing '{name}' failed: {e}\nNothing was written.");
                std::process::exit(1);
            }
        }
    }

    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plans");
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("could not create {}: {e}", out_dir.display());
        std::process::exit(1);
    }
    for (name, xml) in &captured {
        let path = out_dir.join(format!("{name}.sqlplan"));
        if let Err(e) = std::fs::write(&path, xml) {
            eprintln!("FAILED writing {}: {e}", path.display());
            std::process::exit(1);
        }
        println!("wrote {}", path.display());
    }

    println!(
        "\nAll {} captured and secret-scanned.\n\
         Run `just fmt-plans` to pretty-print them, then READ them before committing — \n\
         the secret scan is a net, not a substitute for looking.",
        captured.len()
    );
}

// ------------------------------------------------------------- literal values

/// Every attribute that can carry a literal value out of the SQL being
/// explained — the `12345` in a `WHERE CustomerId = 12345`.
// A DENYLIST of ATTRIBUTES, with both weaknesses that implies: an attribute a
// future server invents is reported by nothing, and element text and CDATA are
// scanned by neither this nor `sanitize`. ShowPlanXML is attribute-only in
// practice, and an ad-hoc capture is refused on `StatementText` regardless.
//
// `StatementText` is the load-bearing entry — the query verbatim, so a literal
// anywhere in the SQL is in it however the optimizer treats the value. It is
// REPORTED, never rewritten: `core::plan::parse` reads it into
// `PlanStatement::text`, and `Expression` carries the implicit-conversion
// evidence; tests assert both exactly.
const LITERAL_ATTRIBUTES: &[&str] = &[
    "StatementText",
    "ParameterizedText",
    "ConstValue",
    "ScalarString",
    "ParameterCompiledValue",
    "ParameterRuntimeValue",
    "Expression",
];

/// What to capture, and where its SQL came from: no arguments means every query
/// in [`FIXTURES`], and a name-and-query pair means that one query. `None` for
/// any other number of arguments, which is a usage error.
// Separate from `main` so the pairing of mode to `Source` is testable. Getting
// it backwards labels an ad-hoc capture `Source::BuiltIn` and opens the gate on
// exactly the SQL it exists to stop, which no test of `permit` alone can see —
// every one of those constructs its own `Source`.
fn plan_work(args: &[String]) -> Option<(Vec<(String, String)>, Source<'_>)> {
    match args {
        [name, sql] => Some((
            vec![(name.clone(), sql.clone())],
            Source::AdHoc { name, sql },
        )),
        [] => Some((
            FIXTURES
                .iter()
                .map(|(n, s)| ((*n).to_string(), (*s).to_string()))
                .collect(),
            Source::BuiltIn,
        )),
        _ => None,
    }
}

/// One literal a document would commit, and the attribute carrying it.
#[derive(Debug, PartialEq, Eq)]
struct Literal {
    attribute: &'static str,
    value: String,
}

/// Where a capture's SQL came from, and so whether anyone has read it.
#[derive(Clone, Copy)]
enum Source<'a> {
    /// A query from [`FIXTURES`], which lives in this file and was read before
    /// it was committed.
    BuiltIn,
    /// A query handed in as a command-line argument, which nobody has read.
    AdHoc { name: &'a str, sql: &'a str },
}

/// Every distinct literal in `xml`, in document order.
///
/// Distinct on the pair of attribute and value, so one value reported by two
/// different attributes appears twice — which is the point, since it shows how
/// many ways the same value reaches the file.
///
/// # Errors
///
/// Returns `Err` if the document is not well-formed XML.
fn literals(xml: &str) -> Result<Vec<Literal>, String> {
    let doc = Document::parse(xml).map_err(|e| format!("the plan XML did not parse: {e}"))?;
    let mut found: Vec<Literal> = Vec::new();
    for n in doc.descendants().filter(Node::is_element) {
        for attribute in LITERAL_ATTRIBUTES {
            let Some(value) = n.attribute(*attribute) else {
                continue;
            };
            if !found
                .iter()
                .any(|l| l.attribute == *attribute && l.value == value)
            {
                found.push(Literal {
                    attribute,
                    value: value.to_string(),
                });
            }
        }
    }
    Ok(found)
}

/// Decide whether a capture may be written, given the literal values it carries.
///
/// A capture of a query from [`FIXTURES`] is always permitted and its literals
/// are not consulted: that SQL is in this file and was read before it was
/// committed, so its values are ours. A capture of a query handed in as a
/// command-line argument is refused if it carries any literal — which is all of
/// them, since a server returns `StatementText` on every statement.
///
/// # Errors
///
/// The `Err` is the message to print: every literal found, and the line to add
/// to [`FIXTURES`] to capture the same query from this file instead.
// No override flag. An escape hatch does not stop someone removing this guard
// under time pressure and leaves no diff behind when they use it; making the
// correct path cheaper than the workaround does. Hence a refusal that is a
// recipe rather than a complaint.
fn permit(source: Source, found: &[Literal]) -> Result<(), String> {
    let Source::AdHoc { name, sql } = source else {
        return Ok(());
    };
    if found.is_empty() {
        return Ok(());
    }

    let mut message = format!(
        "ABORTED: the plan for '{name}' embeds {} literal value(s) from its SQL.\n\n",
        found.len()
    );
    for l in found {
        message.push_str(&format!("  {:<22}  {}\n", l.attribute, l.value));
    }
    message.push_str(
        "\nNothing was written. scan_for_secrets cannot judge these: it searches for the\n\
         configured server, user and database, and a literal is none of those.\n\n\
         A fixture's SQL belongs in source, where it is reviewed before it is committed\n\
         and re-runnable after a driver bump. Add it to FIXTURES in\n\
         core/examples/dump_plan.rs:\n\n",
    );
    message.push_str(&format!("    ({name:?}, {sql:?}),\n\n"));
    message.push_str("then run `just dump-plans` with no arguments.");
    Err(message)
}

/// Values that must never appear in a committed fixture.
struct Secrets {
    server: String,
    username: String,
    database: String,
}

/// Return a description of the first secret found in `xml`, or `None` if clean.
///
/// Case-insensitive substring search. A configured value equal to
/// [`FIXTURE_DB`], or shorter than 4 characters, is skipped — `master` is not a
/// secret, and a very short value would false-positive on ordinary XML text.
/// The server value is also split on `,`/`:` so a `host,1433` form is matched on
/// the host alone.
// Only CONFIGURED values are searched for, so a literal carried in from the
// query being explained is invisible here — this cannot recognize a customer id
// it was never told about. `literals` and `permit` are what cover that, by
// refusing the ad-hoc capture rather than by searching.
fn scan_for_secrets(xml: &str, secrets: &Secrets) -> Option<String> {
    let haystack = xml.to_ascii_lowercase();
    let host = secrets
        .server
        .split([',', ':'])
        .next()
        .unwrap_or(&secrets.server);

    for (label, value) in [
        ("the configured database name", secrets.database.as_str()),
        ("the configured server name", host),
        ("the configured username", secrets.username.as_str()),
    ] {
        if value.len() < 4 || value.eq_ignore_ascii_case(FIXTURE_DB) {
            continue;
        }
        if haystack.contains(&value.to_ascii_lowercase()) {
            return Some(format!("{label} ({value:?})"));
        }
    }
    None
}

// ----------------------------------------------------------------- sanitizer

/// Attributes removed outright, keyed by the element carrying them. Each
/// describes the machine rather than the plan.
const REMOVED_ATTRIBUTES: &[(&str, &str)] = &[
    ("ShowPlanXML", "Build"),
    ("QueryPlan", "CachedPlanSize"),
    ("QueryPlan", "CompileCPU"),
    ("QueryPlan", "CompileMemory"),
    ("QueryPlan", "CompileTime"),
    ("StatisticsInfo", "LastUpdate"),
    ("StatisticsInfo", "ModificationCount"),
    ("StatisticsInfo", "SamplingPercent"),
];

/// Elements removed outright. Each describes the instance rather than the plan
/// — its memory grant, its buffer pool and CPU, the trace flags it runs with —
/// and carries nothing else, so stripping attributes would leave an empty
/// element behind.
const REMOVED_ELEMENTS: &[&str] = &[
    "MemoryGrantInfo",
    "OptimizerHardwareDependentProperties",
    "TraceFlags",
];

/// Every operator's own estimated cost, in hundredths.
const OWN_COST: i64 = 1;

/// Rows each operator is made to return, per operator in its subtree.
const ROWS_PER_OPERATOR: u64 = 100;

/// Rows read per row returned at or above which the input counts as reading far
/// more than it returns.
// This deliberately COPIES `verdict::WASTEFUL_READ_RATIO` rather than importing
// it. What a fixture must preserve should not move silently when the judge's
// provisional threshold moves; `the_preserved_ratio_clears_the_judges_thresholds`
// pins the relationship between the two instead.
const HIGH_READ_RATIO: f64 = 100.0;

/// Rows read per row returned written for an operator that cleared
/// [`HIGH_READ_RATIO`].
// Proportional to the returned rows rather than flat, so an operator that is not
// a leaf still comes out with a ratio worth having. On every real case — scans
// and seeks are leaves, so they return ROWS_PER_OPERATOR — this yields 25000.
const HIGH_READ_MULTIPLIER: u64 = 250;

/// How far the children's total must exceed the parent's before the parent
/// counts as inverted.
// NOT an exact `<`. A parent whose true own cost is zero can compare as
// inverted, because the binary sum of several decimal children can exceed the
// decimal parent. One such false positive presents an ordinary operator as the
// clamp specimen, and `own_cost_of_a_nested_loops_join_is_clamped_at_zero` then
// passes on the wrong node for good. Sized two-sided against the captured plans:
// their worst f64 summation error is below 1e-16 and the one real inversion is
// 1.08e-4. Above, the bound is the server's own printing — at seven significant
// digits a cost of order 1 quantizes near 1e-7. No captured parent overshoots,
// so this is prospective: the only two whose children total exactly equals them
// have one child, where the subtraction is exactly zero.
const INVERSION_MARGIN: f64 = 1e-6;

/// An input impact at or above this counts as worth reporting, and is written as
/// [`HIGH_IMPACT`] rather than [`LOW_IMPACT`].
// This copies `verdict::MISSING_INDEX_IMPACT` for the reason HIGH_READ_RATIO
// copies its counterpart. `the_two_impacts_straddle_the_judges_threshold` asserts
// the two are EQUAL rather than merely ordered: this classifies correctly only
// while it agrees with the value the judge reports on.
const IMPACT_THRESHOLD: f64 = 50.0;

/// The `MissingIndexGroup` impact written for an input at or above
/// [`IMPACT_THRESHOLD`].
// Two constants rather than one, for the reason the rows-read scheme keeps a
// ratio: flattening every impact to a single value makes the judge's
// below-threshold path unreachable from any captured fixture. Both sit 40 points
// off the threshold, so tuning it cannot silently reclassify a fixture.
const HIGH_IMPACT: &str = "90";

/// The `MissingIndexGroup` impact written for an input below
/// [`IMPACT_THRESHOLD`]. See [`HIGH_IMPACT`].
const LOW_IMPACT: &str = "10";

/// `EstimateIO` and `EstimateCPU`, which sum to [`OWN_COST`].
const SPLIT_COST: &str = "0.005";

/// `MemoryFractions`' `Input` and `Output`.
const MEMORY_FRACTION: &str = "0.5";

/// `AvgRowSize`, in bytes.
const AVG_ROW_SIZE: u64 = 100;

/// `TableCardinality`, which must stay above the rows read.
// Flat rather than derived from the rows read, because it cannot vary: only a
// leaf carries a rows-read figure (`sanitize` refuses a document where one does
// not), so the largest possible read is ROWS_PER_OPERATOR × HIGH_READ_MULTIPLIER.
// `a_table_holds_at_least_what_was_read_from_it` pins the two against each other.
const TABLE_CARDINALITY: u64 = 50_000;

/// The floor under `EstimateRowsWithoutRowGoal`, which must stay above the rows
/// the row goal cut the estimate down to.
const MIN_ROWS_WITHOUT_ROW_GOAL: u64 = 1_000;

/// Microsoft's spelling; see `core::plan::parse` for why this is a constant.
const ROWS_READ: &str = "EstimatedRowsRead";

/// What [`sanitize`] found, for the line it prints per capture.
#[derive(Debug, Default)]
struct Report {
    operators: usize,
    inversions_kept: usize,
    /// Operators that inverted in the input and could not be represented as
    /// inverted in the output. See [`plan_operator`] for when that happens.
    // No captured plan has produced one; the guard's only coverage is
    // `an_inversion_that_cannot_be_represented_is_reported`.
    inversions_dropped: Vec<String>,
    high_reads: usize,
}

/// One replacement or removal, as a byte range into the original document.
struct Edit {
    range: Range<usize>,
    replacement: String,
}

/// One operator's synthetic subtree cost, in hundredths, and the number of
/// operators in its subtree including itself.
struct Planned {
    subtree_cost: i64,
    operators: u64,
}

/// Replace every measurement in a captured ShowPlanXML document with a
/// synthetic round value, leaving the document's structure untouched.
///
/// Element nesting, operator names, wrapper elements, attribute spellings, the
/// namespace declaration and every `sys.*`/`master` object name survive
/// byte-for-byte; only numbers change. An attribute is rewritten only where the
/// server emitted it, so this never adds one. Returns the rewritten document
/// alongside a [`Report`] of what it found.
///
/// Three relationships in the input are detected and reproduced in the output,
/// because `core::plan`'s tests rest on them: an operator whose subtree cost
/// falls below its children's keeps a negative own cost, an operator reading far
/// more rows than it returns keeps a ratio at least as extreme, and a missing
/// index keeps which side of the impact threshold it fell on.
///
/// # Errors
///
/// Refuses, rather than sanitizing, a document naming a database outside
/// [`ALLOWED_DATABASES`] or naming a linked server — object names are not
/// rewritten, so a capture that reached beyond the catalog cannot be made safe
/// here and must not be written at all.
///
/// Also returns `Err` when the document is not well-formed XML, when the rewrite
/// would produce overlapping edits, when it did not reach every operator, when
/// an operator reports rows read but has children, when a statement kind carries
/// measurements this does not rewrite, or when the spliced output no longer
/// parses.
// Two consequences constrain what any fixture can prove from here on. A
// sanitized document's largest rows-read figure is 25000 and its largest
// statement cost is OWN_COST per operator, so the judge's volume and cost
// thresholds have only that much headroom below them however large the captured
// plan was — billz-3bz corrects the comments that claim more. And siblings of
// equal size now cost the same, so a test telling two children apart by their
// subtree costs goes vacuous the day a re-capture makes their subtrees match.
fn sanitize(xml: &str) -> Result<(String, Report), String> {
    let doc = Document::parse(xml).map_err(|e| format!("the plan XML did not parse: {e}"))?;
    let mut edits = Vec::new();
    let mut removed: Vec<Range<usize>> = Vec::new();
    let mut report = Report::default();

    // Removals are matched on element and attribute name across the whole
    // document. None of them sits under a `RelOp`: `Build` is on the root,
    // the compile figures on `QueryPlan`, the statistics ones on
    // `StatisticsInfo` under `OptimizerStatsUsage`.
    for n in doc.descendants().filter(Node::is_element) {
        let element = n.tag_name().name();
        if REMOVED_ELEMENTS.contains(&element) {
            removed.push(n.range());
            edits.push(Edit {
                range: with_leading_space(xml, n.range()),
                replacement: String::new(),
            });
            continue;
        }
        for (owner, attribute) in REMOVED_ATTRIBUTES {
            if element == *owner
                && let Some(a) = n.attributes().find(|a| a.name() == *attribute)
            {
                edits.push(Edit {
                    range: with_leading_space(xml, a.range()),
                    replacement: String::new(),
                });
            }
        }
        if element == "MemoryFractions" {
            replace(n, "Input", MEMORY_FRACTION.to_string(), &mut edits);
            replace(n, "Output", MEMORY_FRACTION.to_string(), &mut edits);
        }
        if element == "MissingIndexGroup" {
            let impact = if attr_f64(&n, "Impact") >= IMPACT_THRESHOLD {
                HIGH_IMPACT
            } else {
                LOW_IMPACT
            };
            replace(n, "Impact", impact.to_string(), &mut edits);
        }
    }

    // Neither identifier defense reaches an object the query named directly:
    // `scan_for_secrets` searches for CONFIGURED values, so it cannot recognize a
    // table it was never told about, and a three-part name reaches another
    // database without any `USE`. Four element kinds carry `Database` across the
    // fixtures — `ColumnReference`, `StatisticsInfo`, `Object`, `MissingIndex` —
    // so this asks the document, not a list of elements.
    if let Some(n) = doc.descendants().find(|n| {
        n.attribute("Database")
            .is_some_and(|db| !allowed_database(db))
    }) {
        return Err(format!(
            "<{}> names database {}, and a fixture may only name {}",
            n.tag_name().name(),
            n.attribute("Database").unwrap_or_default(),
            ALLOWED_DATABASES.join(" or ")
        ));
    }

    // A `Server` attribute means the plan crossed a linked server, where the
    // value is a real hostname and no allowlist can vet it. A local capture never
    // emits one, so refusing costs nothing.

    // A bare `Table` with no `Database` beside it names an object in the CURRENT
    // database, which the forcing to FIXTURE_DB already pins. Not overlooked —
    // there is nothing left for a check here to decide.
    if let Some(n) = doc.descendants().find(|n| n.attribute("Server").is_some()) {
        return Err(format!(
            "<{}> names linked server {}, which no allowlist can vet",
            n.tag_name().name(),
            n.attribute("Server").unwrap_or_default()
        ));
    }

    // Only a leaf may report rows read. The whole rows-read scheme rests on it:
    // the figure written is a multiple of the operator's own returned rows, so on
    // a subtree it would scale past the volume threshold the judge grades on and
    // every fixture would start reporting a large scan.
    if let Some(n) = doc.descendants().find(|n| {
        has_name(n, "RelOp") && n.attribute(ROWS_READ).is_some() && !child_operators(*n).is_empty()
    }) {
        return Err(format!(
            "operator {} reports rows read but has child operators",
            label(&n)
        ));
    }

    // Driven from every `QueryPlan`, not from the statements, so a batch whose
    // statements the parser skips — `StmtCond`, `StmtCursor` — still has its
    // operators rewritten. Each operator belongs to exactly one `QueryPlan`,
    // because `child_operators` stops at a nested one.
    let mut plans: Vec<(roxmltree::NodeId, Planned)> = Vec::new();
    for qp in doc.descendants().filter(|n| has_name(n, "QueryPlan")) {
        if let Some(root) = child(qp, "RelOp") {
            let planned = plan_operator(root, &mut edits, &mut report);
            plans.push((qp.id(), planned));
        }
    }

    for stmt in doc.descendants().filter(|n| has_name(n, "StmtSimple")) {
        // A statement with no plan of its own — a `SET` in the batch — has no
        // root to mirror, so both figures go to zero.
        let planned = child(stmt, "QueryPlan")
            .and_then(|qp| plans.iter().find(|(id, _)| *id == qp.id()))
            .map(|(_, p)| p);
        let (cost, rows) = match planned {
            Some(p) => (p.subtree_cost, ROWS_PER_OPERATOR * p.operators),
            None => (0, 0),
        };
        replace(stmt, "StatementSubTreeCost", cost_text(cost), &mut edits);
        replace(stmt, "StatementEstRows", rows.to_string(), &mut edits);
    }

    // Fail closed. An operator this did not reach keeps its measured costs and
    // row counts, and nothing downstream would notice.
    let operators = doc.descendants().filter(|n| has_name(n, "RelOp")).count();
    if operators != report.operators {
        return Err(format!(
            "the document has {operators} operators but {} were rewritten",
            report.operators
        ));
    }

    // Statement measurements are rewritten on `StmtSimple` only, because that is
    // the one statement kind carrying a plan this understands. Any other element
    // holding them would keep its measured figures at full precision.
    if let Some(n) = doc.descendants().find(|n| {
        n.is_element()
            && !has_name(n, "StmtSimple")
            && (n.attribute("StatementSubTreeCost").is_some()
                || n.attribute("StatementEstRows").is_some())
    }) {
        return Err(format!(
            "<{}> carries statement measurements this does not rewrite",
            n.tag_name().name()
        ));
    }

    let out = apply(xml, edits, &removed)?;
    check_output(&out, operators)?;
    Ok((out, report))
}

/// Confirm a spliced document still parses and still holds `operators`
/// operators.
///
/// # Errors
///
/// Returns `Err` if the document is not well-formed XML, or if the splice lost
/// or gained an operator.
// The splice writes byte ranges into the original, and `replace` rests on
// roxmltree's documented caveat about where an attribute value starts. A bad
// range would otherwise reach a committed fixture: `scan_for_secrets` reads the
// bytes as text and would not notice they had stopped being XML. Nothing drives
// this from `sanitize` today — it takes a bug in `replace` to fire — so the
// tests call it directly rather than pretending the path is covered.
fn check_output(out: &str, operators: usize) -> Result<(), String> {
    let doc =
        Document::parse(out).map_err(|e| format!("the sanitized document did not parse: {e}"))?;
    let found = doc.descendants().filter(|n| has_name(n, "RelOp")).count();
    if found != operators {
        return Err(format!(
            "the sanitized document has {found} operators, not {operators}"
        ));
    }
    Ok(())
}

/// Rewrite one operator and its subtree, returning what it was given.
///
/// Recursive because it walks OPERATOR depth, which the optimizer keeps small —
/// the same argument `core::plan::parse` makes at `rel_op`.
fn plan_operator(n: Node, edits: &mut Vec<Edit>, report: &mut Report) -> Planned {
    let children = child_operators(n);
    let planned: Vec<Planned> = children
        .iter()
        .map(|c| plan_operator(*c, edits, report))
        .collect();

    let operators = 1 + planned.iter().map(|p| p.operators).sum::<u64>();
    let children_cost: i64 = planned.iter().map(|p| p.subtree_cost).sum();

    // [`INVERSION_MARGIN`] covers noise, not absence: `attr_f64` defaults a
    // missing cost to zero, so a parent without the attribute whose children
    // carry it reads as inverted by their full total. The ShowPlanXML schema
    // makes the attribute required, which is what puts that out of reach.
    let measured = attr_f64(&n, "EstimatedTotalSubtreeCost");
    let measured_children: f64 = children
        .iter()
        .map(|c| attr_f64(c, "EstimatedTotalSubtreeCost"))
        .sum();
    let inverted = !children.is_empty() && measured_children - measured > INVERSION_MARGIN;

    // A leaf can never invert, so an inverted operator always has children and
    // its children's total is at least OWN_COST. Requiring one OWN_COST of
    // headroom on top keeps every subtree cost positive: without it a chain of
    // single-child inversions walks the total down through zero and past it,
    // which is neither valid-looking nor something the parser should meet.
    let keeps_inversion = inverted && children_cost - OWN_COST >= OWN_COST;
    if inverted && !keeps_inversion {
        report.inversions_dropped.push(label(&n));
    }
    if keeps_inversion {
        report.inversions_kept += 1;
    }
    let subtree_cost = children_cost + if keeps_inversion { -OWN_COST } else { OWN_COST };
    let est_rows = ROWS_PER_OPERATOR * operators;

    let rows_read = n.attribute(ROWS_READ).map(|read| {
        let returned = attr_f64(&n, "EstimateRows").max(1.0);
        if read.parse::<f64>().unwrap_or(0.0) / returned >= HIGH_READ_RATIO {
            report.high_reads += 1;
            est_rows * HIGH_READ_MULTIPLIER
        } else {
            est_rows
        }
    });

    replace(
        n,
        "EstimatedTotalSubtreeCost",
        cost_text(subtree_cost),
        edits,
    );
    replace(n, "EstimateRows", est_rows.to_string(), edits);
    replace(n, "EstimateIO", SPLIT_COST.to_string(), edits);
    replace(n, "EstimateCPU", SPLIT_COST.to_string(), edits);
    replace(n, "AvgRowSize", AVG_ROW_SIZE.to_string(), edits);
    replace(n, "EstimateRebinds", "0".to_string(), edits);
    replace(n, "EstimateRewinds", "0".to_string(), edits);
    if let Some(read) = rows_read {
        replace(n, ROWS_READ, read.to_string(), edits);
    }
    // Both floors keep a relationship the input had: a table holds at least what
    // the operator read from it, and a row goal only ever cuts an estimate down.
    // The floor under the row-goal figure is a real one — a row goal over a
    // subtree of more than ten operators returns more than the floor.
    replace(n, "TableCardinality", TABLE_CARDINALITY.to_string(), edits);
    let without_row_goal = est_rows.max(MIN_ROWS_WITHOUT_ROW_GOAL);
    replace(
        n,
        "EstimateRowsWithoutRowGoal",
        without_row_goal.to_string(),
        edits,
    );

    report.operators += 1;
    Planned {
        subtree_cost,
        operators,
    }
}

/// Splice every edit into `xml`, dropping any that falls inside a removed
/// element.
///
/// # Errors
///
/// Returns `Err` if two edits overlap, which would corrupt the document.
fn apply(xml: &str, mut edits: Vec<Edit>, removed: &[Range<usize>]) -> Result<String, String> {
    edits.retain(|e| {
        !removed
            .iter()
            .any(|r| r.start < e.range.start && e.range.end <= r.end)
    });
    edits.sort_by_key(|e| e.range.start);

    let mut out = String::with_capacity(xml.len());
    let mut cursor = 0;
    for edit in &edits {
        if edit.range.start < cursor {
            return Err(format!("edits overlap at byte {}", edit.range.start));
        }
        out.push_str(&xml[cursor..edit.range.start]);
        out.push_str(&edit.replacement);
        cursor = edit.range.end;
    }
    out.push_str(&xml[cursor..]);
    Ok(out)
}

/// Queue a replacement for `name`'s value, or nothing at all when the element
/// does not carry it.
fn replace(n: Node, name: &str, value: String, edits: &mut Vec<Edit>) {
    if let Some(a) = n.attributes().find(|a| a.name() == name) {
        edits.push(Edit {
            range: a.range_value(),
            replacement: value,
        });
    }
}

/// Extend a range left over the whitespace in front of it, so removing an
/// attribute or element does not leave a gap where it stood.
fn with_leading_space(xml: &str, range: Range<usize>) -> Range<usize> {
    let bytes = xml.as_bytes();
    let mut start = range.start;
    while start > 0 && bytes[start - 1].is_ascii_whitespace() {
        start -= 1;
    }
    start..range.end
}

/// Hundredths as the two-decimal figure ShowPlanXML costs are written in.
fn cost_text(hundredths: i64) -> String {
    format!("{:.2}", hundredths as f64 / 100.0)
}

/// Whether a ShowPlanXML `Database` attribute value is in
/// [`ALLOWED_DATABASES`], bracket-quoted or bare, case-insensitively.
// Unquoting one leading `[` and one trailing `]` rather than reusing
// `fingerprint`'s `strip_database`, which drops the leading component of a joined
// `[db].[schema].[table]` and does not apply to a lone attribute value. ONE
// bracket each side, not `trim_matches`: SQL Server doubles a `]` inside an
// identifier, so a database really named `master]` arrives as `[master]]]` and
// must keep its stray `]` to compare unequal here. Trimming greedily would
// accept it. `a_database_whose_name_contains_a_bracket_is_refused` pins that.
fn allowed_database(database: &str) -> bool {
    let unquoted = database
        .strip_prefix('[')
        .and_then(|n| n.strip_suffix(']'))
        .unwrap_or(database);
    ALLOWED_DATABASES
        .iter()
        .any(|allowed| unquoted.eq_ignore_ascii_case(allowed))
}

/// Name an operator well enough for a warning to be actionable.
fn label(n: &Node) -> String {
    format!(
        "{} ({})",
        n.attribute("NodeId").unwrap_or("?"),
        n.attribute("PhysicalOp").unwrap_or("unknown")
    )
}

// The four helpers below mirror `core::plan::parse`'s private ones. Child
// discovery in particular MUST agree with it: the operator counts this writes
// are the ones its tests read back.

fn has_name(n: &Node, local_name: &str) -> bool {
    n.is_element() && n.tag_name().name() == local_name
}

fn child<'a, 'i>(n: Node<'a, 'i>, local_name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| has_name(c, local_name))
}

fn attr_f64(n: &Node, name: &str) -> f64 {
    n.attribute(name)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

/// The immediate child operators of `n`, stopping at a nested `RelOp` — which
/// owns its own subtree — and at a nested `QueryPlan`, which is another
/// statement's operators.
fn child_operators<'a, 'i>(n: Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    let mut found = Vec::new();
    let mut stack: Vec<Node<'a, 'i>> = n.children().filter(Node::is_element).collect();
    stack.reverse();
    while let Some(c) = stack.pop() {
        if has_name(&c, "RelOp") {
            found.push(c);
            continue;
        }
        if has_name(&c, "QueryPlan") {
            continue;
        }
        let mut grandchildren: Vec<Node<'a, 'i>> = c.children().filter(Node::is_element).collect();
        grandchildren.reverse();
        stack.extend(grandchildren);
    }
    found
}

/// Build a connection from the same `MSSQL_*` vars the integration tests use.
/// The password goes into an in-memory store — this never touches the Keychain.
/// Also returns the values [`scan_for_secrets`] must look for.
fn env_connection() -> Option<(ConnectionConfig, InMemorySecretStore, Secrets)> {
    let server = env::var("MSSQL_SERVER").ok()?;
    let username = env::var("MSSQL_USER").ok()?;
    let database = env::var("MSSQL_DATABASE").ok()?;

    let cfg = ConnectionConfig {
        id: ConnectionId("dump-plan".into()),
        name: "dump-plan".into(),
        server: server.clone(),
        username: username.clone(),
        // The connection's default database; every capture overrides it with
        // FIXTURE_DB via the ExecutionContext.
        default_database: Some(database.clone()),
        encrypt: false,
        trust_server_certificate: true,
        remember_password: false,
    };
    let store = InMemorySecretStore::default();
    store
        .set_password(&cfg.id, &env::var("MSSQL_PASSWORD").ok()?)
        .ok()?;
    Some((
        cfg,
        store,
        Secrets {
            server,
            username,
            database,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets() -> Secrets {
        Secrets {
            server: "dev-sql-01,1433".into(),
            username: "sql_reader".into(),
            database: "Contoso_SE_DEV".into(),
        }
    }

    #[test]
    fn a_clean_master_plan_passes() {
        let xml = r#"<Object Database="[master]" Schema="[sys]" Table="[objects]" />"#;
        assert_eq!(scan_for_secrets(xml, &secrets()), None);
    }

    #[test]
    fn a_tenant_database_name_is_caught_case_insensitively() {
        let xml = r#"<Object Database="[contoso_se_dev]" Schema="[dbo]" Table="[Orders]" />"#;
        assert!(
            scan_for_secrets(xml, &secrets()).is_some_and(|h| h.contains("database")),
            "a tenant database name must never reach a committed fixture"
        );
    }

    #[test]
    fn the_server_host_is_caught_without_its_port() {
        let xml = "<!-- captured from dev-sql-01 -->";
        assert!(scan_for_secrets(xml, &secrets()).is_some_and(|h| h.contains("server")));
    }

    #[test]
    fn the_username_is_caught() {
        let xml = "<ShowPlanXML><!-- sql_reader --></ShowPlanXML>";
        assert!(scan_for_secrets(xml, &secrets()).is_some_and(|h| h.contains("username")));
    }

    #[test]
    fn master_is_not_treated_as_a_secret_even_when_configured() {
        // MSSQL_DATABASE=master must not make every fixture fail the scan.
        let s = Secrets {
            server: "somehost".into(),
            username: "someuser".into(),
            database: "master".into(),
        };
        let xml = r#"<Object Database="[master]" />"#;
        assert_eq!(scan_for_secrets(xml, &s), None);
    }

    #[test]
    fn a_short_value_is_skipped_to_avoid_false_positives() {
        // The short value has to OCCUR in the haystack, or the length guard is
        // not what makes this pass and the test holds whether the guard is there
        // or not. `sys` appears in the document, so dropping the guard turns
        // every plan into a hit.
        let s = Secrets {
            server: "db".into(),
            username: "sa".into(),
            database: "sys".into(),
        };
        assert_eq!(scan_for_secrets(r#"<Object Schema="[sys]"/>"#, &s), None);
    }

    // ------------------------------------------------------------- sanitizer

    // Hand-authored, and written on ONE line because that is how the server
    // sends a document — `just fmt-plans` pretty-prints only after this has run.
    // It carries every attribute and element in the strip lists, an operator
    // whose subtree cost falls below its children's (node 1), one that reads far
    // more than it returns (node 2) and one that does not (node 3), and both
    // `xs:boolean` spellings.
    const DIRTY: &str = concat!(
        r#"<?xml version="1.0"?><ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan" Version="1.539" Build="15.0.4123.1">"#,
        r#"<BatchSequence><Batch><Statements>"#,
        r#"<StmtSimple StatementText="SELECT TOP 10 name FROM sys.objects" StatementSubTreeCost="0.0132877" StatementEstRows="9.41176" RetrievedFromCache="false" SecurityPolicyApplied="false">"#,
        r#"<QueryPlan CachedPlanSize="32" CompileTime="11" CompileCPU="11" CompileMemory="1096">"#,
        r#"<MemoryGrantInfo SerialRequiredMemory="0" SerialDesiredMemory="1024" GrantedMemory="2048" MaxUsedMemory="512"/>"#,
        r#"<OptimizerHardwareDependentProperties EstimatedAvailableMemoryGrant="838972" EstimatedPagesCached="209743" EstimatedAvailableDegreeOfParallelism="4" MaxCompileMemory="7264080"/>"#,
        r#"<OptimizerStatsUsage><StatisticsInfo LastUpdate="2026-08-19T03:14:15.92" ModificationCount="6543" SamplingPercent="63.4218" Statistics="[nc1]" Table="[sysschobjs]" Schema="[sys]" Database="[master]"/></OptimizerStatsUsage>"#,
        r#"<RelOp NodeId="0" PhysicalOp="Top" LogicalOp="Top" EstimateRows="9.41176" EstimateIO="0" EstimateCPU="1.8e-07" AvgRowSize="139" EstimatedTotalSubtreeCost="1" Parallel="0" EstimateRebinds="0" EstimateRewinds="0"><Top RowCount="1">"#,
        r#"<RelOp NodeId="1" PhysicalOp="Nested Loops" LogicalOp="Left Outer Join" EstimateRows="9.41176" EstimateRowsWithoutRowGoal="2577" EstimateIO="0" EstimateCPU="4.18e-05" AvgRowSize="152" EstimatedTotalSubtreeCost="0.9" Parallel="0" EstimateRebinds="8.41176" EstimateRewinds="0"><NestedLoops Optimized="0">"#,
        r#"<MemoryFractions Input="0.982143" Output="0.0178571"/>"#,
        r#"<RelOp NodeId="2" PhysicalOp="Index Scan" LogicalOp="Index Scan" EstimateRows="12.4" EstimatedRowsRead="24680" EstimateIO="0.0074537" EstimateCPU="0.0029153" AvgRowSize="145" EstimatedTotalSubtreeCost="0.8" TableCardinality="24680" Parallel="0" EstimateRebinds="0" EstimateRewinds="0">"#,
        r#"<IndexScan Ordered="0" ForceSeek="0" ForcedIndex="0"><Object Database="[master]" Schema="[sys]" Table="[sysschobjs]" Index="[nc1]"/></IndexScan></RelOp>"#,
        r#"<RelOp NodeId="3" PhysicalOp="Clustered Index Seek" LogicalOp="Clustered Index Seek" EstimateRows="1" EstimatedRowsRead="1" EstimateIO="0.003125" EstimateCPU="0.0001581" AvgRowSize="9" EstimatedTotalSubtreeCost="0.15" TableCardinality="2577" Parallel="0" EstimateRebinds="0" EstimateRewinds="0">"#,
        r#"<IndexScan Ordered="1" ForceSeek="0" ForcedIndex="0"><Object Database="[master]" Schema="[sys]" Table="[sysschobjs]" Index="[clst]"/></IndexScan></RelOp>"#,
        r#"</NestedLoops></RelOp></Top></RelOp>"#,
        r#"</QueryPlan></StmtSimple></Statements></Batch></BatchSequence></ShowPlanXML>"#,
    );

    // The five REAL captures the parser and the judge are tested against. Read
    // here, never written — `sanitize(fixture) == fixture` is only worth
    // asserting because these files were scrubbed by hand, independently of the
    // code under test. Regenerating them from this branch would turn that check
    // into a tautology.
    const FIXTURE_FILES: &[&str] = &[
        "seek.sqlplan",
        "scan.sqlplan",
        "join.sqlplan",
        "aggregate.sqlplan",
        "two-statements.sqlplan",
    ];

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/plans/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_else(|e| panic!("fixture {name}: {e}"))
    }

    fn sanitized(xml: &str) -> String {
        sanitize(xml).expect("sanitizing must succeed").0
    }

    fn report_of(xml: &str) -> Report {
        sanitize(xml).expect("sanitizing must succeed").1
    }

    // A plan node's raw own cost, re-derived exactly as `core::plan::parse` does
    // before its clamp. Below zero is the inversion the clamp exists for.
    fn raw_own_cost(n: &coot_core::PlanNode) -> f64 {
        n.subtree_cost - n.children.iter().map(|c| c.subtree_cost).sum::<f64>()
    }

    fn operators(n: &coot_core::PlanNode) -> Vec<String> {
        let mut out = vec![format!("{}/{}", n.physical_op, n.logical_op)];
        for c in &n.children {
            out.extend(operators(c));
        }
        out
    }

    fn every_node(n: &coot_core::PlanNode) -> Vec<&coot_core::PlanNode> {
        let mut out = vec![n];
        for c in &n.children {
            out.extend(every_node(c));
        }
        out
    }

    fn roots(xml: &str) -> Vec<coot_core::PlanNode> {
        coot_core::parse_plan(xml)
            .expect("must parse")
            .statements
            .into_iter()
            .filter_map(|s| s.root)
            .collect()
    }

    #[test]
    fn every_server_measurement_attribute_is_stripped() {
        let out = sanitized(DIRTY);
        for name in [
            "Build",
            "CachedPlanSize",
            "CompileTime",
            "CompileCPU",
            "CompileMemory",
            "LastUpdate",
            "ModificationCount",
            "SamplingPercent",
            "SerialRequiredMemory",
            "GrantedMemory",
            "EstimatedAvailableMemoryGrant",
            "EstimatedPagesCached",
            "EstimatedAvailableDegreeOfParallelism",
            "MaxCompileMemory",
        ] {
            assert!(!out.contains(name), "{name} survived sanitization");
        }
        for element in REMOVED_ELEMENTS {
            assert!(!out.contains(element), "<{element}> survived sanitization");
        }
    }

    #[test]
    fn stripping_leaves_the_plan_itself_intact() {
        // The removals must take the measurements and nothing else: the
        // structure is the whole reason a captured fixture beats a written one.
        let out = sanitized(DIRTY);
        for kept in [
            r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan" Version="1.539">"#,
            r#"<StatisticsInfo Statistics="[nc1]" Table="[sysschobjs]" Schema="[sys]" Database="[master]"/>"#,
            r#"<QueryPlan>"#,
            r#"<Object Database="[master]" Schema="[sys]" Table="[sysschobjs]" Index="[clst]"/>"#,
            r#"PhysicalOp="Nested Loops" LogicalOp="Left Outer Join""#,
            r#"<Top RowCount="1">"#,
        ] {
            assert!(out.contains(kept), "sanitization lost {kept}");
        }
    }

    #[test]
    fn measured_values_are_replaced_with_round_ones() {
        let out = sanitized(DIRTY);
        for measured in [
            "0.0132877",
            "9.41176",
            "1.8e-07",
            "139",
            "0.0074537",
            "24680",
            "2577",
            "0.982143",
            "0.0178571",
            "8.41176",
            "0.0001581",
        ] {
            assert!(!out.contains(measured), "{measured} survived sanitization");
        }
        for round in [
            r#"EstimateIO="0.005""#,
            r#"EstimateCPU="0.005""#,
            r#"AvgRowSize="100""#,
            r#"TableCardinality="50000""#,
            r#"EstimateRowsWithoutRowGoal="1000""#,
            r#"EstimateRebinds="0""#,
            r#"<MemoryFractions Input="0.5" Output="0.5"/>"#,
            r#"StatementSubTreeCost="0.02""#,
            r#"StatementEstRows="400""#,
        ] {
            assert!(out.contains(round), "sanitization did not write {round}");
        }
    }

    #[test]
    fn formatting_is_pinned_per_attribute() {
        // A single `{:.2}` for every number renders 0.005 as "0.01" and 0.5 as
        // "0.50", and the fixtures stop round-tripping on the first file.
        assert_eq!(cost_text(1), "0.01");
        assert_eq!(cost_text(32), "0.32");
        assert_eq!(cost_text(0), "0.00");
        assert_eq!(SPLIT_COST, "0.005");
        assert_eq!(MEMORY_FRACTION, "0.5");
    }

    #[test]
    fn an_inverted_parent_cost_survives_sanitization() {
        // THE test that a naive scheme fails. Give every operator the same own
        // cost and a parent's subtree cost always exceeds its children's, which
        // silently erases the case `core::plan::parse`'s clamp exists for and
        // leaves `own_cost_of_a_nested_loops_join_is_clamped_at_zero` asserting
        // a zero that the traversal would produce anyway.
        let out = sanitized(DIRTY);
        let root = &roots(&out)[0];
        assert_eq!(root.physical_op, "Top");

        let loops = &root.children[0];
        assert_eq!(loops.physical_op, "Nested Loops");
        let raw = raw_own_cost(loops);
        assert!(raw < 0.0, "the inversion was flattened: raw own cost {raw}");
        assert_eq!(report_of(DIRTY).inversions_kept, 1);
    }

    #[test]
    fn an_uninverted_tree_gains_no_inversion() {
        // Over-detection is as wrong as under-detection: an invented inversion
        // presents an ordinary operator as the clamp specimen.
        let xml = concat!(
            r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan"><BatchSequence><Batch><Statements>"#,
            r#"<StmtSimple StatementText="SELECT 1" StatementSubTreeCost="0.9" StatementEstRows="3"><QueryPlan>"#,
            r#"<RelOp NodeId="0" PhysicalOp="Hash Match" LogicalOp="Inner Join" EstimateRows="3" EstimatedTotalSubtreeCost="0.9">"#,
            r#"<Hash><RelOp NodeId="1" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="2" EstimatedTotalSubtreeCost="0.6"/>"#,
            r#"<RelOp NodeId="2" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="1" EstimatedTotalSubtreeCost="0.3"/>"#,
            r#"</Hash></RelOp></QueryPlan></StmtSimple></Statements></Batch></BatchSequence></ShowPlanXML>"#,
        );
        let report = report_of(xml);
        assert_eq!(report.inversions_kept, 0);
        assert!(report.inversions_dropped.is_empty());

        for n in every_node(&roots(&sanitized(xml))[0]) {
            let raw = raw_own_cost(n);
            assert!(raw >= 0.0, "{} gained an inversion ({raw})", n.physical_op);
            assert!(n.subtree_cost > 0.0, "{} lost its cost", n.physical_op);
        }
    }

    #[test]
    fn a_parent_equal_to_its_children_is_not_treated_as_inverted() {
        // 0.659 + 0.48548 is exactly 1.14448 in decimal and 1.1444800000000002
        // in binary floating point, so an exact `<` calls this parent inverted.
        // Its own cost is really zero, and the fixture would go on to present an
        // ordinary Hash Match as the clamp specimen — passing the clamp test,
        // forever, on a node that never inverted.
        let xml = concat!(
            r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan"><BatchSequence><Batch><Statements>"#,
            r#"<StmtSimple StatementText="SELECT 1" StatementSubTreeCost="1.14448" StatementEstRows="3"><QueryPlan>"#,
            r#"<RelOp NodeId="0" PhysicalOp="Hash Match" LogicalOp="Inner Join" EstimateRows="3" EstimatedTotalSubtreeCost="1.14448">"#,
            r#"<Hash><RelOp NodeId="1" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="2" EstimatedTotalSubtreeCost="0.659"/>"#,
            r#"<RelOp NodeId="2" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="1" EstimatedTotalSubtreeCost="0.48548"/>"#,
            r#"</Hash></RelOp></QueryPlan></StmtSimple></Statements></Batch></BatchSequence></ShowPlanXML>"#,
        );
        assert_eq!(report_of(xml).inversions_kept, 0);
        assert!(raw_own_cost(&roots(&sanitized(xml))[0]) >= 0.0);
    }

    #[test]
    fn an_inversion_that_cannot_be_represented_is_reported() {
        // A single-child inversion has no room: taking an own cost off its one
        // child's 0.01 lands on zero, and a chain of them goes negative. The
        // guard drops the inversion and says so rather than writing a subtree
        // cost the parser should never meet.
        let xml = concat!(
            r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan"><BatchSequence><Batch><Statements>"#,
            r#"<StmtSimple StatementText="SELECT 1" StatementSubTreeCost="0.5" StatementEstRows="1"><QueryPlan>"#,
            r#"<RelOp NodeId="0" PhysicalOp="Top" LogicalOp="Top" EstimateRows="1" EstimatedTotalSubtreeCost="0.5">"#,
            r#"<Top><RelOp NodeId="1" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="1" EstimatedTotalSubtreeCost="0.9"/>"#,
            r#"</Top></RelOp></QueryPlan></StmtSimple></Statements></Batch></BatchSequence></ShowPlanXML>"#,
        );
        let report = report_of(xml);
        assert_eq!(report.inversions_kept, 0);
        assert_eq!(report.inversions_dropped, vec!["0 (Top)".to_string()]);

        // And the output is still sane: every cost positive.
        for n in every_node(&roots(&sanitized(xml))[0]) {
            assert!(n.subtree_cost > 0.0, "{} lost its cost", n.physical_op);
        }
    }

    #[test]
    fn a_high_read_to_return_ratio_survives_sanitization() {
        // The evidence that a raw returned-row threshold cannot see a wasteful
        // scan. Normalize it away and `verdict`'s ratio has no specimen left.
        let out = sanitized(DIRTY);
        let tree = roots(&out);
        let scan = every_node(&tree[0])
            .into_iter()
            .find(|n| n.physical_op == "Index Scan")
            .expect("the dirty document has an Index Scan");

        let read = scan.est_rows_read.expect("a scan reports its rows read");
        assert!(
            read / scan.est_rows >= HIGH_READ_RATIO,
            "{read} read to return {} is not a preserved ratio",
            scan.est_rows
        );
        assert_eq!(report_of(DIRTY).high_reads, 1);
    }

    #[test]
    fn an_ordinary_read_does_not_become_wasteful() {
        // The other direction: waste must not be invented on an operator that
        // read what it returned, or every fixture grows a false specimen.
        let out = sanitized(DIRTY);
        let tree = roots(&out);
        let seek = every_node(&tree[0])
            .into_iter()
            .find(|n| n.physical_op == "Clustered Index Seek")
            .expect("the dirty document has a seek");
        assert_eq!(seek.est_rows_read, Some(seek.est_rows));
    }

    #[test]
    fn the_preserved_ratio_clears_the_judges_thresholds() {
        // Pins the relationship WITHOUT coupling behavior: the preserved ratio
        // has to be extreme enough for the judge to call it waste, while the
        // rows read stay under the volume gate that runs first — or the
        // fixtures would start producing findings.
        use coot_core::plan::verdict::{LARGE_SCAN_ROWS_READ, WASTEFUL_READ_RATIO};

        assert!(HIGH_READ_RATIO >= WASTEFUL_READ_RATIO);
        assert!(HIGH_READ_MULTIPLIER as f64 >= WASTEFUL_READ_RATIO);
        assert!(((ROWS_PER_OPERATOR * HIGH_READ_MULTIPLIER) as f64) < LARGE_SCAN_ROWS_READ);
    }

    #[test]
    fn a_table_holds_at_least_what_was_read_from_it() {
        // `TableCardinality` is flat, so the relationship it has to keep lives
        // between the constants rather than in a per-operator expression.
        assert!(TABLE_CARDINALITY >= ROWS_PER_OPERATOR * HIGH_READ_MULTIPLIER);
    }

    #[test]
    fn a_rows_read_figure_on_a_subtree_is_refused() {
        // Never seen: all 19 rows-read operators across the fixtures are leaves,
        // as ShowPlanXML's data-access operators are. If one ever is not, the
        // rows-read figure scales with the subtree and walks into the judge's
        // volume threshold, so this aborts the capture rather than writing a
        // fixture that reports a large scan on nothing.
        let xml = concat!(
            r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan"><BatchSequence><Batch><Statements>"#,
            r#"<StmtSimple StatementText="SELECT 1" StatementSubTreeCost="1" StatementEstRows="1"><QueryPlan>"#,
            r#"<RelOp NodeId="0" PhysicalOp="Index Scan" LogicalOp="Index Scan" EstimateRows="1" EstimatedRowsRead="900" EstimatedTotalSubtreeCost="1">"#,
            r#"<IndexScan><RelOp NodeId="1" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="1" EstimatedTotalSubtreeCost="0.5"/></IndexScan>"#,
            r#"</RelOp></QueryPlan></StmtSimple></Statements></Batch></BatchSequence></ShowPlanXML>"#,
        );
        let err = sanitize(xml).unwrap_err();
        assert!(
            err.contains("rows read but has child operators"),
            "got {err}"
        );
    }

    #[test]
    fn a_row_goal_over_a_large_subtree_keeps_its_own_figure() {
        // The floor under `EstimateRowsWithoutRowGoal` is not decorative: past
        // ten operators the subtree's own row count wins, and replacing the
        // expression with the constant would write a figure BELOW the rows the
        // operator returns — a row goal that raised an estimate.
        let mut children = String::new();
        for id in 1..=12 {
            children.push_str(&format!(
                r#"<RelOp NodeId="{id}" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="1" EstimatedTotalSubtreeCost="0.1"/>"#
            ));
        }
        let xml = format!(
            concat!(
                r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan"><BatchSequence><Batch><Statements>"#,
                r#"<StmtSimple StatementText="SELECT 1" StatementSubTreeCost="9" StatementEstRows="9"><QueryPlan>"#,
                r#"<RelOp NodeId="0" PhysicalOp="Concatenation" LogicalOp="Concatenation" EstimateRows="9" EstimateRowsWithoutRowGoal="77" EstimatedTotalSubtreeCost="9">"#,
                r#"<Concat>{children}</Concat></RelOp></QueryPlan></StmtSimple></Statements></Batch></BatchSequence></ShowPlanXML>"#,
            ),
            children = children
        );
        let out = sanitized(&xml);
        // 13 operators → 1300 rows returned, so the floor of 1000 must not win.
        assert!(out.contains(r#"EstimateRows="1300""#), "got {out}");
        assert!(
            out.contains(r#"EstimateRowsWithoutRowGoal="1300""#),
            "the row-goal figure fell below the rows returned: {out}"
        );
    }

    #[test]
    fn both_boolean_encodings_survive_untouched() {
        // `core::plan::parse` reads `NoJoinPredicate` in either `xs:boolean`
        // spelling because this server writes both in one document, and the
        // fixtures are the evidence for that claim. Every one of these looks
        // numeric to a sweep that replaces attributes by value shape.
        let out = sanitized(DIRTY);
        for kept in [
            r#"Parallel="0""#,
            r#"ForceSeek="0""#,
            r#"Ordered="1""#,
            r#"Optimized="0""#,
            r#"RetrievedFromCache="false""#,
            r#"SecurityPolicyApplied="false""#,
        ] {
            assert!(out.contains(kept), "sanitization changed {kept}");
        }
    }

    #[test]
    fn a_statement_with_no_plan_of_its_own_is_zeroed() {
        let xml = concat!(
            r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan"><BatchSequence><Batch><Statements>"#,
            r#"<StmtSimple StatementText="SET NOCOUNT ON" StatementSubTreeCost="0.0031" StatementEstRows="1.7"/>"#,
            r#"</Statements></Batch></BatchSequence></ShowPlanXML>"#,
        );
        let out = sanitized(xml);
        assert!(out.contains(r#"StatementSubTreeCost="0.00""#), "got {out}");
        assert!(out.contains(r#"StatementEstRows="0""#), "got {out}");
    }

    #[test]
    fn sanitizing_a_committed_fixture_changes_nothing() {
        // Idempotence, and the evidence that this scheme IS the hand scrub
        // rather than merely resembling it. See FIXTURE_FILES for why these
        // files must not be regenerated from this branch.
        for name in FIXTURE_FILES {
            let before = fixture(name);
            let after = sanitized(&before);
            assert_eq!(after, before, "{name} is not already sanitized");
        }
    }

    #[test]
    fn sanitizing_twice_is_the_same_as_once() {
        let once = sanitized(DIRTY);
        assert_eq!(sanitized(&once), once);
    }

    #[test]
    fn sanitized_output_still_parses_to_the_same_shape() {
        // The proof that matters: unparseable output would otherwise surface
        // only on the DEV box, after the capture it was meant to protect.
        for xml in FIXTURE_FILES
            .iter()
            .map(|n| fixture(n))
            .chain([DIRTY.to_string()])
        {
            let before = coot_core::parse_plan(&xml).expect("input must parse");
            let after = coot_core::parse_plan(&sanitized(&xml)).expect("output must parse");

            assert_eq!(before.statements.len(), after.statements.len());
            for (b, a) in before.statements.iter().zip(&after.statements) {
                assert_eq!(b.text, a.text);
                assert_eq!(b.warnings, a.warnings);
                assert_eq!(b.missing_indexes, a.missing_indexes);
                match (&b.root, &a.root) {
                    (Some(b), Some(a)) => {
                        assert_eq!(operators(b), operators(a));
                        let objects = |n| {
                            every_node(n)
                                .into_iter()
                                .map(|n| n.object.clone())
                                .collect::<Vec<_>>()
                        };
                        assert_eq!(objects(b), objects(a));
                    }
                    (b, a) => assert_eq!(b.is_some(), a.is_some()),
                }
            }
        }
    }

    // A missing-index recommendation, the one plan construct that names tables
    // and columns of the database being queried. Schema-derived: no captured
    // plan contains `<MissingIndexes>`, because `sys.*` views do not generate
    // them.
    fn missing_index_document(impact: &str, database: &str) -> String {
        format!(
            concat!(
                r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan"><BatchSequence><Batch><Statements>"#,
                r#"<StmtSimple StatementText="SELECT * FROM Orders" StatementSubTreeCost="1.5" StatementEstRows="7"><QueryPlan>"#,
                r#"<MissingIndexes><MissingIndexGroup Impact="{impact}">"#,
                r#"<MissingIndex Database="{database}" Schema="[dbo]" Table="[Orders]">"#,
                r#"<ColumnGroup Usage="EQUALITY"><Column Name="[ShipCity]" ColumnId="5"/></ColumnGroup>"#,
                r#"</MissingIndex></MissingIndexGroup></MissingIndexes>"#,
                r#"<RelOp NodeId="0" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="7" EstimatedTotalSubtreeCost="1.5"/>"#,
                r#"</QueryPlan></StmtSimple></Statements></Batch></BatchSequence></ShowPlanXML>"#,
            ),
            impact = impact,
            database = database
        )
    }

    fn missing_index_findings(xml: &str) -> Vec<coot_core::FindingKind> {
        coot_core::judge_plan(&coot_core::parse_plan(xml).expect("must parse"))
            .findings
            .into_iter()
            .map(|f| f.kind)
            .filter(|k| *k == coot_core::FindingKind::MissingIndex)
            .collect()
    }

    #[test]
    fn a_missing_index_keeps_which_side_of_the_threshold_it_fell_on() {
        // Flattening every impact to one constant would make the judge's
        // below-threshold path unreachable from any captured fixture, the same
        // way a flat rows-read figure would erase the wasteful-read evidence.
        // "50" is the BOUNDARY: a finding under `verdict.rs`, whose own test
        // pins that it fires at the constant. A classifier using `>` writes it
        // as low and the fixture stops reporting an index the judge would.
        for measured in ["99.5061", "50"] {
            let high = sanitized(&missing_index_document(measured, "[master]"));
            assert!(high.contains(r#"Impact="90""#), "{measured}: got {high}");
            assert_eq!(
                missing_index_findings(&high),
                vec![coot_core::FindingKind::MissingIndex],
                "{measured} should stay a finding"
            );
        }
        let high = sanitized(&missing_index_document("99.5061", "[master]"));
        assert!(!high.contains("99.5061"), "the measured impact survived");

        let low = sanitized(&missing_index_document("12.3456", "[master]"));
        assert!(low.contains(r#"Impact="10""#), "got {low}");
        assert!(!low.contains("12.3456"), "the measured impact survived");
        assert_eq!(missing_index_findings(&low), vec![]);
    }

    #[test]
    fn the_two_impacts_straddle_the_judges_threshold() {
        // Pins the relationship without coupling behavior, and pins the DISTANCE:
        // a value landing next to the threshold turns a threshold tweak into a
        // reclassified fixture.
        use coot_core::plan::verdict::MISSING_INDEX_IMPACT;

        let high: f64 = HIGH_IMPACT.parse().unwrap();
        let low: f64 = LOW_IMPACT.parse().unwrap();
        assert!(high >= MISSING_INDEX_IMPACT);
        assert!(low < MISSING_INDEX_IMPACT);
        // EQUAL, not `>=`. Classifying at a HIGHER threshold than the judge
        // reports at silently downgrades the band between them: an `Impact="60"`
        // is a finding, and a classifier set to 90 would write it as 10 and lose
        // the missing index from the fixture entirely.
        assert_eq!(IMPACT_THRESHOLD, MISSING_INDEX_IMPACT);
        assert!(
            (high - MISSING_INDEX_IMPACT).min(MISSING_INDEX_IMPACT - low) >= 10.0,
            "an impact next to the threshold breaks when the threshold moves"
        );
    }

    #[test]
    fn the_recommended_table_and_columns_are_left_alone() {
        // Inside `master` these are Microsoft's own names and cost nothing;
        // outside it `sanitize` already refused the document. Rewriting
        // them would be an identifier scheme this module does not own.
        let out = sanitized(&missing_index_document("99.5061", "[master]"));
        for kept in [
            r#"<MissingIndex Database="[master]" Schema="[dbo]" Table="[Orders]">"#,
            r#"<Column Name="[ShipCity]" ColumnId="5"/>"#,
            r#"Usage="EQUALITY""#,
        ] {
            assert!(out.contains(kept), "sanitization lost {kept}");
        }
    }

    // One `Database` attribute on each element kind that carries one, so a check
    // written against a list of elements fails here rather than in a capture.
    // `ColumnReference` and `StatisticsInfo` are the two that matter most: they
    // account for 628 of the 647 `Database` attributes across the fixtures.
    fn document_naming(database: &str) -> [String; 4] {
        let plan = |body: &str| {
            format!(
                concat!(
                    r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan"><BatchSequence><Batch><Statements>"#,
                    r#"<StmtSimple StatementText="SELECT 1" StatementSubTreeCost="1" StatementEstRows="1"><QueryPlan>{body}"#,
                    r#"</QueryPlan></StmtSimple></Statements></Batch></BatchSequence></ShowPlanXML>"#,
                ),
                body = body
            )
        };
        let relop = |inner: &str| {
            format!(
                concat!(
                    r#"<RelOp NodeId="0" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="1" EstimatedTotalSubtreeCost="1">"#,
                    r#"{inner}</RelOp>"#,
                ),
                inner = inner
            )
        };
        [
            plan(&relop(&format!(
                r#"<IndexScan><Object Database="{database}" Schema="[dbo]" Table="[Payroll]"/></IndexScan>"#
            ))),
            plan(&relop(&format!(
                r#"<OutputList><ColumnReference Database="{database}" Schema="[dbo]" Table="[Payroll]" Column="Salary"/></OutputList>"#
            ))),
            plan(&format!(
                r#"<OptimizerStatsUsage><StatisticsInfo Statistics="[nc1]" Table="[Payroll]" Schema="[dbo]" Database="{database}"/></OptimizerStatsUsage>{}"#,
                relop("")
            )),
            missing_index_document("99.5061", database),
        ]
    }

    #[test]
    fn a_database_outside_the_allowlist_is_refused_wherever_it_is_named() {
        // THE case this exists for, and it needs no missing index: a three-part
        // name reaches another database with no `USE` at all, so a plan captured
        // while connected to master can still name one. `scan_for_secrets` cannot
        // catch it — it searches for the CONFIGURED database, and this is a table
        // it was never told about.
        for database in ["[Contoso_Prod]", "[tempdb]", "[MASTERPLAN]", "[msdb]", ""] {
            for xml in document_naming(database) {
                let err = sanitize(&xml).unwrap_err();
                assert!(
                    err.contains("names database"),
                    "database {database:?} was not refused: {err}"
                );
            }
        }
    }

    #[test]
    fn the_allowed_databases_are_accepted_however_they_are_written() {
        // `mssqlsystemresource` backs `sys.*` and appears in a plan over a
        // catalog view whether or not the query mentions it — 103 times across
        // the fixtures. An allowlist of master alone refuses all five.
        for database in [
            "[master]",
            "master",
            "[MASTER]",
            "[mssqlsystemresource]",
            "mssqlsystemresource",
            "[MSSQLSystemResource]",
        ] {
            for xml in document_naming(database) {
                assert!(
                    sanitize(&xml).is_ok(),
                    "database {database:?} should be accepted"
                );
            }
        }
    }

    #[test]
    fn a_database_whose_name_contains_a_bracket_is_refused() {
        // SQL Server doubles a `]` inside an identifier, so a database really
        // named `master]` arrives as `[master]]]`. Unquoting greedily would
        // accept it as `master`; taking one bracket each side leaves the stray
        // `]` and it compares unequal.
        assert!(!allowed_database("[master]]]"));
        assert!(!allowed_database("[mssqlsystemresource]]]"));
        for xml in document_naming("[master]]]") {
            assert!(sanitize(&xml).is_err(), "a bracketed name must be refused");
        }
    }

    #[test]
    fn a_linked_server_is_refused() {
        // No fixture carries `Server`, but the schema permits it and a query
        // across a linked server emits a real hostname — a value with no
        // allowlist to vet it against. A local capture never produces one.
        let xml = concat!(
            r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan"><BatchSequence><Batch><Statements>"#,
            r#"<StmtSimple StatementText="SELECT 1" StatementSubTreeCost="1" StatementEstRows="1"><QueryPlan>"#,
            r#"<RelOp NodeId="0" PhysicalOp="Remote Query" LogicalOp="Remote Query" EstimateRows="1" EstimatedTotalSubtreeCost="1">"#,
            r#"<RemoteQuery><Object Server="[DEVSQL01]" Database="[master]" Schema="[dbo]" Table="[T]"/></RemoteQuery>"#,
            r#"</RelOp></QueryPlan></StmtSimple></Statements></Batch></BatchSequence></ShowPlanXML>"#,
        );
        let err = sanitize(xml).unwrap_err();
        assert!(err.contains("linked server"), "got {err}");
        assert!(err.contains("[DEVSQL01]"), "the error must name it: {err}");
    }

    #[test]
    fn an_object_with_no_database_is_accepted() {
        // A bare `Table` names an object in the CURRENT database, which forcing
        // every capture to master already pins. Nothing is left to decide, so
        // this is deliberately NOT refused.
        let xml = concat!(
            r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan"><BatchSequence><Batch><Statements>"#,
            r#"<StmtSimple StatementText="SELECT 1" StatementSubTreeCost="1" StatementEstRows="1"><QueryPlan>"#,
            r#"<RelOp NodeId="0" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="1" EstimatedTotalSubtreeCost="1">"#,
            r#"<TableScan><Object Table="[@t]"/></TableScan>"#,
            r#"</RelOp></QueryPlan></StmtSimple></Statements></Batch></BatchSequence></ShowPlanXML>"#,
        );
        assert!(sanitize(xml).is_ok());
    }

    #[test]
    fn global_trace_flags_are_stripped() {
        // Instance configuration, not plan shape. Absent from every fixture only
        // because that instance runs with none set; 4199 is ordinary.
        let xml = concat!(
            r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan"><BatchSequence><Batch><Statements>"#,
            r#"<StmtSimple StatementText="SELECT 1" StatementSubTreeCost="1" StatementEstRows="1"><QueryPlan>"#,
            r#"<TraceFlags IsCompileTime="true"><TraceFlag Value="4199" Scope="Global"/></TraceFlags>"#,
            r#"<RelOp NodeId="0" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="1" EstimatedTotalSubtreeCost="1"/>"#,
            r#"</QueryPlan></StmtSimple></Statements></Batch></BatchSequence></ShowPlanXML>"#,
        );
        let out = sanitized(xml);
        for gone in ["TraceFlags", "TraceFlag", "4199", "IsCompileTime", "Global"] {
            assert!(!out.contains(gone), "{gone} survived sanitization");
        }
        assert!(out.contains(r#"PhysicalOp="Table Scan""#), "got {out}");
    }

    #[test]
    fn a_statement_kind_this_does_not_rewrite_is_refused() {
        // Its QueryPlan and operators would be rewritten while the statement's
        // own two figures escaped at full measured precision. Aborting is the
        // same trade the operator count takes: loud beats silently partial.
        let xml = concat!(
            r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan"><BatchSequence><Batch><Statements>"#,
            r#"<StmtCond><Condition><QueryPlan>"#,
            r#"<RelOp NodeId="0" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="4.7" EstimatedTotalSubtreeCost="0.0123456"/>"#,
            r#"</QueryPlan></Condition>"#,
            r#"<Then><StmtSimple StatementText="SELECT 1" StatementSubTreeCost="0.0123456" StatementEstRows="4.7"/></Then>"#,
            r#"</StmtCond>"#,
            r#"</Statements></Batch></BatchSequence></ShowPlanXML>"#,
        );
        // The StmtSimple inside is fine; a StmtCond carrying the attributes is not.
        let with_measurements = xml.replace(
            "<StmtCond>",
            r#"<StmtCond StatementSubTreeCost="0.0123456" StatementEstRows="4.7">"#,
        );
        let err = sanitize(&with_measurements).unwrap_err();
        assert!(
            err.contains("StmtCond") && err.contains("does not rewrite"),
            "got {err}"
        );
    }

    #[test]
    fn a_corrupt_splice_is_caught_before_it_is_returned() {
        // Driven directly: no input reaches this through `sanitize` without a
        // bug in `replace`, so a black-box test would assert nothing. What is
        // covered is the guard's own logic, which is what would have to work on
        // the day a range goes wrong.
        let good = sanitized(DIRTY);
        assert_eq!(check_output(&good, 4), Ok(()));

        let truncated = &good[..good.len() / 2];
        assert!(
            check_output(truncated, 4)
                .unwrap_err()
                .contains("did not parse"),
            "a half-written document must not pass"
        );

        // Well-formed but an operator short: the shape a swallowed tag takes,
        // which parses cleanly and is exactly what well-formedness alone misses.
        assert!(
            check_output(&good, 5)
                .unwrap_err()
                .contains("4 operators, not 5"),
            "a lost operator must not pass"
        );
    }

    // ------------------------------------------------------ literal values

    // Every literal-bearing attribute on one document, so a check that drops one
    // from the list fails here rather than in a capture.
    const EVERY_LITERAL: &str = concat!(
        r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan"><BatchSequence><Batch><Statements>"#,
        r#"<StmtSimple StatementText="SELECT 1 WHERE x = 11" ParameterizedText="(@1 int)SELECT 1 WHERE x = @1" StatementSubTreeCost="1" StatementEstRows="1"><QueryPlan>"#,
        r#"<Warnings><PlanAffectingConvert ConvertIssue="Cardinality Estimate" Expression="CONVERT(int,[t].[c],33)"/></Warnings>"#,
        r#"<RelOp NodeId="0" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="1" EstimatedTotalSubtreeCost="1">"#,
        r#"<TableScan><Predicate><ScalarOperator ScalarString="[t].[c]=(44)"><Const ConstValue="(55)"/></ScalarOperator></Predicate></TableScan>"#,
        r#"</RelOp><ParameterList><ColumnReference Column="@1" ParameterCompiledValue="(66)" ParameterRuntimeValue="(77)"/></ParameterList>"#,
        r#"</QueryPlan></StmtSimple></Statements></Batch></BatchSequence></ShowPlanXML>"#,
    );

    fn found_in(xml: &str) -> Vec<Literal> {
        literals(xml).expect("must parse")
    }

    fn values_from(found: &[Literal], attribute: &str) -> Vec<String> {
        found
            .iter()
            .filter(|l| l.attribute == attribute)
            .map(|l| l.value.clone())
            .collect()
    }

    #[test]
    fn every_literal_channel_is_reported() {
        // The expected names are written out rather than read from
        // LITERAL_ATTRIBUTES. Looping over the list under test would delete this
        // test's own coverage along with any entry someone removed from it.
        let found = found_in(EVERY_LITERAL);
        let mut reported: Vec<&str> = found.iter().map(|l| l.attribute).collect();
        reported.sort_unstable();
        reported.dedup();
        assert_eq!(
            reported,
            vec![
                "ConstValue",
                "Expression",
                "ParameterCompiledValue",
                "ParameterRuntimeValue",
                "ParameterizedText",
                "ScalarString",
                "StatementText",
            ]
        );

        // One value unique to each channel, so an attribute that is found but
        // reported empty still fails.
        for value in [
            "x = 11",
            "(@1 int)",
            "(55)",
            "[t].[c]=(44)",
            "(66)",
            "(77)",
            ",33)",
        ] {
            assert!(
                found.iter().any(|l| l.value.contains(value)),
                "{value} was not reported: {found:?}"
            );
        }
    }

    #[test]
    fn the_literal_attribute_list_is_pinned() {
        // Separate from the test above and asserting the constant directly: an
        // attribute added to the list without a specimen in EVERY_LITERAL would
        // otherwise be silently uncovered.
        assert_eq!(
            LITERAL_ATTRIBUTES,
            &[
                "StatementText",
                "ParameterizedText",
                "ConstValue",
                "ScalarString",
                "ParameterCompiledValue",
                "ParameterRuntimeValue",
                "Expression",
            ]
        );
    }

    #[test]
    fn a_predicate_value_is_reported_from_the_statement_text() {
        // THE regression. `StatementText` is the query verbatim, so it carries a
        // literal whether or not the optimizer also lands one in a `Const` —
        // which is why trimming the list back to Const/ScalarString/Parameter
        // would leave a file that looks checked and is not.
        let xml = EVERY_LITERAL.replace(
            r#"StatementText="SELECT 1 WHERE x = 11""#,
            r#"StatementText="SELECT Name FROM Payroll WHERE CustomerId = 12345""#,
        );
        let found = found_in(&xml);
        assert!(
            values_from(&found, "StatementText")
                .iter()
                .any(|v| v.contains("12345")),
            "the predicate value was not reported from StatementText: {found:?}"
        );
    }

    #[test]
    fn literals_are_distinct_and_in_document_order() {
        let xml = concat!(
            r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan">"#,
            r#"<Const ConstValue="(1)"/><Const ConstValue="(2)"/><Const ConstValue="(1)"/>"#,
            r#"<ScalarOperator ScalarString="(1)"/>"#,
            r#"</ShowPlanXML>"#,
        );
        let found = found_in(xml);
        assert_eq!(values_from(&found, "ConstValue"), vec!["(1)", "(2)"]);
        // Same value, different attribute: reported again, because it shows a
        // second way that value reaches the file.
        assert_eq!(values_from(&found, "ScalarString"), vec!["(1)"]);
        assert_eq!(found.len(), 3);
    }

    #[test]
    fn a_document_with_no_literals_reports_none() {
        let xml = concat!(
            r#"<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan">"#,
            r#"<RelOp NodeId="0" PhysicalOp="Table Scan" EstimateRows="1"/>"#,
            r#"</ShowPlanXML>"#,
        );
        assert_eq!(found_in(xml), vec![]);
    }

    #[test]
    fn literals_rejects_malformed_xml() {
        let err = literals("<not-xml").unwrap_err();
        assert!(err.contains("did not parse"), "got {err}");
    }

    #[test]
    fn every_committed_fixture_carries_literals() {
        // What makes `the_built_in_set_is_never_gated` mean something: these all
        // WOULD be refused, so it is `Source::BuiltIn` that admits them and not
        // an accident of their content.
        for name in FIXTURE_FILES {
            let found = found_in(&fixture(name));
            assert!(!found.is_empty(), "{name} reported no literals");
            assert!(
                found.iter().any(|l| l.attribute == "StatementText"),
                "{name} reported no StatementText"
            );
        }
    }

    #[test]
    fn the_built_in_set_is_never_gated() {
        // Its SQL is in this file, reviewed before it was committed, so its
        // literals are ours. `just dump-plans` must keep working.
        for name in FIXTURE_FILES {
            let found = found_in(&fixture(name));
            assert_eq!(permit(Source::BuiltIn, &found), Ok(()), "{name} was gated");
        }
    }

    #[test]
    fn an_ad_hoc_capture_carrying_a_literal_is_refused() {
        let found = found_in(EVERY_LITERAL);
        let source = Source::AdHoc {
            name: "my-query",
            sql: "SELECT 1 WHERE x = 11",
        };
        let refusal = permit(source, &found).unwrap_err();
        assert!(refusal.contains("ABORTED"), "got {refusal}");
        assert!(refusal.contains("Nothing was written"), "got {refusal}");
        // Every literal is shown, not just a count — the user learns what would
        // have leaked even though the capture is refused.
        for l in &found {
            assert!(
                refusal.contains(&l.value),
                "the refusal hid {:?}: {refusal}",
                l.value
            );
        }
    }

    #[test]
    fn the_refusal_is_a_recipe_not_a_complaint() {
        // Someone who reads this should find adding two lines to an array
        // obviously cheaper than editing the guard out. That is the whole
        // mechanism for keeping the guard in place, since there is no override.
        let source = Source::AdHoc {
            name: "missing-index",
            sql: r#"SELECT "col" FROM T WHERE p LIKE 'C:\dir\%'"#,
        };
        let refusal = permit(source, &found_in(EVERY_LITERAL)).unwrap_err();
        assert!(refusal.contains("FIXTURES"), "got {refusal}");
        assert!(
            refusal.contains("core/examples/dump_plan.rs"),
            "got {refusal}"
        );
        assert!(refusal.contains("just dump-plans"), "got {refusal}");

        // The SQL carries a double quote AND a backslash, because neither `{}`
        // nor `{:?}` escapes a SINGLE quote — a specimen using `'X'` emits the
        // same bytes either way and asserts nothing about escaping. Under `{}`
        // this line would end the Rust literal early at `"col"` and carry a `\d`
        // that does not compile.
        assert!(
            refusal.contains(
                r#"    ("missing-index", "SELECT \"col\" FROM T WHERE p LIKE 'C:\\dir\\%'"),"#
            ),
            "the FIXTURES line is not paste-ready: {refusal}"
        );
    }

    #[test]
    fn a_newline_in_the_sql_stays_on_one_line() {
        // A multi-line ad-hoc query is ordinary — fish and bash both pass one
        // through happily. Emitted raw it would split the FIXTURES entry across
        // two lines and leave an unterminated literal.
        let source = Source::AdHoc {
            name: "two-lines",
            sql: "SELECT 1\nFROM T",
        };
        let refusal = permit(source, &found_in(EVERY_LITERAL)).unwrap_err();
        assert!(
            refusal.contains(r#"    ("two-lines", "SELECT 1\nFROM T"),"#),
            "a newline was not escaped: {refusal}"
        );
    }

    #[test]
    fn a_quote_in_the_fixture_name_is_escaped_too() {
        // The name is interpolated by the same mechanism and is just as
        // user-supplied as the SQL.
        let source = Source::AdHoc {
            name: r#"od"d"#,
            sql: "SELECT 1",
        };
        let refusal = permit(source, &found_in(EVERY_LITERAL)).unwrap_err();
        assert!(
            refusal.contains(r#"    ("od\"d", "SELECT 1"),"#),
            "the name was not escaped: {refusal}"
        );
    }

    #[test]
    fn the_two_modes_get_the_source_that_matches_them() {
        // Wiring, not policy. Every other test here builds its own `Source`, so
        // swapping these two in `plan_work` leaves all of them green while the
        // gate is open on precisely the SQL it exists to stop.
        let args = |v: &[&str]| v.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();

        let pair = args(&["my-query", "SELECT 1"]);
        let (work, source) = plan_work(&pair).expect("two args");
        assert!(
            matches!(source, Source::AdHoc { name, sql } if name == "my-query" && sql == "SELECT 1"),
            "an argument pair must be ad-hoc"
        );
        assert_eq!(work, vec![("my-query".to_string(), "SELECT 1".to_string())]);

        let none = args(&[]);
        let (work, source) = plan_work(&none).expect("no args");
        assert!(
            matches!(source, Source::BuiltIn),
            "no arguments must be the built-in set"
        );
        assert_eq!(work.len(), FIXTURES.len());

        let one = args(&["only-one"]);
        let three = args(&["a", "b", "c"]);
        assert!(plan_work(&one).is_none());
        assert!(plan_work(&three).is_none());
    }

    #[test]
    fn an_ad_hoc_capture_with_no_literals_is_allowed() {
        // Covers a state no real capture reaches: `StatementText` is on every
        // statement element, so a captured plan always carries a literal and
        // every ad-hoc capture is refused. Kept because it pins the shape of the
        // rule — the gate turns on the literals, not on the mode alone — and
        // deleting the `found.is_empty()` early return fails here.
        let source = Source::AdHoc {
            name: "shapes-only",
            sql: "SELECT name FROM sys.objects",
        };
        assert_eq!(permit(source, &[]), Ok(()));
    }

    #[test]
    fn a_real_plan_always_carries_a_literal() {
        // Why the rule above is closed rather than conditional: every statement
        // element carries `StatementText`, so `literals` is never empty for a
        // captured document and no ad-hoc query slips through.
        for name in FIXTURE_FILES {
            let source = Source::AdHoc {
                name: "would-be-adhoc",
                sql: "irrelevant",
            };
            assert!(
                permit(source, &found_in(&fixture(name))).is_err(),
                "{name} would have been permitted as an ad-hoc capture"
            );
        }
    }

    #[test]
    fn sanitize_rejects_malformed_xml() {
        let err = sanitize("<not-xml").unwrap_err();
        assert!(err.contains("did not parse"), "got {err}");
    }

    #[test]
    fn a_well_formed_document_that_is_not_a_plan_passes_through() {
        let xml = "<html><body>nope</body></html>";
        assert_eq!(sanitized(xml), xml);
    }
}
