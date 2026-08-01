//! The plan types the UI sees — `core`'s own, driver-free, serde-serializable.
//! Same boundary rule as [`crate::result`]: no `mssql_client` type appears here.

use serde::{Deserialize, Serialize};

/// A parsed estimated plan. A batch can contain several statements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryPlan {
    pub statements: Vec<PlanStatement>,
}

/// One statement's plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStatement {
    pub text: String,
    pub subtree_cost: f64,
    pub est_rows: f64,
    pub root: Option<PlanNode>,
    /// Warnings the plan reports for the STATEMENT as a whole — ShowPlanXML puts
    /// these on `<QueryPlan>`, a sibling of the root `<RelOp>`, not on any
    /// operator. Distinct from [`PlanNode::warnings`], which is what an
    /// individual operator reported. Both positions are schema-valid and both
    /// occur; [`PlanStatement::all_warnings`] is how a consumer reads the two
    /// together without having to know which is which.
    pub warnings: Vec<PlanWarning>,
    pub missing_indexes: Vec<MissingIndex>,
}

impl PlanStatement {
    /// Every warning in this statement: the statement-level ones first, then
    /// each operator's own in document order (pre-order, depth first).
    ///
    /// The judge must use THIS rather than walking `root` itself. ShowPlanXML
    /// puts a warning in one of two places depending on what kind it is, and a
    /// consumer that reads only the operator tree silently sees nothing at all
    /// for a document whose warnings are all statement-level — which is exactly
    /// the shape of `scan.sqlplan`, the only fixture we own that has any.
    pub fn all_warnings(&self) -> impl Iterator<Item = &PlanWarning> {
        let mut out: Vec<&PlanWarning> = self.warnings.iter().collect();
        let mut stack: Vec<&PlanNode> = self.root.iter().collect();
        // Children pushed in reverse so the LIFO stack pops them in document
        // order — the iteration order is part of what callers assert against.
        while let Some(n) = stack.pop() {
            out.extend(n.warnings.iter());
            stack.extend(n.children.iter().rev());
        }
        out.into_iter()
    }
}

/// One operator in the plan tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanNode {
    pub physical_op: String,
    pub logical_op: String,
    /// `[db].[schema].[table].[index]` as the server reports it, when present.
    pub object: Option<String>,
    pub est_rows: f64,
    /// Rows the operator is estimated to READ, against [`Self::est_rows`]'s rows
    /// RETURNED. `None` on operators that access no data — only scans and seeks
    /// carry it — and the distinction is load-bearing: `Some(0.0)` would mean
    /// "reads nothing", which is the best plan imaginable, the exact inverse of
    /// "we have no idea". The ratio is the wasteful-scan signal a raw row count
    /// cannot see (`join.sqlplan` reads 1153 rows to return 10).
    pub est_rows_read: Option<f64>,
    /// This node alone: its subtree cost minus its children's subtree costs.
    pub est_cost: f64,
    pub subtree_cost: f64,
    pub warnings: Vec<PlanWarning>,
    pub children: Vec<PlanNode>,
}

/// An index the optimizer says it wanted. `impact` is the server's own 0–100
/// estimate of how much this index would reduce the statement's cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingIndex {
    pub impact: f64,
    pub table: String,
    pub columns: Vec<String>,
}

