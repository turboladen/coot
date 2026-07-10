//! Client-side batch splitting. `GO` is not T-SQL — it's a separator the client
//! honors and never sends (PLAN.md §6). Matching is deliberately simple for a
//! personal tool: a `GO` on a line by itself. GO inside a string literal or a
//! block comment is NOT detected (whole-line match only) and would mis-split —
//! an accepted v1 limitation. An optional repeat count (`GO 5`) is NOT honored;
//! such a line is treated as ordinary SQL (documented; a test pins this).
//!
//! Both public fns return **borrowed slices of the input** (zero-copy) and share
//! one private line-walk (`segments`). Lines are indexed with `sql.split('\n')`
//! — NOT `str::lines()` — so the line count matches CodeMirror exactly, including
//! the phantom empty last line after a trailing newline (`"…\n".split('\n')`
//! yields a trailing `""`; `str::lines()` drops it). CM normalizes all line
//! breaks to `\n` in `doc.toString()`, so a Rust line index over the `\n`-joined
//! `sql` matches `doc.lineAt(head).number` even for CRLF/lone-`\r` input.

/// A maximal run of consecutive non-`GO` lines, as a byte slice of the source
/// plus its 1-based inclusive line span. Whitespace-only runs are retained here
/// (so `batch_at_line` can map a cursor in a blank gap to `""`); `split_batches`
/// filters them out.
struct Segment<'a> {
    slice: &'a str,
    first_line: usize, // 1-based, inclusive
    last_line: usize,  // 1-based, inclusive
}

/// A line is a `GO` separator iff it is exactly `GO` (case-insensitive) once
/// surrounding whitespace is trimmed. `trim()` also eats a trailing `\r`, so a
/// CRLF `"GO\r"` line matches; `"GO 5"` / `"GO -- note"` / `"GONZO"` do not.
fn is_go_line(piece: &str) -> bool {
    piece.trim().eq_ignore_ascii_case("GO")
}

/// Walk `sql` once over `split('\n')` lines, grouping the non-`GO` lines into
/// `Segment`s. Byte offsets are accumulated so each segment slice is the exact
/// substring between separators (internal/leading/trailing whitespace and `\r`
/// preserved verbatim, so the server sees faithful line numbers).
fn segments(sql: &str) -> Vec<Segment<'_>> {
    let mut segs = Vec::new();
    let mut start_byte = 0usize; // byte offset of the current piece's start
    // The open segment being accumulated: (start_byte, start_line, end_byte).
    let mut open: Option<(usize, usize, usize)> = None;

    for (idx, piece) in sql.split('\n').enumerate() {
        let line_no = idx + 1;
        let piece_start = start_byte;
        let piece_end = start_byte + piece.len();

        if is_go_line(piece) {
            if let Some((sb, sl, eb)) = open.take() {
                segs.push(Segment {
                    slice: &sql[sb..eb],
                    first_line: sl,
                    last_line: line_no - 1,
                });
            }
        } else {
            match &mut open {
                Some((_sb, _sl, eb)) => *eb = piece_end,
                None => open = Some((piece_start, line_no, piece_end)),
            }
        }

        // Advance past this piece and the `\n` that split() removed. After the
        // final piece there is no more iteration, so the extra +1 is harmless.
        start_byte = piece_end + 1;
    }

    if let Some((sb, sl, eb)) = open.take() {
        let last_line = sql.split('\n').count();
        segs.push(Segment {
            slice: &sql[sb..eb],
            first_line: sl,
            last_line,
        });
    }
    segs
}

/// Split `sql` into batches on `GO` separator lines. A separator is a line
/// matching `^\s*GO\s*$` case-insensitively. The `GO` lines are dropped; batches
/// that are entirely whitespace are dropped. Each returned slice is the exact
/// substring between separators (internal/leading/trailing whitespace within a
/// kept batch is preserved, so the server sees faithful line numbers).
///
/// No GO → one batch (the whole doc). GO-only / empty / whitespace-only → `[]`.
pub fn split_batches(sql: &str) -> Vec<&str> {
    segments(sql)
        .into_iter()
        .map(|s| s.slice)
        .filter(|slice| !slice.trim().is_empty())
        .collect()
}

