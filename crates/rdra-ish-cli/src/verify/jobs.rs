use crate::export::Modules;
use crate::property_cone::{ConeRun, PropertyCone};
use rdra_ish_core::lifecycle::{ActId, Caps, Cone, Target, TransitionSystem, VarSet};
use rdra_ish_core::{in_parallel, jobs};
use rdra_ish_emit::tla::{self, PrintMode, TlaBundle, TlaNames};
use std::collections::BTreeMap;

/// A printed module and its cfg.
pub(super) struct Spec {
    pub(super) module: String,
    pub(super) tla: String,
    pub(super) cfg: String,
}

/// One TLC run: a module, its cfg, and the checks it answers.
pub(super) struct Job {
    pub(super) module: String,
    pub(super) tla: String,
    pub(super) cfg: String,
    pub(super) checks: Vec<String>,
    pub(super) kind: JobKind,
}

/// What a run's counterexamples replay on.
pub(super) enum JobKind {
    Cone(Box<ConeSystem>),
    /// `properties()[i]` on its cone.
    PropertyCone(Box<PropertyJob>),
    /// `properties()[i]` on the whole system, after its cone left it undecided.
    Whole(usize),
}

/// A property on its cone; the `_Full` module beside it is the unreduced cone, run after a
/// counterexample on the reduced one, and the `_WF` module adds `WF_vars(Next)` for the
/// second run an `eventually` or `leads_to` may need. `names` are the unreduced cone's, which
/// both of those print.
pub(super) struct PropertyJob {
    pub(super) index: usize,
    pub(super) cone: PropertyCone,
    pub(super) names: TlaNames,
    pub(super) full: Option<Spec>,
    pub(super) wf: Option<Spec>,
}

impl Job {
    /// A guess at how long the run takes, to start the long ones first: a temporal property
    /// before a safety cone, since TLC also searches its behavior graph, then the system with
    /// more variables.
    pub(super) fn weight(&self) -> (bool, usize) {
        match &self.kind {
            JobKind::Cone(cone) => (false, cone.system.var_ids().count()),
            JobKind::PropertyCone(p) => (true, p.cone.reduced.var_ids().count()),
            JobKind::Whole(_) => (true, usize::MAX),
        }
    }
}

/// A cone as TLC checks it: the whole system's cone, its restricted system, and the names
/// its module prints.
pub(super) struct ConeSystem {
    pub(super) cone: Cone,
    /// The invariants and after-asserts whose cone this is.
    pub(super) targets: Vec<Target>,
    pub(super) system: TransitionSystem,
    pub(super) names: TlaNames,
    /// Per action of the cone before any reduction, by its id in the whole system: how many
    /// guards and params the cone leaves it. A cone drops a guard that reads outside it and
    /// a param whose entity it leaves out, so a larger cone may hold an action back where a
    /// smaller one lets it run (`covering`).
    pub(super) steps: BTreeMap<ActId, (usize, usize)>,
}