/// A warning the PLAN reports — a fact read out of the XML, not a judgement.
/// Contrast [`Finding`], which is our conclusion and is expected to change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// `rename_all` renames VARIANTS only; `rename_all_fields` is what camelCases a
// struct variant's FIELDS. Today's fields are single words, so the wire shape is
// right either way — but the day a variant gains e.g. `spill_level`, serde would
// silently emit `spill_level` while the TS mirror expects `spillLevel`.
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlanWarning {
    /// A type conversion that can prevent index usage — the classic LLM-SQL
    /// smell.
    ///
    /// ONE opaque expression, deliberately: the design spec's illustrative
    /// `{ column, from, to }` sketch (§4.3) does not match the wire. ShowPlanXML
    /// gives a single `PlanAffectingConvert/@Expression` string
    /// (`CONVERT_IMPLICIT(int,[Orders].[OrderNumber],0)`); splitting it would
    /// mean parsing that string. Do not "restore" the three-field shape.
    ImplicitConversion {
        expression: String,
        /// The server's own `@ConvertIssue`, passed through verbatim. NOT every
        /// convert is the index-blocking smell: `"Seek Plan"` is the one that
        /// stops a seek, while `"Cardinality Estimate"` only skews the row
        /// estimate. Collapsing the two makes every `sys.*` plan we own look
        /// like the headline problem. `Option<String>` rather than an enum
        /// because the server's set of issue strings is its to extend, not
        /// ours to close.
        convert_issue: Option<String>,
    },
    /// A join with no predicate: an accidental cartesian product.
    NoJoinPredicate,
    SpillToTempDb,
    UnmatchedIndex {
        index: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Ok,
    Caution,
    Problem,
}

/// What a [`Finding`] is ABOUT. One variant per distinguishable conclusion:
/// consumers group and filter by this, so two conclusions that a reader would
/// act on differently must not share a variant. `SpillToTempDb` and
/// `UnmatchedIndex` exist for exactly that reason — folding them into
/// `ExpensivePlan`/`MissingIndex` (the design spec §4.3's illustrative list,
/// which ends in a `…`) would put two unrelated meanings behind one key: a card
/// would show two `ExpensivePlan` findings, one the batch's total cost and one a
/// tempdb spill, with nothing to tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FindingKind {
    LargeScan,
    MissingIndex,
    ImplicitConversion,
    NoJoinPredicate,
    SpillToTempDb,
    UnmatchedIndex,
    ExpensivePlan,
}

/// Our conclusion about the plan. Deliberately simple — the thresholds behind
/// these are a hypothesis to be revised after real use, not a settled rule set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub kind: FindingKind,
    pub severity: Severity,
    pub message: String,
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanVerdict {
    pub severity: Severity,
    pub total_cost: f64,
    pub findings: Vec<Finding>,
}

/// What one `explain` returns: the parsed plan plus our judgement of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanCapture {
    pub plan: QueryPlan,
    pub verdict: PlanVerdict,
}

