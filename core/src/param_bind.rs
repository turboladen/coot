//! Param substitution model — the driver-free half of d28.2 (`PLAN.md` §5).
//!
//! Two mechanisms, decided by a param's `sql_type` discriminator:
//!   - `Some(SqlType)` → a **bind** param. Its `&str` value is parsed into a
//!     typed [`BindValue`]; the executor turns that into the driver's `SqlValue`
//!     and hands it to `sp_executesql` (typed, safe). See [`parse_bind_value`].
//!   - `None` → a **raw-text** fragment. Its value is spliced literally into the
//!     SQL string before send — NO quoting, NO escaping (the whole point is
//!     `ORDER BY x DESC`, a table name, `TOP @n`). Injectable BY DESIGN; d28.6
//!     renders it loud. See [`splice_raw_text`].
//!
//! This module imports **no** `mssql_client` type (the driver boundary,
//! `CLAUDE.md`): it produces the core-owned [`BindValue`] intermediate, and
//! `executor.rs` — the sole driver-touching module — does the trivial
//! `BindValue → SqlValue` map. Everything here is pure and unit-tested with no DB.

use std::fmt::Display;

use chrono::{NaiveDate, NaiveDateTime};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::error::{CoreError, Result};
use crate::query::SqlType;

/// A parsed, typed bind value — core's own intermediate mirroring the nine target
/// `mssql_client::SqlValue` variants, but built only from `core`'s own deps
/// (`chrono`/`rust_decimal`/`uuid`), never the driver. `executor.rs` maps this to
/// the driver's `SqlValue` in one place (`sql_value_from_bind`), keeping the
/// driver confined to that one module (`PLAN.md` §3, `CLAUDE.md`).
#[derive(Debug, Clone, PartialEq)]
pub enum BindValue {
    Int(i32),
    BigInt(i64),
    Text(String),
    Bool(bool),
    Date(NaiveDate),
    DateTime(NaiveDateTime),
    Decimal(Decimal),
    Uuid(Uuid),
    Money(Decimal),
}

/// An execute-time param: a placeholder name, its substitution discriminator, and
/// the already-resolved concrete value.
///
/// `sql_type` is the discriminator — `Some` → bind, `None` → raw-text. `value` is
/// non-optional: "unset" handling is d28.3's concern, and scope resolution is
/// d28.4's; by execute time every param has a concrete value (`PLAN.md` §5). This
/// deliberately drops `Param`'s `last_value`/`scope` fields (later beads).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedParam {
    /// The placeholder, e.g. `"@cust"` (leading `@` included).
    pub name: String,
    /// `Some` → bind (typed, `sp_executesql`); `None` → raw-text splice.
    pub sql_type: Option<SqlType>,
    /// The already-resolved concrete value.
    pub value: String,
}