/// Each distinct cone of a selected invariant or after-assert, in target order, named after
/// the first check that reads it. Cones over the same vars share one run: an action kept for
/// an after-assert writes nothing in the cone, so it adds edges but no states.
pub(super) fn cone_jobs(
    ts: &TransitionSystem,
    names: &TlaNames,
    targets: &[Target],
    modules: &mut Modules,
    caps: Caps,
) -> Vec<Job> {
    let mut at: BTreeMap<VarSet, usize> = BTreeMap::new();
    let mut order: Vec<(Cone, String, Vec<Target>)> = Vec::new();
    for &target in targets {
        let first = match target {
            // BFS decides a `can_reach`, which TLC cannot check.
            Target::Entity(_) | Target::Reach(_) => continue,
            Target::Invariant(i) => names.invariant(i).to_string(),
            Target::AfterAssert(i) => names.after(i).to_string(),
        };
        let cone = ts.target_cone(target);
        match at.get(&cone.vars) {
            Some(&k) => {
                order[k].0.keep.extend(cone.keep);
                order[k].2.push(target);
            }
            None => {
                at.insert(cone.vars.clone(), order.len());
                order.push((cone, first, vec![target]));
            }
        }
    }
    // The names are claimed in target order; reducing and printing a cone reads nothing of
    // another, so the cones are prepared side by side.
    let named: Vec<(Cone, String, Vec<Target>)> = order
        .into_iter()
        .map(|(cone, first, shared)| (cone, modules.claim(&format!("Cone_{first}")), shared))
        .collect();
    in_parallel(named, jobs(), |(cone, module, shared)| {
        let seeds = shared.iter().map(|t| ts.target_seed(*t));
        let sub = ts.reduced_cone(&cone, seeds, caps);
        let bundle = tla::print(&sub, PrintMode::Safety, &module);
        let n = &bundle.names;
        let checks = (0..sub.invariants().len())
            .map(|i| n.invariant(i).to_string())
            .chain((0..sub.after_asserts().len()).map(|i| n.after(i).to_string()))
            .collect();
        Job {
            module: bundle.module_name,
            tla: bundle.tla,
            cfg: bundle.cfg,
            checks,
            kind: JobKind::Cone(Box::new(ConeSystem {
                steps: ts
                    .restrict_to(&cone.vars, &cone.keep)
                    .actions()
                    .iter()
                    .map(|a| (a.source, (a.guards.len(), a.params.len())))
                    .collect(),
                targets: shared,
                cone,
                system: sub,
                names: bundle.names,
            })),
        }
    })
}

/// One run per `always`, `eventually`, and `leads_to` on its cone, module `Cone_<property>`, with the `_WF`
/// module (the same cone with `WF_vars(Next)`) printed beside it for the second run. Every
/// `Cone_<property>` is claimed before any `_Full` or `_WF`, so each property's own module
/// keeps its name.
pub(super) fn property_jobs(
    ts: &TransitionSystem,
    names: &TlaNames,
    modules: &mut Modules,
    caps: Caps,
) -> Vec<Job> {
    // The forms `LtlKind::is_explored` names are decided on their cones by
    // `explored_properties`, with no run of their own.
    let tlc: Vec<usize> = (0..ts.properties().len())
        .filter(|&i| !ts.properties()[i].formula.kind().is_explored())
        .collect();
    let plain: Vec<String> = tlc
        .iter()
        .map(|&i| modules.claim(&format!("Cone_{}", names.property(i))))
        .collect();
    // Reducing a property's cone reads nothing of another's; the `_Full` and `_WF` names are
    // claimed afterwards, in property order.
    let cones = in_parallel(tlc.clone(), jobs(), |i| PropertyCone::under(ts, i, caps));
    tlc.into_iter()
        .zip(plain.into_iter().zip(cones))
        .map(|(i, (module, cone))| {
            let name = names.property(i);
            let plain = cone.bundle(ConeRun::Plain, &module);
            let spec = |b: TlaBundle| Spec {
                module: b.module_name,
                tla: b.tla,
                cfg: b.cfg,
            };
            let mut names = plain.names;
            let full = cone.is_reduced().then(|| {
                let b = cone.bundle(ConeRun::Full, &modules.claim(&format!("{module}_Full")));
                names = b.names.clone();
                spec(b)
            });
            let wf = cone.kind.is_liveness().then(|| {
                let module = modules.claim(&format!("{module}_WF"));
                spec(cone.bundle(ConeRun::WfNext, &module))
            });
            Job {
                module: plain.module_name,
                tla: plain.tla,
                cfg: plain.cfg,
                checks: vec![name.to_string()],
                kind: JobKind::PropertyCone(Box::new(PropertyJob {
                    index: i,
                    cone,
                    names,
                    full,
                    wf,
                })),
            }
        })
        .collect()
}

/// `properties()[i]` on the whole system, module `module` (`<whole>_<property>`): the same
/// module text as the whole-model module, fairness included, and a cfg that names the one
/// property.
pub(super) fn whole_job(ts: &TransitionSystem, names: &TlaNames, module: &str, i: usize) -> Job {
    let name = names.property(i);
    let bundle = tla::print(ts, PrintMode::Property(i), module);
    Job {
        module: bundle.module_name,
        tla: bundle.tla,
        cfg: bundle.cfg,
        checks: vec![name.to_string()],
        kind: JobKind::Whole(i),
    }
}
