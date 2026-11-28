//! The certificates every build makes of its cones (`cone_check.rs`, residual risk 1 of
//! docs/theory/README.md) and of its actions' slot syntax (`build/syntax.rs`, claims A8,
//! A18 and T1 of docs/theory/claims.md): they pass on the samples, the domain samples and
//! the corpus, and each clause fails on a fault planted to break it.

use std::collections::BTreeSet;

use super::reduce::walked_models;
use super::*;
use crate::lifecycle::cone::{Cone, VarSet};
use crate::lifecycle::cone_check::{ConeClause, Exactness};
use crate::lifecycle::explore::symmetry::witnesses::dropping;
use crate::lifecycle::types::{Effect, Pred};
use crate::lifecycle::Target;

/// Every cone of `ts` a run reads, with its seed and what its use needs: the target and
/// property cones, and both exact cones of each check.
fn cones(ts: &TransitionSystem) -> Vec<(VarSet, Cone, Exactness)> {
    let mut out = Vec::new();
    for t in ts.targets().into_iter().chain(ts.reach_targets()) {
        let need = match t {
            Target::Entity(_) => Exactness {
                marks: true,
                slots: false,
            },
            Target::Reach(_) => Exactness {
                marks: true,
                slots: true,
            },
            Target::Invariant(_) | Target::AfterAssert(_) => Exactness::NONE,
        };
        out.push((ts.target_seed(t).0, ts.target_cone(t), need));
        if need.marks {
            continue;
        }
        for slots in [false, true] {
            let exact = Exactness { marks: true, slots };
            out.push((ts.target_seed(t).0, ts.exact_target_cone(t, slots), exact));
        }
    }
    for i in 0..ts.properties().len() {
        out.push((ts.property_seed(i).0, ts.property_cone(i), Exactness::NONE));
    }
    out
}

fn systems() -> Vec<(String, TransitionSystem)> {
    let mut out = Vec::new();
    let rest = [
        "samples/oidc-auth",
        "samples/api-contract",
        "samples/incremental-order",
    ];
    let models = (walked_models().into_iter())
        .chain(rest.iter().map(|s| (s.to_string(), super::cone::load(s))));
    for (name, model) in models {
        for bounds in [Bounds::STATES, Bounds::VERIFY] {
            out.push((
                name.clone(),
                TransitionSystem::build(&model, bounds, &Scope::ALL).ts,
            ));
        }
    }
    out
}

/// Every cone the samples, the domain samples and the corpus read passes its check, and so
/// does its restriction; every built system is in the slot syntax, and no action has two
/// params that can bind one slot and write one var. The cone builders call the check
/// themselves, and a debug build stops at a fault, so this also shows none fell back.
#[test]
fn every_cone_and_build_passes_its_certificate() {
    let (mut checked, mut exact, mut separated) = (0, 0, 0);
    for (name, ts) in systems() {
        let faults = ts.slot_syntax_faults();
        assert!(faults.is_empty(), "{name}: {}", faults[0]);
        for a in &ts.actions {
            assert_eq!(ts.shared_slot_writes(a), Vec::new(), "{name}: {}", a.name);
            let p = &a.params;
            separated += (0..p.len())
                .flat_map(|i| (i + 1..p.len()).map(move |j| (i, j)))
                .filter(|&(i, j)| p[i].entity == p[j].entity)
                .count();
        }
        for (seed, cone, need) in cones(&ts) {
            let faults = ts.cone_faults(&seed, &cone, need);
            assert!(faults.is_empty(), "{name}: {}", faults[0]);
            let (out, map) = ts.restrict_to_mapped(&cone.vars, &cone.keep, true);
            let faults = ts.restriction_faults(&cone.vars, &cone.keep, &out, &map);
            assert!(faults.is_empty(), "{name}: {}", faults[0]);
            checked += 1;
            exact += usize::from(need.marks);
        }
    }
    assert!(
        checked > 1000 && exact > 100,
        "{checked} cones, {exact} exact"
    );
    assert!(separated > 0, "some action has two params of one entity");
}

