//! Plan → verdict. Pure: no I/O, no driver, no server.
//!
//! [`PlanWarning`] is what the plan REPORTS; [`Finding`] is what we CONCLUDE.
//! Keeping them apart means revising our judgement — which we expect to do,
//! repeatedly, once this has been pointed at real generated SQL — never churns
//! the parser's output type.
//!
//! # The thresholds are a hypothesis
//!
//! Four named constants, in one place, deliberately. The design spec §6 lists
//! the verdict rules as provisional and expects them to change after first real
//! use; `billz-7u0` tracks revisiting the numbers. Do **not** grow this into a
//! configurable rule engine before there is evidence — that is the failure mode
//! the spec names.
//!
//! Every threshold sits ABOVE everything the fixtures contain (the largest read
//! in any captured plan is 25,871 rows; the costliest statement is 0.754). That
//! is deliberate — `no_real_fixture_produces_a_volume_or_cost_finding` is the
//! false-positive guard, and it is the strongest evidence these numbers have.
//! Their `true` branches are reached only by hand-built plans.
//!
//! # What is NOT backed by a captured plan
//!
//! Held to the same standard as [`parse`](crate::plan::parse), which says at
//! each definition whether a path has ever been seen on the wire:
//!
//! - Of the four [`PlanWarning`] variants, only `ImplicitConversion` has a
//!   specimen (`scan.sqlplan`, three of them). `NoJoinPredicate`,
//!   `SpillToTempDb`, and `UnmatchedIndex` have **never been captured**
//!   (`billz-e75`), so their gradings below are a belief about warnings that may
//!   not arrive at all in the shape we imagine.
//! - The tests for those three hand-CONSTRUCT a [`PlanWarning`]. That is one
//!   step further from reality than `parse.rs`'s schema-derived XML, which at
//!   least encodes a belief about the wire format: these prove only that the
//!   MAPPING from warning to finding is what we wrote, never that such a warning
//!   exists.
//! - No fixture contains `<MissingIndexes>` at all, so [`MISSING_INDEX_IMPACT`]
//!   is likewise exercised only by hand-built values.

use crate::plan::model::{
    Finding, FindingKind, MissingIndex, PlanNode, PlanStatement, PlanVerdict, PlanWarning,
    QueryPlan, Severity,
};

/// Rows READ (not returned) at or above which a scan is worth reporting.
///
/// Rows read is the work done; rows returned is what survived. See
/// [`rows_read`] for why this is the better of the two signals.
pub const LARGE_SCAN_ROWS_READ: f64 = 100_000.0;

/// Rows read per row returned at or above which a large scan is mostly waste,
/// which promotes it from `Caution` to `Problem`.
///
/// This ratio deliberately does NOT trigger a finding on its own. At real
/// fixture sizes it is not discriminating: `join.sqlplan` node 5 reads 1,153 to
/// return 10.0021 (115×) and `scan.sqlplan` node 11 reads 218 to return 1
/// (218×), and both are ordinary row-goal / selective-filter shapes with nothing
/// wrong with them. Gating on volume FIRST is what makes the ratio usable.
pub const WASTEFUL_READ_RATIO: f64 = 100.0;

/// Total estimated subtree cost across the batch at or above which the plan is
/// worth reporting. SQL Server's cost unit is a legacy seconds-ish figure whose
/// default parallelism threshold is 5; 10 is decisively past "trivial".
pub const EXPENSIVE_COST: f64 = 10.0;

/// The server's own 0–100 estimate of how much a missing index would cut the
/// statement's cost, at or above which we report it. Real impacts cluster at
/// 90+; this exists to drop the low-impact suggestions that ride along beside a
/// genuine one.
pub const MISSING_INDEX_IMPACT: f64 = 50.0;