/// Parse a bind param's raw `&str` value into a typed [`BindValue`], per the
/// declared [`SqlType`].
///
/// A **closed 9-arm match with no `_` wildcard** — the payoff of `SqlType` being a
/// deliberately closed enum (`query.rs`): a future variant is a *compile error*
/// here, not a silent fallthrough into a broken binding. The driver derives the
/// `sp_executesql` type declaration from the resulting `SqlValue` variant at
/// runtime (`nvarchar` sized from the string, `decimal(38, scale)` from the parsed
/// value), so our whole job on the bind path is producing the right variant — no
/// precision/length metadata needed (the `query.rs` `TODO(d28.2)` is MOOT for the
/// 0.20.2 driver).
///
/// Empty string is valid only for `NVarChar` (an empty nvarchar); it is a parse
/// error for every other type. A parse error is a **pre-flight user error**
/// (before any server contact) → [`CoreError::Param`], naming the type and value.
///
/// Caveat (Money): `Decimal::from_str_exact` accepts an over-precise or
/// out-of-range money value here (e.g. `"12.345"`); the driver's `encode_money`
/// only rejects it at send time, surfacing later as [`CoreError::Query`]. So this
/// fn catches *syntax* errors pre-flight for all nine types, but Money
/// range/scale errors are not fully validatable here.
pub fn parse_bind_value(sql_type: SqlType, raw: &str) -> Result<BindValue> {
    match sql_type {
        SqlType::Int => raw
            .parse::<i32>()
            .map(BindValue::Int)
            .map_err(|e| param_err("int", raw, e)),
        SqlType::BigInt => raw
            .parse::<i64>()
            .map(BindValue::BigInt)
            .map_err(|e| param_err("bigint", raw, e)),
        // Always Ok — an empty string is a valid (empty) nvarchar.
        SqlType::NVarChar => Ok(BindValue::Text(raw.to_string())),
        // Strict set {0,1,true,false}, case-insensitive — never coerce "2"/"yes".
        SqlType::Bit => match raw.to_ascii_lowercase().as_str() {
            "1" | "true" => Ok(BindValue::Bool(true)),
            "0" | "false" => Ok(BindValue::Bool(false)),
            _ => Err(param_err("bit", raw, "expected 0, 1, true, or false")),
        },
        // NaiveDate's FromStr is ISO `%Y-%m-%d`.
        SqlType::Date => raw
            .parse::<NaiveDate>()
            .map(BindValue::Date)
            .map_err(|e| param_err("date", raw, e)),
        SqlType::DateTime2 => parse_datetime2(raw),
        SqlType::Decimal => Decimal::from_str_exact(raw)
            .map(BindValue::Decimal)
            .map_err(|e| param_err("decimal", raw, e)),
        SqlType::UniqueIdentifier => Uuid::parse_str(raw)
            .map(BindValue::Uuid)
            .map_err(|e| param_err("uniqueidentifier", raw, e)),
        SqlType::Money => Decimal::from_str_exact(raw)
            .map(BindValue::Money)
            .map_err(|e| param_err("money", raw, e)),
    }
}

/// `datetime2` accepts the formats a user actually types. chrono's
/// `NaiveDateTime: FromStr` requires the `T` separator and *rejects* a space, so a
/// bare `.parse()` is wrong for `"2024-03-17 12:34:56"`. Parse an explicit list,
/// first match wins; a date-only value becomes midnight.
fn parse_datetime2(raw: &str) -> Result<BindValue> {
    const FORMATS: &[&str] = &[
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ];
    for fmt in FORMATS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(raw, fmt) {
            return Ok(BindValue::DateTime(dt));
        }
    }
    // Date-only → midnight.
    if let Ok(d) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Ok(BindValue::DateTime(d.and_hms_opt(0, 0, 0).unwrap()));
    }
    Err(param_err(
        "datetime2",
        raw,
        "expected e.g. 'YYYY-MM-DD HH:MM:SS' or 'YYYY-MM-DD'",
    ))
}

/// Build a [`CoreError::Param`] naming the declared type and the bad value.
fn param_err(type_name: &str, raw: &str, detail: impl Display) -> CoreError {
    CoreError::Param(format!(
        "invalid value {raw:?} for {type_name} parameter: {detail}"
    ))
}