/// Each clause of the cone check fails on a cone planted to break it: a var taken out of a
/// closed cone, or an inexact cone held to exactness. A planted cone falls back to the
/// whole system's vars with the same kept actions.
#[test]
fn a_planted_cone_fails_the_clause_it_breaks() {
    let mut fired: BTreeSet<ConeClause> = BTreeSet::new();
    let mut fell_back = 0;
    for (_, ts) in systems() {
        for (seed, cone, need) in cones(&ts) {
            let mut plant = |want: ConeClause, vars: VarSet, need: Exactness| {
                let planted = Cone {
                    vars,
                    keep: cone.keep.clone(),
                };
                let got: BTreeSet<ConeClause> = (ts.cone_faults(&seed, &planted, need).iter())
                    .map(|f| f.clause)
                    .collect();
                assert!(got.contains(&want), "{want:?} fails: {got:?}");
                if fired.insert(want) {
                    let back = dropping("cone_choice", || ts.certified(&seed, planted, need));
                    assert_eq!(
                        back.vars,
                        ts.var_ids().collect(),
                        "falls back to the whole system"
                    );
                    assert_eq!(back.keep, cone.keep);
                    fell_back += 1;
                }
            };
            let without = |v| {
                cone.vars
                    .iter()
                    .copied()
                    .filter(|w| *w != v)
                    .collect::<VarSet>()
            };
            if let Some(&v) = seed.first() {
                plant(ConeClause::Seed, without(v), need);
            }
            // A var a kept action reads that it does not write, and no var's existence.
            let plain = (cone.keep.iter().map(|k| &ts.actions[k.index()])).find_map(|a| {
                let w = ts.action_writes(a);
                (ts.plain_reads(a).into_iter()).find(|v| {
                    !w.contains(v)
                        && !seed.contains(v)
                        && cone.vars.iter().all(|u| !ts.existences(*u).contains(v))
                })
            });
            if let Some(v) = plain {
                plant(ConeClause::Reads, without(v), need);
            }
            // A mark a kept action reads, which the cone holds.
            let mark = (cone.keep.iter().map(|k| &ts.actions[k.index()]))
                .flat_map(|a| ts.mark_reads(a))
                .map(|(m, _)| m)
                .find(|m| cone.vars.contains(m) && !seed.contains(m));
            if let Some(m) = mark {
                plant(ConeClause::Mark, without(m), need);
            }
            // What a copy into a var of the cone reads, other than an existence.
            let copy = (ts.actions.iter())
                .flat_map(|a| ts.copy_reads(a))
                .find_map(|(t, r)| {
                    let source = r.into_iter().find(|v| *v != t && !seed.contains(v));
                    source.filter(|v| cone.vars.contains(&t) && cone.vars.contains(v))
                });
            if let Some(v) = copy {
                plant(ConeClause::Copy, without(v), need);
            }
            // An existence a column of the cone needs.
            let existence = (cone.vars.iter()).find_map(|v| ts.existences(*v).first().copied());
            if let Some(e) = existence.filter(|e| !seed.contains(e)) {
                plant(ConeClause::Existence, without(e), need);
            }
            if !need.marks && ts.cone_leaves_marks(&cone) {
                let exact = Exactness {
                    marks: true,
                    slots: false,
                };
                plant(ConeClause::LeftOutMark, cone.vars.clone(), exact);
            }
            if !need.slots && ts.cone_frees_slots(&cone) && !ts.cone_leaves_marks(&cone) {
                let exact = Exactness {
                    marks: true,
                    slots: true,
                };
                plant(ConeClause::FreedSlot, cone.vars.clone(), exact);
            }
        }
    }
    let all = [
        ConeClause::Seed,
        ConeClause::Reads,
        ConeClause::Mark,
        ConeClause::Copy,
        ConeClause::Existence,
        ConeClause::LeftOutMark,
        ConeClause::FreedSlot,
    ];
    assert_eq!(fired, all.into_iter().collect(), "every clause fails once");
    assert_eq!(fell_back, all.len());
}