/// The ONE `@ConvertIssue` value we have a captured specimen for
/// (`scan.sqlplan`, ×3): a conversion that skews the row estimate but does not
/// stop an index seek.
///
/// This is an ALLOWLIST, and the direction matters. Everything else — the
/// index-blocking `"Seek Plan"`, any value a future server version invents, and
/// a missing attribute — grades as `Problem`. Listing the bad values instead
/// would mean writing down a string we have never seen, where a typo is a
/// permanent silent DOWNGRADE of the headline finding, failing nothing. Written
/// this way a typo makes `scan.sqlplan`'s three converts jump to `Problem` and
/// two fixture-backed tests fail on the spot. We do not own the server's set of
/// issue strings, so the only thing we can honestly enumerate is the one benign
/// value we have observed.
const BENIGN_CONVERT_ISSUE: &str = "Cardinality Estimate";

/// Judge a parsed plan.
///
/// Severity is the maximum severity of the findings, `Ok` when there are none —
/// `Severity` derives `Ord` as `Ok < Caution < Problem` for exactly this.
///
/// Findings come out in a DETERMINISTIC order, which is the verdict card's
/// display order and the order two tenants' verdicts diff against each other:
/// per statement in document order — missing indexes, then large scans in
/// operator pre-order, then warnings in [`PlanStatement::all_warnings`] order —
/// and finally the one batch-level cost finding. It is not a priority ranking;
/// sorting for presentation is the UI's job.
///
/// Findings from a multi-statement batch are FLATTENED with no statement
/// attribution, because [`PlanVerdict`] has nowhere to put it. Fine for the one
/// or two ad-hoc statements this explains; worth knowing before pointing it at a
/// long script.
pub fn judge(plan: &QueryPlan) -> PlanVerdict {
    let total_cost: f64 = plan.statements.iter().map(|s| s.subtree_cost).sum();

    let mut findings = Vec::new();
    for stmt in &plan.statements {
        statement_findings(stmt, &mut findings);
    }

    if total_cost >= EXPENSIVE_COST {
        findings.push(Finding {
            kind: FindingKind::ExpensivePlan,
            severity: Severity::Caution,
            message: format!("estimated cost {total_cost:.1}"),
            evidence: None,
        });
    }

    let severity = findings
        .iter()
        .map(|f| f.severity)
        .max()
        .unwrap_or(Severity::Ok);

    PlanVerdict {
        severity,
        total_cost,
        findings,
    }
}

fn statement_findings(stmt: &PlanStatement, out: &mut Vec<Finding>) {
    for mi in &stmt.missing_indexes {
        if mi.impact >= MISSING_INDEX_IMPACT {
            out.push(missing_index_finding(mi));
        }
    }

    if let Some(root) = &stmt.root {
        visit_scans(root, out);
    }

    // THE single entry point for warnings, and not a stylistic preference:
    // ShowPlanXML attaches a warning to `<QueryPlan>` or to a `<RelOp>` depending
    // on its kind, and `scan.sqlplan` — the only fixture with any — has all three
    // of its warnings at STATEMENT level. Walking `root` here instead would find
    // nothing at all in it, silently.
    out.extend(stmt.all_warnings().map(warning_finding));
}

fn missing_index_finding(mi: &MissingIndex) -> Finding {
    Finding {
        kind: FindingKind::MissingIndex,
        severity: Severity::Problem,
        message: format!(
            "missing index would cut cost ~{:.0}% ({})",
            mi.impact,
            mi.columns.join(", ")
        ),
        evidence: Some(mi.table.clone()),
    }
}

