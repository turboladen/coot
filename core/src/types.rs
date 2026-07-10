//! Wire-token → friendly SQL type name mapping (`PLAN.md` §7).
//!
//! `Column.type_name` from the driver is the Debug name of a raw TDS `TypeId`
//! variant (`tds-protocol 0.20.2`) — literally `"Int4"`, `"IntN"`, `"NVarChar"`,
//! `"MoneyN"`, … (see `client/response.rs`, which does
//! `format!("{effective_type_id:?}")`). This maps those to the friendly names a
//! user expects (`int`, `nvarchar`, `money`). Pure `&str` in, `&str` out —
//! imports nothing from `mssql_client`, so the driver stays behind `core`.

/// Map a TDS wire token to a friendly SQL type name. Unknown tokens fall
/// through unchanged so the grid always shows *something*. Case-sensitive exact
/// match on the `TypeId` Debug variant names.
///
/// Returns `&str` (not `String`): friendly names are `'static`; unknowns borrow
/// the input, so this never allocates.
pub fn friendly_type_name(wire_token: &str) -> &str {
    match wire_token {
        "Bit" | "BitN" => "bit",
        "Int1" => "tinyint",
        "Int2" => "smallint",
        // TODO(phase2): width-aware IntN/FloatN via max_length — see bead billz-9qg.
        // Nullable narrow forms (IntN len 8 = bigint, FloatN len 4 = real,
        // Money4/DateTime4) collapse to the common family here; disambiguating
        // them needs the separate `max_length` and is deferred.
        "Int4" | "IntN" => "int",
        "Int8" => "bigint",
        "Float4" => "real",
        "Float8" | "FloatN" => "float",
        "Money" | "Money4" | "MoneyN" => "money",
        "Decimal" | "DecimalN" => "decimal",
        "Numeric" | "NumericN" => "numeric",
        "Guid" => "uniqueidentifier",
        "NVarChar" => "nvarchar",
        "NChar" => "nchar",
        "VarChar" | "BigVarChar" => "varchar",
        "Char" | "BigChar" => "char",
        "Text" => "text",
        "NText" => "ntext",
        "VarBinary" | "BigVarBinary" => "varbinary",
        "Binary" | "BigBinary" => "binary",
        "Image" => "image",
        "Date" => "date",
        "Time" => "time",
        "DateTime2" => "datetime2",
        "DateTime" | "DateTime4" | "DateTimeN" => "datetime",
        "DateTimeOffset" => "datetimeoffset",
        "Xml" => "xml",
        "Variant" => "sql_variant",
        "Udt" => "udt",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_tokens_map_to_friendly_names() {
        let cases = [
            ("Int4", "int"),
            ("Int8", "bigint"),
            ("Int1", "tinyint"),
            ("Int2", "smallint"),
            ("Bit", "bit"),
            ("BitN", "bit"),
            ("Float4", "real"),
            ("Float8", "float"),
            ("NVarChar", "nvarchar"),
            ("VarChar", "varchar"),
            ("BigVarChar", "varchar"),
            ("MoneyN", "money"),
            ("Guid", "uniqueidentifier"),
            ("DecimalN", "decimal"),
            ("NumericN", "numeric"),
            ("DateTimeOffset", "datetimeoffset"),
            ("DateTime2", "datetime2"),
            ("Variant", "sql_variant"),
            ("Xml", "xml"),
        ];
        for (token, friendly) in cases {
            assert_eq!(friendly_type_name(token), friendly, "token {token}");
        }
    }

    #[test]
    fn unknown_token_falls_through_unchanged() {
        assert_eq!(friendly_type_name("SomeFutureType"), "SomeFutureType");
    }

    #[test]
    fn intn_collapses_to_int_pending_width_aware_mapping() {
        // Documents limitation L1 / bead billz-9qg: a nullable bigint arrives as
        // `IntN` (max_length 8) and is called `int` until width-aware mapping lands.
        assert_eq!(friendly_type_name("IntN"), "int");
    }
}
