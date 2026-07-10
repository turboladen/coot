//! Type-decoding spike for a personal SQL Server GUI client.
//!
//! Purpose: answer ONE question before any UI work gets written —
//! "does mssql-client cleanly decode the gnarly types my DEV schema throws
//! at it, or does it choke?"
//!
//! It connects, then runs a series of tiny `SELECT CAST(... AS <type>)`
//! probes, attempting to pull each into a plausible Rust type. Instead of
//! panicking on a failure, every probe records Ok / DECODE ERR / QUERY ERR
//! into a table that prints at the end. So a wrong type-target guess shows up
//! as a readable row, not a crash — which is exactly what you want from a spike.
//!
//! Connection details come from the environment so no secrets live in the file:
//!
//!   MSSQL_SERVER    e.g. myhost,1433   (required)
//!   MSSQL_USER      SQL auth username  (required)
//!   MSSQL_PASSWORD  SQL auth password  (required)
//!   MSSQL_DATABASE  optional
//!
//! fish:
//!   set -x MSSQL_SERVER   "myhost,1433"
//!   set -x MSSQL_USER     "sa"
//!   set -x MSSQL_PASSWORD (op read "op://Private/DevSQL/password")   # 1Password, stays out of history
//!   set -x MSSQL_DATABASE "ESP_Arnotts_Group_DEV"
//!   cargo run -p billz-core --example typed_probe

use std::env;

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use mssql_client::{Client, Config};
use rust_decimal::Decimal;
use uuid::Uuid;

