//! The result-set types the UI sees — `core`'s own, driver-free (`PLAN.md` §8).
//!
//! `mssql_client::SqlValue` / `Column` never cross this boundary. `CellValue`
//! mirrors `SqlValue` but is OURS: everything the grid needs, nothing driver-
//! specific, and serde-serializable straight to the Svelte side. The
//! `SqlValue → CellValue` / `Column → ColumnMeta` mapping is the executor's job
//! (bead ce1.6); this module only defines the target shapes.

use serde::{Deserialize, Serialize};

/// One result set: column metadata, row-major cells, and an optional affected-row
/// count (for non-`SELECT` batches). Field names serialize camelCase for Svelte.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<CellValue>>,
    pub rows_affected: Option<u64>,
}

/// Column header metadata. `sql_type` is the **friendly** name (mapped via
/// [`crate::types::friendly_type_name`]) — NOT the raw wire token. `precision`
/// and `scale` come from the driver `Column` and drive decimal formatting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub precision: Option<u8>,
    pub scale: Option<u8>,
}

/// A single cell value — `core`'s own mirror of `SqlValue`.
///
/// Adjacently tagged (`{"kind": …, "value": …}`) so every variant — including
/// `Null` — serializes to one uniform shape the grid can switch on (right-align
/// numbers, render NULL, hex binary, …). External tagging would make `Null` a
/// bare string while others are objects; adjacent tagging avoids that.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CellValue {
    Null,
    Bool(bool),
    /// Widened: TinyInt/SmallInt/Int land here (8/16/32-bit — all fit f64
    /// exactly). Crosses JSON as a number. `bigint` does NOT map here; it exceeds
    /// f64's safe integer range, so it goes to [`CellValue::BigInt`] (billz-s7p).
    Int(i64),
    /// `bigint` (i64), string-encoded so a value beyond f64's safe integer range
    /// (`|n| > 2^53` — snowflake IDs, bigint hashes) survives JS `JSON.parse`
    /// without precision loss, exactly as `Decimal`/`Money` do (billz-s7p).
    BigInt(String),
    /// Widened: f32 `REAL` + f64 `FLOAT`.
    Float(f64),
    /// String-encoded so no f64 precision is lost over JSON — `decimal`, `money`,
    /// and `smallmoney` all map here.
    Decimal(String),
    Text(String),
    Uuid(String),
    Date(String),
    Time(String),
    DateTime(String),
    DateTimeOffset(String),
    /// Hex, e.g. `"0xdeadbeef"` (the spike's `render_cell` format).
    Binary(String),
    Xml(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn roundtrip(v: &CellValue) {
        let s = serde_json::to_string(v).unwrap();
        let back: CellValue = serde_json::from_str(&s).unwrap();
        assert_eq!(&back, v, "round-trip changed the value");
    }

    #[test]
    fn null_serializes_to_kind_only() {
        let v = CellValue::Null;
        assert_eq!(serde_json::to_value(&v).unwrap(), json!({"kind": "Null"}));
        roundtrip(&v);
    }

    #[test]
    fn int_serializes_as_json_number() {
        let v = CellValue::Int(42);
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            json!({"kind": "Int", "value": 42})
        );
        roundtrip(&v);
    }

    #[test]
    fn decimal_value_is_a_json_string_not_a_number() {
        // The anti-precision-loss guarantee: the `value` must carry quotes.
        let v = CellValue::Decimal("1234.5678".into());
        let s = serde_json::to_string(&v).unwrap();
        assert!(
            s.contains(r#""value":"1234.5678""#),
            "decimal value must serialize as a JSON string, got {s}"
        );
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            json!({"kind": "Decimal", "value": "1234.5678"})
        );
        roundtrip(&v);
    }

    #[test]
    fn bigint_value_is_a_json_string_not_a_number() {
        // billz-s7p: a bigint beyond f64's safe integer range must not serialize
        // as a JSON number (JS `JSON.parse` would corrupt it) — it carries quotes.
        let v = CellValue::BigInt("9007199254740993".into()); // 2^53 + 1
        let s = serde_json::to_string(&v).unwrap();
        assert!(
            s.contains(r#""value":"9007199254740993""#),
            "bigint value must serialize as a JSON string, got {s}"
        );
        roundtrip(&v);
    }

    #[test]
    fn binary_is_hex_string() {
        let v = CellValue::Binary("0xdeadbeef".into());
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            json!({"kind": "Binary", "value": "0xdeadbeef"})
        );
        roundtrip(&v);
    }

    #[test]
    fn every_variant_round_trips() {
        for v in [
            CellValue::Null,
            CellValue::Bool(true),
            CellValue::Int(-2_000_000_000),
            CellValue::BigInt("-9223372036854775808".into()),
            CellValue::Float(2.5),
            CellValue::Decimal("0.0001".into()),
            CellValue::Text("héllo ☃".into()),
            CellValue::Uuid("6F9619FF-8B86-D011-B42D-00C04FC964FF".into()),
            CellValue::Date("2024-03-17".into()),
            CellValue::Time("12:34:56".into()),
            CellValue::DateTime("2024-03-17 12:34:56.123".into()),
            CellValue::DateTimeOffset("2024-03-17 12:34:56 +00:00".into()),
            CellValue::Binary("0x00ff".into()),
            CellValue::Xml("<r/>".into()),
        ] {
            roundtrip(&v);
        }
    }

    #[test]
    fn column_meta_serializes_sql_type_as_camel_case() {
        let c = ColumnMeta {
            name: "amount".into(),
            sql_type: "decimal".into(),
            nullable: true,
            precision: Some(19),
            scale: Some(4),
        };
        let s = serde_json::to_string(&c).unwrap();
        assert!(s.contains(r#""sqlType":"decimal""#), "got {s}");
    }

    #[test]
    fn query_result_round_trips_and_uses_camel_case() {
        let qr = QueryResult {
            columns: vec![
                ColumnMeta {
                    name: "id".into(),
                    sql_type: "int".into(),
                    nullable: false,
                    precision: None,
                    scale: None,
                },
                ColumnMeta {
                    name: "note".into(),
                    sql_type: "nvarchar".into(),
                    nullable: true,
                    precision: None,
                    scale: None,
                },
            ],
            rows: vec![
                vec![CellValue::Int(1), CellValue::Text("first".into())],
                vec![CellValue::Int(2), CellValue::Null],
            ],
            rows_affected: Some(2),
        };
        let s = serde_json::to_string(&qr).unwrap();
        assert!(s.contains(r#""rowsAffected":2"#), "got {s}");
        let back: QueryResult = serde_json::from_str(&s).unwrap();
        assert_eq!(back, qr);
    }
}
