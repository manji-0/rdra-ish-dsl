//! TLA+ identifiers for every item of a system, distinct after sanitizing.

use std::collections::{BTreeMap, HashMap, HashSet};

use rdra_ish_core::lifecycle::{ActId, EntId, State, TransitionSystem, Value, VarId, VarKind};

use super::{render_value, sanitize_ident};

/// Identifiers the module defines itself, the TLA+ keywords, and the Integers operators.
#[rustfmt::skip]
const RESERVED: &[&str] = &[
    "vars", "Init", "Next", "Spec", "TypeOK", "InstanceCount", "IntRange", "FirstAbsent",
    "now", "nothing", "ex", "i", "j", "Int", "Nat",
    "Print", "PrintT", "Assert", "JavaTime", "Permutations", "SortSeq", "RandomElement", "Any",
    "ToString", "TLCEval", "TLCGet", "TLCSet",
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
    /// `Fair_<property>` for a property with `.fair(...)`.
    fair: Vec<Option<String>>,
    /// The class var of a reduced cone (`TransitionSystem::reduced`), which stands for the S
    /// vars: `Rep_<var>` tables give each S var the printed module reads, and `Succ_<action>`
    /// the class moves of each S action.
    class: Option<String>,
    /// `Rep_<var>` per S var the rest reads, and `Succ_<action>` per S action, by index.
    rep: Vec<Option<String>>,
    succ: Vec<Option<String>>,
}

/// The names claimed so far, and every name the module asks for: a renamed item's `_<k>`
/// skips a name another item asks for, so `Start` taking `Start_2` cannot push a use case
/// `Start_2` to `Start_2_2`.
struct Taken {
    taken: HashSet<String>,
    wanted: HashSet<String>,
}

impl Taken {
    fn claim(&mut self, raw: &str) -> String {
        let base = sanitize_ident(raw);
        let mut name = base.clone();
        let mut n = 2;
        while is_binder(&name)
            || RESERVED.contains(&name.as_str())
            || !self.taken.insert(name.clone())
        {
            name = format!("{base}_{n}");
            n += 1;
            while self.wanted.contains(&name) {
                name = format!("{base}_{n}");
                n += 1;
            }
        }
        name
    }

    fn claim_all(&mut self, raws: &[String]) -> Vec<String> {
        raws.iter().map(|raw| self.claim(raw)).collect()
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
        let entity = |e: EntId| ts.entity(e).id.clone();
        let properties: Vec<String> = ts.properties().iter().map(|x| x.name.clone()).collect();
        let invariants: Vec<String> = ts.invariants().iter().map(|x| x.name.clone()).collect();
        let after: Vec<String> = ts.after_asserts().iter().map(|x| x.name.clone()).collect();
        let fair: Vec<Option<String>> = ts
            .properties()
            .iter()
            .map(|x| (!x.fair.is_empty()).then(|| format!("Fair_{}", x.name)))
            .collect();
        let ids: Vec<String> = ts
            .entities()
            .iter()
            .map(|e| format!("{}_Ids", e.id))
            .collect();
        // `None` for `now`, which is reserved.
        let vars: Vec<Option<String>> = ts
            .vars()
            .iter()
            .map(|d| match &d.kind {
                VarKind::Now => None,
                VarKind::Exists(e) => Some(format!("{}_exists", entity(*e))),
                VarKind::Column {
                    entity: e, column, ..
                } => Some(format!("{}_{column}", entity(*e))),
                VarKind::Prop { entity: e, key } => Some(format!("{}_prop_{key}", entity(*e))),
                VarKind::Run { entity: e, usecase } => {
                    Some(format!("{}_run_{usecase}", entity(*e)))
                }
                VarKind::Owner {
                    child,
                    parent,
                    role,
                    ..
                } => {
                    let name = role.clone().unwrap_or_else(|| entity(*parent));
                    Some(format!("{}_owner_{name}", entity(*child)))
                }
            })
            .collect();
        let actions: Vec<String> = ts.actions().iter().map(|a| a.name.clone()).collect();
        let mut taken = Taken {
            taken: HashSet::new(),
            wanted: properties
                .iter()
                .chain(&invariants)
                .chain(&after)
                .chain(fair.iter().flatten())
                .chain(&ids)
                .chain(vars.iter().flatten())
                .chain(&actions)
                .map(|n| sanitize_ident(n))
                .collect(),
        };
        // The names docs/compatibility.md ("The TLA+ export") promises are claimed first, so
        // an `<E>_Ids` set, a var, or an action spelled like one takes the suffix. Core keeps
        // the rule names apart from the properties and from each other, so a rule prints here
        // as `check` prints it.
        let properties = taken.claim_all(&properties);
        let invariants = taken.claim_all(&invariants);
        let after = taken.claim_all(&after);
        let fair = fair
            .iter()
            .map(|f| f.as_ref().map(|f| taken.claim(f)))
            .collect();
        let ids = taken.claim_all(&ids);
        let vars: Vec<String> = vars
            .iter()
            .map(|v| {
                v.as_ref()
                    .map_or_else(|| "now".to_string(), |v| taken.claim(v))
            })
            .collect();
        let actions = taken.claim_all(&actions);
        let class = ts.reduction().map(|_| taken.claim("S_class"));
        // Claimed last, so a model id that spells one takes no name from anything else.
        let mut rep = vec![None; vars.len()];
        let mut succ = vec![None; actions.len()];
        if let Some(r) = ts.reduction() {
            for &v in r.obs() {
                rep[v.index()] = Some(taken.claim(&format!("Rep_{}", vars[v.index()])));
            }
            for &a in r.s_actions() {
                succ[a.index()] = Some(taken.claim(&format!("Succ_{}", actions[a.index()])));
            }
        }
        TlaNames {
            vars,
            ids,
            actions,
            invariants,
            after,
            properties,
            fair,
            class,
            rep,
            succ,
        }
    }

