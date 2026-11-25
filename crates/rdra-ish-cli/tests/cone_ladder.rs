//! The cone ladder of phase 4 (docs/design/native-temporal-verification.md, "Modes and the
//! cone"), against the whole model, with no TLC: `verify` decides each `always`, `eventually`,
//! and `leads_to` of a small model, and its final verdict must be the one the native checker
//! gives on the whole model's graph, which phases 2 and 3 compared with TLC. A wrong `ok` from
//! a step of the ladder fails here in seconds; `engine_equivalence` compares the same cases
//! with TLC's whole-model run.
//!
//! `RDRA_ISH_LADDER_STATS` names the step that decided each property its cone left open, and
//! each case states the step it expects.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use rdra_ish_core::lifecycle::{Bounds, Caps, Fairness, PropertyVerdict, Scope, TransitionSystem};
use rdra_ish_core::load_project;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A model: a path under the repository, or a source written to a scratch file.
enum Model {
    File(&'static str),
    Source(&'static str, &'static str),
}

/// A case, and per property the step of `PropertyCone::settle` that decides it (`Step`'s
/// `Debug` name), for the properties the cone leaves open.
type Case = (Model, &'static [(&'static str, &'static str)]);

const CASES: &[Case] = &[
    (
        Model::File("samples/formal-verification/order.rdra"),
        &[
            ("PaidLeadsToShipped", "Monotone"),
            ("EventuallyTerminal", "Monotone"),
        ],
    ),
    (
        Model::File("samples/formal-verification/temporal_fair.rdra"),
        &[],
    ),
    (
        Model::File("samples/formal-verification/temporal_unmet.rdra"),
        &[],
    ),
    (
        Model::File("samples/formal-verification/temporal_along.rdra"),
        &[],
    ),
    (
        Model::File("crates/rdra-ish-cli/tests/engine_corpus/cone_receipt_frees_slots.rdra"),
        &[("PendingPays", "Exact")],
    ),
    (
        Model::File("crates/rdra-ish-cli/tests/engine_corpus/cone_always_frees_slots.rdra"),
        &[("NeverD", "Exact")],
    ),
    (
        Model::File("crates/rdra-ish-cli/tests/engine_corpus/cone_handler_mark_left_out.rdra"),
        &[("ShipsUnfair", "Exact")],
    ),
    (
        Model::File("crates/rdra-ish-cli/tests/engine_corpus/cone_independent_loop.rdra"),
        &[("PaidShipsUnfair", "Independent")],
    ),
    (
        Model::File("crates/rdra-ish-cli/tests/engine_corpus/cone_outside_monotone.rdra"),
        &[
            ("PaidShipsStamp", "Monotone"),
            ("PaidShipsNotes", "Monotone"),
        ],
    ),
    (
        Model::File("crates/rdra-ish-cli/tests/engine_corpus/cone_copy_from_parent.rdra"),
        &[("IssueEventuallyPlain", "Loops")],
    ),
    (
        Model::Source("guard_reads_other", GUARD_READS_OTHER),
        &[("PaidShipsHalfFair", "Monotone")],
    ),
    (
        Model::Source("quantifier_chain", QUANTIFIER_CHAIN),
        &[
            ("OrderShips", "Independent"),
            ("OrderShipsPlain", "Independent"),
        ],
    ),
    (
        Model::Source("frees_slots", FREES_SLOTS),
        &[("PaidShips", "Exact")],
    ),
    (
        Model::Source("shared_writer", SHARED_WRITER),
        &[
            ("PaidLeadsToShipped", "Loops"),
            ("EventuallyTerminal", "Loops"),
        ],
    ),
    (
        Model::Source("loop_needs_a_mark", LOOP_NEEDS_A_MARK),
        &[("PaidShipsUnfair", "Loops")],
    ),
];

/// The flag toggles outside the cone, but Pay and Ship write it with the status: the flag's
/// cone shares their writes, so it is no independent loop (step 5), and the whole model,
/// where the flag moves only with the order, is `ok`.
const SHARED_WRITER: &str = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, shipped) @default(pending)
  open: Bool @default(true)
}
usecase Place "注文"
usecase Pay "支払い"
usecase Ship "出荷"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
sets(Pay, Order.open == false)
transitions(Ship, Order.status, paid -> shipped)
sets(Ship, Order.open == true)
property PaidLeadsToShipped leads_to(Order.status == paid, Order.status == shipped)
property EventuallyTerminal eventually(Order.status == shipped)
"#;