/// Each clause of the restriction check fails on a restriction planted to break it: an
/// action, an effect or a guard taken out of a cone's restricted system, a param marked
/// dropped, a var added. `restrict_to` stops at such a fault.
#[test]
fn a_planted_restriction_fails_the_clause_it_breaks() {
    let mut fired: BTreeSet<ConeClause> = BTreeSet::new();
    for (_, ts) in systems().into_iter().take(40) {
        for (_, cone, _) in cones(&ts) {
            let (vars, keep) = (&cone.vars, &cone.keep);
            let clauses = |out: &TransitionSystem, map: &crate::lifecycle::restrict::Remap| {
                let faults = ts.restriction_faults(vars, keep, out, map);
                faults.iter().map(|f| f.clause).collect::<BTreeSet<_>>()
            };
            let mut plant = |want: ConeClause,
                             change: &dyn Fn(
                &mut TransitionSystem,
                &mut crate::lifecycle::restrict::Remap,
            ) -> bool| {
                let (mut out, mut map) = ts.restrict_to_mapped(vars, keep, true);
                if change(&mut out, &mut map) {
                    let got = clauses(&out, &map);
                    assert!(got.contains(&want), "{want:?} fails: {got:?}");
                    fired.insert(want);
                }
            };
            plant(ConeClause::Writer, &|_, map| {
                let at = map.act.iter().position(Option::is_some);
                at.is_some_and(|i| map.act[i].take().is_some())
            });
            plant(ConeClause::Effect, &|out, _| {
                let a = out.actions.iter_mut().find(|a| !a.effects.is_empty());
                a.is_some_and(|a| a.effects.pop().is_some())
            });
            plant(ConeClause::Guard, &|out, _| {
                let a = out.actions.iter_mut().find(|a| !a.guards.is_empty());
                a.is_some_and(|a| a.guards.pop().is_some())
            });
            plant(ConeClause::Param, &|out, map| {
                let Some((i, j)) = (map.act.iter().enumerate()).find_map(|(i, j)| {
                    j.filter(|j| !out.actions[j.index()].params.is_empty())
                        .map(|j| (i, j))
                }) else {
                    return false;
                };
                let n = ts.actions[i].params.len();
                let mut index: Vec<Option<u8>> = match &map.params[i] {
                    Some(ix) => ix.clone(),
                    None => (0..n).map(|p| Some(u8::try_from(p).unwrap())).collect(),
                };
                let Some(p) = index.iter().position(Option::is_some) else {
                    return false;
                };
                index[p] = None;
                map.params[i] = Some(index);
                let _ = j;
                true
            });
            plant(ConeClause::Stub, &|out, _| {
                let v = out.vars.first().cloned();
                v.is_some_and(|v| {
                    out.vars.push(v);
                    true
                })
            });
        }
    }
    let all = [
        ConeClause::Writer,
        ConeClause::Effect,
        ConeClause::Guard,
        ConeClause::Param,
        ConeClause::Stub,
    ];
    assert_eq!(fired, all.into_iter().collect(), "every clause fails once");
}

/// `restrict_to` stops the run at a restriction fault: here a cone whose closure is broken
/// on purpose, so the restriction drops a guard of a kept action that Definition 2 does not
/// list.
#[test]
#[should_panic(expected = "tool error: restrict_to")]
fn restrict_to_stops_at_a_restriction_fault() {
    for (_, ts) in systems() {
        for (_, cone, _) in cones(&ts) {
            for k in &cone.keep {
                let a = &ts.actions[k.index()];
                let w = ts.action_writes(a);
                let read = (a.guards.iter())
                    .filter(|g| g.copy_target().is_none())
                    .find_map(|g| {
                        let mut r = VarSet::new();
                        ts.pred_reads(&g.pred, &mut r);
                        r.into_iter().find(|v| {
                            !w.contains(v)
                                && cone.vars.contains(v)
                                && !matches!(
                                    ts.var(*v).kind,
                                    crate::lifecycle::types::VarKind::Exists(_)
                                )
                        })
                    });
                if let Some(v) = read {
                    let vars: VarSet = cone.vars.iter().copied().filter(|u| *u != v).collect();
                    ts.restrict_to(&vars, &cone.keep);
                }
            }
        }
    }
}

