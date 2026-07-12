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
/// `max_length` is the driver `Column.max_length` (byte width). It disambiguates
/// the nullable "N" families whose token alone doesn't carry the width (billz-9qg):
/// `IntN` 1/2/4/8 → tinyint/smallint/int/bigint, `FloatN` 4/8 → real/float,
/// `MoneyN` 4/8 → smallmoney/money, `DateTimeN` 4/8 → smalldatetime/datetime.
/// Unknown/absent widths fall back to each family's default (`IntN`→int,
/// `FloatN`→float, `MoneyN`→money, `DateTimeN`→datetime), matching the driver's
/// own `_ => SqlValue::…` default so the header and cell never disagree.
///
/// Returns `&str` (not `String`): friendly names are `'static`; unknowns borrow
/// the input, so this never allocates.
pub fn friendly_type_name(wire_token: &str, max_length: Option<u32>) -> &str {
    match wire_token {
        "Bit" | "BitN" => "bit",
        "Int1" => "tinyint",
        "Int2" => "smallint",
        "Int4" => "int",
        "Int8" => "bigint",
        // Nullable integer: the width is in max_length, not the token.
        "IntN" => match max_length {
            Some(1) => "tinyint",
            Some(2) => "smallint",
            Some(8) => "bigint",
            _ => "int", // 4, or unknown → int
        },
        "Float4" => "real",
        "Float8" => "float",
        "FloatN" => match max_length {
            Some(4) => "real",
            _ => "float", // 8, or unknown → float
        },
        // Money4 is the fixed 4-byte smallmoney; Money is the 8-byte money.
        "Money4" => "smallmoney",
        "Money" => "money",
        "MoneyN" => match max_length {
            Some(4) => "smallmoney",
            _ => "money", // 8, or unknown → money
        },
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
        // DateTime4 is the fixed 4-byte smalldatetime; DateTime is 8-byte datetime.
        "DateTime4" => "smalldatetime",
        "DateTime" => "datetime",
        "DateTimeN" => match max_length {
            Some(4) => "smalldatetime",
            _ => "datetime", // 8, or unknown → datetime
        },
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
        // Fixed-width tokens ignore max_length (pass None).
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
            ("Money", "money"),
            ("Money4", "smallmoney"),
            ("Guid", "uniqueidentifier"),
            ("DecimalN", "decimal"),
            ("NumericN", "numeric"),
            ("DateTimeOffset", "datetimeoffset"),
            ("DateTime2", "datetime2"),
            ("DateTime", "datetime"),
            ("DateTime4", "smalldatetime"),
            ("Variant", "sql_variant"),
            ("Xml", "xml"),
        ];
        for (token, friendly) in cases {
            assert_eq!(friendly_type_name(token, None), friendly, "token {token}");
        }
    }

    #[test]
    fn unknown_token_falls_through_unchanged() {
        assert_eq!(friendly_type_name("SomeFutureType", None), "SomeFutureType");
    }

    #[test]
    fn nullable_n_tokens_are_width_aware() {
        // billz-9qg: the nullable "N" families carry their width in max_length.
        assert_eq!(friendly_type_name("IntN", Some(1)), "tinyint");
        assert_eq!(friendly_type_name("IntN", Some(2)), "smallint");
        assert_eq!(friendly_type_name("IntN", Some(4)), "int");
        assert_eq!(friendly_type_name("IntN", Some(8)), "bigint");
        assert_eq!(friendly_type_name("FloatN", Some(4)), "real");
        assert_eq!(friendly_type_name("FloatN", Some(8)), "float");
        assert_eq!(friendly_type_name("MoneyN", Some(4)), "smallmoney");
        assert_eq!(friendly_type_name("MoneyN", Some(8)), "money");
        assert_eq!(friendly_type_name("DateTimeN", Some(4)), "smalldatetime");
        assert_eq!(friendly_type_name("DateTimeN", Some(8)), "datetime");
    }

    #[test]
    fn n_tokens_fall_back_to_widest_family_on_unknown_width() {
        // Absent/unexpected width → the common family, never a panic.
        assert_eq!(friendly_type_name("IntN", None), "int");
        assert_eq!(friendly_type_name("FloatN", None), "float");
        assert_eq!(friendly_type_name("MoneyN", None), "money");
        assert_eq!(friendly_type_name("DateTimeN", Some(99)), "datetime");
    }
}