/// The ticket's status can go back to open only on an escalation, and nothing escalates, since
/// no use case makes a ticket urgent. A candidate loop that left the event's mark out would
/// reopen tickets freely and loop; the exact one holds the mark and Escalate, finds no cycle,
/// and step 5 does not fire. The whole model is `ok`: the ticket closes once, and the paid
/// order must then ship.
const LOOP_NEEDS_A_MARK: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, shipped) @default(pending) }
entity Ticket "問い合わせ" {
  id: Int @pk
  status: Enum(open, closed) @default(open)
  urgent: Bool @default(false)
}
usecase Place "注文"
usecase Pay "支払い"
usecase Ship "出荷"
usecase Open "起票"
usecase Close "完了"
usecase Escalate "エスカレーション"
usecase Reopen "再開"
event EvEscalated "エスカレーション済"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
creates(Open, Ticket)
transitions(Close, Ticket.status, open -> closed)
updates(Escalate, Ticket)
condition Urgent "緊急"
holds(Urgent, Ticket.urgent == true)
applies_to(Urgent, Escalate)
raises(Escalate, EvEscalated)
triggers(EvEscalated, Reopen)
transitions(Reopen, Ticket.status, closed -> open)
property PaidShipsUnfair leads_to(Order.status == paid, Order.status == shipped)
"#;

/// The fair use case's guard reads another entity, which is built once and only opens; the
/// order's note is set once, outside the cone (engine_equivalence.rs, `GUARD_READS_OTHER`).
const GUARD_READS_OTHER: &str = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, shipped) @default(pending)
  note: Bool @default(false)
}
entity Warehouse "倉庫" { id: Int @pk  state: Enum(closed, open) @default(closed) }
usecase Place "注文"
usecase Pay "支払い"
usecase Ship "出荷"
usecase Annotate "備考"
usecase Build "開設"
usecase OpenWh "開場"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
sets(Annotate, Order.note == true)
creates(Build, Warehouse)
transitions(OpenWh, Warehouse.state, closed -> open)
condition Open "倉庫が開いている"
holds(Open, Order.status == paid, Warehouse.state == open)
applies_to(Open, Ship)
property PaidShipsFair leads_to(Order.status == paid, Order.status == shipped).fair(Ship, Build, OpenWh)
property PaidShipsHalfFair leads_to(Order.status == paid, Order.status == shipped).fair(Ship)
"#;

/// A hold that is never released blocks the order while the clerk loops outside every cone
/// (engine_equivalence.rs, `QUANTIFIER_CHAIN`).
const QUANTIFIER_CHAIN: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(open, shipped) @default(open) }
entity Hold "保留" { id: Int @pk  state: Enum(active, released) @default(active) }
entity Clerk "担当" { id: Int @pk  state: Enum(away, here) @default(away) }
relate(Hold, Order, N:1)
usecase Place "注文"
usecase PlaceHold "保留登録"
usecase Release "保留解除"
usecase Ship "出荷"
usecase Arrive "出勤"
usecase Leave "退勤"
creates(Place, Order)
creates(PlaceHold, Hold)
transitions(Release, Hold.state, active -> released)
updates(Ship, Order)
transitions(Ship, Order.status, open -> shipped)
transitions(Arrive, Clerk.state, away -> here)
transitions(Leave, Clerk.state, here -> away)
condition Clear "保留がない"
holds(Clear, Order.status == open).none(Hold.state == active)
applies_to(Clear, Ship)
property OrderShips leads_to(Order.status == open, Order.status == shipped).fair(Ship)
property OrderShipsReleased leads_to(Order.status == open, Order.status == shipped).fair(Ship, Release)
property OrderShipsPlain leads_to(Order.status == open, Order.status == shipped)
"#;

/// Pay creates a receipt the cone does not read, and Reopen undoes the payment
/// (engine_equivalence.rs, `FREES_SLOTS`).
const FREES_SLOTS: &str = r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, shipped) @default(pending) }
entity Receipt "領収書" { id: Int @pk }
relate(Receipt, Order, N:1)
usecase Place "注文"
usecase Pay "支払い"
usecase Reopen "差し戻し"
usecase Ship "出荷"
creates(Place, Order)
transitions(Pay, Order.status, pending -> paid)
creates(Pay, Receipt)
transitions(Reopen, Order.status, paid -> pending)
transitions(Ship, Order.status, paid -> shipped)
property PaidShips leads_to(Order.status == paid, Order.status == shipped).fair(Ship)
"#;

/// The final verdict `verify` printed per property: the whole-model line where there is one,
/// else the cone line; and the step that decided each property its cone left open.
struct Printed {
    verdicts: BTreeMap<String, String>,
    steps: BTreeMap<String, String>,
    /// The `RdraSpec_` lines: whole-model runs, which the ladder replaces.
    wholes: Vec<String>,
}

