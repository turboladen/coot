//! ShowPlanXML → [`QueryPlan`]. Pure: no I/O, no driver, no server.
//!
//! Every test here runs offline. Most are backed by the real `.sqlplan` files in
//! `core/tests/fixtures/plans/`, captured from a live SQL Server; three are not,
//! and say so at their definitions — the missing-index test uses a hand-authored
//! schema-derived document, and two feed deliberately malformed or non-plan
//! input. No path in this module is fixture-backed unless it says it is.
//!
//! Element names are matched on their LOCAL name throughout, so the showplan
//! namespace declaration is irrelevant and a future namespace bump cannot break
//! parsing.
//!
//! Missing or unparseable attributes default rather than fail. A plan is a
//! diagnostic aid; refusing to show any of it because one operator lacks one
//! attribute is the wrong trade. Only malformed XML is an error.

use roxmltree::{Document, Node};

use crate::error::{CoreError, Result};
use crate::plan::model::{MissingIndex, PlanNode, PlanStatement, PlanWarning, QueryPlan};

/// Microsoft spells this one `Estimated…` while every sibling on the same
/// element is `Estimate…` (`EstimateRows`, `EstimateCPU`, `EstimateIO`). Getting
/// it wrong does not fail to compile and does not fail loudly — it yields `None`
/// for every operator in every plan, forever. Hence a named constant, and hence
/// `est_rows_read_is_read_from_the_scan_operators` below, which is the only
/// thing standing between that typo and shipping.
const ROWS_READ: &str = "EstimatedRowsRead";

/// Parse a `SET SHOWPLAN_XML ON` document into `core`'s own plan model.
///
/// A document covers every statement in the explained batch, so the result is a
/// list (`two-statements.sqlplan` proves the multi-statement case). A well-formed
/// document that is not a plan yields zero statements rather than an error.
///
/// The [`CoreError::Query`] on a parse failure is a deliberate reuse, not a
/// sloppy one. `error.rs` documents that variant as a failure "on the server or
/// in the driver", and this is neither — it is local. A new variant is not worth
/// churning a module that is otherwise strict about what each variant means, and
/// nothing user-facing is misleading: the message says the plan XML could not be
/// parsed.
pub fn parse_plan(xml: &str) -> Result<QueryPlan> {
    let doc = Document::parse(xml)
        .map_err(|e| CoreError::Query(format!("could not parse execution plan XML: {e}")))?;

    // Two known omissions, both fine for the ad-hoc SELECTs this app explains,
    // neither verifiable offline:
    //
    // - Only `StmtSimple` is collected. A batch wrapping statements in `StmtCond`
    //   (`IF`), `StmtCursor`, or `StmtUseDb` yields fewer statements than it had.
    // - A flat `descendants()` sweep also picks up a `StmtSimple` nested inside a
    //   `<StoredProc>`/`<UDF>` block and flattens it into the top-level list.
    let statements = doc
        .descendants()
        .filter(|n| has_local_name(n, "StmtSimple"))
        .map(statement)
        .collect();

    Ok(QueryPlan { statements })
}

fn has_local_name(n: &Node, local_name: &str) -> bool {
    n.is_element() && n.tag_name().name() == local_name
}

/// The first direct child element with this local name.
fn child<'a, 'i>(n: Node<'a, 'i>, local_name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| has_local_name(c, local_name))
}

fn elements<'a, 'i>(n: Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    n.children().filter(|c| c.is_element()).collect()
}

