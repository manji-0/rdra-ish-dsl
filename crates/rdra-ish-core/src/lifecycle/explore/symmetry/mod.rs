//! Instance symmetry (#212, docs/design/instance-symmetry.md): the group that permutes the
//! slots of each entity, and the canonical form a run up to symmetry stores in place of a
//! state.
//!
//! Entities form classes that permute together: an owner that starts at its own slot
//! (`Start::Identity`) couples its child and its parent. The group of a state `s`, `G_s`,
//! permutes the slots of each class that are not absent in `s` and fixes the absent ones,
//! so the lowest absent slot a creation takes is the same in every member of a class. An
//! owner value on a present or deleted row is relabelled by its parent's permutation (a
//! deleted row keeps the key it had, which a later step of a split use case can read); on
//! an absent row it holds the base value, which names no row, and stays.
//!
//! `breaks` scans the system for what would break the group and leaves those classes out:
//! their slots stay where they are, so every run stays sound and loses only reduction.
//! `canon` computes the canonical form, the least layout of the state over `G_s`;
//! `reference` is its twin, which enumerates `G_s`.

use crate::lifecycle::narrow::id8;
mod breaks;
mod canon;
#[cfg(test)]
mod g6;
#[cfg(any(test, feature = "shadow"))]
pub(super) mod reference;
#[cfg(test)]
mod snapshot;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod witnesses;

pub(crate) use canon::{Canon, Canonical};

use crate::lifecycle::types::{Domain, EntId, Start, Value, VarKind};
use crate::lifecycle::TransitionSystem;

/// When `verify` explores a safety cone up to instance symmetry (`--symmetry`, `symmetry`
/// under `[verify]` in `rdra.toml`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Symmetry {
    /// Never: every cone runs as it always has.
    #[default]
    Never,
    /// Every cone the current phase covers that no plain run decides.
    Always,
}

impl std::str::FromStr for Symmetry {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "never" => Ok(Symmetry::Never),
            "always" => Ok(Symmetry::Always),
            other => Err(format!("`{other}` is not a symmetry mode: never or always")),
        }
    }
}

/// Whether a `shadow` build drops the group rule `name` (`RDRA_ISH_DROP_RULE=<name>`): a
/// scan rule by its variant (`OrderedId`), a reason `Reduction::fixed_why` gives as
/// `Reduced:<reason>` (`Reduced:S var`), a reason the extended sub-lifecycle fixes an
/// entity for as `Observed:<reason>` (`Observed:obs`), and `bind_stub_params`, which leaves
/// every param on a stub of the extended sub-lifecycle as it is, so that its check fails and
/// the cone is decided unreduced (`reduce/extended.rs`). A test-only switch that plants a bug
/// per rule, so that a witness can show the rule is needed (`witnesses`, and engine_corpus
/// under `shadow`); a unit test sets it per thread. A release build drops nothing.
#[cfg_attr(not(any(test, feature = "shadow")), allow(unused_variables))]
pub(crate) fn dropped(name: &str) -> bool {
    #[cfg(test)]
    let by_test = witnesses::DROPPED.with(|d| d.borrow().as_deref() == Some(name));
    #[cfg(not(test))]
    let by_test = false;
    #[cfg(feature = "shadow")]
    let by_env = std::env::var("RDRA_ISH_DROP_RULE").is_ok_and(|v| v == name);
    #[cfg(not(feature = "shadow"))]
    let by_env = false;
    by_test || by_env
}

/// Under the `shadow` feature, holds the canonical form `out` of `s` to the twin that
/// enumerates `G_s`, when it has at most `REFERENCE_CAP` elements.
#[cfg_attr(not(feature = "shadow"), allow(unused_variables))]
#[inline]
pub(crate) fn check(ts: &TransitionSystem, g: &Group, s: &[u8], out: &[u8], found: Canonical) {
    #[cfg(feature = "shadow")]
    {
        use crate::lifecycle::expr::StateSlice;
        use crate::shadow::{compared, skipped, Skip, Twin};
        match reference::canonical(ts, g, StateSlice::new(s)) {
            None => skipped(Twin::Canonical, Skip::OverCap),
            Some(want) => {
                assert!(
                    want.canonical == out && (found.cut || found.orbit == want.size / want.aut),
                    "shadow: Canon::canonical({s:?}): {out:?} with orbit {}, reference {:?} with orbit {}",
                    found.orbit,
                    want.canonical,
                    want.size / want.aut
                );
                compared(Twin::Canonical);
            }
        }
    }
}

