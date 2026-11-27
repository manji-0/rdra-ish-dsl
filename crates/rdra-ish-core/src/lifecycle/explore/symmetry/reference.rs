//! The twin of `Canon::canonical`: the least layout over every element of `G_s`,
//! enumerated, with each element applied by reading the state through the system's own
//! accessors rather than the group's tables. A `shadow` build holds every canonical form of
//! a state whose group has at most `REFERENCE_CAP` elements to it.

use crate::lifecycle::expr::StateSlice;
use crate::lifecycle::narrow::id8;
use crate::lifecycle::types::{Domain, Value, VarKind};
use crate::lifecycle::TransitionSystem;

use super::canon::permutations;
use super::Group;

/// The most elements of `G_s` the twin enumerates: 6^4, four classes of three slots.
pub(crate) const REFERENCE_CAP: u128 = 1296;

/// What the twin found of one state.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Reference {
    /// The least layout's state, in the state's own layout.
    pub(crate) canonical: Vec<u8>,
    /// `|G_s|`.
    pub(crate) size: u128,
    /// The elements that leave `s` as it is.
    pub(crate) aut: u128,
}

/// Every element of `G_s`, as a permutation per class (old slot to new position, at
/// `Class::first`); `None` past `REFERENCE_CAP`.
pub(crate) fn elements(ts: &TransitionSystem, g: &Group, s: &StateSlice) -> Option<Vec<Vec<u8>>> {
    let mut out: Vec<Vec<u8>> = vec![g.identity()];
    for class in &g.classes {
        if !class.moves {
            continue;
        }
        let movable: Vec<usize> = (0..class.slots)
            .filter(|&slot| {
                class
                    .entities
                    .iter()
                    .all(|e| s.get(ts, ts.entity(*e).exists, slot) != Value::ABSENT)
            })
            .collect();
        let arrangements = permutations(movable.len());
        if (out.len() as u128) * (arrangements.len() as u128) > REFERENCE_CAP {
            return None;
        }
        out = out
            .into_iter()
            .flat_map(|g| {
                arrangements.iter().map({
                    let movable = &movable;
                    move |arr| {
                        let mut g = g.clone();
                        for (i, &old) in movable.iter().enumerate() {
                            g[class.first + old] = id8(movable[usize::from(arr[i])]);
                        }
                        g
                    }
                })
            })
            .collect();
    }
    Some(out)
}

/// `s` under `perm`: each var of an entity moves with its class's slots, and an owner on a
/// present or deleted row names its parent's new slot.
pub(crate) fn apply(ts: &TransitionSystem, g: &Group, perm: &[u8], s: &StateSlice) -> Vec<u8> {
    let mut out = s.to_owned();
    for v in ts.var_ids() {
        let d = ts.var(v);
        let Some(e) = d.kind.entity() else { continue };
        let class = &g.classes[g.class_of(e)];
        for slot in 0..ts.slots(e) {
            let mut value = s.get(ts, v, slot);
            if let (VarKind::Owner { .. }, Domain::Instance { parent, .. }) = (&d.kind, &d.domain) {
                if s.get(ts, ts.entity(e).exists, slot) != Value::ABSENT {
                    if let Some(p) = d.domain.slot(value) {
                        let pc = &g.classes[g.class_of(*parent)];
                        value = d.domain.of_slot(perm[pc.first + usize::from(p)]);
                    }
                }
            }
            out.set(ts, v, usize::from(perm[class.first + slot]), value);
        }
    }
    out.0.into_vec()
}

/// The layout the canonical form minimizes: block by block, class by class, row by row,
/// each row every var of the class's entities in var order; then the vars of no entity.
pub(crate) fn layout(ts: &TransitionSystem, g: &Group, s: &StateSlice) -> Vec<u8> {
    let mut out = Vec::new();
    for block in &g.blocks {
        for &c in &block.classes {
            let class = &g.classes[c];
            for slot in 0..class.slots {
                for e in &class.entities {
                    for v in ts
                        .var_ids()
                        .filter(|v| ts.var(*v).kind.entity() == Some(*e))
                    {
                        out.push(s.get(ts, v, slot).0);
                    }
                }
            }
        }
    }
    for v in ts.var_ids().filter(|v| ts.var(*v).kind.entity().is_none()) {
        out.push(s.get(ts, v, 0).0);
    }
    out
}

/// The least layout of `s` over `G_s`, enumerated; `None` past `REFERENCE_CAP`.
pub(crate) fn canonical(ts: &TransitionSystem, g: &Group, s: &StateSlice) -> Option<Reference> {
    let elements = elements(ts, g, s)?;
    let mut best: Option<(Vec<u8>, Vec<u8>)> = None;
    let mut aut = 0u128;
    for perm in &elements {
        let image = apply(ts, g, perm, s);
        let key = layout(ts, g, StateSlice::new(&image));
        match &best {
            Some((k, _)) if key > *k => {}
            Some((k, _)) if key == *k => aut += 1,
            _ => {
                best = Some((key, image));
                aut = 1;
            }
        }
    }
    let (_, canonical) = best.expect("the identity is an element");
    Some(Reference {
        canonical,
        size: elements.len() as u128,
        aut,
    })
}