/// Pre-order, and recursive for the same reason [`rel_op`](crate::plan::parse)
/// is: this walks OPERATOR depth, which the optimizer keeps small (20 in
/// `scan.sqlplan`, the deepest plan we have), not the unbounded element depth of
/// a predicate tree.
fn visit_scans(node: &PlanNode, out: &mut Vec<Finding>) {
    if is_scan(&node.physical_op) {
        let read = rows_read(node);
        if read >= LARGE_SCAN_ROWS_READ {
            let returned = node.est_rows;
            // `max(1.0)` is INERT, not defensive: the branch above already
            // requires `read >= 100_000`, so if the clamp ever binds the ratio is
            // at least 100,000 and the escalation fires regardless. It is here so
            // a plan whose `EstimateRows` attribute was missing (`parse.rs`
            // defaults numerics to 0.0) yields a number rather than `inf`.
            // Rewriting it as an `if` changes no behaviour.
            let (severity, message) = if read / returned.max(1.0) >= WASTEFUL_READ_RATIO {
                (
                    Severity::Problem,
                    format!(
                        "{} reads {read:.0} rows to return {returned:.0}",
                        node.physical_op
                    ),
                )
            } else {
                (
                    Severity::Caution,
                    format!("{} reads {read:.0} rows", node.physical_op),
                )
            };
            out.push(Finding {
                kind: FindingKind::LargeScan,
                severity,
                message,
                // The FULL `[db].[schema].[table].[index]` name, database
                // included — a reader of the card wants to know which one. Only
                // `fingerprint::shape` strips it, and for its own reasons.
                evidence: node.object.clone(),
            });
        }
    }

    for child in &node.children {
        visit_scans(child, out);
    }
}

/// Grade one warning.
///
/// Takes `&PlanWarning` and nothing else, because
/// [`PlanStatement::all_warnings`] — the only correct way to reach every warning
/// — yields warnings without their operator. That costs `NoJoinPredicate` and
/// `SpillToTempDb` their `evidence`, which is the right trade: reading only the
/// operator tree to recover an object name would go blind on every
/// statement-level warning, which is all of the ones we have ever captured.
///
/// TODO(billz-e75): three of these four arms have no captured specimen. Capture
/// a `CROSS JOIN` with no predicate and a big `ORDER BY` under a low memory
/// grant, then revisit both the gradings and the `evidence: None` above.
fn warning_finding(warning: &PlanWarning) -> Finding {
    match warning {
        PlanWarning::ImplicitConversion {
            expression,
            convert_issue,
        } => {
            let benign = convert_issue.as_deref() == Some(BENIGN_CONVERT_ISSUE);
            Finding {
                kind: FindingKind::ImplicitConversion,
                severity: if benign {
                    Severity::Caution
                } else {
                    Severity::Problem
                },
                message: if benign {
                    format!("implicit conversion skews row estimates ({BENIGN_CONVERT_ISSUE})")
                } else {
                    match convert_issue.as_deref() {
                        Some(issue) => {
                            format!("implicit conversion may prevent an index seek ({issue})")
                        }
                        None => "implicit conversion may prevent an index seek".to_string(),
                    }
                },
                evidence: Some(expression.clone()),
            }
        }
        // UNVERIFIED — no captured specimen. Graded `Problem` on the strength of
        // the ShowPlanXML schema alone, because an accidental cartesian product
        // is among the loudest things this feature exists to catch and its cost
        // estimate is exactly what you cannot trust about it.
        PlanWarning::NoJoinPredicate => Finding {
            kind: FindingKind::NoJoinPredicate,
            severity: Severity::Problem,
            message: "join has no predicate (cartesian product)".to_string(),
            evidence: None,
        },
        // UNVERIFIED — no captured specimen, and this one may be UNREACHABLE
        // here: a spill is an actual-execution phenomenon, and an ESTIMATED plan
        // essentially never reports one. Which is a second reason we have none.
        PlanWarning::SpillToTempDb => Finding {
            kind: FindingKind::SpillToTempDb,
            severity: Severity::Caution,
            message: "operator spills to tempdb".to_string(),
            evidence: None,
        },
        // UNVERIFIED — no captured specimen.
        PlanWarning::UnmatchedIndex { index } => Finding {
            kind: FindingKind::UnmatchedIndex,
            severity: Severity::Caution,
            message: "an index could not be matched".to_string(),
            evidence: Some(index.clone()),
        },
    }
}

