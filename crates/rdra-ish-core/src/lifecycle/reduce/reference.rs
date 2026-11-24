//! The reference twin of the reduced step's class moves (#252, #275): `moves_from` as it was
//! before `moves_of_into` wrote the class moves into `Moves` from buffers it keeps, with the
//! class of the state looked up afresh and each successor built in a state of its own. A
//! `shadow` build holds the class moves of every reduced step to it.

use crate::lifecycle::explore::Moves;
use crate::lifecycle::expr::StateSlice;
use crate::lifecycle::TransitionSystem;

use super::{Reduction, Step};

/// The class moves from `s`: the last action, its params, and the successor.
fn moves_from(r: &Reduction, ts: &TransitionSystem, s: &StateSlice) -> Vec<Step> {
    let class = r
        .class(ts, s)
        .expect("a reduced state holds a representative");
    r.moves[class as usize]
        .iter()
        .map(|(act, params, to)| {
            let mut s2 = s.to_owned();
            r.write(&mut s2, &r.reps[*to as usize]);
            (*act, params.clone(), s2)
        })
        .collect()
}

/// Holds `out`'s moves from index `from` on, the class moves `moves_of_into` pushed for `s`,
/// to `moves_from`, and panics naming the call when they differ.
pub(super) fn check_class_moves(
    r: &Reduction,
    ts: &TransitionSystem,
    s: &StateSlice,
    out: &Moves,
    from: usize,
) {
    let want = moves_from(r, ts, s);
    let got: Vec<Step> = (from..out.len())
        .map(|i| {
            let (act, params, next) = out.get(i);
            (act, params.into(), StateSlice::new(next).to_owned())
        })
        .collect();
    assert!(
        got == want,
        "shadow: moves_of_into({:?}): the class moves {:?}, reference moves_from {:?}",
        &s.0,
        got.iter()
            .map(|(a, p, _)| (ts.actions[a.index()].name.as_str(), p))
            .collect::<Vec<_>>(),
        want.iter()
            .map(|(a, p, _)| (ts.actions[a.index()].name.as_str(), p))
            .collect::<Vec<_>>()
    );
    crate::shadow::compared(crate::shadow::Twin::ClassMoves);
}