/// Splice raw-text fragments into `sql`, replacing each `@name` key from `raw` with
/// its value **literally** — no quoting, no escaping (injectable BY DESIGN).
///
/// A **single left-to-right pass** over the ORIGINAL `sql` (never `str::replace` in
/// a loop, which would rescan already-spliced text and re-match). At each `@`:
///   - `@@…` (system vars `@@ROWCOUNT`, `@@IDENTITY`): emitted literally.
///   - Otherwise the full identifier after `@` is read to its boundary *before*
///     lookup, so a `@col` key can never match a `@column` token (processing order
///     is irrelevant — no "longest first" needed). A matching raw key emits its
///     value; anything else (a bind `@name`, an unknown alias) is left literal.
///
/// Because a spliced value is emitted and never re-scanned, a raw value that itself
/// contains `@x` stays literal. The pass is **lexer-aware** (billz-7c9): a `@name` is
/// only recognized in NORMAL SQL context — inside single-quote strings (incl. `N'…'`
/// and the `''` escape), `[..]` / `"…"` quoted identifiers (`]]` / `""` escapes), `--`
/// line comments, and `/* */` block comments (which T-SQL NESTS), the text is copied
/// verbatim and no `@name` is spliced. `@@…` system vars stay literal, and a lone `@`
/// (or `@` before a non-identifier char) emits `@` literally — no hang, no panic. Bind
/// `@name`s left literal here compose correctly with `query_named`. This is the shared
/// lexical contract with the frontend `scanParamNames` (paramBarLogic.ts), kept in
/// lockstep by a mirrored corpus (the billz-7c9 splice_* tests).
///
/// All delimiters (`@ ' [ ] " - / *` and `\n`) are ASCII (< 0x80), so byte-scanning is
/// UTF-8-safe: none appears inside a multibyte sequence, and every slice boundary lands
/// on a char boundary.
pub fn splice_raw_text(sql: &str, raw: &[(&str, &str)]) -> String {
    // n=normal, string, line/block comment, bracket, double-quote identifier.
    enum Lex {
        Normal,
        SingleQuote,
        LineComment,
        BlockComment(u32),
        Bracket,
        DoubleQuote,
    }
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    let mut state = Lex::Normal;
    while i < bytes.len() {
        match state {
            // The `@` handling below is byte-identical to the pre-billz-7c9 splicer —
            // it is merely gated behind Normal state, so all splice_1..10 guarantees
            // (prefix-collision boundary, replace-all, @@, single-pass, lone-@) hold.
            Lex::Normal => match bytes[i] {
                b'@' => {
                    // `@@…` system var — emit both `@`s literally.
                    if i + 1 < bytes.len() && bytes[i + 1] == b'@' {
                        out.push_str("@@");
                        i += 2;
                    } else {
                        // Read the full identifier after `@`.
                        let start = i + 1;
                        let mut j = start;
                        while j < bytes.len() && is_ident_byte(bytes[j]) {
                            j += 1;
                        }
                        if j == start {
                            // Lone `@` (or `@` before a non-identifier char).
                            out.push('@');
                            i += 1;
                        } else {
                            let candidate = &sql[i..j]; // includes the leading `@`
                            match raw.iter().find(|(name, _)| *name == candidate) {
                                Some((_, value)) => out.push_str(value),
                                None => out.push_str(candidate),
                            }
                            i = j;
                        }
                    }
                }
                b'\'' => {
                    out.push('\'');
                    state = Lex::SingleQuote;
                    i += 1;
                }
                b'[' => {
                    out.push('[');
                    state = Lex::Bracket;
                    i += 1;
                }
                b'"' => {
                    out.push('"');
                    state = Lex::DoubleQuote;
                    i += 1;
                }
                b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                    out.push_str("--");
                    state = Lex::LineComment;
                    i += 2;
                }
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                    out.push_str("/*");
                    state = Lex::BlockComment(1);
                    i += 2;
                }
                _ => {
                    // Ordinary text — copy in one slice up to the next byte that could
                    // open a param/string/comment/identifier. A lone `-`/`/` lands here
                    // and is emitted literally (minus / division operators).
                    let start = i;
                    i += 1;
                    while i < bytes.len()
                        && !matches!(bytes[i], b'@' | b'\'' | b'[' | b'"' | b'-' | b'/')
                    {
                        i += 1;
                    }
                    out.push_str(&sql[start..i]);
                }
            },
            Lex::SingleQuote => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'\'' {
                    i += 1;
                }
                if i < bytes.len() {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        i += 2; // `''` escape — stay in the string
                    } else {
                        i += 1; // closing quote
                        state = Lex::Normal;
                    }
                }
                out.push_str(&sql[start..i]);
            }
            Lex::Bracket => {
                let start = i;
                while i < bytes.len() && bytes[i] != b']' {
                    i += 1;
                }
                if i < bytes.len() {
                    if i + 1 < bytes.len() && bytes[i + 1] == b']' {
                        i += 2; // `]]` escape
                    } else {
                        i += 1; // closing bracket
                        state = Lex::Normal;
                    }
                }
                out.push_str(&sql[start..i]);
            }
            Lex::DoubleQuote => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += 1;
                }
                if i < bytes.len() {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                        i += 2; // `""` escape
                    } else {
                        i += 1; // closing quote
                        state = Lex::Normal;
                    }
                }
                out.push_str(&sql[start..i]);
            }
            Lex::LineComment => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1; // consume the newline (itself normal)
                    state = Lex::Normal;
                }
                out.push_str(&sql[start..i]);
            }
            Lex::BlockComment(mut depth) => {
                let start = i;
                while i < bytes.len() {
                    if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                        depth += 1; // T-SQL block comments nest
                        i += 2;
                    } else if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            state = Lex::Normal;
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
                out.push_str(&sql[start..i]);
            }
        }
    }
    out
}