    /// The class var of a reduced cone.
    pub fn class(&self) -> Option<&str> {
        self.class.as_deref()
    }

    /// The module's state variables: every var outside S, then the class var of a reduced
    /// cone.
    pub fn state_vars(&self, ts: &TransitionSystem) -> Vec<String> {
        let r = ts.reduction();
        ts.var_ids()
            .filter(|v| !r.is_some_and(|r| r.is_s_var(*v)))
            .map(|v| self.vars[v.index()].clone())
            .chain(self.class.clone())
            .collect()
    }

    /// `Rep_<var>`: the table an S var reads by class.
    pub fn rep_table(&self, v: VarId) -> &str {
        self.rep[v.index()]
            .as_deref()
            .expect("an S var the rest reads")
    }

    /// `Succ_<action>`: an S action's class moves.
    pub fn succ_table(&self, a: ActId) -> &str {
        self.succ[a.index()].as_deref().expect("an S action")
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

    /// The operator that conjoins the fairness `properties()[i]` assumes, if it lists any.
    pub fn fair(&self, i: usize) -> Option<&str> {
        self.fair[i].as_deref()
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

    /// The key of each slot: `name[k]` (TLA index, from 1) per instance, bare `now`, and the
    /// class var of a reduced cone in place of its S vars.
    pub fn slot_keys(&self, ts: &TransitionSystem) -> Vec<String> {
        let r = ts.reduction();
        let mut out = Vec::new();
        for (i, d) in ts.vars().iter().enumerate() {
            if r.is_some_and(|r| r.is_s_var(VarId::at_index(i))) {
                continue;
            }
            let name = &self.vars[i];
            match d.kind {
                VarKind::Now => out.push(name.clone()),
                _ => out.extend((1..=ts.bounds.slots()).map(|k| format!("{name}[{k}]"))),
            }
        }
        out.extend(self.class.clone());
        out
    }

    /// A reached state as TLC prints it: slot key to value text.
    pub fn canon(&self, ts: &TransitionSystem, state: &State) -> BTreeMap<String, String> {
        let r = ts.reduction();
        let mut out = BTreeMap::new();
        for var in ts.var_ids() {
            if r.is_some_and(|r| r.is_s_var(var)) {
                continue;
            }
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
        if let (Some(r), Some(class)) = (r, &self.class) {
            let c = r
                .class(state)
                .expect("a reduced state holds a representative");
            out.insert(class.clone(), (c + 1).to_string());
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
        Decoder {
            ts,
            vars,
            class: self.class.clone(),
        }
    }
}

/// `TlaNames::decode` with its tables built.
pub struct Decoder<'a> {
    ts: &'a TransitionSystem,
    /// Per var: the key of each slot, and the value each text names.
    vars: Vec<(Vec<String>, HashMap<String, Value>)>,
    class: Option<String>,
}

impl Decoder<'_> {
    pub fn decode(&self, canon: &BTreeMap<String, String>) -> Option<State> {
        let mut used = 0;
        // A reduced cone's S vars come from the class var's representative.
        let class: Option<u32> = match (self.ts.reduction(), &self.class) {
            (Some(r), Some(key)) => {
                let text = canon.get(key)?;
                let c = text.parse::<u32>().ok()?.checked_sub(1)?;
                (usize::try_from(c).ok()? < r.classes()).then_some(c)?;
                used += 1;
                Some(c)
            }
            _ => None,
        };
        let state = self.ts.state_from(|var, slot| {
            if let (Some(r), Some(c)) = (self.ts.reduction(), class) {
                if let Some(v) = r.rep_value(c, var, slot) {
                    return Some(v);
                }
            }
            let (keys, values) = &self.vars[var.index()];
            let text = canon.get(&keys[slot])?;
            used += 1;
            values.get(text).copied()
        })?;
        (used == canon.len()).then_some(state)
    }
}
