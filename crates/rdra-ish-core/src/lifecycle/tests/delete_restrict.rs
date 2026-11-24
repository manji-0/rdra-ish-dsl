//! `deletes(UC, Parent)` over a key whose database rejects the delete (`restrict`,
//! `no_action`, or no `.on_delete`, the SQL default): the step waits until no present child
//! names the row (#199), and `DanglingKey_<Child>_<role>` checks that no child is left
//! naming a gone row. `cascade`, `set_null`, and `set_default` still delete the one row with
//! no guard and no such rule, as before #199.

use super::*;
use crate::lifecycle::types::{EntId, GuardWhy, Value};
use crate::lifecycle::ComponentRun;

/// The issue's model: an Order with lines, and a use case that cancels the draft Order.
/// `key` follows the relate; `extra` adds statements.
fn order(key: &str, extra: &str) -> String {
    format!(
        r#"
entity Order "注文" {{ id: Int @pk  status: Enum(draft, placed) @default(draft) }}
entity OrderLine "明細" {{ id: Int @pk }}
relate(OrderLine, Order, N:1){key}
usecase Open "起票"
usecase AddLine "明細追加"
usecase CancelDraft "下書き取消"
usecase Place "確定"
creates(Open, Order)
creates(AddLine, OrderLine)
deletes(CancelDraft, Order)
transitions(Place, Order.status, draft -> placed)
{extra}
"#
    )
}

const REJECTING: [&str; 3] = ["", ".on_delete(restrict)", ".on_delete(no_action)"];

fn system(src: &str, bounds: Bounds) -> TransitionSystem {
    TransitionSystem::build(&model_from(src), bounds, &Scope::ALL).ts
}

fn explore(src: &str, bounds: Bounds) -> ComponentRun {
    let run = system(src, bounds).run(Caps::new(usize::MAX, 1 << 20));
    assert!(run.complete);
    run
}

fn ent(ts: &TransitionSystem, id: &str) -> EntId {
    ts.entity_ids()
        .find(|e| ts.entity(*e).id == id)
        .unwrap_or_else(|| panic!("no entity {id}"))
}

/// Whether some reached state holds every slot of `Order` at `order` and every slot of
/// `OrderLine` at `line`, each `Value::PRESENT`, `Value::DELETED` or `Value::ABSENT`.
fn reaches(run: &ComponentRun, order: Value, line: Value) -> bool {
    let ts = &run.system;
    let (o, l) = (ent(ts, "Order"), ent(ts, "OrderLine"));
    let slots = ts.bounds.slots();
    run.states.iter().any(|s| {
        (0..slots).all(|i| s.get(ts, ts.entity(o).exists, i) == order)
            && (0..slots).all(|i| s.get(ts, ts.entity(l).exists, i) == line)
    })
}

fn restrict_guards(ts: &TransitionSystem, action: &str) -> usize {
    let a = ts
        .actions()
        .iter()
        .find(|a| a.name == action)
        .unwrap_or_else(|| panic!("no action {action}"));
    a.guards
        .iter()
        .filter(|g| matches!(g.why, GuardWhy::Restrict { .. }))
        .count()
}

fn dangling_violated(src: &str) -> bool {
    derive_src(src).diags.iter().any(|x| {
        matches!(&x.diag, StateDiag::ConstraintViolated { name, .. }
            if name == "DanglingKey_OrderLine_Order")
    })
}

/// The issue's model: Postgres rejects deleting an Order a line names, so CancelDraft waits
/// until the Order has no line, under no `.on_delete` as under `restrict` and `no_action`.
/// No state holds a deleted Order beside a present line, while an Order with no line still
/// goes.
#[test]
fn a_delete_waits_until_no_child_names_the_row() {
    for key in REJECTING {
        let src = order(key, "");
        let ts = system(&src, Bounds::STATES);
        assert_eq!(restrict_guards(&ts, "CancelDraft"), 1, "{key}");
        assert_eq!(restrict_guards(&ts, "Open"), 0, "{key}");
        let run = explore(&src, Bounds::STATES);
        assert!(
            !reaches(&run, Value::DELETED, Value::PRESENT),
            "{key}: an Order went while its line stayed"
        );
        assert!(reaches(&run, Value::DELETED, Value::ABSENT), "{key}");
        assert!(!dangling_violated(&src), "{key}");
    }
}