/// Scans read more than they need; seeks do not. Substring so "Table Scan",
/// "Index Scan", "Clustered Index Scan" and "Columnstore Index Scan" all
/// qualify while no `… Seek` does.
///
/// "Constant Scan" matches too but carries no `EstimatedRowsRead` and returns
/// about one row, so [`LARGE_SCAN_ROWS_READ`] keeps it silent.
fn is_scan(physical_op: &str) -> bool {
    physical_op.contains("Scan")
}

/// Rows this operator READS.
///
/// `est_rows_read` is the better wasteful-scan signal and a raw returned-row
/// count cannot see it: `join.sqlplan` returns 10 while reading 1,153, and
/// `scan.sqlplan` node 11 returns 1 while reading 218. The inverse matters too —
/// a returned-row threshold fires on `SELECT * FROM BigTable`, where nothing is
/// wrong that an index could fix.
///
/// `None` means "this operator accesses no data", never "read nothing", so the
/// fallback applies only to something already identified as a scan whose
/// `EstimatedRowsRead` the server omitted — never observed; all 14 scan and seek
/// operators across the fixtures carry it. For a scan, rows returned ≤ rows
/// read, so the fallback can only UNDER-report. It cannot inflate.
fn rows_read(node: &PlanNode) -> f64 {
    node.est_rows_read.unwrap_or(node.est_rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::parse::parse_plan;

    /// The five REAL fixtures, captured from a live SQL Server by
    /// `just dump-plans`. Assertion values come from the files.
    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/plans/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_else(|e| panic!("fixture {name}: {e}"))
    }

    fn judge_fixture(name: &str) -> PlanVerdict {
        judge(&parse_plan(&fixture(name)).unwrap())
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// A hand-built operator. NOT captured — every test using this is exercising
    /// the mapping, not a real plan.
    fn node(physical_op: &str, est_rows: f64, est_rows_read: Option<f64>) -> PlanNode {
        PlanNode {
            physical_op: physical_op.into(),
            logical_op: physical_op.into(),
            object: Some("[master].[dbo].[Orders].[PK_Orders]".into()),
            est_rows,
            est_rows_read,
            est_cost: 1.0,
            subtree_cost: 1.0,
            warnings: vec![],
            children: vec![],
        }
    }

    /// A hand-built statement. NOT captured.
    fn statement(root: Option<PlanNode>, subtree_cost: f64) -> PlanStatement {
        PlanStatement {
            text: "SELECT 1".into(),
            subtree_cost,
            est_rows: 1.0,
            root,
            warnings: vec![],
            missing_indexes: vec![],
        }
    }

    /// A hand-built one-statement plan around `root`. NOT captured.
    fn plan_of(root: Option<PlanNode>, subtree_cost: f64) -> QueryPlan {
        QueryPlan {
            statements: vec![statement(root, subtree_cost)],
        }
    }

    fn kinds(v: &PlanVerdict) -> Vec<FindingKind> {
        v.findings.iter().map(|f| f.kind).collect()
    }

    // ---------------------------------------------------------------- fixtures

    #[test]
    fn a_cheap_seek_is_ok_with_no_findings() {
        let v = judge_fixture("seek.sqlplan");
        assert_eq!(v.severity, Severity::Ok);
        assert_eq!(v.findings, vec![]);
        assert!(close(v.total_cost, 0.00328328), "got {}", v.total_cost);
    }

    #[test]
    fn the_statement_level_convert_warnings_are_found() {
        // THE guard that `judge` consults `all_warnings()` rather than walking
        // the operator tree. All three of `scan.sqlplan`'s warnings hang off
        // `<QueryPlan>`, a SIBLING of the root `<RelOp>`; its 32 operators carry
        // none. A judge that walks only the tree returns an EMPTY findings list
        // here and looks perfectly healthy doing it.
        //
        // The whole vector is asserted, not just the kinds: the order is the
        // card's display order and the cross-tenant diff order.
        let v = judge_fixture("scan.sqlplan");
        assert!(close(v.total_cost, 0.75402), "got {}", v.total_cost);
        assert_eq!(
            v.findings,
            vec![
                Finding {
                    kind: FindingKind::ImplicitConversion,
                    severity: Severity::Caution,
                    message: "implicit conversion skews row estimates (Cardinality Estimate)"
                        .into(),
                    evidence: Some("CONVERT(bigint,[sov].[value],0)".into()),
                },
                Finding {
                    kind: FindingKind::ImplicitConversion,
                    severity: Severity::Caution,
                    message: "implicit conversion skews row estimates (Cardinality Estimate)"
                        .into(),
                    evidence: Some("CONVERT(int,[sov4].[value],0)".into()),
                },
                Finding {
                    kind: FindingKind::ImplicitConversion,
                    severity: Severity::Caution,
                    message: "implicit conversion skews row estimates (Cardinality Estimate)"
                        .into(),
                    evidence: Some("CONVERT(nvarchar(128),[sov2].[value],0)".into()),
                },
            ]
        );
    }

    #[test]
    fn cardinality_estimate_converts_are_not_graded_as_problems() {
        // The named regression for a false positive an earlier review caught:
        // every convert we have ever captured is `Cardinality Estimate`, which
        // skews row estimates but does NOT stop a seek. Grading all converts
        // alike makes our only warning fixture look like the headline problem.
        //
        // This is also what makes BENIGN_CONVERT_ISSUE's spelling self-checking —
        // a typo there flips this fixture to `Problem` and fails here.
        let v = judge_fixture("scan.sqlplan");
        assert_eq!(v.severity, Severity::Caution);
    }

    #[test]
    fn total_cost_sums_every_statement() {
        // 0.0495128 + 0.0032832, both read off `two-statements.sqlplan`.
        let v = judge_fixture("two-statements.sqlplan");
        assert!(close(v.total_cost, 0.052796), "got {}", v.total_cost);
    }

    #[test]
    fn no_real_fixture_produces_a_volume_or_cost_finding() {
        // The false-positive sweep, and the strongest evidence the three volume
        // and cost thresholds have. Across every operator in all five captured
        // plans — including the `TOP 1` seek that reads 13 rows to return 1, the
        // index scan that reads 1,153 to return 10, and the clustered scan that
        // reads 25,871 to return 410 — nothing but the converts fires.
        for name in ["seek.sqlplan", "aggregate.sqlplan", "join.sqlplan"] {
            let v = judge_fixture(name);
            assert_eq!(v.findings, vec![], "{name}");
            assert_eq!(v.severity, Severity::Ok, "{name}");
        }
        let v = judge_fixture("two-statements.sqlplan");
        assert_eq!(v.findings, vec![]);

        // `scan.sqlplan` is the one fixture with findings, and every one of them
        // is a convert — no LargeScan, no ExpensivePlan.
        let v = judge_fixture("scan.sqlplan");
        assert_eq!(
            kinds(&v),
            vec![FindingKind::ImplicitConversion; 3],
            "got {:?}",
            v.findings
        );
    }

    #[test]
    fn a_document_with_no_statements_is_ok() {
        let v = judge(&parse_plan("<html><body>nope</body></html>").unwrap());
        assert_eq!(v.severity, Severity::Ok);
        assert_eq!(v.findings, vec![]);
        assert!(close(v.total_cost, 0.0), "got {}", v.total_cost);
    }

    // ------------------------------------------------- thresholds (hand-built)

    #[test]
    fn a_large_wasteful_scan_is_a_problem() {
        let v = judge(&plan_of(
            Some(node("Clustered Index Scan", 12.0, Some(5_000_000.0))),
            1.0,
        ));
        assert_eq!(v.severity, Severity::Problem);
        assert_eq!(v.findings.len(), 1);
        let f = &v.findings[0];
        assert_eq!(f.kind, FindingKind::LargeScan);
        assert_eq!(
            f.message,
            "Clustered Index Scan reads 5000000 rows to return 12"
        );
        // Evidence must name the object so the card can say WHERE, and it keeps
        // the database component that `fingerprint::shape` strips.
        assert_eq!(
            f.evidence.as_deref(),
            Some("[master].[dbo].[Orders].[PK_Orders]")
        );
    }

    #[test]
    fn a_large_but_not_wasteful_scan_is_only_a_caution() {
        // `SELECT * FROM BigTable`: reads a lot, returns a lot, and no index
        // would change that. A raw returned-row threshold cannot tell this apart
        // from the test above; the read/return ratio is the whole point.
        let v = judge(&plan_of(
            Some(node("Table Scan", 5_000_000.0, Some(5_000_000.0))),
            1.0,
        ));
        assert_eq!(v.severity, Severity::Caution);
        assert_eq!(kinds(&v), vec![FindingKind::LargeScan]);
        assert_eq!(v.findings[0].message, "Table Scan reads 5000000 rows");
    }

    #[test]
    fn the_large_scan_threshold_fires_at_the_constant_and_not_below_it() {
        let at = judge(&plan_of(
            Some(node("Table Scan", 1.0, Some(LARGE_SCAN_ROWS_READ))),
            1.0,
        ));
        assert_eq!(kinds(&at), vec![FindingKind::LargeScan]);

        let below = judge(&plan_of(
            Some(node("Table Scan", 1.0, Some(LARGE_SCAN_ROWS_READ - 1.0))),
            1.0,
        ));
        assert_eq!(
            below.findings,
            vec![],
            "just below the threshold must be silent"
        );
    }

    #[test]
    fn the_wasteful_ratio_promotes_at_the_constant_and_not_below_it() {
        let read = 1_000_000.0;
        let at = judge(&plan_of(
            Some(node("Table Scan", read / WASTEFUL_READ_RATIO, Some(read))),
            1.0,
        ));
        assert_eq!(at.severity, Severity::Problem);

        let below = judge(&plan_of(
            Some(node(
                "Table Scan",
                read / (WASTEFUL_READ_RATIO - 1.0),
                Some(read),
            )),
            1.0,
        ));
        assert_eq!(below.severity, Severity::Caution);
        assert_eq!(kinds(&below), vec![FindingKind::LargeScan]);
    }

    #[test]
    fn a_seek_is_never_a_large_scan_however_many_rows_it_reads() {
        // The negative side of the `contains("Scan")` rule. A seek reading five
        // million rows is a row-goal artefact, not a scan.
        let v = judge(&plan_of(
            Some(node("Clustered Index Seek", 1.0, Some(5_000_000.0))),
            1.0,
        ));
        assert_eq!(v.findings, vec![]);
    }

    #[test]
    fn a_scan_with_no_rows_read_attribute_falls_back_to_rows_returned() {
        // Never observed on a real scan; the fallback is conservative by
        // construction (rows returned ≤ rows read), so it can only under-report.
        let v = judge(&plan_of(Some(node("Table Scan", 5_000_000.0, None)), 1.0));
        assert_eq!(kinds(&v), vec![FindingKind::LargeScan]);
        assert_eq!(v.severity, Severity::Caution, "ratio is 1, so not waste");
    }

    #[test]
    fn an_expensive_plan_fires_at_the_constant_and_not_below_it() {
        let at = judge(&plan_of(None, EXPENSIVE_COST));
        assert_eq!(at.severity, Severity::Caution);
        assert_eq!(kinds(&at), vec![FindingKind::ExpensivePlan]);
        assert_eq!(at.findings[0].message, "estimated cost 10.0");
        assert_eq!(at.findings[0].evidence, None);

        let below = judge(&plan_of(None, EXPENSIVE_COST - 0.01));
        assert_eq!(below.findings, vec![]);
    }

    #[test]
    fn the_cost_finding_is_about_the_whole_batch_not_one_statement() {
        // Two statements, each individually under the threshold, together over
        // it: one finding, and it counts the sum.
        let plan = QueryPlan {
            statements: vec![statement(None, 6.0), statement(None, 6.0)],
        };
        let v = judge(&plan);
        assert!(close(v.total_cost, 12.0), "got {}", v.total_cost);
        assert_eq!(kinds(&v), vec![FindingKind::ExpensivePlan]);
    }

    #[test]
    fn a_missing_index_fires_at_the_impact_threshold_and_not_below_it() {
        // NOT fixture coverage: no captured plan contains `<MissingIndexes>` —
        // `sys.*` views do not generate them (`billz-e75`).
        let with_impact = |impact: f64| {
            let mut plan = plan_of(None, 1.0);
            plan.statements[0].missing_indexes = vec![MissingIndex {
                impact,
                table: "[master].[dbo].[Orders]".into(),
                columns: vec!["[ShipCity]".into(), "[OrderDate]".into()],
            }];
            judge(&plan)
        };

        let at = with_impact(MISSING_INDEX_IMPACT);
        assert_eq!(at.severity, Severity::Problem);
        assert_eq!(kinds(&at), vec![FindingKind::MissingIndex]);

        let high = with_impact(99.5061);
        assert_eq!(
            high.findings[0].message,
            "missing index would cut cost ~100% ([ShipCity], [OrderDate])"
        );
        assert_eq!(
            high.findings[0].evidence.as_deref(),
            Some("[master].[dbo].[Orders]")
        );

        assert_eq!(with_impact(MISSING_INDEX_IMPACT - 0.1).findings, vec![]);
    }

    // ------------------------------------------------- warnings (hand-built)

    #[test]
    fn a_convert_issue_we_have_never_seen_is_a_problem() {
        // The allowlist's whole point. `"Seek Plan"` is the index-blocking issue
        // per the ShowPlanXML schema, but we have NO captured specimen of it —
        // and neither it nor any future value the server invents needs to be
        // named here for it to be graded correctly.
        for issue in [Some("Seek Plan"), Some("Something New In SQL 2030"), None] {
            let mut plan = plan_of(None, 1.0);
            plan.statements[0].warnings = vec![PlanWarning::ImplicitConversion {
                expression: "CONVERT_IMPLICIT(int,[Orders].[OrderNumber],0)".into(),
                convert_issue: issue.map(str::to_string),
            }];
            let v = judge(&plan);
            assert_eq!(v.severity, Severity::Problem, "issue {issue:?}");
            assert_eq!(kinds(&v), vec![FindingKind::ImplicitConversion]);
            assert!(
                v.findings[0].message.contains("may prevent an index seek"),
                "issue {issue:?}: got {}",
                v.findings[0].message
            );
            assert_eq!(
                v.findings[0].evidence.as_deref(),
                Some("CONVERT_IMPLICIT(int,[Orders].[OrderNumber],0)")
            );
        }
    }

    #[test]
    fn a_no_join_predicate_is_a_problem_however_cheap() {
        // NOT fixture coverage, and one step weaker than that: this hand-BUILDS
        // the warning, so it proves the mapping only. Nothing here says a real
        // server ever emits one, or in what shape (`billz-e75`).
        let mut plan = plan_of(None, 0.01);
        plan.statements[0].warnings = vec![PlanWarning::NoJoinPredicate];
        let v = judge(&plan);
        assert_eq!(v.severity, Severity::Problem);
        assert_eq!(kinds(&v), vec![FindingKind::NoJoinPredicate]);
    }

    #[test]
    fn a_spill_and_an_unmatched_index_map_to_their_own_kinds() {
        // NOT fixture coverage — both warnings are hand-built; see above.
        // A spill may not even be REACHABLE from an estimated plan.
        let mut plan = plan_of(None, 1.0);
        plan.statements[0].warnings = vec![
            PlanWarning::SpillToTempDb,
            PlanWarning::UnmatchedIndex {
                index: "[master].[dbo].[Orders].[IX_Filtered]".into(),
            },
        ];
        let v = judge(&plan);
        assert_eq!(v.severity, Severity::Caution);
        assert_eq!(
            kinds(&v),
            vec![FindingKind::SpillToTempDb, FindingKind::UnmatchedIndex]
        );
        assert_eq!(
            v.findings[1].evidence.as_deref(),
            Some("[master].[dbo].[Orders].[IX_Filtered]")
        );
    }

    #[test]
    fn warnings_without_an_operator_carry_no_evidence() {
        // Pinned rather than left to be rediscovered as a regression:
        // `all_warnings()` yields a warning without its node, so a warning whose
        // own payload names nothing has no evidence to give. Recovering an object
        // name would mean walking the operator tree, which is precisely the bug
        // `the_statement_level_convert_warnings_are_found` exists to prevent.
        let mut plan = plan_of(Some(node("Table Scan", 1.0, Some(1.0))), 1.0);
        plan.statements[0].warnings =
            vec![PlanWarning::NoJoinPredicate, PlanWarning::SpillToTempDb];
        let v = judge(&plan);
        assert_eq!(v.findings.len(), 2);
        assert_eq!(v.findings[0].evidence, None);
        assert_eq!(v.findings[1].evidence, None);
    }

    #[test]
    fn operator_level_warnings_are_found_too() {
        // The other half of `all_warnings()`. The fixture-backed test above only
        // covers statement-level warnings, so without this a judge reading just
        // `stmt.warnings` would pass everything.
        let mut root = node("Table Scan", 1.0, Some(1.0));
        root.warnings = vec![PlanWarning::SpillToTempDb];
        root.children = vec![{
            let mut child = node("Table Scan", 1.0, Some(1.0));
            child.warnings = vec![PlanWarning::NoJoinPredicate];
            child
        }];
        let v = judge(&plan_of(Some(root), 1.0));
        assert_eq!(
            kinds(&v),
            vec![FindingKind::SpillToTempDb, FindingKind::NoJoinPredicate]
        );
    }

    // ------------------------------------------------------ severity and order

    #[test]
    fn severity_is_the_max_of_its_findings() {
        let mut plan = plan_of(None, 1.0);
        plan.statements[0].warnings = vec![
            PlanWarning::ImplicitConversion {
                expression: "CONVERT(bigint,[sov].[value],0)".into(),
                convert_issue: Some(BENIGN_CONVERT_ISSUE.into()),
            },
            PlanWarning::NoJoinPredicate,
        ];
        let v = judge(&plan);
        assert_eq!(
            v.findings.iter().map(|f| f.severity).collect::<Vec<_>>(),
            vec![Severity::Caution, Severity::Problem]
        );
        assert_eq!(v.severity, Severity::Problem);
    }

    #[test]
    fn finding_order_is_deterministic() {
        // The card's display order and the cross-tenant diff order, pinned so it
        // cannot drift silently: missing indexes, then scans in tree pre-order,
        // then warnings, then the batch-level cost finding last.
        let mut root = node("Table Scan", 1.0, Some(5_000_000.0));
        root.children = vec![node("Index Scan", 1.0, Some(5_000_000.0))];
        let mut plan = plan_of(Some(root), EXPENSIVE_COST);
        plan.statements[0].missing_indexes = vec![MissingIndex {
            impact: 99.0,
            table: "[master].[dbo].[Orders]".into(),
            columns: vec!["[ShipCity]".into()],
        }];
        plan.statements[0].warnings = vec![PlanWarning::NoJoinPredicate];

        assert_eq!(
            kinds(&judge(&plan)),
            vec![
                FindingKind::MissingIndex,
                FindingKind::LargeScan,
                FindingKind::LargeScan,
                FindingKind::NoJoinPredicate,
                FindingKind::ExpensivePlan,
            ]
        );
    }
}
