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
    pub missing_indexes: Vec<MissingIndex>,
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
#[serde(tag = "kind", rename_all = "camelCase")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FindingKind {
    LargeScan,
    MissingIndex,
    ImplicitConversion,
    NoJoinPredicate,
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
    }

    #[test]
    fn plan_warning_is_internally_tagged_including_its_unit_variant() {
        // Internally-tagged enums with a mix of struct and UNIT variants are the
        // one serde shape here that can serialize wrong without failing: a unit
        // variant must still be an OBJECT (`{"kind":"noJoinPredicate"}`), not a
        // bare string. `parse.rs` produces these and the UI switches on `kind`.
        let conv = PlanWarning::ImplicitConversion {
            expression: "CONVERT_IMPLICIT(int,[Orders].[OrderNumber],0)".into(),
        };
        let s = serde_json::to_string(&conv).unwrap();
        assert!(s.contains(r#""kind":"implicitConversion""#), "got {s}");
        assert!(s.contains(r#""expression":"CONVERT_IMPLICIT"#), "got {s}");
        assert_eq!(serde_json::from_str::<PlanWarning>(&s).unwrap(), conv);

        let none = PlanWarning::NoJoinPredicate;
        let s = serde_json::to_string(&none).unwrap();
        assert_eq!(s, r#"{"kind":"noJoinPredicate"}"#, "got {s}");
        assert_eq!(serde_json::from_str::<PlanWarning>(&s).unwrap(), none);
    }
}
