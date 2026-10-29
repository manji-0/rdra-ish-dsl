//! What a run covers. `--buc` restricts the use cases when the system is built; `--entity`
//! chooses the targets. Each selected target is explored on its own cone, as in a whole run,
//! so the cones are the only restriction after the build.

use std::fmt;

use super::cone::Target;
use super::diag::{Diag, Severity};
use super::targets::Undecided;
use super::TransitionSystem;
use crate::model::{EntityKey, SemanticModel, UseCaseKey};

/// What a run covers; empty fields cover everything. `parse` checks the ids at the CLI
/// boundary, so inside the ids name real BUCs and entities.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scope {
    /// BUC ids: the system keeps only the use cases reachable from them.
    pub bucs: Vec<String>,
    /// The run answers these entities' views and the invariants and asserts that read them.
    pub entities: Vec<EntityKey>,
}

/// An id `--buc` or `--entity` names that the model does not declare.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownId {
    pub kind: &'static str,
    pub id: String,
}

impl fmt::Display for UnknownId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown {} `{}`", self.kind, self.id)
    }
}

impl std::error::Error for UnknownId {}

impl Scope {
    pub const ALL: Scope = Scope {
        bucs: Vec::new(),
        entities: Vec::new(),
    };

    pub fn parse(
        model: &SemanticModel,
        bucs: &[String],
        entities: &[String],
    ) -> Result<Scope, UnknownId> {
        let unknown = |kind, id: &String| UnknownId {
            kind,
            id: id.clone(),
        };
        for id in bucs {
            if !model.bucs.values().any(|b| b.id == *id) {
                return Err(unknown("buc", id));
            }
        }
        let mut keys = Vec::new();
        for id in entities {
            let (key, _) = model
                .entities
                .iter()
                .find(|(_, e)| e.id == *id)
                .ok_or_else(|| unknown("entity", id))?;
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        Ok(Scope {
            bucs: bucs.to_vec(),
            entities: keys,
        })
    }

    /// `--buc` alone, for callers whose ids an emitter view already carries.
    pub fn from_bucs(bucs: &[String]) -> Self {
        Scope {
            bucs: bucs.to_vec(),
            entities: Vec::new(),
        }
    }

    pub fn is_whole(&self) -> bool {
        self.bucs.is_empty() && self.entities.is_empty()
    }

    /// The `scope:` line: each flag with what it leaves out. `None` for a whole run.
    pub fn line(&self, model: &SemanticModel, selection: &Selection) -> Option<String> {
        if self.is_whole() {
            return None;
        }
        let mut parts = Vec::new();
        if !self.bucs.is_empty() {
            let n = selection.inactive.len();
            parts.push(format!(
                "--buc {} ({})",
                self.bucs.join(" --buc "),
                plural(
                    n,
                    "use case outside it does not run",
                    "use cases outside it do not run"
                )
            ));
        }
        if !self.entities.is_empty() {
            let ids: Vec<&str> = self
                .entities
                .iter()
                .map(|k| model.entities[*k].id.as_str())
                .collect();
            parts.push(format!("--entity {}", ids.join(" --entity ")));
        }
        Some(format!("scope: {}", parts.join(", ")))
    }
}

/// The targets a run answers, and what it leaves out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub targets: Vec<Target>,
    /// Targets outside `--entity`.
    pub unchecked: Vec<Target>,
    /// Use cases with a write that `--buc` leaves out.
    pub inactive: Vec<UseCaseKey>,
    /// Build errors that involve only entities no selected cone reads. They do not block the
    /// run; a run without `--entity` reports them as errors.
    pub deferred: Vec<Diag>,
}

impl TransitionSystem {
    /// The entities a target's verdict names: the entity itself, an invariant's binders, or
    /// an assert's binders and every entity its anchor binds.
    pub(crate) fn target_entities(&self, target: Target) -> Vec<EntityKey> {
        match target {
            Target::Entity(e) => vec![self.entity(e).key],
            Target::Invariant(i) => self.invariants[i]
                .binders
                .iter()
                .map(|e| self.entity(*e).key)
                .collect(),
            Target::AfterAssert(i) => {
                let mut entities = Vec::new();
                for step in &self.after[i].steps {
                    let params = self.actions[step.act.index()].params.iter();
                    for e in step.binders.iter().copied().chain(params.map(|p| p.entity)) {
                        let key = self.entity(e).key;
                        if !entities.contains(&key) {
                            entities.push(key);
                        }
                    }
                }
                entities
            }
        }
    }