fn attr_f64(n: &Node, name: &str) -> f64 {
    n.attribute(name)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

/// Everything below a statement is reached by DIRECT-child navigation
/// (`StmtSimple > QueryPlan > {RelOp, Warnings, MissingIndexes}`), never by
/// `descendants()`. A scalar UDF's plan appears as a nested `<QueryPlan>` deeper
/// in the tree, and a descendant sweep would hoist ITS missing indexes and
/// warnings onto this statement.
fn statement(n: Node) -> PlanStatement {
    let query_plan = child(n, "QueryPlan");

    let root = query_plan.and_then(|qp| child(qp, "RelOp")).map(rel_op);

    // Statement-level, NOT hoisted onto `root`: ShowPlanXML puts these on
    // `<QueryPlan>`, a sibling of the root operator. Which of the two positions
    // a warning lands in depends on its kind, and both are schema-valid — see
    // `PlanStatement::all_warnings`.
    let warnings = query_plan
        .and_then(|qp| child(qp, "Warnings"))
        .map(warnings_from)
        .unwrap_or_default();

    let missing_indexes = query_plan
        .and_then(|qp| child(qp, "MissingIndexes"))
        .map(|mi| {
            elements(mi)
                .into_iter()
                .filter(|g| has_local_name(g, "MissingIndexGroup"))
                .filter_map(missing_index)
                .collect()
        })
        .unwrap_or_default();

    PlanStatement {
        text: n.attribute("StatementText").unwrap_or_default().to_string(),
        subtree_cost: attr_f64(&n, "StatementSubTreeCost"),
        est_rows: attr_f64(&n, "StatementEstRows"),
        root,
        warnings,
        missing_indexes,
    }
}

/// A `MissingIndexGroup` carries the impact; its child `MissingIndex` names the
/// table and its `ColumnGroup`s name the columns.
///
/// **Written from the ShowPlanXML schema, not from a captured document.** No
/// fixture we own contains `<MissingIndexes>` — `sys.*` views do not generate
/// them — so the only test covering this is a hand-authored XML string, which
/// can prove the parser matches what we BELIEVE the schema says and cannot prove
/// the belief is right. See `billz-e75` for capturing a real one.
fn missing_index(group: Node) -> Option<MissingIndex> {
    let idx = child(group, "MissingIndex")?;
    let columns = idx
        .descendants()
        .filter(|d| has_local_name(d, "Column"))
        .filter_map(|c| c.attribute("Name").map(str::to_string))
        .collect();
    Some(MissingIndex {
        impact: attr_f64(&group, "Impact"),
        // A `MissingIndex` element has no `Index` attribute, so this yields
        // `[db].[schema].[table]`.
        table: object_name(&idx),
        columns,
    })
}

/// `[db].[schema].[table]` plus `.[index]` when the element carries one. Absent
/// parts are skipped rather than rendered as empty brackets.
fn object_name(n: &Node) -> String {
    ["Database", "Schema", "Table", "Index"]
        .iter()
        .filter_map(|k| n.attribute(*k))
        .collect::<Vec<_>>()
        .join(".")
}

/// Splits everything below `n` into the elements this operator OWNS and its
/// IMMEDIATE child operators, both in document order.
///
/// Two stop rules:
///
/// - **At a nested `RelOp`** — that operator owns its own subtree. This is what
///   makes child discovery wrapper-agnostic: nested operators sit under a
///   physical-op wrapper (`<Hash>`, `<NestedLoops>`, `<Filter>`, `<Concat>`,
///   `<ComputeScalar>`, `<StreamAggregate>`, `<Top>`, `<IndexScan>`, … — all
///   eight of those appear in the fixtures), and a hardcoded wrapper list would
///   silently drop a subtree the first time a plan used an unlisted one.
///   Exercised by every fixture.
/// - **At a nested `QueryPlan`** — a scalar UDF's plan is a different statement's
///   operators, not ours. Schema-correct and free, but UNVERIFIED against
///   captured data: all six `QueryPlan` elements across the five fixtures are
///   direct children of a `StmtSimple`, so nothing here proves this rule fires.
///
/// An explicit stack rather than recursion: the depth walked here is XML depth
/// (`ScalarOperator` predicate trees get deep), not operator depth.
fn scan_owned<'a, 'i>(n: Node<'a, 'i>) -> (Vec<Node<'a, 'i>>, Vec<Node<'a, 'i>>) {
    let mut owned = Vec::new();
    let mut child_ops = Vec::new();

    let mut stack: Vec<Node<'a, 'i>> = elements(n);
    stack.reverse();
    while let Some(c) = stack.pop() {
        if has_local_name(&c, "RelOp") {
            child_ops.push(c);
            continue;
        }
        if has_local_name(&c, "QueryPlan") {
            continue;
        }
        owned.push(c);
        let mut grandchildren = elements(c);
        grandchildren.reverse();
        stack.extend(grandchildren);
    }

    (owned, child_ops)
}

/// One operator, and its children recursively.
///
/// The asymmetry with [`scan_owned`]'s explicit stack is deliberate, not an
/// oversight. That function walks ELEMENT depth, which a `<ScalarOperator>`
/// predicate tree can run away with — a deeply parenthesised `WHERE` clause
/// nests without bound and is user-controlled. This one walks OPERATOR depth,
/// which the optimizer keeps small (20 in `scan.sqlplan`, the deepest plan we
/// have). Recursion is the natural shape for building a recursive tree, so it is
/// used where the depth is bounded and avoided where it is not.
fn rel_op(n: Node) -> PlanNode {
    let subtree_cost = attr_f64(&n, "EstimatedTotalSubtreeCost");
    let (owned, child_ops) = scan_owned(n);
    let children: Vec<PlanNode> = child_ops.into_iter().map(rel_op).collect();
    let children_cost: f64 = children.iter().map(|c| c.subtree_cost).sum();

    // The first `Object` under this operator but NOT under a nested one. Taking
    // the first is right for every operator here (a scan or seek names exactly
    // one); DML operators can carry several, which nothing in this app explains.
    let object = owned
        .iter()
        .find(|d| has_local_name(d, "Object"))
        .map(object_name);

    let warnings = owned
        .iter()
        .filter(|d| has_local_name(d, "Warnings"))
        .flat_map(|w| warnings_from(*w))
        .collect();

    PlanNode {
        physical_op: n.attribute("PhysicalOp").unwrap_or_default().to_string(),
        logical_op: n.attribute("LogicalOp").unwrap_or_default().to_string(),
        object,
        est_rows: attr_f64(&n, "EstimateRows"),
        est_rows_read: n.attribute(ROWS_READ).and_then(|v| v.parse().ok()),
        // CLAMPED, and not as defensive programming — delete the `.max` and
        // `join.sqlplan` node 1 (`Nested Loops` / `Left Outer Join`) parses to
        // −0.00010769: its two children's subtree costs sum to 0.01669429
        // against its own 0.0165866. That is 0.65% of the subtree, ~300× too
        // large to be display rounding; it is the `Top` row-goal rescaling the
        // inner side. Real plans do this, so a negative own cost is a fact to
        // absorb here rather than a bug to hunt.
        est_cost: (subtree_cost - children_cost).max(0.0),
        subtree_cost,
        warnings,
        children,
    }
}

/// One `<Warnings>` element's contents. Unknown children are ignored — the
/// element gains new kinds across server versions and an unrecognised one is not
/// a reason to fail a parse.
///
/// **Only `PlanAffectingConvert` has ever been seen in a captured document.**
/// `NoJoinPredicate`, `SpillToTempDb`, and `UnmatchedIndexes` are written from
/// the ShowPlanXML schema with no specimen to check them against — the same
/// standing as [`missing_index`], and the same warning applies: these branches
/// prove only that the parser does what we BELIEVE the schema says. `billz-e75`
/// covers capturing one of each (a `CROSS JOIN` with no predicate, a big
/// `ORDER BY` under a low memory grant).
fn warnings_from(w: Node) -> Vec<PlanWarning> {
    let mut out = Vec::new();

    // An ATTRIBUTE on `<Warnings>`, not a child element like the rest — and BOTH
    // `xs:boolean` lexical forms are accepted, deliberately. This server emits
    // both in one document, on adjacent elements: `SecurityPolicyApplied="false"`
    // and `RetrievedFromCache="false"` sit on `<StmtSimple>` while `ForceSeek="0"`
    // and `Ordered="1"` sit on the `<IndexScan>` below it. With no captured
    // specimen there is no way to know which form this attribute uses, and
    // guessing `"true"` alone buys a permanent silent false negative on an
    // accidental cartesian product — one of the loudest things this feature
    // exists to catch.
    if matches!(w.attribute("NoJoinPredicate"), Some("true" | "1")) {
        out.push(PlanWarning::NoJoinPredicate);
    }

    for c in elements(w) {
        match c.tag_name().name() {
            "PlanAffectingConvert" => out.push(PlanWarning::ImplicitConversion {
                expression: c.attribute("Expression").unwrap_or_default().to_string(),
                convert_issue: c.attribute("ConvertIssue").map(str::to_string),
            }),
            "SpillToTempDb" => out.push(PlanWarning::SpillToTempDb),
            "UnmatchedIndexes" => out.extend(
                c.descendants()
                    .filter(|d| has_local_name(d, "Object"))
                    .map(|o| PlanWarning::UnmatchedIndex {
                        index: object_name(&o),
                    }),
            ),
            _ => {}
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every fixture here is REAL — captured from a live SQL Server by
    /// `just dump-plans` and committed. Take assertion values from the files,
    /// never from a document describing them.
    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/plans/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_else(|e| panic!("fixture {name}: {e}"))
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn count(n: &PlanNode) -> usize {
        1 + n.children.iter().map(count).sum::<usize>()
    }

    fn depth(n: &PlanNode) -> usize {
        1 + n.children.iter().map(depth).max().unwrap_or(0)
    }

    /// The chain from `n` down whichever child is deepest (`max_by_key` takes
    /// the last on a tie). No tie occurs anywhere along `scan.sqlplan`'s spine —
    /// each `Hash Match` pairs a one-level seek with a deep branch — so the
    /// path it returns is unambiguous.
    fn deepest_path(n: &PlanNode) -> Vec<&str> {
        let mut path = vec![n.physical_op.as_str()];
        if let Some(next) = n.children.iter().max_by_key(|c| depth(c)) {
            path.extend(deepest_path(next));
        }
        path
    }

    #[test]
    fn parses_a_seek_statement() {
        // Also the regression test for object scoping, in BOTH directions. The
        // `<Object>` lives under the CHILD seek's `<IndexScan>` wrapper: a naive
        // `descendants()` search hands it to the root `Filter` too, and an
        // ancestor-walking filter that does not stop at `n` strips it from the
        // child. Asserting only the root would catch neither.
        let plan = parse_plan(&fixture("seek.sqlplan")).unwrap();
        assert_eq!(plan.statements.len(), 1);

        let s = &plan.statements[0];
        assert_eq!(s.text, "SELECT name FROM sys.objects WHERE object_id = 1");
        assert!(close(s.subtree_cost, 0.00328328), "got {}", s.subtree_cost);
        assert!(close(s.est_rows, 1.0), "got {}", s.est_rows);
        assert!(s.warnings.is_empty());
        assert!(s.missing_indexes.is_empty());

        let root = s.root.as_ref().expect("a seek plan has a root operator");
        assert_eq!(root.physical_op, "Filter");
        assert_eq!(root.logical_op, "Filter");
        assert_eq!(root.object, None, "a Filter touches no object");
        assert_eq!(root.est_rows_read, None);
        assert_eq!(root.children.len(), 1);

        let seek = &root.children[0];
        assert_eq!(seek.physical_op, "Clustered Index Seek");
        assert_eq!(
            seek.object.as_deref(),
            Some("[master].[sys].[sysschobjs].[clst]")
        );
        assert!(
            close(seek.subtree_cost, 0.0032831),
            "got {}",
            seek.subtree_cost
        );
        assert!(seek.children.is_empty());
    }

    #[test]
    fn logical_op_can_differ_from_physical_op() {
        // `Hash Match` is one PHYSICAL operator serving several logical roles.
        // Conflating the two attributes reads fine on `seek.sqlplan`, where every
        // operator's two names are identical.
        let plan = parse_plan(&fixture("aggregate.sqlplan")).unwrap();
        let root = plan.statements[0].root.as_ref().unwrap();
        assert_eq!(count(root), 6);

        let aggregate = &root.children[0];
        assert_eq!(aggregate.physical_op, "Hash Match");
        assert_eq!(aggregate.logical_op, "Aggregate");

        let join = &aggregate.children[0];
        assert_eq!(join.physical_op, "Hash Match");
        assert_eq!(join.logical_op, "Right Outer Join");
    }

    #[test]
    fn parses_every_statement_in_a_multi_statement_batch() {
        let plan = parse_plan(&fixture("two-statements.sqlplan")).unwrap();
        assert_eq!(plan.statements.len(), 2);

        let first = &plan.statements[0];
        assert_eq!(first.text, "SELECT COUNT(*) FROM sys.objects");
        assert!(close(first.subtree_cost, 0.0495128));
        assert_eq!(first.root.as_ref().unwrap().physical_op, "Compute Scalar");

        let second = &plan.statements[1];
        // The leading "; " is really in the document — the server echoes the
        // batch separator into the second statement's text. Asserted verbatim
        // rather than trimmed, because trimming would be inventing.
        assert_eq!(second.text, "; SELECT TOP 1 name FROM sys.schemas");
        assert!(close(second.subtree_cost, 0.0032832));
        assert_eq!(second.root.as_ref().unwrap().physical_op, "Top");
    }

    #[test]
    fn own_cost_of_a_nested_loops_join_is_clamped_at_zero() {
        // THE test for the clamp — `join.sqlplan` node 1 is the only node in any
        // fixture whose raw own cost is negative.
        let plan = parse_plan(&fixture("join.sqlplan")).unwrap();
        let root = plan.statements[0].root.as_ref().unwrap();
        assert_eq!(count(root), 8);

        assert_eq!(root.physical_op, "Top");
        let loops = &root.children[0];
        assert_eq!(loops.physical_op, "Nested Loops");
        assert_eq!(loops.logical_op, "Left Outer Join");
        assert_eq!(loops.children.len(), 2);

        // Re-derive the raw arithmetic from the parsed values, so this test says
        // WHY the clamp exists rather than asserting a zero that any number of
        // unrelated bugs could also produce.
        let raw = loops.subtree_cost - loops.children.iter().map(|c| c.subtree_cost).sum::<f64>();
        assert!(
            raw < 0.0,
            "join.sqlplan node 1 is supposed to be the negative-own-cost case, got {raw}"
        );
        assert!(close(loops.est_cost, 0.0), "got {}", loops.est_cost);

        // A node whose arithmetic is ordinary, so the clamp is not just masking
        // a traversal that returns zero everywhere.
        let inner = &loops.children[0];
        assert_eq!(inner.physical_op, "Nested Loops");
        assert_eq!(inner.logical_op, "Inner Join");
        assert!(close(inner.est_cost, 3.901e-5), "got {}", inner.est_cost);
    }

    #[test]
    fn est_rows_read_is_read_from_the_scan_operators() {
        // Guards the `EstimatedRowsRead` / `Estimate…` spelling trap: a wrong
        // name yields `None` everywhere and fails nothing else.
        let plan = parse_plan(&fixture("join.sqlplan")).unwrap();
        let root = plan.statements[0].root.as_ref().unwrap();
        let scan = &root.children[0].children[0].children[0].children[0];

        assert_eq!(scan.physical_op, "Index Scan");
        // 1153 rows read to return 10 — the wasteful-scan signal that a raw
        // returned-row threshold cannot see at this size.
        assert_eq!(scan.est_rows_read, Some(1153.0));
        assert!(close(scan.est_rows, 10.0021), "got {}", scan.est_rows);

        // Absent, not zero, on an operator that accesses no data.
        let filter = &root.children[0].children[0].children[0];
        assert_eq!(filter.physical_op, "Filter");
        assert_eq!(filter.est_rows_read, None);
    }

    #[test]
    fn discovers_children_through_wrappers_in_a_deep_tree() {
        let plan = parse_plan(&fixture("scan.sqlplan")).unwrap();
        let s = &plan.statements[0];
        assert_eq!(s.text, "SELECT * FROM sys.all_columns");
        assert!(close(s.subtree_cost, 0.75402));
        assert!(close(s.est_rows, 1563.45));

        let root = s.root.as_ref().unwrap();
        assert_eq!(count(root), 32, "every operator in the document");
        assert_eq!(root.physical_op, "Concatenation");
        assert_eq!(root.children.len(), 2);

        // Child ORDER, via costs — both children are `Compute Scalar`, so names
        // alone would not catch a reversed traversal.
        assert!(close(root.children[0].subtree_cost, 0.306424));
        assert!(close(root.children[1].subtree_cost, 0.447439));

        // The deepest chain is 20 operators and runs down the SECOND child of
        // each `Hash Match` (the probe side), so it exists only if the traversal
        // keeps every branch, in order.
        assert_eq!(depth(root), 20);
        assert_eq!(
            deepest_path(root),
            vec![
                "Concatenation",
                "Compute Scalar",
                "Hash Match",
                "Compute Scalar",
                "Hash Match",
                "Compute Scalar",
                "Hash Match",
                "Hash Match",
                "Compute Scalar",
                "Hash Match",
                "Compute Scalar",
                "Hash Match",
                "Hash Match",
                "Compute Scalar",
                "Hash Match",
                "Compute Scalar",
                "Hash Match",
                "Filter",
                "Compute Scalar",
                "Clustered Index Scan",
            ]
        );
    }

    #[test]
    fn no_raw_own_cost_in_the_deep_tree_is_negative() {
        // A sanity sweep, NOT clamp coverage — and it asserts the RAW arithmetic,
        // re-derived from subtree costs, because `est_cost >= 0.0` is a tautology
        // the clamp guarantees and so could not fail in either configuration.
        // What this pins is the real claim: `scan.sqlplan` needs no clamping,
        // which is why `own_cost_of_a_nested_loops_join_is_clamped_at_zero` and
        // its single `join.sqlplan` node are the only clamp coverage there is.
        let plan = parse_plan(&fixture("scan.sqlplan")).unwrap();
        let root = plan.statements[0].root.as_ref().unwrap();

        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            let raw = n.subtree_cost - n.children.iter().map(|c| c.subtree_cost).sum::<f64>();
            assert!(raw >= 0.0, "{} had a raw own cost of {raw}", n.physical_op);
            assert!(
                n.subtree_cost > 0.0,
                "{} had no subtree cost",
                n.physical_op
            );
            stack.extend(n.children.iter());
        }
    }

    #[test]
    fn parses_statement_level_convert_warnings() {
        // `scan.sqlplan`'s `<Warnings>` is a child of `<QueryPlan>` — a SIBLING
        // of the root `<RelOp>`, not part of the operator tree. A parser that
        // only looks under operators finds nothing here.
        let plan = parse_plan(&fixture("scan.sqlplan")).unwrap();
        let s = &plan.statements[0];

        assert_eq!(
            s.warnings,
            vec![
                PlanWarning::ImplicitConversion {
                    expression: "CONVERT(bigint,[sov].[value],0)".into(),
                    convert_issue: Some("Cardinality Estimate".into()),
                },
                PlanWarning::ImplicitConversion {
                    expression: "CONVERT(int,[sov4].[value],0)".into(),
                    convert_issue: Some("Cardinality Estimate".into()),
                },
                PlanWarning::ImplicitConversion {
                    expression: "CONVERT(nvarchar(128),[sov2].[value],0)".into(),
                    convert_issue: Some("Cardinality Estimate".into()),
                },
            ]
        );

        // Not hoisted onto the root operator — provenance is preserved.
        assert!(s.root.as_ref().unwrap().warnings.is_empty());

        // …and reachable through the one accessor the judge is meant to use.
        assert_eq!(s.all_warnings().count(), 3);
    }

    #[test]
    fn convert_issue_distinguishes_a_skewed_estimate_from_a_blocked_seek() {
        // The distinction the judge grades on. Every convert we have captured is
        // `Cardinality Estimate`, which skews row estimates but does NOT stop a
        // seek; `Seek Plan` is the index-blocking one. Dropping the attribute
        // would make our only warning fixture look like the headline problem.
        let plan = parse_plan(&fixture("scan.sqlplan")).unwrap();
        let s = &plan.statements[0];

        // Count first: a `for` over an empty iterator asserts nothing, so without
        // this the whole test passes vacuously the day warnings stop being found.
        assert_eq!(s.all_warnings().count(), 3);

        for w in s.all_warnings() {
            let PlanWarning::ImplicitConversion { convert_issue, .. } = w else {
                panic!("expected only converts, got {w:?}");
            };
            assert_eq!(convert_issue.as_deref(), Some("Cardinality Estimate"));
        }
    }

    /// Hand-authored from the ShowPlanXML schema — **not captured from a
    /// server**, and deliberately inline rather than a file in
    /// `tests/fixtures/plans/`, which means "real capture" and would be
    /// corrupted by a fabricated document sitting in it.
    ///
    /// No plan we own contains `<MissingIndexes>`: `sys.*` views do not generate
    /// them, and capturing one needs a scratch table on a DEV box this machine
    /// cannot reach. So the test below proves the parser does what we BELIEVE
    /// the schema says. It cannot prove the belief is correct, and it must not
    /// be read as coverage of the real path.
    ///
    /// TODO(billz-e75): replace with a captured fixture.
    const SCHEMA_DERIVED_MISSING_INDEX: &str = r#"<?xml version="1.0"?>
<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan">
 <BatchSequence><Batch><Statements>
  <StmtSimple StatementText="SELECT * FROM Orders WHERE ShipCity = 'X'" StatementSubTreeCost="1.5" StatementEstRows="7">
   <QueryPlan>
    <MissingIndexes>
     <MissingIndexGroup Impact="99.5061">
      <MissingIndex Database="[master]" Schema="[dbo]" Table="[Orders]">
       <ColumnGroup Usage="EQUALITY"><Column Name="[ShipCity]" ColumnId="5"/></ColumnGroup>
       <ColumnGroup Usage="INCLUDE"><Column Name="[OrderDate]" ColumnId="6"/></ColumnGroup>
      </MissingIndex>
     </MissingIndexGroup>
    </MissingIndexes>
    <RelOp NodeId="0" PhysicalOp="Table Scan" LogicalOp="Table Scan" EstimateRows="7" EstimatedTotalSubtreeCost="1.5"/>
   </QueryPlan>
  </StmtSimple>
 </Statements></Batch></BatchSequence>
</ShowPlanXML>"#;

    #[test]
    fn parses_a_missing_index_from_a_schema_derived_document() {
        // NOT fixture coverage — see `SCHEMA_DERIVED_MISSING_INDEX`.
        let plan = parse_plan(SCHEMA_DERIVED_MISSING_INDEX).unwrap();
        let s = &plan.statements[0];
        assert_eq!(s.missing_indexes.len(), 1);

        let mi = &s.missing_indexes[0];
        assert!(close(mi.impact, 99.5061), "got {}", mi.impact);
        // No `Index` attribute on a `MissingIndex`, so three parts, not four.
        assert_eq!(mi.table, "[master].[dbo].[Orders]");
        assert_eq!(mi.columns, vec!["[ShipCity]", "[OrderDate]"]);
    }

    /// A statement-level `<Warnings NoJoinPredicate=…>` with `attr` as the
    /// attribute's literal text. Schema-derived, like
    /// `SCHEMA_DERIVED_MISSING_INDEX` — no captured plan contains this warning.
    fn schema_derived_no_join_predicate(attr: &str) -> String {
        format!(
            r#"<?xml version="1.0"?>
<ShowPlanXML xmlns="http://schemas.microsoft.com/sqlserver/2004/07/showplan">
 <BatchSequence><Batch><Statements>
  <StmtSimple StatementText="SELECT * FROM a, b" StatementSubTreeCost="9.0" StatementEstRows="1000">
   <QueryPlan>
    <Warnings NoJoinPredicate="{attr}"/>
    <RelOp NodeId="0" PhysicalOp="Nested Loops" LogicalOp="Inner Join" EstimateRows="1000" EstimatedTotalSubtreeCost="9.0"/>
   </QueryPlan>
  </StmtSimple>
 </Statements></Batch></BatchSequence>
</ShowPlanXML>"#
        )
    }

    #[test]
    fn no_join_predicate_is_read_in_either_boolean_encoding() {
        // NOT fixture coverage — see `warnings_from`. This server writes
        // `xs:boolean` BOTH ways in a single document (`SecurityPolicyApplied=
        // "false"` on `<StmtSimple>`, `ForceSeek="0"` on the `<IndexScan>` under
        // it), and we have no specimen of this attribute, so matching only
        // `"true"` would be a coin flip whose losing side is a permanent silent
        // false negative on an accidental cartesian product.
        for attr in ["true", "1"] {
            let plan = parse_plan(&schema_derived_no_join_predicate(attr)).unwrap();
            assert_eq!(
                plan.statements[0].warnings,
                vec![PlanWarning::NoJoinPredicate],
                "NoJoinPredicate=\"{attr}\" was not recognised"
            );
        }

        // The negative forms must stay silent, or the warning fires on every
        // plan that explicitly says the join HAS a predicate.
        for attr in ["false", "0"] {
            let plan = parse_plan(&schema_derived_no_join_predicate(attr)).unwrap();
            assert!(
                plan.statements[0].warnings.is_empty(),
                "NoJoinPredicate=\"{attr}\" wrongly produced a warning"
            );
        }
    }

    #[test]
    fn malformed_xml_is_a_query_error_not_a_panic() {
        let err = parse_plan("<not-xml").unwrap_err();
        assert!(matches!(err, CoreError::Query(_)), "got {err:?}");
        assert!(
            err.to_string().contains("could not parse execution plan"),
            "got {err}"
        );
    }

    #[test]
    fn a_well_formed_document_that_is_not_a_plan_has_no_statements() {
        // Honest behaviour, pinned: nothing to report is not an error.
        let plan = parse_plan("<html><body>nope</body></html>").unwrap();
        assert!(plan.statements.is_empty());
    }
}
