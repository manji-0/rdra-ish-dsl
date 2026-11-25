//! Per-entity views of the reached states, in the shape `states` prints: one pattern per
//! distinct column assignment of a present slot, with initial, terminal, and provenance.

use std::collections::{BTreeMap, BTreeSet};

use super::diag::{Diag, StateDiag};
use super::explore::ComponentRun;
use super::expr::StateSlice;
use super::trace::Trace;
use super::types::{ActionOrigin, Birth, Domain, EntId, Value, VarKind};
use super::TransitionSystem;
use crate::model::SemanticModel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxisKind {
    Enum(Vec<String>),
    /// null, then the declared values: an `Enum` column marked `@null`.
    NullableEnum(Vec<String>),
    Bool,
    /// null, false, true: a `Bool` column marked `@null`.
    NullableBool,
    Nullable,
    Int {
        max: u8,
    },
    NullableInt {
        max: u8,
    },
    Proposition {
        axis_key: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateAxis {
    pub column: String,
    pub kind: AxisKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AbstractValue {
    Enum(String),
    Bool(bool),
    Present,
    Null,
    Int(i64),
}

impl AbstractValue {
    pub(crate) fn of(domain: &Domain, value: Value) -> Self {
        match domain {
            Domain::Enum(values) => AbstractValue::Enum(values[usize::from(value.0)].clone()),
            Domain::Bool => AbstractValue::Bool(value == Value::TRUE),
            Domain::NullableEnum(_) | Domain::NullableBool | Domain::Nullable
                if value == Value::NULL =>
            {
                AbstractValue::Null
            }
            Domain::NullableEnum(_) => {
                AbstractValue::Enum(domain.name(value).expect("a declared value").to_string())
            }
            Domain::NullableBool => AbstractValue::Bool(domain.truth(value) == Some(true)),
            Domain::Nullable => AbstractValue::Present,
            Domain::Int | Domain::NullableInt => match domain.number(value) {
                Some(n) => AbstractValue::Int(n),
                None => AbstractValue::Null,
            },
            Domain::Existence | Domain::Instance { .. } => AbstractValue::Int(i64::from(value.0)),
        }
    }

    /// Display form: `paid`, `true`, `3`, `present`, `null`.
    pub fn display(&self) -> String {
        match self {
            AbstractValue::Present => "present".to_string(),
            AbstractValue::Null => "null".to_string(),
            AbstractValue::Enum(s) => s.clone(),
            AbstractValue::Bool(b) => b.to_string(),
            AbstractValue::Int(n) => n.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StatePattern {
    pub values: BTreeMap<String, AbstractValue>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Provenance {
    /// (BUC id, use case id) of the actions on some path to the pattern.
    pub via: BTreeSet<(Option<String>, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReachablePattern {
    pub pattern: StatePattern,
    /// A create action produces it, or it is a pre-existing entity's start.
    pub is_initial: bool,
    /// No enabled action moves a slot off it, or some enabled action deletes it.
    pub is_terminal: bool,
    pub provenance: Provenance,
    /// A shortest path to the first state where some instance holds the pattern; `None` for
    /// the trivial pattern of an entity without state columns, or when a cap stopped the run
    /// before it admitted that state.
    pub witness: Option<Trace>,
}

#[derive(Debug, Clone)]
pub struct EntityStateResult {
    pub entity_id: String,
    pub entity_label: String,
    pub axes: Vec<StateAxis>,
    /// Discovery order.
    pub patterns: Vec<ReachablePattern>,
    pub truncated: bool,
    pub no_creation_path: bool,
    pub diagnostics: Vec<StateDiag>,
}

impl EntityStateResult {
    /// Product of the axes' domain sizes.
    pub fn bound(&self) -> usize {
        self.axes
            .iter()
            .map(|a| match &a.kind {
                AxisKind::Enum(v) => v.len(),
                AxisKind::NullableEnum(v) => v.len() + 1,
                AxisKind::Bool | AxisKind::Nullable | AxisKind::Proposition { .. } => 2,
                AxisKind::NullableBool => 3,
                AxisKind::Int { max } => usize::from(*max) + 1,
                AxisKind::NullableInt { max } => usize::from(*max) + 2,
            })
            .fold(1usize, usize::saturating_mul)
    }
}

pub(crate) fn pattern_bound(ts: &TransitionSystem, e: EntId) -> usize {
    ts.entity(e)
        .columns
        .iter()
        .map(|v| ts.var(*v).domain.size(ts))
        .fold(1usize, usize::saturating_mul)
}

fn pattern(ts: &TransitionSystem, e: EntId, key: &[u8]) -> StatePattern {
    let values = ts
        .entity(e)
        .columns
        .iter()
        .zip(key)
        .map(|(v, raw)| {
            (
                ts.var_name(*v),
                AbstractValue::of(&ts.var(*v).domain, Value(*raw)),
            )
        })
        .collect();
    StatePattern { values }
}

/// `col=val, ..` of one slot (columns in name order), or `Entity(col=val, ..)` when qualified.
pub(crate) fn describe_slot(
    ts: &TransitionSystem,
    s: &StateSlice,
    e: EntId,
    slot: usize,
    qualified: bool,
) -> String {
    let p = pattern(ts, e, &ts.pattern_key(s, e, slot));
    let body = p
        .values
        .iter()
        .map(|(c, v)| format!("{c}={}", v.display()))
        .collect::<Vec<_>>()
        .join(", ");
    let id = &ts.entity(e).id;
    match (qualified, ts.slots(e) > 1) {
        (false, _) => body,
        (true, false) => format!("{id}({body})"),
        (true, true) => format!("{id}[{}]({body})", slot + 1),
    }
}

fn axis(ts: &TransitionSystem, var: super::types::VarId) -> StateAxis {
    let decl = ts.var(var);
    let max = ts.bounds.int_max;
    let kind = match (&decl.kind, &decl.domain) {
        (VarKind::Prop { key, .. }, _) => AxisKind::Proposition {
            axis_key: key.clone(),
        },
        (_, Domain::Enum(values)) => AxisKind::Enum(values.clone()),
        (_, Domain::NullableEnum(values)) => AxisKind::NullableEnum(values.clone()),
        (_, Domain::Bool) => AxisKind::Bool,
        (_, Domain::NullableBool) => AxisKind::NullableBool,
        (_, Domain::Int) => AxisKind::Int { max },
        (_, Domain::NullableInt) => AxisKind::NullableInt { max },
        _ => AxisKind::Nullable,
    };
    StateAxis {
        column: ts.var_name(var),
        kind,
    }
}

/// Per pattern of `e`'s view, the first state in discovery order where a present slot holds
/// it.
fn first_states(run: &ComponentRun, e: EntId) -> Vec<Option<u32>> {
    let ts = &run.system;
    crate::shadow_assert!(
        run.gathered_views(),
        "a view is read off a run that gathered it"
    );
    let view = &run.views[e.index()];
    let mut first = vec![None; view.patterns.len()];
    for (at, s) in run.states.iter().enumerate() {
        for slot in 0..ts.slots(e) {
            if !s.is_present(ts, e, slot) {
                continue;
            }
            if let Some(id) = view.id(&ts.pattern_key(&s, e, slot)) {
                first[id].get_or_insert(at as u32);
            }
        }
    }
    first
}

fn is_cap(d: &StateDiag) -> bool {
    matches!(
        d,
        StateDiag::PatternCapReached { .. }
            | StateDiag::StateCapReached { .. }
            | StateDiag::MemoryCapReached { .. }
            | StateDiag::TimeBudgetReached { .. }
    )
}

/// The view of `e` in `run`, the entity's cone: a cap anywhere in it makes the view
/// incomplete, and the constraint verdicts (`verdicts`) come from their own cones.
pub(crate) fn entity_view(
    model: &SemanticModel,
    run: &ComponentRun,
    e: EntId,
    build_diags: &[Diag],
    verdicts: Vec<StateDiag>,
) -> EntityStateResult {
    let ts = &run.system;
    let decl = ts.entity(e);
    let axes: Vec<StateAxis> = decl.columns.iter().map(|v| axis(ts, *v)).collect();
    crate::shadow_assert!(
        run.gathered_views(),
        "a view is read off a run that gathered it"
    );
    let view = &run.views[e.index()];
    let mut diagnostics: Vec<StateDiag> = build_diags
        .iter()
        .filter(|d| d.entities.contains(&decl.key))
        .map(|d| d.diag.clone())
        .collect();
    if axes.is_empty() {
        // An entity with no state columns has one trivial pattern, as `states` reports it,
        // and still the verdicts of the constraints it binds.
        diagnostics.retain(|d| *d != StateDiag::NoCreationPath);
        diagnostics.extend(verdicts);
        return EntityStateResult {
            entity_id: decl.id.clone(),
            entity_label: decl.label.clone(),
            axes,
            patterns: vec![ReachablePattern {
                pattern: StatePattern {
                    values: BTreeMap::new(),
                },
                is_initial: true,
                is_terminal: true,
                provenance: Provenance::default(),
                witness: None,
            }],
            truncated: false,
            no_creation_path: false,
            diagnostics,
        };
    }
    let via = view.via();
    let first = first_states(run, e);
    let patterns: Vec<ReachablePattern> = (0..view.patterns.len())
        .map(|id| ReachablePattern {
            pattern: pattern(ts, e, &view.patterns[id]),
            is_initial: view.initial[id],
            is_terminal: view.deletable[id] || !view.changes[id],
            provenance: Provenance {
                via: via[id]
                    .iter()
                    .filter_map(|act| match &ts.actions()[act.index()].origin {
                        ActionOrigin::UseCase { uc, buc, .. } => Some((
                            buc.map(|b| model.bucs[b].id.clone()),
                            model.use_cases[*uc].id.clone(),
                        )),
                        ActionOrigin::Clock => None,
                    })
                    .collect(),
            },
            witness: first[id].map(|at| run.trace_to(at)),
        })
        .collect();
    let caps = run.diags.iter().filter(|d| is_cap(&d.diag));
    for d in caps {
        if !diagnostics.contains(&d.diag) {
            diagnostics.push(d.diag.clone());
        }
    }
    // A state of a state model is witnessed or `NoWitness`; only other values warn.
    let state_model = ts.state_model_values(e);
    for &var in &decl.columns {
        let domain = &ts.var(var).domain;
        let (
            VarKind::Column {
                variation: false, ..
            },
            Some(values),
        ) = (&ts.var(var).kind, domain.names())
        else {
            continue;
        };
        let column = ts.var_name(var);
        for (i, variant) in values.iter().enumerate() {
            if state_model.contains(&(var, domain.of_variant(i))) {
                continue;
            }
            let held = AbstractValue::Enum(variant.clone());
            if !patterns
                .iter()
                .any(|p| p.pattern.values.get(&column) == Some(&held))
            {
                diagnostics.push(StateDiag::UnreachableEnumVariant {
                    column: column.clone(),
                    variant: variant.clone(),
                });
            }
        }
    }
    diagnostics.extend(verdicts);
    EntityStateResult {
        entity_id: decl.id.clone(),
        entity_label: decl.label.clone(),
        axes,
        patterns,
        truncated: view.truncated || !run.complete,
        no_creation_path: decl.birth == Birth::PreExisting,
        diagnostics,
    }
}