fn verify(path: &Path) -> Printed {
    let out = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .env("RDRA_ISH_LADDER_STATS", "1")
        .arg("verify")
        .arg(path)
        .output()
        .expect("rdra-ish runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut verdicts = BTreeMap::new();
    let mut wholes = Vec::new();
    for line in stdout.lines() {
        let (module, rest) = match line.split_once(" [") {
            Some(pair) => pair,
            None => continue,
        };
        let Some((checks, rest)) = rest.split_once("]: ") else {
            continue;
        };
        if !(module.starts_with("Cone_") || module.starts_with("RdraSpec_")) || checks.contains(',')
        {
            continue;
        }
        let verdict = ["ok", "counterexample", "not decided", "tool_error"]
            .into_iter()
            .find(|v| rest.starts_with(v))
            .unwrap_or_else(|| panic!("a verdict on {line}"));
        // The whole-model line comes after the cone's and overrides it.
        verdicts.insert(checks.to_string(), verdict.to_string());
        if module.starts_with("RdraSpec_") {
            wholes.push(line.to_string());
        }
    }
    let mut steps = BTreeMap::new();
    for line in stderr.lines() {
        let Some(rest) = line.strip_prefix("ladder: ") else {
            continue;
        };
        let mut words = rest.split(' ');
        let name = words.next().expect("a property").to_string();
        let step = words
            .next()
            .and_then(|w| w.strip_prefix("step="))
            .expect("a step");
        steps.insert(name, step.to_string());
    }
    Printed {
        verdicts,
        steps,
        wholes,
    }
}

/// What the native checker decides of each `always`, `eventually`, and `leads_to` on the
/// whole model's graph, with `WF_vars(Next)` for the liveness forms.
fn whole(path: &Path) -> BTreeMap<String, String> {
    let model = load_project(&[path.to_path_buf()], None)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .model;
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let run = ts.clone().run(Caps::VERIFY);
    assert!(run.complete, "{}: the whole model finishes", path.display());
    let mut out = BTreeMap::new();
    for (i, p) in ts.properties().iter().enumerate() {
        let kind = p.formula.kind();
        if kind.is_explored() {
            continue;
        }
        let verdict = match kind.is_liveness() {
            true => run
                .decide_liveness(i, Fairness::WfNext)
                .expect("a lasso that checks itself"),
            false => run.decide_property(i).expect("an always"),
        };
        let said = match verdict {
            PropertyVerdict::Holds => "ok",
            PropertyVerdict::Violated(_) => "counterexample",
            PropertyVerdict::Undecided => unreachable!("a complete run"),
        };
        out.insert(p.name.clone(), said.to_string());
    }
    out
}

#[test]
fn the_cone_ladder_decides_what_the_whole_model_decides() {
    let scratch = std::env::temp_dir().join(format!("rdra-ladder-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let mut failures = Vec::new();
    for (model, expected) in CASES {
        let path = match model {
            Model::File(rel) => repo().join(rel),
            Model::Source(name, src) => {
                let file = scratch.join(format!("{name}.rdra"));
                std::fs::write(&file, src).expect("write model");
                file
            }
        };
        let label = path.file_name().unwrap().to_string_lossy().to_string();
        let printed = verify(&path);
        for line in &printed.wholes {
            failures.push(format!("{label}: a whole-model run: {line}"));
        }
        let whole = whole(&path);
        for (name, want) in &whole {
            match printed.verdicts.get(name) {
                Some(got) if got == want => {}
                got => failures.push(format!(
                    "{label}: {name}: verify says {got:?}, the whole model {want}"
                )),
            }
        }
        let expected: BTreeMap<String, String> = expected
            .iter()
            .map(|(n, s)| (n.to_string(), s.to_string()))
            .collect();
        if printed.steps != expected {
            failures.push(format!(
                "{label}: the steps that decided {:?}, expected {expected:?}",
                printed.steps
            ));
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `fired` counts each step as `settle` decides with it, in this process: the measure the
/// owner reads to decide whether step 5 stays (docs/design/native-temporal-verification.md,
/// "Decisions").
#[test]
fn settle_counts_the_step_that_decides() {
    use rdra_ish_cli::property_cone::{fired, PropertyCone, Step, Verdict};
    let path = repo().join("crates/rdra-ish-cli/tests/engine_corpus/cone_independent_loop.rdra");
    let model = load_project(&[path], None).expect("loads").model;
    let ts = TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL).ts;
    let i = (ts.properties().iter())
        .position(|p| p.name == "PaidShipsUnfair")
        .expect("the property");
    let before = fired(Step::Independent);
    let settled = PropertyCone::new(&ts, i).settle(&ts, Caps::VERIFY);
    assert_eq!(settled.step, Some(Step::Independent));
    assert_eq!(settled.verdict, Verdict::Counterexample);
    assert!(fired(Step::Independent) > before);
}
