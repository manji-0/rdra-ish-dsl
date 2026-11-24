//! The reference twins of `observe`'s two shortcuts (#251, #275), as `observe` read a
//! written slot before them. Under the `shadow` feature each shortcut is held to its twin.

use crate::lifecycle::expr::StateSlice;
use crate::lifecycle::types::EntId;
use crate::lifecycle::TransitionSystem;

use super::EntityView;

/// The pattern id of `e`'s `slot` in `s`, looked up afresh: what `source` keeps for the
/// edges after the first from one state.
pub(super) fn source(
    ts: &TransitionSystem,
    view: &EntityView,
    s: &StateSlice,
    e: EntId,
    slot: usize,
) -> Option<usize> {
    view.id(&ts.pattern_key(s, e, slot))
}

/// The pattern ids of `e`'s `slot` before and after a move that leaves it present, as
/// `observe` read them on every such edge. Where the two are equal the edge recorded
/// nothing, which is what the skip of a row whose columns are unchanged assumes.
pub(super) fn row_ids(
    ts: &TransitionSystem,
    view: &EntityView,
    s: &StateSlice,
    s2: &StateSlice,
    e: EntId,
    slot: usize,
) -> (Option<usize>, Option<usize>) {
    (
        view.id(&ts.pattern_key(s, e, slot)),
        view.id(&ts.pattern_key(s2, e, slot)),
    )
}