/// Each field of `WF` and `Built` the build checks fails on an action planted to break it,
/// and a build whose builder plants one (`slot_syntax`) fails with `BuildFault`, an error.
#[test]
fn a_planted_action_fails_the_syntax_field_it_breaks() {
    use crate::lifecycle::build::syntax::SyntaxClause;
    use crate::lifecycle::types::{At, ParamRole, Term};
    let mut fired: BTreeSet<SyntaxClause> = BTreeSet::new();
    for (_, ts) in systems().into_iter().take(60) {
        for i in 0..ts.actions.len() {
            let mut plant = |want: SyntaxClause, change: &dyn Fn(&mut TransitionSystem) -> bool| {
                let mut t = ts.clone();
                if change(&mut t) {
                    let got: BTreeSet<SyntaxClause> =
                        (t.slot_syntax_faults().iter()).map(|f| f.clause).collect();
                    assert!(got.contains(&want), "{want:?} fails: {got:?}");
                    fired.insert(want);
                }
            };
            // A guard's slot term read at a param of another entity.
            plant(SyntaxClause::GuardWf, &|t| {
                let a = &t.actions[i];
                let Some(other) =
                    (0..a.params.len()).find(|&q| a.params[q].entity != a.params[0].entity)
                else {
                    return false;
                };
                let other = u8::try_from(other).unwrap();
                let found = (t.actions[i].guards.iter_mut()).find_map(|g| match &mut g.pred {
                    Pred::Cmp(
                        Term::Slot {
                            at: at @ At::Param(0),
                            ..
                        },
                        ..,
                    ) => Some(at),
                    _ => None,
                });
                found.map(|at| *at = At::Param(other)).is_some()
            });
            // An effect on a param that does not exist.
            plant(SyntaxClause::EffectWf, &|t| {
                let n = u8::try_from(t.actions[i].params.len()).unwrap();
                let e = t.actions[i].effects.iter_mut().find_map(|e| match e {
                    Effect::Create { param } | Effect::Delete { param } => Some(param),
                    _ => None,
                });
                e.map(|p| *p = n).is_some()
            });
            // A `New` param's `Create` removed.
            plant(SyntaxClause::NewCreate, &|t| {
                let a = &mut t.actions[i];
                let at = a
                    .effects
                    .iter()
                    .position(|e| matches!(e, Effect::Create { .. }));
                at.map(|k| a.effects.remove(k)).is_some()
            });
            // An `Earlier` param's run-mark conjunct removed.
            plant(SyntaxClause::EarlierMark, &|t| {
                let a = &mut t.actions[i];
                let earlier = a
                    .params
                    .iter()
                    .position(|p| matches!(p.role, ParamRole::Earlier { .. }));
                let Some(p) = earlier else { return false };
                let p = u8::try_from(p).unwrap();
                let before = a.guards.len();
                a.guards.retain(|g| !matches!(&g.pred, Pred::Cmp(Term::Slot { at: At::Param(q), .. }, ..) if *q == p));
                a.guards.len() < before
            });
        }
    }
    let all = [
        SyntaxClause::GuardWf,
        SyntaxClause::EffectWf,
        SyntaxClause::NewCreate,
        SyntaxClause::EarlierMark,
    ];
    assert_eq!(fired, all.into_iter().collect(), "every field fails once");
    let (name, model) = &walked_models()[0];
    let built = dropping("slot_syntax", || {
        TransitionSystem::build(model, Bounds::VERIFY, &Scope::ALL)
    });
    let fault = (built.diags.iter()).find(|d| matches!(d.diag, StateDiag::BuildFault { .. }));
    assert!(fault.is_some(), "{name}: the build fails");
    assert!(
        crate::lifecycle::has_errors(&built.diags),
        "{name}: as an error"
    );
}

/// An action two of whose params can bind one slot and write one var there fails the build
/// with `ConflictingEffects` (Proposition T1's aliasing assumption): two present rows of one
/// entity each given a write of one column.
#[test]
fn two_params_that_can_share_a_slot_and_write_one_var_fail_the_build() {
    let mut refused = 0;
    for (name, model) in walked_models() {
        let built = dropping("shared_slot", || {
            TransitionSystem::build(&model, Bounds::VERIFY, &Scope::ALL)
        });
        let planted = built
            .ts
            .actions
            .iter()
            .any(|a| !built.ts.shared_slot_writes(a).is_empty());
        if !planted {
            continue;
        }
        assert!(
            built
                .diags
                .iter()
                .any(|d| matches!(d.diag, StateDiag::ConflictingEffects { .. })),
            "{name}: refused"
        );
        refused += 1;
    }
    assert!(
        refused > 0,
        "some model has two present params of one entity"
    );
}
