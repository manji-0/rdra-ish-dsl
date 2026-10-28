//! Per-entity views of the reached states, in the shape `states` prints: one pattern per
//! distinct column assignment of a present slot, with initial, terminal, and provenance.

use std::collections::{BTreeMap, BTreeSet};

use super::diag::{Diag, StateDiag};
use super::explore::{ComponentRun, Exploration};
use super::expr::State;
use super::types::{ActionOrigin, Birth, Domain, EntId, Value, VarKind};
use super::TransitionSystem;
use crate::model::SemanticModel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxisKind {
    Enum(Vec<String>),
    Bool,
    Nullable { pg_type: Option<String> },
    Int { max: u8 },
    NullableInt { max: u8 },
    Proposition { axis_key: String },
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
            Domain::Nullable if value == Value::NULL => AbstractValue::Null,
            Domain::Nullable => AbstractValue::Present,
            Domain::Int | Domain::NullableInt => match domain.number(value) {
                Some(n) => AbstractValue::Int(n),
                None => AbstractValue::Null,
            },
            Domain::Existence | Domain::Instance { .. } => AbstractValue::Int(i64::from(value.0)),
        }
    }

    /// Display form; a present nullable column names the PostgreSQL type a `sets` declared.
    pub fn display_with_type(&self, pg_type: Option<&str>) -> String {
        match (self, pg_type) {
            (AbstractValue::Present, Some(t)) => format!("present:{t}"),
            (AbstractValue::Present, None) => "present".to_string(),
            (AbstractValue::Null, _) => "null".to_string(),
            (AbstractValue::Enum(s), _) => s.clone(),
            (AbstractValue::Bool(b), _) => b.to_string(),
            (AbstractValue::Int(n), _) => n.to_string(),
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
                AxisKind::Bool | AxisKind::Nullable { .. } | AxisKind::Proposition { .. } => 2,
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
        .map(|v| ts.var(*v).domain.size(ts.bounds))
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
    s: &State,
    e: EntId,
    slot: usize,
    qualified: bool,
) -> String {
    let p = pattern(ts, e, &ts.pattern_key(s, e, slot));
    let body = p
        .values
        .iter()
        .map(|(c, v)| format!("{c}={}", v.display_with_type(None)))
        .collect::<Vec<_>>()
        .join(", ");
    let id = &ts.entity(e).id;
    match (qualified, ts.bounds.slots() > 1) {
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
        (_, Domain::Bool) => AxisKind::Bool,
        (VarKind::Column { pg_type, .. }, Domain::Nullable) => AxisKind::Nullable {
            pg_type: pg_type.clone(),
        },
        (_, Domain::Int) => AxisKind::Int { max },
        (_, Domain::NullableInt) => AxisKind::NullableInt { max },
        _ => AxisKind::Nullable { pg_type: None },
    };
    StateAxis {
        column: ts.var_name(var),
        kind,
    }
}

impl Exploration {
    /// Every diagnostic the runs found.
    pub fn diags(&self) -> impl Iterator<Item = &Diag> {
        self.runs.iter().flat_map(|r| r.diags.iter())
    }

    /// Per-entity views sorted by entity id. `build_diags` are reported on the entities they
    /// name, ahead of what exploration found.
    pub fn entity_views(
        &self,
        model: &SemanticModel,
        build_diags: &[Diag],
    ) -> Vec<EntityStateResult> {
        let mut out: Vec<EntityStateResult> = self
            .runs
            .iter()
            .flat_map(|run| {
                (0..run.system.entities().len())
                    .map(move |i| entity_view(model, run, EntId::at(i), build_diags, None))
            })
            .collect();
        out.sort_by(|a, b| a.entity_id.cmp(&b.entity_id));
        out
    }
}

fn is_cap(d: &StateDiag) -> bool {
    matches!(
        d,
        StateDiag::PatternCapReached { .. } | StateDiag::StateCapReached { .. }
    )
}

/// The view of `e` in `run`. With `verdicts`, the run is the entity's cone: a cap anywhere in
/// it makes the view incomplete, and the constraint verdicts come from their own cones.
pub(crate) fn entity_view(
    model: &SemanticModel,
    run: &ComponentRun,
    e: EntId,
    build_diags: &[Diag],
    verdicts: Option<Vec<StateDiag>>,
) -> EntityStateResult {
    let ts = &run.system;
    let decl = ts.entity(e);
    let axes: Vec<StateAxis> = decl.columns.iter().map(|v| axis(ts, *v)).collect();
    let view = &run.views[e.index()];
    let mut diagnostics: Vec<StateDiag> = build_diags
        .iter()
        .filter(|d| d.entities.contains(&decl.key))
        .map(|d| d.diag.clone())
        .collect();
    let in_cone = verdicts.is_some();
    let found: Vec<StateDiag> = verdicts.unwrap_or_else(|| {
        run.diags
            .iter()
            .filter(|d| !is_cap(&d.diag) && d.entities.contains(&decl.key))
            .map(|d| d.diag.clone())
            .collect()
    });
    if axes.is_empty() {
        // An entity with no state columns has one trivial pattern, as `states` reports it,
        // and still the verdicts of the constraints it binds.
        diagnostics.retain(|d| *d != StateDiag::NoCreationPath);
        diagnostics.extend(found);
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
            }],
            truncated: false,
            no_creation_path: false,
            diagnostics,
        };
    }
    let via = view.via();
    let patterns: Vec<ReachablePattern> = (0..view.patterns.len())
        .map(|id| ReachablePattern {
            pattern: pattern(ts, e, &view.patterns[id]),
            is_initial: view.initial[id],
            is_terminal: view.deletable[id] || !view.changes[id],
            provenance: Provenance {
                via: via[id]
                    .iter()
                    .filter_map(|act| match &ts.actions()[act.index()].origin {
                        ActionOrigin::UseCase { uc, buc } => Some((
                            buc.map(|b| model.bucs[b].id.clone()),
                            model.use_cases[*uc].id.clone(),
                        )),
                        ActionOrigin::Clock => None,
                    })
                    .collect(),
            },
        })
        .collect();
    let caps = run
        .diags
        .iter()
        .filter(|d| is_cap(&d.diag) && (in_cone || d.entities.contains(&decl.key)));
    for d in caps {
        if !diagnostics.contains(&d.diag) {
            diagnostics.push(d.diag.clone());
        }
    }
    for &var in &decl.columns {
        let (
            VarKind::Column {
                variation: false, ..
            },
            Domain::Enum(values),
        ) = (&ts.var(var).kind, &ts.var(var).domain)
        else {
            continue;
        };
        let column = ts.var_name(var);
        for variant in values {
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
    diagnostics.extend(found);
    EntityStateResult {
        entity_id: decl.id.clone(),
        entity_label: decl.label.clone(),
        axes,
        patterns,
        truncated: view.truncated || (in_cone && !run.complete),
        no_creation_path: decl.birth == Birth::PreExisting,
        diagnostics,
    }
}
