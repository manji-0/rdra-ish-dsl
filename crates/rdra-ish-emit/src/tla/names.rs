//! TLA+ identifiers for every item of a system, distinct after sanitizing.

use std::collections::{BTreeMap, HashMap, HashSet};

use rdra_ish_core::lifecycle::{ActId, EntId, State, TransitionSystem, Value, VarId, VarKind};

use super::{render_value, sanitize_ident};

/// Identifiers the module defines itself, the TLA+ keywords, and the Integers operators.
#[rustfmt::skip]
const RESERVED: &[&str] = &[
    "vars", "Init", "Next", "Spec", "TypeOK", "InstanceCount", "IntRange", "FirstAbsent",
    "now", "nothing", "ex", "i", "j", "Int", "Nat",
    "ASSUME", "ASSUMPTION", "AXIOM", "BOOLEAN", "CASE", "CHOOSE", "CONSTANT", "CONSTANTS",
    "DOMAIN", "ELSE", "ENABLED", "EXCEPT", "EXTENDS", "FALSE", "IF", "IN", "INSTANCE", "LAMBDA",
    "LET", "LOCAL", "MODULE", "OTHER", "STRING", "SUBSET", "THEN", "THEOREM", "TRUE",
    "UNCHANGED", "UNION", "VARIABLE", "VARIABLES", "WITH",
];

/// Names a printed module uses, indexed like the system's items.
#[derive(Clone, Debug)]
pub struct TlaNames {
    vars: Vec<String>,
    /// `<E>_Ids` per entity.
    ids: Vec<String>,
    actions: Vec<String>,
    invariants: Vec<String>,
    after: Vec<String>,
    properties: Vec<String>,
}

struct Taken(HashSet<String>);

impl Taken {
    fn claim(&mut self, raw: &str) -> String {
        let base = sanitize_ident(raw);
        let mut name = base.clone();
        let mut n = 2;
        while is_binder(&name) || RESERVED.contains(&name.as_str()) || !self.0.insert(name.clone())
        {
            name = format!("{base}_{n}");
            n += 1;
        }
        name
    }
}

/// `p0`, `b1`, `v2`: the printer's own bound identifiers.
fn is_binder(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some('p' | 'b' | 'v'))
        && !chars.as_str().is_empty()
        && chars.all(|c| c.is_ascii_digit())
}

impl TlaNames {
    pub fn new(ts: &TransitionSystem) -> Self {
        let mut taken = Taken(HashSet::new());
        let entity = |e: EntId| ts.entity(e).id.clone();
        let ids = (0..ts.entities().len())
            .map(|i| taken.claim(&format!("{}_Ids", ts.entities()[i].id)))
            .collect();
        let vars = ts
            .vars()
            .iter()
            .map(|d| match &d.kind {
                VarKind::Now => "now".to_string(),
                VarKind::Exists(e) => taken.claim(&format!("{}_exists", entity(*e))),
                VarKind::Column {
                    entity: e, column, ..
                } => taken.claim(&format!("{}_{column}", entity(*e))),
                VarKind::Prop { entity: e, key } => {
                    taken.claim(&format!("{}_prop_{key}", entity(*e)))
                }
                VarKind::Owner {
                    child,
                    parent,
                    role,
                    ..
                } => {
                    let name = role.clone().unwrap_or_else(|| entity(*parent));
                    taken.claim(&format!("{}_owner_{name}", entity(*child)))
                }
            })
            .collect();
        let actions = ts.actions().iter().map(|a| taken.claim(&a.name)).collect();
        let invariants = ts
            .invariants()
            .iter()
            .map(|x| taken.claim(&x.name))
            .collect();
        let after = ts
            .after_asserts()
            .iter()
            .map(|x| taken.claim(&x.name))
            .collect();
        let properties = ts
            .properties()
            .iter()
            .map(|x| taken.claim(&x.name))
            .collect();
        TlaNames {
            vars,
            ids,
            actions,
            invariants,
            after,
            properties,
        }
    }

    pub fn var(&self, v: VarId) -> &str {
        &self.vars[v.index()]
    }