/// What a run up to symmetry counts of the states it stores.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Orbits {
    /// The states of the plain run the stored ones stand for, `|G_r| / |Aut(r)|` each: an
    /// estimate, high where some members of an orbit are not reachable.
    pub(crate) states: u128,
    /// Stored states `BRANCH_CAP` cut, which may not be their class's least form.
    pub(crate) cut: u64,
}

impl Orbits {
    pub(crate) fn admit(&mut self, found: Canonical) {
        self.states = self.states.saturating_add(found.orbit);
        self.cut += u64::from(found.cut);
    }
}

/// Past this many branches the canonicalizer keeps the first and counts the state as stored
/// without its canonical form (the design's `BRANCH_CAP`).
pub(crate) const BRANCH_CAP: usize = 4096;

/// The most elements a block enumerated jointly may have per branch (`Rule::JointTooLarge`).
const JOINT_CAP: u128 = 5040;

/// Why a class is left out of the group: its slots stay where they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    /// A row id of the entity, or an owner naming its rows, under `<`, `<=`, `>`, `>=`, or in
    /// an `In` list: the comparison orders row ids.
    OrderedId,
    /// A row id or an owner naming its rows compared with a literal, other than an optional
    /// owner against `null`.
    IdAgainstLiteral,
    /// An effect writes an owner naming its rows a literal other than `null`.
    OwnerLiteral,
    /// A choice or a transition on an owner naming its rows that does not range over every
    /// row.
    OwnerChoice,
    /// A start that pins one of its slots: an identity start between unequal slot counts, or
    /// a pre-existing row whose owner starts at a row.
    PinnedStart,
    /// A creation that leaves an owner naming its rows at the base value, which is slot 0.
    UnlinkedCreate,
    /// A guard reads, before the step, an owner naming its rows on the row the step creates:
    /// that row is absent, its owner holds the base value, slot 0, and `Group::apply` does
    /// not relabel an owner on an absent row (claim A18 of docs/theory/claims.md).
    AbsentOwner,
    /// A row id of the entity compared with, or written into, something that is not a row
    /// id of the same class.
    MixedIds,
    /// A block of classes that name each other's rows has more arrangements than the
    /// canonicalizer enumerates.
    JointTooLarge,
    /// The sub-lifecycle of a reduced cone runs up to `H` (#212 phase 2): its classes keep
    /// what the quotient reads, so an entity that holds `obs`, an input or a fold bit, a stub
    /// of a T parent, and an entity whose rows a T key can name stay where they are.
    Observed,
    /// A #74-reduced cone runs up to `G_T` (#212 phase 3): its state holds a class
    /// representative for S and the class tables key on the inputs and the bits row by row,
    /// so an S entity, an entity that holds an S var or an input or whose rows such a key
    /// names, an entity a bit is kept per row of, and an entity an S action binds stay where
    /// they are (`Reduction::fixed_why`).
    Reduced,
}

impl Rule {
    /// The rule as a note under a cone line names it.
    pub fn text(self) -> &'static str {
        match self {
            Rule::OrderedId => "a comparison orders its row ids",
            Rule::IdAgainstLiteral => "a row id is compared with a literal",
            Rule::OwnerLiteral => "a key is set to a literal row",
            Rule::OwnerChoice => "a key is chosen among some of its rows",
            Rule::PinnedStart => "a start pins one of its rows",
            Rule::UnlinkedCreate => "a creation leaves a key at its first row",
            Rule::AbsentOwner => "a guard reads the key of a row before it is created",
            Rule::MixedIds => "a row id is compared with or written into another value",
            Rule::JointTooLarge => {
                "its rows and the rows that name them have too many arrangements"
            }
            Rule::Observed => "the reduction of its cone reads its rows",
            Rule::Reduced => "the reduced sub-lifecycle of its cone keys on its rows",
        }
    }
}

/// A class the group leaves out, by one of its entities, and why.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LeftOut {
    /// The entity's id.
    pub entity: String,
    pub rule: Rule,
}

/// One byte of a class's row: a var of one of the class's entities.
#[derive(Clone, Debug)]
pub(crate) struct Col {
    /// Where the var starts in the state.
    pub(crate) off: usize,
    /// The var's entity, by its index in `Class::exists`.
    pub(crate) ent: usize,
    /// For an owner: its parent's class, and whether value 0 is `null`.
    pub(crate) owner: Option<(usize, bool)>,
}

