//! Dynamic read-path spike — the "does the SQL runner's grid have a data layer?"
//! question, answered concretely.
//!
//! The typed spike (typed_probe.rs) used `get::<KnownType>()`, where WE told the compiler
//! each column's type. A generic SQL runner can't do that: the user types arbitrary
//! SQL and you don't know the columns until the result comes back. This binary
//! exercises the untyped path the grid actually needs:
//!
//!   1. run an arbitrary query
//!   2. read `row.columns()` for the header (name + SQL type, straight off the wire)
//!   3. read each cell via `row.get_raw(i) -> Option<SqlValue>` (no type known ahead)
//!   4. `match` the SqlValue enum to render it
//!
//! Step 4 — `render_cell` below — IS your grid's data layer in miniature. Everything
//! the real app does to turn a result set into a table is here in ~30 lines.
//!
//! Run it (fish), same env vars as the typed spike:
//!   cargo run -p billz-core --example dynamic_dump

use std::env;

use mssql_client::{Client, Config, SqlValue};

/// Turn any SqlValue into a display string. This is the grid renderer.
/// The enum is `#[non_exhaustive]`, so the wildcard arm is required — it also
/// future-proofs against new variants and covers Tvp (which a SELECT never returns).
fn render_cell(v: &SqlValue) -> String {
    match v {
        SqlValue::Null => "NULL".to_string(),
        SqlValue::Bool(b) => b.to_string(),
        SqlValue::TinyInt(n) => n.to_string(),
        SqlValue::SmallInt(n) => n.to_string(),
        SqlValue::Int(n) => n.to_string(),
        SqlValue::BigInt(n) => n.to_string(),
        SqlValue::Float(f) => f.to_string(),
        SqlValue::Double(f) => f.to_string(),
        SqlValue::String(s) => s.clone(),
        SqlValue::Xml(s) => s.clone(),
        SqlValue::Decimal(d) => d.to_string(),
        SqlValue::Uuid(u) => u.to_string(),
        SqlValue::Date(d) => d.to_string(),
        SqlValue::Time(t) => t.to_string(),
        SqlValue::DateTime(dt) => dt.to_string(),
        SqlValue::DateTimeOffset(dt) => dt.to_string(),
        SqlValue::Binary(b) => {
            let hex: String = b.iter().map(|byte| format!("{byte:02x}")).collect();
            format!("0x{hex}")
        }
        // Tvp + any future variant. `type_name()` gives the driver's own label.
        other => format!("<{}>", other.type_name()),
    }
}

/// Run a query and dump it the way the grid would: header from column metadata,
/// then each row rendered cell-by-cell through the untyped path. Written as a
/// macro so we don't have to name Client's type-state generic — the call site
/// has the concrete type.
macro_rules! dump {
    ($client:expr, $label:expr, $sql:expr) => {{
        println!("\n=== {} ===", $label);
        println!("SQL: {}\n", $sql);
        match $client.query($sql, &[]).await {
            Ok(rows) => {
                let mut header_done = false;
                let mut n = 0usize;
                for r in rows {
                    match r {
                        Ok(row) => {
                            if !header_done {
                                // HEADER: purely from metadata, no values touched.
                                // (Column also carries max_length/scale/collation if
                                // you want them; precision+scale shown here for decimals.)
                                for c in row.columns() {
                                    let mut ty = c.type_name.clone();
                                    if let (Some(p), Some(s)) = (c.precision, c.scale) {
                                        ty = format!("{ty}({p},{s})");
                                    }
                                    if c.nullable {
                                        ty.push_str(" NULL");
                                    }
                                    println!("  column  {:<22} {}", c.name, ty);
                                }
                                println!();
                                header_done = true;
                            }
                            // CELLS: untyped, type discovered per value.
                            for i in 0..row.len() {
                                let name = row.columns()[i].name.clone();
                                let cell = match row.get_raw(i) {
                                    Some(v) => render_cell(&v),
                                    None => "<out-of-range>".to_string(),
                                };
                                println!("  {:<22} = {}", name, cell);
                            }
                            println!();
                            n += 1;
                        }
                        Err(e) => println!("  ROW ERR   {e}"),
                    }
                }
                if n == 0 {
                    println!("  (no rows)");
                }
            }
            Err(e) => println!("  QUERY ERR {e}"),
        }
    }};
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = env::var("MSSQL_SERVER").map_err(|_| "set MSSQL_SERVER")?;
    let user = env::var("MSSQL_USER").map_err(|_| "set MSSQL_USER")?;
    let password = env::var("MSSQL_PASSWORD").map_err(|_| "set MSSQL_PASSWORD")?;
    let database = env::var("MSSQL_DATABASE").ok();

    let mut conn = format!(
        "Server={server};User Id={user};Password={password};\
         Encrypt=false;TrustServerCertificate=true;Application Name=schema-spike-dynamic"
    );
    if let Some(db) = &database {
        conn.push_str(&format!(";Database={db}"));
    }

    let config = Config::from_connection_string(&conn)?;
    let mut client = Client::connect(config).await?;
    println!("connected: {server}");

    // 1) A synthetic mixed-type row that exercises most SqlValue variants at once —
    //    the "what if a user pastes a wide, ugly SELECT" case.
    dump!(
        client,
        "mixed synthetic result set",
        "SELECT
            CAST(42 AS int)                                              AS the_int,
            CAST(N'héllo ☃' AS nvarchar(20))                            AS the_text,
            CAST(1 AS bit)                                               AS the_flag,
            CAST(1234.5678 AS decimal(19,4))                            AS the_decimal,
            CAST(1234.5678 AS money)                                    AS the_money,
            CAST('6F9619FF-8B86-D011-B42D-00C04FC964FF' AS uniqueidentifier) AS the_guid,
            SYSDATETIMEOFFSET()                                         AS the_dto,
            CAST('2024-03-17 12:34:56.123' AS datetime2)               AS the_dt2,
            CAST('2024-03-17' AS date)                                  AS the_date,
            CAST(0xDEADBEEF AS varbinary(8))                           AS the_binary,
            CAST('<r><i k=\"1\"/></r>' AS xml)                          AS the_xml,
            CAST(NULL AS int)                                           AS the_null"
    );

    // 2) A REAL result set from a system catalog you'll actually browse — proves the
    //    same path works on non-CAST columns and multiple rows. sys.databases is
    //    readable by anyone, so this needs no special permissions.
    dump!(
        client,
        "sys.databases (real multi-row result set)",
        "SELECT name, database_id, create_date, is_read_only, state_desc
           FROM sys.databases
          ORDER BY database_id"
    );

    client.close().await?;
    Ok(())
}
