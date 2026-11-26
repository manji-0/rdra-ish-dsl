//! The reference twin of `each_planned` (#253, #275): the enabled bindings as their
//! definition states them, every binding `bindings` lists whose pre-state guards hold, each
//! guard read by `eval`. A build with debug assertions or the `shadow` feature holds every
//! `each_enabled` call to it.

use crate::lifecycle::expr::StateSlice;
use crate::lifecycle::types::Action;
use crate::lifecycle::TransitionSystem;

/// The bindings of `action` in `state` whose pre-guards hold, in `bindings` order.
pub(super) fn enabled_bindings(
    ts: &TransitionSystem,
    action: &Action,
    state: &StateSlice,
) -> Vec<Box<[u8]>> {
    ts.bindings(action, state)
        .into_iter()
        .filter(|params| ts.pre_guards_hold(action, state, params))
        .map(Vec::into_boxed_slice)
        .collect()
}