/// Entities whose slots permute together.
#[derive(Clone, Debug)]
pub(crate) struct Class {
    pub(crate) slots: usize,
    pub(crate) entities: Vec<EntId>,
    /// Per entity: where its existence var starts.
    pub(crate) exists: Vec<usize>,
    /// The row layout: every var of each entity, entity by entity, in var order.
    pub(crate) cols: Vec<Col>,
    /// Whether the group permutes it: not left out, and more than one slot.
    pub(crate) moves: bool,
    /// Where its slots start in an array of every class's slots.
    pub(crate) first: usize,
}

/// Classes laid out together: one class, or classes whose rows name each other's, which
/// the canonicalizer enumerates jointly.
#[derive(Clone, Debug)]
pub(crate) struct Block {
    pub(crate) classes: Vec<usize>,
    /// A class of the block names rows of a class of the block.
    pub(crate) joint: bool,
    /// The classes some later block names rows of: a branch matters only through them.
    pub(crate) later: Vec<usize>,
}

/// An owner var that names rows of a class.
#[derive(Clone, Debug)]
pub(crate) struct ChildRef {
    pub(crate) off: usize,
    /// Where the child's existence var starts: an absent row names no parent.
    pub(crate) exists: usize,
    pub(crate) slots: usize,
    pub(crate) optional: bool,
}

/// The symmetry group of one system, in the shape the canonicalizer walks.
#[derive(Clone, Debug)]
pub(crate) struct Group {
    pub(crate) classes: Vec<Class>,
    /// In an order that puts every parent's block before its children's.
    pub(crate) blocks: Vec<Block>,
    /// Per class: the owners that name its rows.
    pub(crate) children: Vec<Vec<ChildRef>>,
    /// Per entity: its class.
    pub(crate) class_of: Vec<usize>,
    /// Where the vars of no entity (`now`) are: they stay.
    pub(crate) free: Vec<usize>,
    /// Every class's slots.
    pub(crate) slots: usize,
    left_out: Vec<LeftOut>,
}

