//! The prover with a real z3. Without `z3` on PATH (or `RDRA_Z3`) each test is skipped with a
//! printed reason; `RDRA_REQUIRE_Z3=1` turns the skip into a failure.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::*;
use crate::lifecycle::prove::{self, ProveOptions, Unproved, Verdict, Z3};
use crate::lifecycle::types::Value;

fn z3() -> Option<PathBuf> {
    let found = std::env::var_os("RDRA_Z3").map(PathBuf::from).or_else(|| {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|d| d.join("z3"))
            .find(|p| p.is_file())
    });
    if found.is_none() {
        assert!(
            std::env::var("RDRA_REQUIRE_Z3").as_deref() != Ok("1"),
            "RDRA_REQUIRE_Z3=1 but z3 is not on PATH (run inside `nix develop`)"
        );
        println!("skipped: z3 is not on PATH");
    }
    found
}

const DOOR: &str = r#"
entity Door "扉" { id: Int @pk  state: Enum(closed, open) @default(closed) }
usecase Open "開ける"
transitions(Open, Door.state, closed -> open)
forbidden(Door, state == open)
"#;

/// Every check of a proof assumes the declarations and the initial states. Were they
/// contradictory, every check would be `unsat` and every rule `proved`, the false ones too.
/// No model loads with such a premise: the encoding over-approximates the explorer (B1 in
/// docs/theory/claims.md), so the explorer's initial state is a model of it. The test stands
/// in for a builder or encoder bug that breaks that: it gives the existence of a pre-existing
/// `Door` the base value `present`, which the encoding gives the `nil` row too, while the
/// declarations say `nil` is never a row. The explorer starts every pre-existing row present
/// whatever the base, so it runs as before; the prover's initial states are empty. Then no
/// rule is proved, and the verdict says why. On main the false rule was `proved`.
#[test]
fn a_contradictory_premise_proves_no_rule() {
    let Some(z3) = z3() else { return };
    let mut ts = system_from(DOOR, Bounds::VERIFY);
    let exists = ts.entity(ent(&ts, "Door")).exists;
    let rules: Vec<usize> = (0..ts.invariants().len()).collect();
    let solve = |ts: &TransitionSystem| {
        let mut solvers = [Z3::start(&z3).expect("z3 starts")];
        prove::prove(
            ts,
            None,
            &rules,
            &BTreeSet::new(),
            &mut solvers,
            ProveOptions::default(),
        )
        .expect("z3 answers")
    };

    // As built, the rule is false (Open breaks it) and the premise has a model.
    let proof = solve(&ts);
    assert!(
        proof.rules.iter().all(|r| matches!(
            r.verdict,
            Verdict::Unproved(Unproved::Initial | Unproved::NotInductive(_))
        )),
        "{proof:?}"
    );

    ts.vars[exists.index()].base = Value::PRESENT;
    let proof = solve(&ts);
    assert_eq!(
        proof.proved(),
        0,
        "a contradictory premise proved {proof:?}"
    );
    for r in &proof.rules {
        match &r.verdict {
            Verdict::Unproved(Unproved::Premise(why)) => {
                assert!(
                    why.contains("the canary `false` survived initiation")
                        && why.contains("inconsistent"),
                    "{why}"
                )
            }
            other => panic!("{}: {other:?}", ts.invariants()[r.inv].name),
        }
    }
}

/// A rule that holds for any number of rows, beside a guard that compares with `<`.
const FILL: &str = r#"
entity Item "商品" { id: Int @pk  k: Int @null  n: Int }
usecase Add "追加"
usecase Bump "増加"
usecase Fill "記入"
creates(Add, Item)
updates(Bump, Item)
sets(Bump, Item.k == present)
updates(Fill, Item)
condition Small "小さい"
holds(Small, Item.k < 3)
sets(Fill, Item.n == 1).when(Small)
forbidden(Item, n == 2)
"#;

/// The checks of a proof's own output (claims B1 and B16): with nothing planted the rule is
/// proved and no check fails; a planted wrong comparison in the encoding (`<` written as
/// `<=`) and a planted false lemma kept beside the rule are each caught, and the proof then
/// proves nothing and says why.
#[test]
fn a_planted_fault_in_the_proof_proves_nothing() {
    use crate::lifecycle::explore::symmetry::witnesses::dropping;
    let Some(z3) = z3() else { return };
    let ts = system_from(FILL, Bounds::VERIFY);
    let rules: Vec<usize> = (0..ts.invariants().len()).collect();
    let solve = || {
        let mut solvers = [Z3::start(&z3).expect("z3 starts")];
        prove::prove(
            &ts,
            None,
            &rules,
            &BTreeSet::new(),
            &mut solvers,
            ProveOptions::default(),
        )
        .expect("z3 answers")
    };
    let proof = solve();
    assert_eq!(proof.proved(), rules.len(), "{proof:?}");
    assert!(proof.faults.is_empty(), "{:?}", proof.faults);
    for (plant, says) in [
        ("prove_compare", "disagrees with the native comparison"),
        (
            "prove_false_lemma",
            "the kept lemma `a planted false lemma` fails",
        ),
    ] {
        let proof = dropping(plant, solve);
        assert_eq!(proof.proved(), 0, "{plant}: {proof:?}");
        assert!(
            proof.faults.iter().any(|f| f.contains(says)),
            "{plant}: {:?}",
            proof.faults
        );
        assert!(proof.lemmas.is_empty(), "{plant}");
        assert!(
            proof
                .rules
                .iter()
                .all(|r| matches!(r.verdict, Verdict::Unproved(Unproved::Uncertified(_)))),
            "{plant}: {proof:?}"
        );
    }
}