    /// The selection of `scope` over the built system. Each build diagnostic comes with the
    /// entities it involves: those it names, and those of the action it altered, if any. An
    /// error is deferred when they are all outside every selected cone.
    pub(crate) fn select(
        &self,
        scope: &Scope,
        diags: Vec<(Diag, Vec<EntityKey>)>,
        inactive: Vec<UseCaseKey>,
    ) -> (Vec<Diag>, Selection) {
        let (targets, unchecked): (Vec<Target>, Vec<Target>) =
            self.targets().into_iter().partition(|t| {
                scope.entities.is_empty()
                    || self
                        .target_entities(*t)
                        .iter()
                        .any(|k| scope.entities.contains(k))
            });
        let mut read: Vec<EntityKey> = Vec::new();
        if !unchecked.is_empty() {
            for &t in &targets {
                let cone = self.target_cone(t);
                for d in &self.entities {
                    if cone.vars.contains(&d.exists) && !read.contains(&d.key) {
                        read.push(d.key);
                    }
                }
            }
        }
        let mut kept = Vec::new();
        let mut deferred = Vec::new();
        for (d, involves) in diags {
            let outside = !unchecked.is_empty()
                && d.diag.severity() == Severity::Error
                && !involves.is_empty()
                && involves.iter().all(|k| !read.contains(k));
            if outside {
                deferred.push(d);
            } else {
                kept.push(d);
            }
        }
        (
            kept,
            Selection {
                targets,
                unchecked,
                inactive,
                deferred,
            },
        )
    }
}

/// The one `not checked:` block: what the selected scope leaves out, the build errors it
/// defers, and each finding a scope or a cap left undecided. Prints nothing for a whole run
/// that decided everything.
pub struct NotChecked<'a> {
    pub ts: &'a TransitionSystem,
    pub model: &'a SemanticModel,
    pub scope: &'a Scope,
    pub selection: &'a Selection,
    pub undecided: &'a [Undecided],
    /// Temporal properties a scoped `verify` does not run.
    pub temporal: usize,
}

impl NotChecked<'_> {
    /// Nothing to list: no `--entity`, no temporal property a scope skips, nothing undecided.
    pub fn is_empty(&self) -> bool {
        self.scope.entities.is_empty()
            && (self.scope.is_whole() || self.temporal == 0)
            && self.undecided.is_empty()
    }

    fn names(&self, targets: impl Iterator<Item = Target>) -> Vec<String> {
        targets
            .map(|t| match t {
                Target::Entity(e) => self.ts.entity(e).id.clone(),
                Target::Invariant(i) => self.ts.invariants()[i].name.clone(),
                Target::AfterAssert(i) => {
                    let anchor = self.ts.after_asserts()[i].steps[0].act;
                    format!("after({})", self.ts.actions()[anchor.index()].name)
                }
            })
            .collect()
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `2 invariants (A, B)`, naming at most three.
fn counted(names: &[String], one: &str, many: &str) -> String {
    let head = plural(names.len(), one, many);
    match names.len() {
        0 => head,
        1..=3 => format!("{head} ({})", names.join(", ")),
        _ => format!("{head} ({}, ...)", names[..3].join(", ")),
    }
}

impl fmt::Display for NotChecked<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return Ok(());
        }
        let mut parts = Vec::new();
        if !self.scope.entities.is_empty() {
            let of = |pick: fn(&Target) -> bool| {
                self.names(self.selection.unchecked.iter().copied().filter(&pick))
            };
            parts.push(format!(
                "{}, {}, {}",
                counted(
                    &of(|t| matches!(t, Target::Entity(_))),
                    "entity view",
                    "entity views"
                ),
                counted(
                    &of(|t| matches!(t, Target::Invariant(_))),
                    "invariant",
                    "invariants"
                ),
                counted(
                    &of(|t| matches!(t, Target::AfterAssert(_))),
                    "after-assert",
                    "after-asserts"
                ),
            ));
        }
        if !self.scope.is_whole() && self.temporal > 0 {
            parts.push(plural(
                self.temporal,
                "temporal property",
                "temporal properties",
            ));
        }
        let mut head = parts.join(", ");
        let mut tail = Vec::new();
        if !self.scope.entities.is_empty() {
            tail.push(format!(
                "{} deferred",
                plural(self.selection.deferred.len(), "build error", "build errors")
            ));
        }
        if !self.undecided.is_empty() {
            tail.push(format!(
                "{} not decided",
                plural(self.undecided.len(), "finding", "findings")
            ));
        }
        if !tail.is_empty() {
            if !head.is_empty() {
                head.push_str("; ");
            }
            head.push_str(&tail.join("; "));
        }
        write!(f, "not checked: {head}")?;
        for d in &self.selection.deferred {
            let ids: Vec<&str> = d
                .entities
                .iter()
                .map(|k| self.model.entities[*k].id.as_str())
                .collect();
            write!(
                f,
                "\n  deferred {}[{}]: {}: {}",
                d.diag.severity().as_label(),
                d.diag.name(&ids),
                ids.join(", "),
                d.diag
            )?;
        }
        for u in self.undecided {
            write!(f, "\n  {u}")?;
        }
        Ok(())
    }
}