/// A union-find over entities.
fn find(parent: &mut [usize], x: usize) -> usize {
    let mut x = x;
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

impl Group {
    /// The group of `ts`, with every class its scan finds a reason to leave out left out. On
    /// a #74-reduced cone it is `G_T` (#212 phase 3): the classes of `Reduction::fixed_why`, which
    /// the representative and the class tables read, are left out too (`Rule::Reduced`).
    pub(crate) fn of(ts: &TransitionSystem) -> Group {
        let fixed: Vec<(EntId, Rule)> = match ts.reduction() {
            Some(r) => r
                .fixed_why(ts)
                .into_keys()
                .map(|e| (e, Rule::Reduced))
                .collect(),
            None => Vec::new(),
        };
        Self::leaving_out(ts, &fixed)
    }

    /// Under `RDRA_ISH_G_T_STATS` in a `perf` build (`crate::perf::stats`), one line on stderr
    /// for a run of `ts`, a reduced cone, up to this group: per entity, whether it moves, and
    /// why `Reduction::fixed_why` leaves it out.
    pub(crate) fn g_t_stats(&self, ts: &TransitionSystem) {
        let Some(r) = ts.reduction() else {
            return;
        };
        if !crate::perf::stats("RDRA_ISH_G_T_STATS") {
            return;
        }
        let why = r.fixed_why(ts);
        let ents: Vec<String> = (0..ts.entities().len())
            .map(EntId::at)
            .map(|e| {
                let state = match (self.moves(e), why.get(&e)) {
                    (true, _) => "moves".to_string(),
                    (false, Some(w)) => format!("fixed: {}", w.join(", ")),
                    (false, None) => "fixed by the scan or one slot".to_string(),
                };
                format!("{}[{}] {state}", ts.entity(e).id, ts.slots(e))
            })
            .collect();
        eprintln!("g-t: {}", ents.join("; "));
    }

    /// The group of `ts` with the classes of `fixed` left out as well (`Rule::Observed`):
    /// the subgroup `H` the sub-lifecycle of a reduced cone runs up to (#212 phase 2).
    pub(crate) fn fixing(ts: &TransitionSystem, fixed: &[EntId]) -> Group {
        debug_assert!(
            ts.reduction().is_none(),
            "the sub-lifecycle runs up to H unreduced"
        );
        let fixed: Vec<(EntId, Rule)> = fixed.iter().map(|e| (*e, Rule::Observed)).collect();
        Self::leaving_out(ts, &fixed)
    }

    /// The group of `ts` with the classes of `fixed` left out for the rule beside each, on
    /// top of what the scan leaves out.
    fn leaving_out(ts: &TransitionSystem, fixed: &[(EntId, Rule)]) -> Group {
        let n_ent = ts.entities().len();
        // Identity starts couple a child and its parent.
        let mut uf: Vec<usize> = (0..n_ent).collect();
        for v in ts.var_ids() {
            let d = ts.var(v);
            if let (VarKind::Owner { child, parent, .. }, Start::Identity) = (&d.kind, d.start) {
                if ts.slots(*child) == ts.slots(*parent) {
                    let (a, b) = (find(&mut uf, child.index()), find(&mut uf, parent.index()));
                    uf[a] = b;
                }
            }
        }
        let mut class_of = vec![usize::MAX; n_ent];
        let mut classes: Vec<Class> = Vec::new();
        let mut root_class: Vec<Option<usize>> = vec![None; n_ent];
        for (i, slot) in class_of.iter_mut().enumerate() {
            let r = find(&mut uf, i);
            let c = *root_class[r].get_or_insert_with(|| {
                classes.push(Class {
                    slots: ts.slots(EntId::at(i)),
                    entities: Vec::new(),
                    exists: Vec::new(),
                    cols: Vec::new(),
                    moves: true,
                    first: 0,
                });
                classes.len() - 1
            });
            classes[c].entities.push(EntId::at(i));
            classes[c]
                .exists
                .push(ts.offset(ts.entity(EntId::at(i)).exists));
            *slot = c;
        }
        // What breaks the group, by entity; a class with one such entity is left out.
        let mut left_out: Vec<LeftOut> = Vec::new();
        for (e, rule) in breaks::scan(ts, &class_of)
            .into_iter()
            .filter(|(_, rule)| !dropped(&format!("{rule:?}")))
            .chain(fixed.iter().copied())
        {
            classes[class_of[e.index()]].moves = false;
            left_out.push(LeftOut {
                entity: ts.entity(e).id.clone(),
                rule,
            });
        }
        let mut free = Vec::new();
        let mut children: Vec<Vec<ChildRef>> = vec![Vec::new(); classes.len()];
        for v in ts.var_ids() {
            let d = ts.var(v);
            let off = ts.offset(v);
            let Some(e) = d.kind.entity() else {
                free.push(off);
                continue;
            };
            let c = class_of[e.index()];
            let ent = classes[c]
                .entities
                .iter()
                .position(|x| *x == e)
                .expect("an entity is in its class");
            let owner = match (&d.kind, &d.domain) {
                (VarKind::Owner { .. }, Domain::Instance { parent, optional }) => {
                    let pc = class_of[parent.index()];
                    children[pc].push(ChildRef {
                        off,
                        exists: ts.offset(ts.entity(e).exists),
                        slots: ts.slots(e),
                        optional: *optional,
                    });
                    Some((pc, *optional))
                }
                _ => None,
            };
            classes[c].cols.push(Col { off, ent, owner });
        }
        // Entity by entity within a class, so a row reads one entity's vars together.
        for class in &mut classes {
            class.cols.sort_by_key(|col| (col.ent, col.off));
        }
        let blocks = Self::blocks(&classes);
        let mut group = Group {
            classes,
            blocks,
            children,
            class_of,
            free,
            slots: 0,
            left_out,
        };
        group.cap_joint_blocks(ts);
        let mut first = 0;
        for class in &mut group.classes {
            class.first = first;
            first += class.slots;
            class.moves &= class.slots > 1;
        }
        group.slots = first;
        group.left_out.sort();
        group.left_out.dedup();
        group
    }

    /// The classes in blocks, parents first: the strongly connected parts of the graph from
    /// each class to the classes its owners name, ordered by how many classes each reaches.
    fn blocks(classes: &[Class]) -> Vec<Block> {
        let nc = classes.len();
        let mut reach = vec![vec![false; nc]; nc];
        for (c, class) in classes.iter().enumerate() {
            for col in &class.cols {
                if let Some((p, _)) = col.owner {
                    reach[c][p] = true;
                }
            }
        }
        for k in 0..nc {
            let via = reach[k].clone();
            for row in reach.iter_mut() {
                if row[k] {
                    for (r, v) in row.iter_mut().zip(&via) {
                        *r |= *v;
                    }
                }
            }
        }
        let mut assigned = vec![false; nc];
        let mut blocks: Vec<Vec<usize>> = Vec::new();
        for i in 0..nc {
            if assigned[i] {
                continue;
            }
            let block: Vec<usize> = (0..nc)
                .filter(|&j| j == i || (reach[i][j] && reach[j][i]))
                .collect();
            for &j in &block {
                assigned[j] = true;
            }
            blocks.push(block);
        }
        // A parent reaches strictly fewer classes than its child.
        let reached = |b: &[usize]| reach[b[0]].iter().filter(|x| **x).count();
        blocks.sort_by_key(|b| (reached(b), b[0]));
        let names = |b: &[usize]| -> Vec<usize> {
            let mut out: Vec<usize> = b
                .iter()
                .flat_map(|c| classes[*c].cols.iter().filter_map(|col| col.owner))
                .map(|(p, _)| p)
                .collect();
            out.sort_unstable();
            out.dedup();
            out
        };
        (0..blocks.len())
            .map(|i| {
                let mut later: Vec<usize> = blocks[i + 1..].iter().flat_map(|b| names(b)).collect();
                later.sort_unstable();
                later.dedup();
                let joint = blocks[i].len() > 1 || names(&blocks[i]).contains(&blocks[i][0]);
                Block {
                    classes: blocks[i].clone(),
                    joint,
                    later,
                }
            })
            .collect()
    }

    /// Leaves out the classes of a joint block with more arrangements than `JOINT_CAP`.
    fn cap_joint_blocks(&mut self, ts: &TransitionSystem) {
        for b in 0..self.blocks.len() {
            if !self.blocks[b].joint {
                continue;
            }
            let size = self.blocks[b]
                .classes
                .iter()
                .filter(|c| self.classes[**c].moves)
                .map(|c| factorial(self.classes[*c].slots))
                .fold(1u128, u128::saturating_mul);
            if size <= JOINT_CAP || dropped("JointTooLarge") {
                continue;
            }
            for &c in &self.blocks[b].classes {
                self.classes[c].moves = false;
            }
            let entities: Vec<EntId> = self.blocks[b]
                .classes
                .iter()
                .flat_map(|c| self.classes[*c].entities.clone())
                .collect();
            self.left_out.extend(entities.into_iter().map(|e| LeftOut {
                entity: ts.entity(e).id.clone(),
                rule: Rule::JointTooLarge,
            }));
        }
    }

    /// The classes left out, by entity, each with why; empty when every class joins.
    pub(crate) fn left_out(&self) -> &[LeftOut] {
        &self.left_out
    }

    /// Whether the group moves nothing: no class joins with more than one slot.
    pub(crate) fn is_trivial(&self) -> bool {
        !self.classes.iter().any(|c| c.moves)
    }

    /// The class of an entity.
    #[cfg(any(test, feature = "shadow"))]
    pub(crate) fn class_of(&self, e: EntId) -> usize {
        self.class_of[e.index()]
    }

    /// Whether the group moves the rows of `e`.
    pub(crate) fn moves(&self, e: EntId) -> bool {
        self.classes[self.class_of[e.index()]].moves
    }

    /// `s` under the permutation `perm` (old slot to new position, per class at `first`), in
    /// the state's own layout: every var of a class moves with its slots, and an owner on a
    /// row that is not absent names its parent's new position.
    pub(crate) fn apply(&self, perm: &[u8], s: &[u8], out: &mut [u8]) {
        for &off in &self.free {
            out[off] = s[off];
        }
        for class in &self.classes {
            let p = &perm[class.first..class.first + class.slots];
            for col in &class.cols {
                let exists = class.exists[col.ent];
                for (old, &new) in p.iter().enumerate() {
                    let v = s[col.off + old];
                    let v = match col.owner {
                        Some((pc, optional)) if s[exists + old] != Value::ABSENT.0 => {
                            self.relabel(perm, pc, optional, v)
                        }
                        _ => v,
                    };
                    out[col.off + usize::from(new)] = v;
                }
            }
        }
    }

    /// An owner value of class `pc`'s rows under `perm`.
    #[inline]
    pub(crate) fn relabel(&self, perm: &[u8], pc: usize, optional: bool, v: u8) -> u8 {
        let class = &self.classes[pc];
        let slot = match optional {
            true if v == 0 => return 0,
            true => v - 1,
            false => v,
        };
        if usize::from(slot) >= class.slots {
            return v;
        }
        perm[class.first + usize::from(slot)] + u8::from(optional)
    }

    /// The identity permutation.
    pub(crate) fn identity(&self) -> Vec<u8> {
        self.classes
            .iter()
            .flat_map(|c| (0..c.slots).map(|s| id8(s)))
            .collect()
    }

    /// `g` after `h`: `apply(compose(g, h)) = apply(g) . apply(h)`.
    pub(crate) fn compose(&self, g: &[u8], h: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; self.slots];
        for class in &self.classes {
            for s in 0..class.slots {
                let mid = usize::from(h[class.first + s]);
                out[class.first + s] = g[class.first + mid];
            }
        }
        out
    }

    /// The inverse of `g`.
    pub(crate) fn inverse(&self, g: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; self.slots];
        for class in &self.classes {
            for s in 0..class.slots {
                out[class.first + usize::from(g[class.first + s])] = id8(s);
            }
        }
        out
    }

    /// The slot of entity `e` that `perm` takes `slot` to.
    pub(crate) fn map_slot(&self, perm: &[u8], e: EntId, slot: u8) -> u8 {
        let class = &self.classes[self.class_of[e.index()]];
        perm[class.first + usize::from(slot)]
    }
}

