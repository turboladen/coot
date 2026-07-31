//! Plan → shape fingerprint. Pure: no I/O, no driver, no server.
//!
//! Two tenants' plans belong in the same equivalence class when they run the
//! same operators over the same objects in the same structure. This is what
//! partitions ~27 tenant databases into the handful of classes the variance view
//! shows.
//!
//! # Two things must stay out of the key, for the same reason
//!
//! Both would put every tenant in a class of its own — the feature failing
//! SILENTLY (27 classes of one database each reads as "every tenant differs")
//! rather than visibly.
//!
//! 1. **Cost and row estimates.** Precisely what differs between tenants holding
//!    different volumes of data. [`write_node`] reads three fields and no
//!    numbers at all; `cost_and_row_estimates_do_not_change_the_shape` makes
//!    that a property of the real 32-operator `scan.sqlplan` rather than a claim
//!    about the code.
//! 2. **The database name.** Less obvious and just as fatal:
//!    [`parse`](crate::plan::parse) builds an object as
//!    `[Database].[Schema].[Table].[Index]`, so the same query yields
//!    `[TenantA_DEV].[dbo].[Orders].[PK]` on one tenant and
//!    `[TenantB_DEV].[dbo].[Orders].[PK]` on the next. [`strip_database`] drops
//!    that leading component.
//!
//! # What the strip costs
//!
//! It is a pure function of the string, applied uniformly to every plan, so the
//! partition it produces is well defined. But it does COLLAPSE distinctions, and
//! there is a specimen: `scan.sqlplan` contains both
//! `[master].[sys].[syscolpars].[clst]` and
//! `[mssqlsystemresource].[sys].[syscolpars].[clst]`, two genuinely different
//! objects that share a key afterwards. Accepted, because `core` has no notion
//! of which database in a plan is the tenant's — a selective strip is not
//! possible here.
//!
//! # Not in the key
//!
//! Warnings and missing indexes: the key is structure and objects, per design
//! spec §4.1. Statement text likewise — a fan-out runs one SQL string
//! everywhere, so it cannot discriminate.

use crate::plan::model::{PlanNode, QueryPlan};

/// A stable, readable shape key.
///
/// ```text
/// plan      := statement (";" statement)*
/// statement := node | "-"                   // "-" when the statement has no operator tree
/// node      := op (":" object)? ("(" node ("," node)* ")")?
/// op        := physical_op | physical_op "/" logical_op
/// ```
///
/// Readable rather than hashed, deliberately: this is the grouping key the
/// variance view shows, two classes diff against each other line by line, and a
/// test asserting an exact key proves WHAT the shape is where comparing two
/// hashes proves only that they match. The longest fixture key is about 1.5 kB
/// (32 operators), which is nothing as a `HashMap` key for 27 tenants.
///
/// Object names are not escaped, so an identifier containing `(`, `)`, `,`, `;`
/// or `:` could in principle alias two shapes. Accepted: these are ordinary
/// tenant table names, and the failure would merge two classes (visible) rather
/// than split one (silent).
pub fn shape(plan: &QueryPlan) -> String {
    let mut out = String::new();
    for (i, stmt) in plan.statements.iter().enumerate() {
        if i > 0 {
            out.push(';');
        }
        match &stmt.root {
            Some(root) => write_node(root, &mut out),
            // A `StmtSimple` with no `<QueryPlan>` still occupies a position: a
            // statement must never vanish from the key, or a batch that lost one
            // would look identical to one that never had it.
            None => out.push('-'),
        }
    }
    out
}

/// Reads `physical_op`, `logical_op`, `object` and `children`. Nothing else, and
/// in particular no number — that is the whole design.
fn write_node(node: &PlanNode, out: &mut String) {
    // `parse.rs` defaults a missing `PhysicalOp` to `""`, so a degenerate node
    // contributes an empty token here. Unambiguous, and not worth special-casing.
    out.push_str(&node.physical_op);
    // Only when it differs, because the two are identical on most operators and
    // always emitting both adds about 40% length for no information. Injective as
    // long as no operator name contains a `/` — none of the twelve across the
    // fixtures does, and `Hash Match` (which is `Aggregate` in one place and
    // `Right Outer Join` in another, in the same `aggregate.sqlplan` tree) is why
    // the logical name has to be here at all.
    if node.logical_op != node.physical_op {
        out.push('/');
        out.push_str(&node.logical_op);
    }
    if let Some(object) = &node.object {
        out.push(':');
        out.push_str(strip_database(object));
    }
    if !node.children.is_empty() {
        out.push('(');
        for (i, child) in node.children.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            write_node(child, out);
        }
        out.push(')');
    }
}