/// The rewrite: a use case that deletes the line before the Order is cancelled lets the
/// Order go. So does a use case that deletes the line in the same step, as long as no other
/// line names the Order.
#[test]
fn deleting_the_children_first_lets_the_parent_go() {
    let first = order(
        "",
        "usecase RemoveLine \"明細削除\"\ndeletes(RemoveLine, OrderLine)",
    );
    assert!(reaches(
        &explore(&first, Bounds::STATES),
        Value::DELETED,
        Value::DELETED
    ));
    let together = order("", "deletes(CancelDraft, OrderLine)");
    assert!(reaches(
        &explore(&together, Bounds::STATES),
        Value::DELETED,
        Value::DELETED
    ));
    // At two slots an Order can hold two lines, and the step deletes one of them.
    let run = explore(&together, Bounds::VERIFY);
    let ts = &run.system;
    let (o, l) = (ent(ts, "Order"), ent(ts, "OrderLine"));
    let owner = ts
        .var_ids()
        .find(|v| matches!(ts.var(*v).kind, crate::lifecycle::VarKind::Owner { .. }))
        .expect("the line's key");
    let dangling = run.states.iter().any(|s| {
        (0..2).any(|i| {
            s.is_present(ts, l, i)
                && s.get(ts, ts.entity(o).exists, usize::from(s.get(ts, owner, i).0))
                    == Value::DELETED
        })
    });
    assert!(!dangling, "a line names a deleted Order");
}

/// `cascade`, `set_null`, and `set_default` keep the step they had before #199: the delete
/// removes the one row with no guard, and the children stay. The lifecycle does not run
/// those actions yet, so the key gets no `DanglingKey_*` and nothing fails; `cascade` keeps
/// the info `DeleteLeavesChildren`. Deleting the lines in another use case changes nothing.
#[test]
fn a_cascade_set_null_or_set_default_delete_keeps_its_one_row_step() {
    // An optional line starts unlinked, so Attach links it before the Order goes.
    let attach = "usecase Attach \"紐付け\"\nlinks(Attach, OrderLine, Order)";
    let remove = "usecase RemoveLine \"明細削除\"\ndeletes(RemoveLine, OrderLine)";
    for (key, extra) in [
        (".on_delete(cascade)", ""),
        (".on_delete(cascade)", remove),
        (".optional().on_delete(set_null)", attach),
        (".on_delete(set_default)", ""),
    ] {
        let src = order(key, extra);
        let ts = system(&src, Bounds::STATES);
        assert_eq!(restrict_guards(&ts, "CancelDraft"), 0, "{key}");
        assert!(
            ts.invariants()
                .iter()
                .all(|i| !i.name.starts_with("DanglingKey_")),
            "{key}"
        );
        assert!(
            reaches(
                &explore(&src, Bounds::STATES),
                Value::DELETED,
                Value::PRESENT
            ),
            "{key}"
        );
        let diags = derive_scoped(&model_from(&src), Bounds::VERIFY, &Scope::ALL).diags;
        assert!(
            !diags
                .iter()
                .any(|x| matches!(x.diag, StateDiag::ConstraintViolated { .. })),
            "{key}"
        );
        let leaves = diags
            .iter()
            .any(|x| matches!(x.diag, StateDiag::DeleteLeavesChildren { .. }));
        assert_eq!(leaves, key.contains("cascade"), "{key}");
    }
}

/// A key's rule exists only where a step deletes its parent, so a model that deletes no
/// parent keeps its checks as they were. A parent with a `@soft_delete` column is marked, not
/// removed, so no key rejects its delete and its children keep naming a row.
#[test]
fn only_a_deleted_hard_parent_has_a_guard_and_a_rule() {
    let rules = |src: &str| {
        let ts = system(src, Bounds::STATES);
        let names: Vec<String> = ts
            .invariants()
            .iter()
            .map(|i| i.name.clone())
            .filter(|n| n.starts_with("DanglingKey_"))
            .collect();
        (restrict_guards(&ts, "CancelDraft"), names)
    };
    assert_eq!(
        rules(&order("", "")),
        (1, vec!["DanglingKey_OrderLine_Order".to_string()])
    );
    let soft = order("", "").replace(
        "status: Enum(draft, placed) @default(draft) }",
        "status: Enum(draft, placed) @default(draft)  deleted_at: DateTime @null @soft_delete }",
    );
    assert!(soft.contains("@soft_delete"));
    assert_eq!(rules(&soft), (0, Vec::new()));
    assert_eq!(rules(&order(".on_delete(cascade)", "")), (0, Vec::new()));
    let no_delete = order("", "").replace("deletes(CancelDraft, Order)", "");
    let ts = system(&no_delete, Bounds::STATES);
    assert!(ts
        .invariants()
        .iter()
        .all(|i| !i.name.starts_with("DanglingKey_")));
    // A pre-existing line of a created Order starts naming an Order that does not exist yet,
    // which `PreExistingChildOfCreatedParent` reports; no delete causes it, so no rule.
    let pre = order("", "").replace("creates(AddLine, OrderLine)", "");
    assert_eq!(rules(&pre).1, Vec::<String>::new());
}

/// A step that unlinks a line and deletes its Order is what the database accepts: the line
/// no longer names the row when the delete runs, so the guard leaves it out.
#[test]
fn a_child_the_step_unlinks_does_not_hold_the_delete() {
    let src = order(
        ".optional()",
        "usecase Attach \"紐付け\"\nlinks(Attach, OrderLine, Order)\nunlinks(CancelDraft, OrderLine, Order)",
    );
    let run = explore(&src, Bounds::STATES);
    assert!(reaches(&run, Value::DELETED, Value::PRESENT));
    assert!(!dangling_violated(&src));
}