/// `n!`, saturating.
pub(crate) fn factorial(n: usize) -> u128 {
    (2..=n as u128).fold(1u128, u128::saturating_mul)
}

#[cfg(kani)]
mod proofs {
    use super::*;

    /// Shop: 2 slots, existence at 0..2. Order: 2 slots, existence at 2..4 and a mandatory
    /// key to Shop at 4..6. `now` at 6. A column that is no key moves as existence does, so
    /// the layout leaves it out.
    fn group() -> Group {
        let col = |off, owner| Col { off, ent: 0, owner };
        Group {
            classes: vec![
                Class {
                    slots: 2,
                    entities: vec![EntId::at(0)],
                    exists: vec![0],
                    cols: vec![col(0, None)],
                    moves: true,
                    first: 0,
                },
                Class {
                    slots: 2,
                    entities: vec![EntId::at(1)],
                    exists: vec![2],
                    cols: vec![col(2, None), col(4, Some((0, false)))],
                    moves: true,
                    first: 2,
                },
            ],
            blocks: vec![
                Block {
                    classes: vec![0],
                    joint: false,
                    later: vec![0],
                },
                Block {
                    classes: vec![1],
                    joint: false,
                    later: vec![],
                },
            ],
            children: vec![
                vec![ChildRef {
                    off: 4,
                    exists: 2,
                    slots: 2,
                    optional: false,
                }],
                vec![],
            ],
            class_of: vec![0, 1],
            free: vec![6],
            slots: 4,
            left_out: vec![],
        }
    }