/// Drop the leading `[Database].` from a `[db].[schema].[table].[index]` name.
///
/// Bracket-aware rather than `split('.')`: `object_name` joins bracket-quoted
/// identifiers with `.` and escapes nothing, so `[my.db].[dbo].[Order.Details]`
/// is a legal product and splitting on `.` mangles it. Cutting after the first
/// `].` is correct for every name whose first component is bracketed.
///
/// Guarded at three or more components, not four. `object_name` is a `filter_map`
/// over four OPTIONAL attributes, so shorter names are reachable and an
/// unconditional strip would turn a bare `Table="[@t]"` into an empty string.
/// Three is the right cut because a heap scan yields exactly
/// `[db].[schema].[table]` — the `LargeScan` case, where leaking the tenant name
/// would hurt most. The guard is also what makes the result provably non-empty:
/// a string with fewer than two `].` boundaries never reaches the cut, and one
/// with two or more always leaves at least a component behind it.
///
/// All 19 `<Object>` elements across the five fixtures carry a `Database`
/// attribute, so in practice this always fires on an operator's object. If a
/// future server omitted it we would strip the schema instead — uniformly, on
/// every tenant, so classes would still partition correctly and only the key
/// would get uglier.
fn strip_database(object: &str) -> &str {
    if !object.starts_with('[') || object.matches("].[").count() + 1 < 3 {
        return object;
    }
    match object.find("].") {
        Some(i) => &object[i + 2..],
        None => object,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::model::{MissingIndex, PlanStatement, PlanWarning};
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

    fn parsed(name: &str) -> QueryPlan {
        parse_plan(&fixture(name)).unwrap()
    }

    /// A hand-built operator. NOT captured.
    fn node(physical_op: &str, object: Option<&str>, children: Vec<PlanNode>) -> PlanNode {
        PlanNode {
            physical_op: physical_op.into(),
            logical_op: physical_op.into(),
            object: object.map(str::to_string),
            est_rows: 1.0,
            est_rows_read: None,
            est_cost: 1.0,
            subtree_cost: 1.0,
            warnings: vec![],
            children,
        }
    }

    /// A hand-built one-statement plan around `root`. NOT captured.
    fn plan_of(root: Option<PlanNode>) -> QueryPlan {
        QueryPlan {
            statements: vec![PlanStatement {
                text: "SELECT 1".into(),
                subtree_cost: 1.0,
                est_rows: 1.0,
                root,
                warnings: vec![],
                missing_indexes: vec![],
            }],
        }
    }

    /// Replace the leading `[Database]` of every object in the tree.
    ///
    /// Deliberately does NOT call [`strip_database`] — a test that builds its
    /// input with the function under test proves nothing.
    fn rewrite_database(node: &mut PlanNode, database: &str) {
        if let Some(object) = &node.object {
            let cut = object
                .find("].")
                .expect("every fixture object is [db].[schema].[table][.index]");
            node.object = Some(format!("[{database}].{}", &object[cut + 2..]));
        }
        for child in &mut node.children {
            rewrite_database(child, database);
        }
    }

    /// Multiply every number in the tree by something, and add a warning.
    fn scramble(node: &mut PlanNode) {
        node.est_rows = node.est_rows * 1000.0 + 7.0;
        node.est_rows_read = Some(node.est_rows_read.unwrap_or(0.0) * 13.0 + 1.0);
        node.est_cost = node.est_cost * 97.0 + 3.0;
        node.subtree_cost = node.subtree_cost * 97.0 + 3.0;
        node.warnings.push(PlanWarning::NoJoinPredicate);
        for child in &mut node.children {
            scramble(child);
        }
    }

    // --------------------------------------------------- the load-bearing pair

    #[test]
    fn the_tenant_database_name_is_not_part_of_the_shape() {
        // THE cross-tenant test. `scan.sqlplan` has 11 objects spanning two
        // databases, so it exercises the strip on every arity the fixtures have.
        //
        // Three shapes must agree — not two. Comparing one rewrite against the
        // original passes for a strip that merely maps every database to the
        // same thing; three distinct tenant names pin that the component is gone
        // rather than normalised. And the `contains` sweep at the end is what
        // fails if `strip_database` silently no-ops: shapes would still be
        // pairwise unequal, but the assertion above would already have caught
        // that, whereas a strip that dropped the wrong component would pass it.
        let original = shape(&parsed("scan.sqlplan"));

        let mut a = parsed("scan.sqlplan");
        rewrite_database(a.statements[0].root.as_mut().unwrap(), "TenantA_DEV");
        let mut b = parsed("scan.sqlplan");
        rewrite_database(b.statements[0].root.as_mut().unwrap(), "TenantB_DEV");

        assert_eq!(original, shape(&a));
        assert_eq!(original, shape(&b));

        for leaked in ["master", "mssqlsystemresource", "TenantA", "TenantB"] {
            assert!(
                !original.contains(leaked),
                "database name {leaked} survived into the shape key: {original}"
            );
        }
        // …and the rest of the object is still there, so the strip did not simply
        // drop the object.
        assert!(
            original.contains("[sys].[syscolpars].[clst]"),
            "got {original}"
        );
    }

    #[test]
    fn cost_and_row_estimates_do_not_change_the_shape() {
        // The property, over the real 32-operator tree rather than a hand-built
        // two-node one: EVERY numeric field tree-wide and statement-wide is
        // scrambled, the warnings replaced, the text changed. A fingerprint that
        // reads any of them fails here.
        let plan = parsed("scan.sqlplan");
        let before = shape(&plan);

        let mut after = parsed("scan.sqlplan");
        for stmt in &mut after.statements {
            stmt.text = "SELECT something else entirely".into();
            stmt.subtree_cost = stmt.subtree_cost * 500.0 + 11.0;
            stmt.est_rows = stmt.est_rows * 500.0 + 11.0;
            stmt.warnings.clear();
            stmt.missing_indexes.push(MissingIndex {
                impact: 99.0,
                table: "[master].[dbo].[Orders]".into(),
                columns: vec!["[ShipCity]".into()],
            });
            if let Some(root) = &mut stmt.root {
                scramble(root);
            }
        }

        assert_eq!(before, shape(&after));
    }

    // -------------------------------------------------------- exact keys

    #[test]
    fn the_shape_of_a_seek_is_the_exact_string_we_expect() {
        assert_eq!(
            shape(&parsed("seek.sqlplan")),
            "Filter(Clustered Index Seek:[sys].[sysschobjs].[clst])"
        );
    }

    #[test]
    fn every_statement_appears_in_the_shape() {
        assert_eq!(
            shape(&parsed("two-statements.sqlplan")),
            "Compute Scalar(Stream Aggregate/Aggregate(Filter(Clustered Index Scan:\
             [sys].[sysschobjs].[clst])));Top(Clustered Index Seek:[sys].[sysclsobjs].[clst])"
        );
    }

    #[test]
    fn the_logical_operator_appears_only_when_it_differs() {
        // `aggregate.sqlplan` is why the logical name is in the key at all: one
        // physical `Hash Match` serving two different logical roles in one tree.
        let s = shape(&parsed("aggregate.sqlplan"));
        assert!(s.contains("Hash Match/Aggregate"), "got {s}");
        assert!(s.contains("Hash Match/Right Outer Join"), "got {s}");
        assert!(
            !s.contains("Filter/Filter"),
            "an operator whose two names agree must appear once: {s}"
        );
    }

    // ------------------------------------------------------------ discrimination

    #[test]
    fn a_different_physical_operator_changes_the_shape() {
        let seek = plan_of(Some(node(
            "Index Seek",
            Some("[db].[dbo].[T].[IX]"),
            vec![],
        )));
        let scan = plan_of(Some(node(
            "Clustered Index Scan",
            Some("[db].[dbo].[T].[IX]"),
            vec![],
        )));
        assert_ne!(shape(&seek), shape(&scan));
    }

    #[test]
    fn a_different_object_changes_the_shape() {
        let orders = plan_of(Some(node(
            "Index Seek",
            Some("[db].[dbo].[Orders].[IX]"),
            vec![],
        )));
        let customers = plan_of(Some(node(
            "Index Seek",
            Some("[db].[dbo].[Customers].[IX]"),
            vec![],
        )));
        assert_ne!(shape(&orders), shape(&customers));
    }

    #[test]
    fn a_different_index_on_the_same_table_changes_the_shape() {
        // The variance this feature exists to surface: same table, different
        // index chosen. The index component must survive the database strip.
        let clustered = plan_of(Some(node(
            "Index Seek",
            Some("[db].[dbo].[Orders].[PK_Orders]"),
            vec![],
        )));
        let covering = plan_of(Some(node(
            "Index Seek",
            Some("[db].[dbo].[Orders].[IX_Covering]"),
            vec![],
        )));
        assert_ne!(shape(&clustered), shape(&covering));
    }

    #[test]
    fn nesting_structure_changes_the_shape() {
        let flat = plan_of(Some(node("Hash Match", None, vec![])));
        let nested = plan_of(Some(node(
            "Hash Match",
            None,
            vec![node("Table Scan", None, vec![])],
        )));
        assert_ne!(shape(&flat), shape(&nested));
    }

    #[test]
    fn child_order_is_significant() {
        let ab = plan_of(Some(node(
            "Hash Match",
            None,
            vec![
                node("Table Scan", None, vec![]),
                node("Index Seek", None, vec![]),
            ],
        )));
        let ba = plan_of(Some(node(
            "Hash Match",
            None,
            vec![
                node("Index Seek", None, vec![]),
                node("Table Scan", None, vec![]),
            ],
        )));
        assert_ne!(shape(&ab), shape(&ba));
    }

    #[test]
    fn a_statement_with_no_operator_tree_still_occupies_its_position() {
        assert_eq!(shape(&plan_of(None)), "-");

        let mut two = plan_of(None);
        two.statements.push(plan_of(None).statements.pop().unwrap());
        assert_eq!(shape(&two), "-;-");

        // A plan with no statements at all is empty, not an error.
        assert_eq!(shape(&QueryPlan { statements: vec![] }), "");
    }

    // ---------------------------------------------------------- strip_database

    #[test]
    fn strip_database_drops_the_leading_component() {
        assert_eq!(
            strip_database("[master].[sys].[sysschobjs].[clst]"),
            "[sys].[sysschobjs].[clst]"
        );
        // A heap scan: three components, no index. Exactly the case a >= 4 guard
        // would have leaked the tenant name on.
        assert_eq!(
            strip_database("[TenantA_DEV].[dbo].[Orders]"),
            "[dbo].[Orders]"
        );
    }

    #[test]
    fn strip_database_survives_a_dotted_identifier() {
        // `object_name` escapes nothing, so a `.` inside a bracketed identifier
        // reaches us verbatim. `split('.')` would mangle both of these.
        assert_eq!(
            strip_database("[my.db].[dbo].[Order.Details]"),
            "[dbo].[Order.Details]"
        );
        assert_eq!(
            strip_database("[db].[my.schema].[t].[ix]"),
            "[my.schema].[t].[ix]"
        );
    }

    #[test]
    fn strip_database_leaves_short_and_odd_names_alone_and_never_empties_them() {
        for short in ["[dbo].[Orders]", "[@t]", "", "NoBracketsHere", "[only]"] {
            assert_eq!(strip_database(short), short);
        }
        // The guard's real job: nothing it returns is ever empty unless it was
        // handed an empty string.
        for name in [
            "[master].[sys].[sysschobjs].[clst]",
            "[db].[dbo].[Orders]",
            "[dbo].[Orders]",
            "[@t]",
        ] {
            assert!(!strip_database(name).is_empty(), "{name} stripped to empty");
        }
    }

    #[test]
    fn the_strip_collapses_two_genuinely_different_objects() {
        // The documented COST of the strip, with its specimen: `scan.sqlplan`
        // contains both of these, and they share a key afterwards. `core` cannot
        // tell which database in a plan is the tenant's, so a selective strip is
        // not available — this is accepted, not overlooked.
        assert_eq!(
            strip_database("[master].[sys].[syscolpars].[clst]"),
            strip_database("[mssqlsystemresource].[sys].[syscolpars].[clst]")
        );
    }
}