/// The batch containing 1-based `line` — the "current batch" rule when there is
/// no selection. `line` is a CodeMirror line number (`doc.lineAt(head).number`),
/// clamped to `[1, line_count]` where `line_count == sql.split('\n').count()`.
///
/// Returns the whole doc when there are no `GO` lines, `""` when the containing
/// region is empty/whitespace (GO-only doc, or the cursor in a blank inter-`GO`
/// gap). When `line` falls on a `GO` separator line, the batch it terminates
/// (the nearest preceding one) is returned.
pub fn batch_at_line(sql: &str, line: usize) -> &str {
    let segs = segments(sql);
    let line_count = sql.split('\n').count(); // always >= 1
    let line = line.clamp(1, line_count);

    // A non-GO line lands inside exactly one segment.
    if let Some(seg) = segs
        .iter()
        .find(|s| line >= s.first_line && line <= s.last_line)
    {
        return if seg.slice.trim().is_empty() {
            ""
        } else {
            seg.slice
        };
    }

    // Otherwise `line` is a GO separator line: attribute it to the nearest
    // preceding segment (segments are in increasing line order, so `.rev()`
    // finds the closest one below `line`).
    match segs.iter().rev().find(|s| s.last_line < line) {
        Some(s) if !s.slice.trim().is_empty() => s.slice,
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // split_batches
    // ---------------------------------------------------------------

    #[test]
    fn no_go_is_one_batch_of_the_whole_doc() {
        assert_eq!(
            split_batches("SELECT 1\nSELECT 2"),
            vec!["SELECT 1\nSELECT 2"]
        );
    }

    #[test]
    fn one_go_splits_into_two() {
        assert_eq!(
            split_batches("SELECT 1\nGO\nSELECT 2"),
            vec!["SELECT 1", "SELECT 2"]
        );
    }

    #[test]
    fn three_go_yields_the_non_empty_batches() {
        assert_eq!(split_batches("A\nGO\nB\nGO\nC"), vec!["A", "B", "C"]);
    }

    #[test]
    fn leading_go_drops_the_empty_first_batch() {
        assert_eq!(split_batches("GO\nSELECT 1"), vec!["SELECT 1"]);
    }

    #[test]
    fn trailing_go_drops_the_empty_last_batch() {
        assert_eq!(split_batches("SELECT 1\nGO"), vec!["SELECT 1"]);
    }

    #[test]
    fn consecutive_go_drops_the_empty_middle() {
        assert_eq!(
            split_batches("SELECT 1\nGO\nGO\nSELECT 2"),
            vec!["SELECT 1", "SELECT 2"]
        );
    }

    #[test]
    fn go_only_doc_is_empty() {
        assert!(split_batches("GO").is_empty());
    }

    #[test]
    fn empty_and_whitespace_only_are_empty() {
        assert!(split_batches("").is_empty());
        assert!(split_batches("  \n\t").is_empty());
    }

    #[test]
    fn go_with_surrounding_whitespace_and_tabs_is_a_separator() {
        assert_eq!(
            split_batches("SELECT 1\n   GO   \nSELECT 2"),
            vec!["SELECT 1", "SELECT 2"]
        );
        assert_eq!(
            split_batches("SELECT 1\n\tGO\t\nSELECT 2"),
            vec!["SELECT 1", "SELECT 2"]
        );
    }

    #[test]
    fn go_is_case_insensitive() {
        for go in ["go", "Go", "gO", "GO"] {
            assert_eq!(
                split_batches(&format!("SELECT 1\n{go}\nSELECT 2")),
                vec!["SELECT 1", "SELECT 2"],
                "separator {go:?}"
            );
        }
    }

    #[test]
    fn crlf_doc_splits_and_keeps_internal_carriage_returns() {
        let batches = split_batches("SELECT 1\r\nGO\r\nSELECT 2\r\n");
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0], "SELECT 1\r");
        // The last kept slice retains its trailing CRLF verbatim.
        assert_eq!(batches[1], "SELECT 2\r\n");
    }

    #[test]
    fn go_substrings_and_quoted_go_are_not_separators() {
        for sql in ["GONZO", "SELECT 'GO'", "EXEC GO_proc", "-- GO"] {
            assert_eq!(split_batches(sql), vec![sql], "not a separator: {sql:?}");
        }
    }

    #[test]
    fn go_with_repeat_count_is_ordinary_sql() {
        // `GO 5` (repeat count) is NOT honored — treated as one ordinary batch.
        assert_eq!(
            split_batches("SELECT 1\nGO 5\nSELECT 2"),
            vec!["SELECT 1\nGO 5\nSELECT 2"]
        );
    }

    #[test]
    fn comment_only_batch_is_preserved() {
        // A comment-only batch is not whitespace → kept, sent to the server.
        assert_eq!(
            split_batches("-- just a note\nGO\nSELECT 1"),
            vec!["-- just a note", "SELECT 1"]
        );
    }

    #[test]
    fn kept_batch_preserves_internal_blank_lines_and_indentation() {
        let sql = "SELECT 1\n\n    AND 2\nGO\nSELECT 3";
        assert_eq!(
            split_batches(sql),
            vec!["SELECT 1\n\n    AND 2", "SELECT 3"]
        );
    }

    // ---------------------------------------------------------------
    // batch_at_line (1-based, split('\n') indexing)
    // ---------------------------------------------------------------

    #[test]
    fn at_line_no_go_returns_whole_doc_for_any_line() {
        let sql = "SELECT 1\nSELECT 2";
        assert_eq!(batch_at_line(sql, 1), sql);
        assert_eq!(batch_at_line(sql, 2), sql);
    }

    #[test]
    fn at_line_first_and_last() {
        let sql = "SELECT 1\nGO\nSELECT 2";
        assert_eq!(batch_at_line(sql, 1), "SELECT 1"); // line == 1 → first
        assert_eq!(batch_at_line(sql, 3), "SELECT 2"); // line == line_count → last
    }

    #[test]
    fn at_line_beyond_end_clamps_to_last() {
        let sql = "SELECT 1\nGO\nSELECT 2";
        assert_eq!(batch_at_line(sql, 99), "SELECT 2");
    }

    #[test]
    fn at_line_zero_clamps_to_first() {
        // Defensive: `line` is 1-based, but a 0 must not underflow the clamp.
        let sql = "SELECT 1\nGO\nSELECT 2";
        assert_eq!(batch_at_line(sql, 0), "SELECT 1");
    }

    #[test]
    fn at_line_in_each_of_two_batches() {
        let sql = "SELECT 1\nGO\nSELECT 2";
        assert_eq!(batch_at_line(sql, 1), "SELECT 1"); // in first
        assert_eq!(batch_at_line(sql, 3), "SELECT 2"); // in second
    }

    #[test]
    fn at_line_on_the_go_separator_returns_the_preceding_batch() {
        let sql = "SELECT 1\nGO\nSELECT 2";
        assert_eq!(batch_at_line(sql, 2), "SELECT 1"); // line 2 is the GO line
    }

    #[test]
    fn at_line_go_only_doc_is_empty() {
        assert_eq!(batch_at_line("GO", 1), "");
    }

    #[test]
    fn at_line_in_whitespace_gap_is_empty() {
        // Blank inter-GO gap on line 3.
        let sql = "SELECT 1\nGO\n   \nGO\nSELECT 2";
        assert_eq!(batch_at_line(sql, 3), "");
    }

    #[test]
    fn at_line_trailing_newline_phantom_last_line_maps_to_last_batch() {
        // The split('\n') regression guard: "SELECT 1\nGO\nSELECT 2\n" has FOUR
        // lines by CodeMirror's count (trailing empty line 4). str::lines() would
        // report 3 and the caret on line 4 would clamp wrong. With split('\n')
        // line 4 falls inside the last segment.
        let sql = "SELECT 1\nGO\nSELECT 2\n";
        assert_eq!(sql.split('\n').count(), 4);
        assert_eq!(batch_at_line(sql, 4), "SELECT 2\n");
        // And it is exactly the last batch split_batches produces.
        assert_eq!(batch_at_line(sql, 4), *split_batches(sql).last().unwrap());
    }

    #[test]
    fn at_line_crlf_doc_selects_the_right_batch() {
        let sql = "SELECT 1\r\nGO\r\nSELECT 2\r\n";
        assert_eq!(batch_at_line(sql, 1), "SELECT 1\r");
        assert_eq!(batch_at_line(sql, 3), "SELECT 2\r\n");
    }
}