    /// Any element of the group of every class's slots: a swap or not, per class.
    fn any_perm() -> [u8; 4] {
        let (a, b): (bool, bool) = (kani::any(), kani::any());
        [u8::from(a), u8::from(!a), u8::from(b), u8::from(!b)]
    }

    /// Any state of the layout: absent, present or deleted rows, and keys that name a slot
    /// of Shop.
    fn any_state() -> [u8; 7] {
        let s: [u8; 7] = kani::any();
        kani::assume(s[0] <= 2 && s[1] <= 2 && s[2] <= 2 && s[3] <= 2);
        kani::assume(s[4] <= 1 && s[5] <= 1 && s[6] <= 1);
        s
    }

    /// `apply` is an action: applying `g` after `h` is applying their composition.
    #[kani::proof]
    #[kani::unwind(8)]
    fn apply_of_a_composition_is_the_composition_of_applies() {
        let g = group();
        let (a, b) = (any_perm(), any_perm());
        let s = any_state();
        let (mut once, mut mid, mut twice) = ([0u8; 7], [0u8; 7], [0u8; 7]);
        g.apply(&g.compose(&a, &b), &s, &mut once);
        g.apply(&b, &s, &mut mid);
        g.apply(&a, &mid, &mut twice);
        assert_eq!(once, twice);
    }
}