/// SQL identifier continuation bytes: `[A-Za-z0-9_]` plus `$`/`#`/`@` to be safe
/// (all ASCII).
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b'#' | b'@')
}

/// Split params into `(raw-text (name, value) pairs, bind param refs)` on the
/// `sql_type.is_some()` discriminator, preserving order within each. The raw pairs
/// feed [`splice_raw_text`]; the bind refs feed the typed `sp_executesql` path.
pub fn partition(params: &[ResolvedParam]) -> (Vec<(&str, &str)>, Vec<&ResolvedParam>) {
    let mut raw = Vec::new();
    let mut bind = Vec::new();
    for p in params {
        match p.sql_type {
            Some(_) => bind.push(p),
            None => raw.push((p.name.as_str(), p.value.as_str())),
        }
    }
    (raw, bind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_param_serde_is_camel_case() {
        let p = ResolvedParam {
            name: "@cust".into(),
            sql_type: Some(crate::query::SqlType::Int),
            value: "12345".into(),
        };
        let s = serde_json::to_string(&p).unwrap();
        assert!(s.contains(r#""sqlType":"int""#), "{s}");
        assert!(s.contains(r#""name":"@cust""#), "{s}");
        let back: ResolvedParam = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
        // raw-text param: sqlType null.
        let raw = ResolvedParam {
            name: "@col".into(),
            sql_type: None,
            value: "ord".into(),
        };
        assert!(
            serde_json::to_string(&raw)
                .unwrap()
                .contains(r#""sqlType":null"#)
        );
    }

    // ------------------------------------------------------------------
    // splice_raw_text
    // ------------------------------------------------------------------

    #[test]
    fn splice_1_single_raw_param() {
        assert_eq!(
            splice_raw_text("SELECT * FROM t ORDER BY @col", &[("@col", "name DESC")]),
            "SELECT * FROM t ORDER BY name DESC"
        );
    }

    #[test]
    fn splice_2_prefix_collision_both_present() {
        assert_eq!(
            splice_raw_text("@col, @column", &[("@col", "A"), ("@column", "B")]),
            "A, B"
        );
    }

    #[test]
    fn splice_3_prefix_collision_only_short_key() {
        // `@column` is read to its full boundary, so a `@col` key can't match it.
        assert_eq!(
            splice_raw_text("SELECT @column", &[("@col", "X")]),
            "SELECT @column"
        );
    }

    #[test]
    fn splice_4_underscore_digit_continuation_not_matched() {
        assert_eq!(
            splice_raw_text("SELECT @col_1", &[("@col", "X")]),
            "SELECT @col_1"
        );
    }

    #[test]
    fn splice_5_same_name_twice_both_replaced() {
        assert_eq!(
            splice_raw_text("@t a JOIN @t b", &[("@t", "dbo.orders")]),
            "dbo.orders a JOIN dbo.orders b"
        );
    }

    #[test]
    fn splice_6_system_var_untouched() {
        assert_eq!(
            splice_raw_text("SELECT @@ROWCOUNT, @dir", &[("@dir", "x")]),
            "SELECT @@ROWCOUNT, x"
        );
    }

    #[test]
    fn splice_7_single_pass_spliced_value_not_rescanned() {
        // The injected `@cust` in the value is emitted literally, NOT re-matched.
        assert_eq!(
            splice_raw_text("ORDER BY @col", &[("@col", "@cust DESC")]),
            "ORDER BY @cust DESC"
        );
    }

    #[test]
    fn splice_8_bind_name_left_literal() {
        assert_eq!(
            splice_raw_text("WHERE cust=@cust ORDER BY @dir", &[("@dir", "id")]),
            "WHERE cust=@cust ORDER BY id"
        );
    }

    #[test]
    fn splice_9_name_at_end_and_empty_raw_unchanged() {
        assert_eq!(splice_raw_text("SELECT @dir", &[]), "SELECT @dir");
    }

    #[test]
    fn splice_10_lone_at_and_at_before_non_ident() {
        // Bare `@` and `@,` emit literally (no hang/panic); `@dir` splices.
        assert_eq!(
            splice_raw_text("SELECT @ , @, @dir", &[("@dir", "x")]),
            "SELECT @ , @, x"
        );
    }

    #[test]
    fn splice_11_string_literal_left_literal() {
        // billz-7c9: a `@name` inside a string literal is NOT spliced — SQL Server
        // itself does not substitute `@params` inside string literals, so the lexer
        // leaves them alone (both raw keys AND bind names).
        assert_eq!(
            splice_raw_text("WHERE note = '@dir'", &[("@dir", "x")]),
            "WHERE note = '@dir'"
        );
        assert_eq!(
            splice_raw_text("WHERE note='@cust'", &[]),
            "WHERE note='@cust'"
        );
    }

    // ---- billz-7c9 lexer-safe corpus (mirrors paramBarLogic.test.ts scanParamNames) ----

    #[test]
    fn splice_12_n_prefixed_string_left_literal() {
        // #3: N'…' unicode string — the `N` is ordinary text, the `'` opens the string.
        assert_eq!(
            splice_raw_text("WHERE n = N'@dir'", &[("@dir", "x")]),
            "WHERE n = N'@dir'"
        );
    }

    #[test]
    fn splice_13_doubled_quote_escape_keeps_one_string() {
        // #4: '@a''@b' is a single string (the '' is an escaped quote); @c splices.
        assert_eq!(
            splice_raw_text(
                "SELECT '@a''@b', @c",
                &[("@a", "1"), ("@b", "2"), ("@c", "Z")]
            ),
            "SELECT '@a''@b', Z"
        );
    }

    #[test]
    fn splice_14_line_comment_left_literal() {
        // #5/#13: a `--` comment runs to end of line; @dir on the next line splices,
        // and the same name echoed in the comment stays literal.
        assert_eq!(
            splice_raw_text("SELECT 1 -- @x\nWHERE y=@z", &[("@x", "1"), ("@z", "9")]),
            "SELECT 1 -- @x\nWHERE y=9"
        );
        assert_eq!(
            splice_raw_text("ORDER BY @dir -- keep @dir", &[("@dir", "name DESC")]),
            "ORDER BY name DESC -- keep @dir"
        );
    }

    #[test]
    fn splice_15_block_comment_nested_left_literal() {
        // #6: simple block comment.
        assert_eq!(
            splice_raw_text("SELECT /* @a */ @b", &[("@a", "1"), ("@b", "2")]),
            "SELECT /* @a */ 2"
        );
        // #7: nested — a single `*/` does not exit the outer comment.
        assert_eq!(
            splice_raw_text(
                "SELECT /* @a /* @b */ @c */ @d",
                &[("@a", "X"), ("@b", "X"), ("@c", "X"), ("@d", "D")]
            ),
            "SELECT /* @a /* @b */ @c */ D"
        );
    }

    #[test]
    fn splice_16_bracketed_identifier_left_literal() {
        // #8: [@col] is a quoted identifier — not spliced; @real is.
        assert_eq!(
            splice_raw_text("SELECT [@col], @real", &[("@col", "C"), ("@real", "R")]),
            "SELECT [@col], R"
        );
        // #9: ]] is an escaped bracket, so the identifier closes at the final `]`.
        assert_eq!(
            splice_raw_text("SELECT [we]]ird @x], @y", &[("@x", "1"), ("@y", "2")]),
            "SELECT [we]]ird @x], 2"
        );
    }

    #[test]
    fn splice_17_the_reason_for_the_backend_fix() {
        // #14: a REAL raw-text param whose name ALSO appears inside a string literal.
        // Splices in normal context, stays literal inside the string — the case a
        // frontend-only fix cannot catch.
        assert_eq!(
            splice_raw_text("WHERE note='@dir' ORDER BY @dir", &[("@dir", "c DESC")]),
            "WHERE note='@dir' ORDER BY c DESC"
        );
    }

    #[test]
    fn splice_18_double_quote_and_unterminated_string() {
        // #15: "@col" quoted identifier skipped; @x splices.
        assert_eq!(
            splice_raw_text("SELECT \"@col\", @x", &[("@col", "C"), ("@x", "X")]),
            "SELECT \"@col\", X"
        );
        // #16: an unterminated string does not hang and leaves the tail literal.
        assert_eq!(
            splice_raw_text("WHERE a='@x", &[("@x", "y")]),
            "WHERE a='@x"
        );
    }

    // ------------------------------------------------------------------
    // parse_bind_value — all nine, happy + error
    // ------------------------------------------------------------------

    fn dec(s: &str) -> Decimal {
        Decimal::from_str_exact(s).unwrap()
    }

    #[test]
    fn parse_int_happy_and_errors() {
        assert_eq!(
            parse_bind_value(SqlType::Int, "42").unwrap(),
            BindValue::Int(42)
        );
        assert_eq!(
            parse_bind_value(SqlType::Int, "-7").unwrap(),
            BindValue::Int(-7)
        );
        assert!(parse_bind_value(SqlType::Int, "abc").is_err());
        assert!(parse_bind_value(SqlType::Int, "").is_err());
        // > i32::MAX — a real out-of-range error (SQL `int` is 32-bit).
        assert!(parse_bind_value(SqlType::Int, "9999999999").is_err());
    }

    #[test]
    fn parse_bigint_happy_and_error() {
        assert_eq!(
            parse_bind_value(SqlType::BigInt, "9000000000").unwrap(),
            BindValue::BigInt(9_000_000_000)
        );
        assert!(parse_bind_value(SqlType::BigInt, "x").is_err());
    }

    #[test]
    fn parse_nvarchar_always_ok_including_empty_and_unicode() {
        assert_eq!(
            parse_bind_value(SqlType::NVarChar, "héllo ☃").unwrap(),
            BindValue::Text("héllo ☃".into())
        );
        assert_eq!(
            parse_bind_value(SqlType::NVarChar, "").unwrap(),
            BindValue::Text("".into())
        );
    }

    #[test]
    fn parse_bit_strict_set() {
        for s in ["1", "true", "TRUE"] {
            assert_eq!(
                parse_bind_value(SqlType::Bit, s).unwrap(),
                BindValue::Bool(true)
            );
        }
        for s in ["0", "false", "False"] {
            assert_eq!(
                parse_bind_value(SqlType::Bit, s).unwrap(),
                BindValue::Bool(false)
            );
        }
        for s in ["2", "yes", ""] {
            assert!(
                parse_bind_value(SqlType::Bit, s).is_err(),
                "{s:?} must error"
            );
        }
    }

    #[test]
    fn parse_date_iso_and_errors() {
        assert_eq!(
            parse_bind_value(SqlType::Date, "2024-03-17").unwrap(),
            BindValue::Date(NaiveDate::from_ymd_opt(2024, 3, 17).unwrap())
        );
        assert!(parse_bind_value(SqlType::Date, "nope").is_err());
        assert!(parse_bind_value(SqlType::Date, "2024-13-01").is_err());
    }

    #[test]
    fn parse_datetime2_space_t_and_date_only() {
        let expect = NaiveDate::from_ymd_opt(2024, 3, 17)
            .unwrap()
            .and_hms_opt(12, 34, 56)
            .unwrap();
        // Space separator (chrono's FromStr rejects this — hence the format list).
        assert_eq!(
            parse_bind_value(SqlType::DateTime2, "2024-03-17 12:34:56").unwrap(),
            BindValue::DateTime(expect)
        );
        // `T` separator with fractional seconds.
        let expect_frac = NaiveDate::from_ymd_opt(2024, 3, 17)
            .unwrap()
            .and_hms_milli_opt(12, 34, 56, 123)
            .unwrap();
        assert_eq!(
            parse_bind_value(SqlType::DateTime2, "2024-03-17T12:34:56.123").unwrap(),
            BindValue::DateTime(expect_frac)
        );
        // Date-only → midnight.
        let midnight = NaiveDate::from_ymd_opt(2024, 3, 17)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert_eq!(
            parse_bind_value(SqlType::DateTime2, "2024-03-17").unwrap(),
            BindValue::DateTime(midnight)
        );
        assert!(parse_bind_value(SqlType::DateTime2, "nope").is_err());
    }

    #[test]
    fn parse_decimal_happy_and_error() {
        assert_eq!(
            parse_bind_value(SqlType::Decimal, "1234.5678").unwrap(),
            BindValue::Decimal(dec("1234.5678"))
        );
        assert!(parse_bind_value(SqlType::Decimal, "abc").is_err());
    }

    #[test]
    fn parse_uuid_happy_and_error() {
        let raw = "6F9619FF-8B86-D011-B42D-00C04FC964FF";
        assert_eq!(
            parse_bind_value(SqlType::UniqueIdentifier, raw).unwrap(),
            BindValue::Uuid(Uuid::parse_str(raw).unwrap())
        );
        assert!(parse_bind_value(SqlType::UniqueIdentifier, "not-guid").is_err());
    }

    #[test]
    fn parse_money_happy_error_and_overprecision_is_ok_preflight() {
        assert_eq!(
            parse_bind_value(SqlType::Money, "12.34").unwrap(),
            BindValue::Money(dec("12.34"))
        );
        assert!(parse_bind_value(SqlType::Money, "x").is_err());
        // (R1) 5-digit over-precision is Ok HERE — `from_str_exact` accepts it. The
        // driver's encode_money rejects it at SEND time (a `Query` error), so Money
        // range/scale is NOT fully validatable pre-flight.
        assert_eq!(
            parse_bind_value(SqlType::Money, "12.345").unwrap(),
            BindValue::Money(dec("12.345"))
        );
    }

    #[test]
    fn parse_error_message_names_type_and_value() {
        let err = parse_bind_value(SqlType::Int, "abc").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("int"), "{msg}");
        assert!(msg.contains("abc"), "{msg}");
    }

    // ------------------------------------------------------------------
    // partition
    // ------------------------------------------------------------------

    #[test]
    fn partition_splits_and_preserves_order() {
        let params = vec![
            ResolvedParam {
                name: "@dir".into(),
                sql_type: None,
                value: "x DESC".into(),
            },
            ResolvedParam {
                name: "@cust".into(),
                sql_type: Some(SqlType::Int),
                value: "42".into(),
            },
            ResolvedParam {
                name: "@tbl".into(),
                sql_type: None,
                value: "dbo.orders".into(),
            },
            ResolvedParam {
                name: "@lim".into(),
                sql_type: Some(SqlType::BigInt),
                value: "5".into(),
            },
        ];
        let (raw, bind) = partition(&params);
        assert_eq!(raw, vec![("@dir", "x DESC"), ("@tbl", "dbo.orders")]);
        let bind_names: Vec<&str> = bind.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(bind_names, vec!["@cust", "@lim"]);
    }
}
