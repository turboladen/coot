//! Capture real ShowPlanXML documents and write them as `.sqlplan` fixtures.
//!
//! **Why this exists.** `core::plan`'s parser is tested offline against
//! checked-in fixtures. Hand-authored ShowPlanXML is a trap: the tests all pass
//! while the parser returns empty plans against a real server, because one
//! attribute name differs. So the fixtures must be genuine server output — and
//! only a machine that can reach the DEV box can produce them. Run this there,
//! commit what it writes, and the parser gets developed against reality.
//!
//! # These fixtures go into git. Read this before changing the queries.
//!
//! ShowPlanXML stamps `Database="[…]"` onto **every** `<Object>` element, so a
//! plan captured while connected to a tenant database embeds that database's
//! name throughout — even for a query that only reads `sys.*`. Committing work
//! database, table, or column names to this repo is not acceptable.
//!
//! Two independent defences, because one is not enough:
//!
//! 1. **Everything is captured against [`FIXTURE_DB`] (`master`), never
//!    `MSSQL_DATABASE`.** Combined with `sys.*`-only queries, every identifier
//!    in the output is a Microsoft-standard name (`master`, `sys`, `dbo`). The
//!    sensitive value is never captured in the first place, which beats
//!    scrubbing it out afterwards.
//! 2. **Nothing is written until it passes [`scan_for_secrets`].** Each document
//!    is searched for the configured server, username, and database values; a
//!    hit aborts the whole run without touching the filesystem. This is the net
//!    under the ad-hoc mode below, where a careless query could target a real
//!    database.
//!
//! Cases needing contrived shapes (missing-index suggestions, implicit
//! conversions) need a scratch table. Create it in `master` or `tempdb` with
//! generic column names — never in a tenant database.
//!
//! **Nothing it runs executes.** `SET SHOWPLAN_XML ON` makes the server compile
//! each query and hand back the plan without running it.
//!
//! Unlike the two spike probes beside it (`typed_probe`, `dynamic_dump`), which
//! predate the `core` boundary and drive `mssql-client` directly, this one goes
//! through `core`'s own `capture_plan_xml`, so it exercises the real capture
//! path including the `USE`-before-`SHOWPLAN` ordering.
//!
//! Run it (fish), same env vars as the other probes:
//!   set -x MSSQL_SERVER   …
//!   set -x MSSQL_USER     …
//!   set -x MSSQL_PASSWORD (op read "op://…")
//!   set -x MSSQL_DATABASE …
//!   just dump-plans
//!
//! Or capture one ad-hoc query instead of the built-in set (still forced to
//! `master`, still secret-scanned):
//!   cargo run -p coot-core --example dump_plan -- my-name "SELECT 1"

use std::env;
use std::path::PathBuf;

use coot_core::{
    ConnectionConfig, ConnectionId, ExecutionContext, InMemorySecretStore, SecretStore,
};

/// Every fixture is captured here, NOT against `MSSQL_DATABASE`. `master` is a
/// standard SQL Server name that reveals nothing, and it carries the same
/// `sys.*` catalog views, so the plans are structurally identical to what a
/// tenant database would produce.
const FIXTURE_DB: &str = "master";

/// The built-in set. Each exercises a different shape the parser must handle.
/// `sys.*` only — see the module doc before adding one.
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

    // Forced to master — see defence 1 in the module doc.
    let ctx = ExecutionContext::new(cfg.id.clone()).with_database(FIXTURE_DB);

    let args: Vec<String> = env::args().skip(1).collect();
    let work: Vec<(String, String)> = match args.as_slice() {
        [name, sql] => vec![(name.clone(), sql.clone())],
        [] => FIXTURES
            .iter()
            .map(|(n, s)| ((*n).to_string(), (*s).to_string()))
            .collect(),
        _ => {
            eprintln!("usage: dump_plan [<fixture-name> <sql>]");
            std::process::exit(1);
        }
    };

    // Capture EVERYTHING first and secret-scan it before writing a single file.
    // A fixture that leaks must never reach the filesystem, where it could be
    // swept into a commit by `git add -A`.
    let mut captured: Vec<(String, String)> = Vec::with_capacity(work.len());
    for (name, sql) in &work {
        match coot_core::capture_plan_xml(&cfg, &store, &ctx, sql).await {
            Ok(xml) => {
                if let Some(hit) = scan_for_secrets(&xml, &secrets) {
                    eprintln!(
                        "ABORTED: the plan for '{name}' contains {hit}.\n\
                         Nothing was written. Capture against {FIXTURE_DB} with sys.* objects \
                         only — see this example's module doc."
                    );
                    std::process::exit(1);
                }
                println!("captured {name} ({} bytes, clean)", xml.len());
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
        "\nAll {} captured and secret-scanned. Safe to commit and push.",
        captured.len()
    );
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
            server: "E4-DEV-ESP-01,1433".into(),
            username: "esp_reader".into(),
            database: "ESP_Nomad_SE_DEV".into(),
        }
    }

    #[test]
    fn a_clean_master_plan_passes() {
        let xml = r#"<Object Database="[master]" Schema="[sys]" Table="[objects]" />"#;
        assert_eq!(scan_for_secrets(xml, &secrets()), None);
    }

    #[test]
    fn a_tenant_database_name_is_caught_case_insensitively() {
        let xml = r#"<Object Database="[esp_nomad_se_dev]" Schema="[dbo]" Table="[Orders]" />"#;
        assert!(
            scan_for_secrets(xml, &secrets()).is_some_and(|h| h.contains("database")),
            "a tenant database name must never reach a committed fixture"
        );
    }

    #[test]
    fn the_server_host_is_caught_without_its_port() {
        let xml = "<!-- captured from E4-DEV-ESP-01 -->";
        assert!(scan_for_secrets(xml, &secrets()).is_some_and(|h| h.contains("server")));
    }

    #[test]
    fn the_username_is_caught() {
        let xml = "<ShowPlanXML><!-- esp_reader --></ShowPlanXML>";
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
        // A 2-3 char name would match ordinary XML text everywhere.
        let s = Secrets {
            server: "db".into(),
            username: "sa".into(),
            database: "db".into(),
        };
        assert_eq!(scan_for_secrets("<Database>anything</Database>", &s), None);
    }
}