/// Run one single-column probe and record the outcome as a string.
/// Uses a macro (not a generic fn) so we never have to name the driver's
/// internal FromSql trait — `row.get::<$rust>(0)` is resolved syntactically.
macro_rules! probe {
    ($client:expr, $results:expr, $sql_type:expr, $sql:expr, $rust:ty) => {{
        let outcome = match $client.query($sql, &[]).await {
            Ok(rows) => {
                let mut got = String::from("(no rows returned)");
                for r in rows {
                    match r {
                        Ok(row) => {
                            got = match row.get::<$rust>(0) {
                                Ok(v) => format!("ok         {:?}", v),
                                Err(e) => format!("DECODE ERR {e}"),
                            };
                        }
                        Err(e) => got = format!("ROW ERR    {e}"),
                    }
                    break; // one row is enough
                }
                got
            }
            Err(e) => format!("QUERY ERR  {e}"),
        };
        $results.push((
            $sql_type.to_string(),
            stringify!($rust).to_string(),
            outcome,
        ));
    }};
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = env::var("MSSQL_SERVER").map_err(|_| "set MSSQL_SERVER")?;
    let user = env::var("MSSQL_USER").map_err(|_| "set MSSQL_USER")?;
    let password = env::var("MSSQL_PASSWORD").map_err(|_| "set MSSQL_PASSWORD")?;
    let database = env::var("MSSQL_DATABASE").ok();

    // Your stated defaults: encrypt "optional" + always trust the cert.
    // Encrypt=false => login handshake still encrypted, data unencrypted (classic
    // "optional"). If the box is TLS-less, swap to Encrypt=no_tls. If it's SQL 2022+
    // strict, use Encrypt=strict.
    let mut conn = format!(
        "Server={server};User Id={user};Password={password};\
         Encrypt=false;TrustServerCertificate=true;Application Name=schema-spike"
    );
    if let Some(db) = &database {
        conn.push_str(&format!(";Database={db}"));
    }

    let config = Config::from_connection_string(&conn)?;
    let mut client = Client::connect(config).await?;
    println!(
        "connected: {server}{}\n",
        database
            .as_deref()
            .map(|d| format!("  db={d}"))
            .unwrap_or_default()
    );

    let mut results: Vec<(String, String, String)> = Vec::new();

    // --- sanity / common types ---
    probe!(client, results, "int", "SELECT CAST(42 AS int)", i32);
    probe!(
        client,
        results,
        "bigint",
        "SELECT CAST(9000000000 AS bigint)",
        i64
    );
    probe!(client, results, "bit", "SELECT CAST(1 AS bit)", bool);
    probe!(
        client,
        results,
        "float",
        "SELECT CAST(3.14159 AS float)",
        f64
    );
    probe!(
        client,
        results,
        "nvarchar",
        "SELECT CAST(N'héllo ☃' AS nvarchar(50))",
        String
    );
    probe!(
        client,
        results,
        "varchar",
        "SELECT CAST('ascii-only' AS varchar(50))",
        String
    );

    // --- the ones that actually decide the project ---
    probe!(
        client,
        results,
        "decimal(19,4)",
        "SELECT CAST(1234.5678 AS decimal(19,4))",
        Decimal
    );
    probe!(
        client,
        results,
        "money",
        "SELECT CAST(1234.5678 AS money)",
        Decimal
    );
    probe!(
        client,
        results,
        "uniqueidentifier",
        "SELECT CAST('6F9619FF-8B86-D011-B42D-00C04FC964FF' AS uniqueidentifier)",
        Uuid
    );
    probe!(
        client,
        results,
        "varbinary",
        "SELECT CAST(0xDEADBEEF AS varbinary(8))",
        Vec<u8>
    );
    probe!(
        client,
        results,
        "xml",
        "SELECT CAST('<r><i k=\"1\"/></r>' AS xml)",
        String
    );

    // datetimeoffset carries a zone — probe BOTH likely targets so we learn which
    // one the driver actually decodes into. Use SYSDATETIMEOFFSET() rather than a
    // string literal so this tests the DRIVER's decode, not SQL Server's parsing of
    // an ISO string (a bad literal here throws server error 241, which is not the
    // driver's fault). The driver's own SqlValue::DateTimeOffset is DateTime<FixedOffset>.
    probe!(
        client,
        results,
        "datetimeoffset->FixedOffset",
        "SELECT SYSDATETIMEOFFSET()",
        DateTime<FixedOffset>
    );
    probe!(
        client,
        results,
        "datetimeoffset->Utc",
        "SELECT SYSDATETIMEOFFSET()",
        DateTime<Utc>
    );

    probe!(
        client,
        results,
        "datetime2",
        "SELECT CAST('2024-03-17 12:34:56.123' AS datetime2)",
        NaiveDateTime
    );
    probe!(
        client,
        results,
        "date",
        "SELECT CAST('2024-03-17' AS date)",
        NaiveDate
    );
    probe!(
        client,
        results,
        "time",
        "SELECT CAST('12:34:56.1234567' AS time)",
        NaiveTime
    );

    // --- stressors: expected to fail or surprise; that's informative ---
    // sql_variant rarely has a clean Rust mapping — seeing HOW it fails is the point.
    probe!(
        client,
        results,
        "sql_variant",
        "SELECT CAST(CAST(42 AS int) AS sql_variant)",
        String
    );
    // NULL handling through Option<T>.
    probe!(
        client,
        results,
        "int NULL",
        "SELECT CAST(NULL AS int)",
        Option<i32>
    );

    print_report(&results);
    client.close().await?;
    Ok(())
}

fn print_report(results: &[(String, String, String)]) {
    let w0 = results.iter().map(|r| r.0.len()).max().unwrap_or(0).max(8);
    let w1 = results.iter().map(|r| r.1.len()).max().unwrap_or(0).max(9);

    println!(
        "{:<w0$}  {:<w1$}  RESULT",
        "SQL TYPE",
        "RUST TYPE",
        w0 = w0,
        w1 = w1
    );
    println!("{}", "-".repeat(w0 + w1 + 40));
    for (sql_type, rust_type, outcome) in results {
        println!(
            "{sql_type:<w0$}  {rust_type:<w1$}  {outcome}",
            w0 = w0,
            w1 = w1
        );
    }

    let failures = results.iter().filter(|r| !r.2.starts_with("ok")).count();
    println!(
        "\n{} of {} probes decoded cleanly. \
         (A failing sql_variant is expected and usually fine; \
         datetimeoffset should succeed under at least one of the two targets.)",
        results.len() - failures,
        results.len()
    );
}