    pub fn vars(&self) -> &[String] {
        &self.vars
    }

    pub fn ids(&self, e: EntId) -> &str {
        &self.ids[e.index()]
    }

    pub fn action(&self, a: ActId) -> &str {
        &self.actions[a.index()]
    }

    pub fn invariant(&self, i: usize) -> &str {
        &self.invariants[i]
    }

    pub fn after(&self, i: usize) -> &str {
        &self.after[i]
    }

    pub fn property(&self, i: usize) -> &str {
        &self.properties[i]
    }

    pub fn invariant_index(&self, name: &str) -> Option<usize> {
        self.invariants.iter().position(|n| n == name)
    }

    pub fn after_index(&self, name: &str) -> Option<usize> {
        self.after.iter().position(|n| n == name)
    }

    /// The action a TLC trace label names.
    pub fn action_index(&self, ts: &TransitionSystem, label: &str) -> Option<ActId> {
        let i = self.actions.iter().position(|n| n == label)?;
        ts.action_ids().nth(i)
    }

    /// The key of each slot: `name[k]` (TLA index, from 1) per instance, bare `now`.
    pub fn slot_keys(&self, ts: &TransitionSystem) -> Vec<String> {
        let mut out = Vec::new();
        for (i, d) in ts.vars().iter().enumerate() {
            let name = &self.vars[i];
            match d.kind {
                VarKind::Now => out.push(name.clone()),
                _ => out.extend((1..=ts.bounds.slots()).map(|k| format!("{name}[{k}]"))),
            }
        }
        out
    }

    /// A reached state as TLC prints it: slot key to value text.
    pub fn canon(&self, ts: &TransitionSystem, state: &State) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for var in ts.var_ids() {
            let d = ts.var(var);
            let name = &self.vars[var.index()];
            match d.kind {
                VarKind::Now => {
                    out.insert(name.clone(), render_value(&d.domain, state.get(ts, var, 0)));
                }
                _ => {
                    for slot in 0..ts.bounds.slots() {
                        out.insert(
                            format!("{name}[{}]", slot + 1),
                            render_value(&d.domain, state.get(ts, var, slot)),
                        );
                    }
                }
            }
        }
        out
    }

    /// The inverse of `canon`: each slot's value is the one whose text `render_value` prints,
    /// so the two cannot drift. `None` when a key is missing or extra, or a text is no value
    /// of its var's domain.
    pub fn decode(&self, ts: &TransitionSystem, canon: &BTreeMap<String, String>) -> Option<State> {
        self.decoder(ts).decode(canon)
    }

    /// `decode` for many states: every key and value text is rendered once.
    pub fn decoder<'a>(&self, ts: &'a TransitionSystem) -> Decoder<'a> {
        let vars = ts
            .var_ids()
            .map(|var| {
                let d = ts.var(var);
                let name = &self.vars[var.index()];
                let keys = match d.kind {
                    VarKind::Now => vec![name.clone()],
                    _ => (1..=ts.bounds.slots())
                        .map(|k| format!("{name}[{k}]"))
                        .collect(),
                };
                let mut values = HashMap::new();
                for v in (0..d.domain.size(ts.bounds)).map(|v| Value(v as u8)) {
                    values.entry(render_value(&d.domain, v)).or_insert(v);
                }
                (keys, values)
            })
            .collect();
        Decoder { ts, vars }
    }
}

/// `TlaNames::decode` with its tables built.
pub struct Decoder<'a> {
    ts: &'a TransitionSystem,
    /// Per var: the key of each slot, and the value each text names.
    vars: Vec<(Vec<String>, HashMap<String, Value>)>,
}

impl Decoder<'_> {
    pub fn decode(&self, canon: &BTreeMap<String, String>) -> Option<State> {
        let mut used = 0;
        let state = self.ts.state_from(|var, slot| {
            let (keys, values) = &self.vars[var.index()];
            let text = canon.get(&keys[slot])?;
            used += 1;
            values.get(text).copied()
        })?;
        (used == canon.len()).then_some(state)
    }
}