/// One database's slice of a plan fan-out. Mirrors [`crate::DbRunOutcome`]'s
/// capture-don't-propagate contract: a database where the query will not COMPILE
/// lands in `error`, which is the signal, not a failure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbPlanOutcome {
    pub database: String,
    pub capture: Option<PlanCapture>,
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_verdict_serializes_camel_case() {
        let v = PlanVerdict {
            severity: Severity::Problem,
            total_cost: 412.3,
            findings: vec![Finding {
                kind: FindingKind::LargeScan,
                severity: Severity::Problem,
                message: "table scan on Orders".into(),
                evidence: Some("[dbo].[Orders]".into()),
            }],
        };
        let s = serde_json::to_string(&v).unwrap();
        assert!(s.contains(r#""totalCost":412.3"#), "got {s}");
        assert!(s.contains(r#""kind":"largeScan""#), "got {s}");
        let back: PlanVerdict = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn db_plan_outcome_carries_error_instead_of_failing() {
        let o = DbPlanOutcome {
            database: "ESP_Nomad_SE_DEV".into(),
            capture: None,
            error: Some("Invalid column name 'ShipDate'.".into()),
            elapsed_ms: 12,
        };
        let s = serde_json::to_string(&o).unwrap();
        assert!(s.contains(r#""elapsedMs":12"#), "got {s}");
        assert!(s.contains("ShipDate"), "got {s}");
        assert_eq!(serde_json::from_str::<DbPlanOutcome>(&s).unwrap(), o);
    }

    #[test]
    fn a_populated_capture_round_trips_with_every_field_camel_cased() {
        // One pass over EVERY multi-word field name in the module. These names are
        // the contract the TS mirror is typed against, so a `rename_all` dropped
        // from any struct here must fail loudly rather than reach the UI.
        let capture = PlanCapture {
            plan: QueryPlan {
                statements: vec![PlanStatement {
                    text: "SELECT * FROM Orders WHERE ShipCity = 'X'".into(),
                    subtree_cost: 412.3,
                    est_rows: 4_200_000.0,
                    root: Some(PlanNode {
                        physical_op: "Clustered Index Scan".into(),
                        logical_op: "Clustered Index Scan".into(),
                        object: Some("[dbo].[Orders].[PK_Orders]".into()),
                        est_rows: 4_200_000.0,
                        est_rows_read: Some(9_000_000.0),
                        est_cost: 400.0,
                        subtree_cost: 412.3,
                        warnings: vec![PlanWarning::SpillToTempDb],
                        children: vec![PlanNode {
                            physical_op: "Table Scan".into(),
                            logical_op: "Table Scan".into(),
                            object: None,
                            est_rows: 1.0,
                            est_rows_read: None,
                            est_cost: 12.3,
                            subtree_cost: 12.3,
                            warnings: vec![],
                            children: vec![],
                        }],
                    }),
                    warnings: vec![PlanWarning::ImplicitConversion {
                        expression: "CONVERT(bigint,[sov].[value],0)".into(),
                        convert_issue: Some("Cardinality Estimate".into()),
                    }],
                    missing_indexes: vec![MissingIndex {
                        impact: 99.5061,
                        table: "[dbo].[Orders]".into(),
                        columns: vec!["[ShipCity]".into()],
                    }],
                }],
            },
            verdict: PlanVerdict {
                severity: Severity::Problem,
                total_cost: 412.3,
                findings: vec![Finding {
                    kind: FindingKind::LargeScan,
                    severity: Severity::Problem,
                    message: "table scan on Orders".into(),
                    evidence: Some("[dbo].[Orders]".into()),
                }],
            },
        };
        let s = serde_json::to_string(&capture).unwrap();
        for (camel, snake) in [
            ("subtreeCost", "subtree_cost"),
            ("estRows", "est_rows"),
            ("estRowsRead", "est_rows_read"),
            ("missingIndexes", "missing_indexes"),
            ("physicalOp", "physical_op"),
            ("logicalOp", "logical_op"),
            ("estCost", "est_cost"),
            ("convertIssue", "convert_issue"),
            ("totalCost", "total_cost"),
        ] {
            assert!(
                s.contains(&format!(r#""{camel}":"#)),
                "missing {camel}: {s}"
            );
            // Scoped to KEY position — object names in the data legitimately carry
            // underscores (`[PK_Orders]`), so a blanket underscore ban is wrong.
            assert!(
                !s.contains(&format!(r#""{snake}":"#)),
                "snake_case key {snake} survived: {s}"
            );
        }
        assert_eq!(serde_json::from_str::<PlanCapture>(&s).unwrap(), capture);
    }

    #[test]
    fn severity_orders_ok_below_caution_below_problem() {
        // `verdict::judge` takes the MAX severity of its findings, so reordering
        // these variants (e.g. to sort a UI list) would silently invert every
        // verdict the tool produces.
        assert!(Severity::Problem > Severity::Caution);
        assert!(Severity::Caution > Severity::Ok);
        assert_eq!(
            [Severity::Caution, Severity::Problem, Severity::Ok]
                .into_iter()
                .max(),
            Some(Severity::Problem)
        );
    }

    #[test]
    fn severity_serializes_to_the_strings_the_ui_switches_on() {
        for (variant, expected) in [
            (Severity::Ok, r#""ok""#),
            (Severity::Caution, r#""caution""#),
            (Severity::Problem, r#""problem""#),
        ] {
            let s = serde_json::to_string(&variant).unwrap();
            assert_eq!(s, expected, "got {s}");
            assert_eq!(serde_json::from_str::<Severity>(&s).unwrap(), variant);
        }
    }

    #[test]
    fn finding_kind_serializes_to_the_strings_the_ui_switches_on() {
        // Every variant, because these are the keys the verdict card groups by.
        for (variant, expected) in [
            (FindingKind::LargeScan, r#""largeScan""#),
            (FindingKind::MissingIndex, r#""missingIndex""#),
            (FindingKind::ImplicitConversion, r#""implicitConversion""#),
            (FindingKind::NoJoinPredicate, r#""noJoinPredicate""#),
            (FindingKind::SpillToTempDb, r#""spillToTempDb""#),
            (FindingKind::UnmatchedIndex, r#""unmatchedIndex""#),
            (FindingKind::ExpensivePlan, r#""expensivePlan""#),
        ] {
            let s = serde_json::to_string(&variant).unwrap();
            assert_eq!(s, expected, "got {s}");
            assert_eq!(serde_json::from_str::<FindingKind>(&s).unwrap(), variant);
        }
    }

    #[test]
    fn plan_warning_is_internally_tagged_including_its_unit_variant() {
        // Internally-tagged enums with a mix of struct and UNIT variants are the
        // one serde shape here that can serialize wrong without failing: a unit
        // variant must still be an OBJECT (`{"kind":"noJoinPredicate"}`), not a
        // bare string. `parse.rs` produces these and the UI switches on `kind`.
        let conv = PlanWarning::ImplicitConversion {
            expression: "CONVERT_IMPLICIT(int,[Orders].[OrderNumber],0)".into(),
            convert_issue: Some("Seek Plan".into()),
        };
        let s = serde_json::to_string(&conv).unwrap();
        assert!(s.contains(r#""kind":"implicitConversion""#), "got {s}");
        assert!(s.contains(r#""expression":"CONVERT_IMPLICIT"#), "got {s}");
        assert!(s.contains(r#""convertIssue":"Seek Plan""#), "got {s}");
        assert_eq!(serde_json::from_str::<PlanWarning>(&s).unwrap(), conv);

        let none = PlanWarning::NoJoinPredicate;
        let s = serde_json::to_string(&none).unwrap();
        assert_eq!(s, r#"{"kind":"noJoinPredicate"}"#, "got {s}");
        assert_eq!(serde_json::from_str::<PlanWarning>(&s).unwrap(), none);
    }

    fn node(warnings: Vec<PlanWarning>, children: Vec<PlanNode>) -> PlanNode {
        PlanNode {
            physical_op: "Table Scan".into(),
            logical_op: "Table Scan".into(),
            object: None,
            est_rows: 1.0,
            est_rows_read: None,
            est_cost: 1.0,
            subtree_cost: 1.0,
            warnings,
            children,
        }
    }

    fn unmatched(index: &str) -> PlanWarning {
        PlanWarning::UnmatchedIndex {
            index: index.into(),
        }
    }

    #[test]
    fn all_warnings_chains_statement_level_and_operator_warnings_in_document_order() {
        // The judge's ONE entry point. ShowPlanXML puts warnings on `<QueryPlan>`
        // OR on a `<RelOp>` depending on the kind, so a consumer that reads only
        // one of the two places goes silently blind on half of them.
        let statement = PlanStatement {
            text: "SELECT 1".into(),
            subtree_cost: 1.0,
            est_rows: 1.0,
            root: Some(node(
                vec![PlanWarning::NoJoinPredicate],
                vec![
                    node(
                        vec![PlanWarning::SpillToTempDb],
                        vec![node(vec![unmatched("[grandchild]")], vec![])],
                    ),
                    node(vec![unmatched("[second child]")], vec![]),
                ],
            )),
            warnings: vec![PlanWarning::ImplicitConversion {
                expression: "CONVERT(int,[t].[v],0)".into(),
                convert_issue: Some("Seek Plan".into()),
            }],
            missing_indexes: vec![],
        };

        // Order is asserted, not just membership: the second child must come
        // AFTER the first child's own descendants (pre-order), and both children
        // in the order the document listed them.
        assert_eq!(
            statement.all_warnings().collect::<Vec<_>>(),
            vec![
                &PlanWarning::ImplicitConversion {
                    expression: "CONVERT(int,[t].[v],0)".into(),
                    convert_issue: Some("Seek Plan".into()),
                },
                &PlanWarning::NoJoinPredicate,
                &PlanWarning::SpillToTempDb,
                &unmatched("[grandchild]"),
                &unmatched("[second child]"),
            ]
        );
    }

    #[test]
    fn all_warnings_survives_a_statement_with_no_operator_tree() {
        // A `StmtSimple` with no `<QueryPlan>` parses to `root: None`. Its
        // statement-level warnings must still be reachable — hoisting them onto
        // a root node, the design we rejected, would have dropped them here.
        let statement = PlanStatement {
            text: "SET NOCOUNT ON".into(),
            subtree_cost: 0.0,
            est_rows: 0.0,
            root: None,
            warnings: vec![PlanWarning::NoJoinPredicate],
            missing_indexes: vec![],
        };
        assert_eq!(
            statement.all_warnings().collect::<Vec<_>>(),
            vec![&PlanWarning::NoJoinPredicate]
        );
    }
}
