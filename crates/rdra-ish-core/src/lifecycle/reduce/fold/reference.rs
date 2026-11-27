//! The reference twin of `Fold::holds` (#296, #275): the bit as it was computed before the
//! fast path dropped its allocations, with the params, the free T params and their radix in
//! fresh `Vec`s on every call. A build with debug assertions or the `shadow` feature holds
//! every `Fold::holds` call to it.

use crate::lifecycle::expr::StateSlice;
use crate::lifecycle::narrow::id8;
use crate::lifecycle::TransitionSystem;

use super::{Fold, FoldKey};

/// Whether the bit of `fold` holds for slot `c` of the key in `s`.
fn holds(fold: &Fold, ts: &TransitionSystem, s: &StateSlice, c: u8) -> bool {
    let a = &ts.actions[fold.act.index()];
    let mut params = vec![0u8; a.params.len()];
    let key = match &fold.key {
        FoldKey::Param(key) => *key,
        FoldKey::Linked { entity, body, .. } => {
            if !s.is_present(ts, *entity, usize::from(c)) {
                return false;
            }
            let mut env = crate::lifecycle::expr::Env::new(&params);
            env.bound.push(c);
            return ts.eval(body, s, &mut env);
        }
    };
    params[usize::from(key)] = c;
    let free: Vec<u8> = fold
        .t_params
        .iter()
        .map(|(p, _)| *p)
        .filter(|p| *p != key)
        .collect();
    let radix: Vec<usize> = free
        .iter()
        .map(|p| ts.slots(a.params[usize::from(*p)].entity))
        .collect();
    (0..radix.iter().product::<usize>()).any(|binding| {
        let mut rest = binding;
        for (p, n) in free.iter().zip(&radix) {
            params[usize::from(*p)] = id8(rest % n);
            rest /= n;
        }
        let present = fold.t_params.iter().all(|(p, must)| {
            !must
                || s.is_present(
                    ts,
                    a.params[usize::from(*p)].entity,
                    usize::from(params[usize::from(*p)]),
                )
        });
        let mut env = crate::lifecycle::expr::Env::new(&params);
        present
            && fold
                .guards
                .iter()
                .all(|g| ts.eval(&a.guards[*g].pred, s, &mut env))
    })
}

/// Holds `got`, what `Fold::holds` said of slot `c` in `s`, to the reference, and panics
/// naming the call when they differ.
pub(super) fn check_holds(fold: &Fold, ts: &TransitionSystem, s: &StateSlice, c: u8, got: bool) {
    let want = holds(fold, ts, s, c);
    assert!(
        got == want,
        "shadow: Fold::holds({}, slot {c}) in state {:?}: {got}, reference {want}",
        ts.actions[fold.act.index()].name,
        &s.0
    );
    #[cfg(feature = "shadow")]
    crate::shadow::compared(crate::shadow::Twin::FoldHolds);
}
