//! Run-time certificates of hypotheses the Lean proofs take (proofs/lean/README.md, "What the
//! hypotheses are"). Every build checks them on what `Classes::of` reads off a run of the
//! extended sub-lifecycle `X`, before the reduction uses it:
//!
//! - `rise_faults`: `RiseTable` and the stages of `Lemma6Staged` (Stages.lean). The rise
//!   table holds exactly the rises of the representatives, under their class and the input
//!   bytes they end at, with the class they enter, and each is one stage of `stages`. That
//!   the other states of a class rise alike is (B3), which `reference::stable` certifies.
//! - `layout_faults`: the engines read the input bytes as `X` lays them out.
//! - `loop_faults`: `hloop` (Fairness.lean, Definition 5 case 1). A class whose silent edges
//!   between different states hold a cycle is marked as able to loop.
//! - `inner_faults`: the nested case's choice of observation (`InnerMap`, Nested.lean). The
//!   inner split keeps every rise in its `T_a`, and puts each `obs` var and input of `X` on its
//!   `T` side or in its own `obs`, so (B1) of the inner partition fixes them on an inner class.
//!
//! Each recomputes what it checks from the run's edges, the state bytes and the split's sets,
//! and reads nothing of how the table, the marks or the inner split were made. A fault makes
//! `reduced_with` decide the cone unreduced.

use std::collections::BTreeMap;
use std::fmt;

use super::extended::Extended;
use super::Reduction;
use crate::fasthash::FastMap;
use crate::lifecycle::explore::ComponentRun;
use crate::lifecycle::narrow::id32;
use crate::lifecycle::TransitionSystem;

/// The hypothesis a fault breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Hypothesis {
    /// A rise of `X` is not one stage of `stages`, or the engines' stages split the input
    /// bytes elsewhere than `X` does (`Lemma6Staged`).
    Stage,
    /// The rise table misses a rise of a representative, holds an entry no such rise makes,
    /// or two rises of a representative to the same input bytes enter different classes
    /// (`RiseTable`).
    Table,
    /// A class not marked as able to loop holds a cycle of silent edges (`hloop`).
    Loop,
    /// The inner split leaves a rise in its `S_a`, or an `obs` var or input of `X` in its `S`
    /// outside its `obs`.
    Inner,
}

/// One fault: the hypothesis and what breaks it.
#[derive(Debug)]
pub(super) struct Fault {
    pub(super) hypothesis: Hypothesis,
    what: String,
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.hypothesis {
            Hypothesis::Stage => "Lemma6Staged",
            Hypothesis::Table => "RiseTable",
            Hypothesis::Loop => "hloop",
            Hypothesis::Inner => "the inner split",
        };
        write!(f, "{name}: {}", self.what)
    }
}

/// The stages of a change of the input bytes from `before` to `after`, whose first `raw`
/// bytes are the raw inputs and the rest the bit slots: first the raw inputs, then each bit
/// slot that flips, one at a time, each stage the input bytes after it (`Reduction::stages`).
pub(super) fn stages(raw: usize, before: &[u8], after: &[u8]) -> Vec<Vec<u8>> {
    let mut stages = Vec::new();
    let mut cur = before.to_vec();
    if cur[..raw] != after[..raw] {
        cur[..raw].copy_from_slice(&after[..raw]);
        stages.push(cur.clone());
    }
    for k in raw..after.len() {
        if cur[k] != after[k] {
            cur[k] = after[k];
            stages.push(cur.clone());
        }
    }
    stages
}

/// The raw inputs' bytes at the head of `X`'s input bytes: its inputs other than the bits.
pub(super) fn raw_width(ext: &Extended) -> usize {
    let x = &ext.system;
    (ext.inputs_local.iter())
        .filter(|v| !ext.bit_vars.contains(v))
        .map(|v| x.slots_of(&x.var(*v).kind))
        .sum()
}

/// The engines read the input bytes as `X` lays them out: `r_raw` raw bytes of `r_len`, the
/// widths `Reduction::stages` and `input_bytes` use, are `X`'s, so the engines' stages split
/// the input bytes where the rises of `X` do (`Lemma6Staged`).
pub(super) fn layout_faults(ext: &Extended, r_raw: usize, r_len: usize) -> Vec<Fault> {
    let x = &ext.system;
    let len: usize = (ext.inputs_local.iter())
        .map(|v| x.slots_of(&x.var(*v).kind))
        .sum();
    let raw = raw_width(ext);
    match raw == r_raw && len == r_len {
        true => Vec::new(),
        false => vec![Fault {
            hypothesis: Hypothesis::Stage,
            what: format!(
                "the engines read {r_raw} raw bytes of {r_len} input bytes, and X lays out {raw} of {len}"
            ),
        }],
    }
}

/// The input bytes of the states of `run`, a run of `sub`, the extended sub-lifecycle `ext`
/// under its own reduction: `inputs(i, out)` writes state `i`'s into `out`.
pub(super) fn input_bytes<'a>(
    ext: &Extended,
    sub: &TransitionSystem,
    run: &'a ComponentRun,
) -> impl FnMut(u32, &mut Vec<u8>) + 'a {
    let spans: Vec<(usize, usize)> = (ext.inputs_local.iter())
        .map(|v| (sub.offset(*v), sub.slots_of(&sub.var(*v).kind)))
        .collect();
    let mut state = vec![0u8; run.states.width()];
    move |i, out| {
        run.states.read_into(i as usize, &mut state);
        out.clear();
        for &(at, w) in &spans {
            out.extend_from_slice(&state[at..at + w]);
        }
    }
}

/// Every fault of `rises`, the rise table read off a run with `class` each state's class and
/// `inputs` writing each state's input bytes, whose first `raw` are the raw inputs, against
/// the run's rise edges `rise_edges`. The representative of a class is its first state in
/// discovery order. The table holds exactly the rises of the representatives, each under its
/// class and the input bytes it ends at, with the class it enters (`RiseTable`), and each of
/// those rises is one stage of `stage` (`stages`, or a planted one in a test). That every
/// other state of a class rises as its representative does is (B3), which
/// `reference::stable` certifies.
pub(super) fn rise_faults(
    raw: usize,
    mut inputs: impl FnMut(u32, &mut Vec<u8>),
    rise_edges: &[(u32, u32)],
    class: &[u32],
    rises: &BTreeMap<(u32, Box<[u8]>), u32>,
    stage: impl Fn(usize, &[u8], &[u8]) -> Vec<Vec<u8>>,
) -> Vec<Fault> {
    let mut out = Vec::new();
    if rise_edges.is_empty() && rises.is_empty() {
        return out;
    }
    // The table the rises make, keyed by the class's bytes and the input bytes; and the
    // changes of the input bytes whose stages are checked, each once.
    let mut table: FastMap<Box<[u8]>, u32> = FastMap::default();
    let mut staged: FastMap<Box<[u8]>, ()> = FastMap::default();
    let (mut before, mut after, mut key) = (Vec::new(), Vec::new(), Vec::new());
    let classes = class.iter().max().map_or(0, |c| *c as usize + 1);
    let mut first = vec![u32::MAX; classes];
    for (i, c) in class.iter().enumerate() {
        if first[*c as usize] == u32::MAX {
            first[*c as usize] = id32(i);
        }
    }
    let mut source = None;
    for &(x, y) in rise_edges {
        let (cx, cy) = (class[x as usize], class[y as usize]);
        if first[cx as usize] != x {
            continue;
        }
        // The edges of one source come together in a run.
        if source != Some(x) {
            inputs(x, &mut before);
            source = Some(x);
        }
        inputs(y, &mut after);
        key.clear();
        key.extend_from_slice(&before);
        key.extend_from_slice(&after);
        if !staged.contains_key(key.as_slice()) {
            // A rise sets the raw inputs, or flips one bit slot (Definition 4).
            let raw_moved = before[..raw] != after[..raw];
            let bits_moved = (raw..after.len())
                .filter(|k| before[*k] != after[*k])
                .count();
            let want: Option<Vec<Vec<u8>>> = match (raw_moved, bits_moved) {
                (false, 0) => Some(Vec::new()),
                (true, 0) | (false, 1) => Some(vec![after.clone()]),
                _ => None,
            };
            let got = stage(raw, &before, &after);
            if want.as_ref() != Some(&got) {
                out.push(Fault {
                    hypothesis: Hypothesis::Stage,
                    what: format!(
                        "a rise of class {cx} from inputs {before:?} to {after:?} has the stages {got:?}, and is {}",
                        match want {
                            Some(_) => "one stage",
                            None => "more than one",
                        }
                    ),
                });
            }
            staged.insert(key.as_slice().into(), ());
        }
        key.clear();
        key.extend_from_slice(&cx.to_le_bytes());
        key.extend_from_slice(&after);
        match table.get(key.as_slice()) {
            None => {
                table.insert(key.as_slice().into(), cy);
            }
            Some(&c) if c != cy => out.push(Fault {
                hypothesis: Hypothesis::Table,
                what: format!(
                    "two rises of class {cx} to inputs {after:?} enter classes {c} and {cy}"
                ),
            }),
            Some(_) => {}
        }
    }
    for ((c, i), to) in rises {
        key.clear();
        key.extend_from_slice(&c.to_le_bytes());
        key.extend_from_slice(i);
        if table.get(key.as_slice()) != Some(to) {
            out.push(Fault {
                hypothesis: Hypothesis::Table,
                what: format!(
                    "the table holds class {c} to inputs {i:?} into class {to}, and the rises {:?}",
                    table.get(key.as_slice())
                ),
            });
        }
    }
    if table.len() != rises.len() {
        out.push(Fault {
            hypothesis: Hypothesis::Table,
            what: format!(
                "the rises make {} entries, and the table holds {}",
                table.len(),
                rises.len()
            ),
        });
    }
    out
}

/// Every fault of `loops`, each class's mark, against the silent edges `silent` of a run of
/// `n` states with `class` each state's class: a class that is not marked holds no cycle of
/// silent edges between two different states of it (`hloop`). The edges inside unmarked
/// classes are peeled from the states nothing enters (Kahn's algorithm); a state left has an
/// edge in from a cycle of its class. It keeps two words per state: the edges are read in
/// place, by source, when `silent` lists them so, as a run does.
pub(super) fn loop_faults(
    n: usize,
    silent: &[(u32, u32)],
    class: &[u32],
    loops: &[bool],
) -> Vec<Fault> {
    let inside = |&(x, y): &(u32, u32)| {
        let c = class[x as usize];
        x != y && c == class[y as usize] && !loops[c as usize]
    };
    let mut into = vec![0u32; n];
    let mut start = vec![0u32; n + 1];
    let (mut any, mut sorted, mut last) = (false, true, 0);
    for e in silent {
        start[e.0 as usize + 1] += 1;
        sorted &= last <= e.0;
        last = e.0;
        if inside(e) {
            into[e.1 as usize] += 1;
            any = true;
        }
    }
    if !any {
        return Vec::new();
    }
    // An edge list not sorted by source is copied and sorted.
    let copy;
    let edges = match sorted {
        true => silent,
        false => {
            copy = {
                let mut e: Vec<(u32, u32)> = silent.to_vec();
                e.sort_unstable();
                e
            };
            &copy[..]
        }
    };
    for i in 0..n {
        start[i + 1] += start[i];
    }
    // A state is peeled once nothing enters it, in index order; one whose last edge in goes
    // after the scan passed it waits on `late`. A peeled state's count is `PEELED`.
    const PEELED: u32 = u32::MAX;
    let mut late: Vec<u32> = Vec::new();
    for root in 0..id32(n) {
        if into[root as usize] != 0 {
            continue;
        }
        late.push(root);
        while let Some(x) = late.pop() {
            into[x as usize] = PEELED;
            let out = &edges[start[x as usize] as usize..start[x as usize + 1] as usize];
            for e in out.iter().filter(|e| inside(e)) {
                let y = e.1 as usize;
                into[y] -= 1;
                if into[y] == 0 && e.1 < root {
                    late.push(e.1);
                }
            }
        }
    }
    let mut faulty = vec![false; loops.len()];
    let mut out = Vec::new();
    for i in (0..n).filter(|&i| into[i] != 0 && into[i] != PEELED) {
        let c = class[i] as usize;
        if !faulty[c] {
            faulty[c] = true;
            out.push(Fault {
                hypothesis: Hypothesis::Loop,
                what: format!(
                    "class {c} holds a cycle of silent edges and is not marked as able to loop"
                ),
            });
        }
    }
    out
}

/// Every fault of `inner`, the reduction `X` was explored with, against what `Classes::of`
/// asks of it: every rise of `X` is in its `T_a`, and every `obs` var and input of `X` is on
/// its `T` side or in its `obs`.
pub(super) fn inner_faults(ext: &Extended, inner: &Reduction) -> Vec<Fault> {
    let x = &ext.system;
    let mut out = Vec::new();
    for a in ext.rise_actions() {
        if inner.s_actions.contains(&a) {
            out.push(Fault {
                hypothesis: Hypothesis::Inner,
                what: format!("the rise {} is an S action", x.actions[a.index()].name),
            });
        }
    }
    for v in ext.obs_local.iter().chain(&ext.inputs_local) {
        if inner.is_s_var(*v) && !inner.obs.contains(v) {
            out.push(Fault {
                hypothesis: Hypothesis::Inner,
                what: format!("{} is in S outside obs", x.var_name(*v)),
            });
        }
    }
    out
}

#[cfg(test)]
pub(super) mod tests {
    use std::cell::Cell;
    use std::collections::{BTreeMap, BTreeSet};

    use super::{layout_faults, loop_faults, raw_width, rise_faults, stages, Hypothesis};
    use crate::lifecycle::explore::symmetry::witnesses::dropping;
    use crate::lifecycle::explore::Caps;
    use crate::lifecycle::reduce::extended::Extended;
    use crate::lifecycle::tests::model_from;
    use crate::lifecycle::{Bounds, Scope, Seed, Target, TransitionSystem};

    /// What the certificates checked on this thread, for the walk over the samples and the
    /// planted faults: rise edges, runs whose loop marks were checked, of them those with a
    /// marked class, inner reductions, unobserved sub-lifecycles whose run was checked, and
    /// the faults that decided a cone unreduced.
    #[derive(Clone, Copy, Debug, Default)]
    pub(crate) struct Checked {
        pub(crate) rises: usize,
        pub(crate) loop_runs: usize,
        pub(crate) looping: usize,
        pub(crate) inner: usize,
        pub(crate) unobserved: usize,
        /// The faults found, by `Hypothesis`.
        pub(crate) faults: [usize; 4],
    }

    thread_local! {
        pub(crate) static CHECKED: Cell<Checked> = Cell::new(Checked::default());
    }

    pub(crate) fn count(f: impl FnOnce(&mut Checked)) {
        CHECKED.with(|c| {
            let mut v = c.get();
            f(&mut v);
            c.set(v);
        });
    }

    type Table = BTreeMap<(u32, Box<[u8]>), u32>;

    /// The hypotheses `rise_faults` finds broken.
    fn rise_check(
        inputs: &[[u8; 3]],
        edges: &[(u32, u32)],
        class: &[u32],
        table: &Table,
        stage: impl Fn(usize, &[u8], &[u8]) -> Vec<Vec<u8>>,
    ) -> BTreeSet<Hypothesis> {
        let inputs = |i: u32, out: &mut Vec<u8>| {
            out.clear();
            out.extend_from_slice(&inputs[i as usize]);
        };
        let faults = rise_faults(1, inputs, edges, class, table, stage);
        faults.iter().map(|f| f.hypothesis).collect()
    }

    /// The rise table and the stages of a run of five states, one raw input byte and two bit
    /// slots: state 0 sets the raw input (1), which flips a bit (2); state 3, in 0's class,
    /// rises as 0 does (4), which (B3) and not this check covers. The table as `of_graph` fills
    /// it passes; each planted fault fails the hypothesis it breaks.
    #[test]
    fn a_planted_stage_or_rise_table_fails_its_check() {
        let inputs = [[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 0, 0], [1, 0, 0]];
        let edges = [(0, 1), (1, 2), (3, 4)];
        let class = [0, 1, 2, 0, 1];
        let table: Table = [((0, [1, 0, 0].into()), 1), ((1, [1, 1, 0].into()), 2)].into();
        let none = BTreeSet::new();
        assert_eq!(rise_check(&inputs, &edges, &class, &table, stages), none);
        let stage = BTreeSet::from([Hypothesis::Stage]);
        let table_fault = BTreeSet::from([Hypothesis::Table]);
        // The raw inputs listed as a stage on a flip that leaves them.
        let raw_always = |raw: usize, before: &[u8], after: &[u8]| {
            let mut out = stages(raw, before, after);
            if before[..raw] == after[..raw] {
                out.insert(0, before.to_vec());
            }
            out
        };
        assert_eq!(
            rise_check(&inputs, &edges, &class, &table, raw_always),
            stage
        );
        // A rise that sets the raw input and flips a bit at once is two stages.
        let both = [[0, 0, 0], [1, 1, 0], [1, 1, 0], [0, 0, 0], [1, 1, 0]];
        let both_table: Table = [((0, [1, 1, 0].into()), 1)].into();
        let edges_both = [(0, 1), (3, 4)];
        assert_eq!(
            rise_check(&both, &edges_both, &class, &both_table, stages),
            stage
        );
        // An entry missing, an entry no rise makes, and a rise into another class.
        let mut missing = table.clone();
        missing.remove(&(1, [1, 1, 0].into()));
        assert_eq!(
            rise_check(&inputs, &edges, &class, &missing, stages),
            table_fault
        );
        let mut extra = table.clone();
        extra.insert((2, [0, 0, 1].into()), 0);
        assert_eq!(
            rise_check(&inputs, &edges, &class, &extra, stages),
            table_fault
        );
        let mut wrong = table.clone();
        wrong.insert((0, [1, 0, 0].into()), 2);
        assert_eq!(
            rise_check(&inputs, &edges, &class, &wrong, stages),
            table_fault
        );
        // Keyed by the inputs a rise leaves, as `rise_table_by_source` plants.
        let by_source: Table = [((0, [0, 0, 0].into()), 1), ((1, [1, 0, 0].into()), 2)].into();
        assert_eq!(
            rise_check(&inputs, &edges, &class, &by_source, stages),
            table_fault
        );
    }

    /// `loop_faults` on a run of six states: class 0 holds the cycle 0 → 1 → 0, class 1 the
    /// path 2 → 3 and a self-loop at 3, and 4 → 5 → 4 crosses from class 2 to class 3. Only
    /// class 0 must be marked; with its mark dropped, as `divergence_back_edges` and
    /// `s_cycles` plant, the check names it. The edges come sorted by source, as a run lists
    /// them, and unsorted, which the check copies and sorts.
    #[test]
    fn a_planted_loop_mark_fails_its_check() {
        let sorted = [(0, 1), (1, 0), (1, 2), (2, 3), (3, 3), (4, 5), (5, 4)];
        let unsorted = [(0, 1), (1, 0), (2, 3), (3, 3), (4, 5), (5, 4), (1, 2)];
        let class = [0, 0, 1, 1, 2, 3];
        for silent in [&sorted[..], &unsorted[..]] {
            let classes = |loops: &[bool]| -> Vec<String> {
                let faults = loop_faults(6, silent, &class, loops);
                assert!(faults.iter().all(|f| f.hypothesis == Hypothesis::Loop));
                faults.iter().map(ToString::to_string).collect()
            };
            assert!(classes(&[true, false, false, false]).is_empty());
            assert!(
                classes(&[true, true, true, true]).is_empty(),
                "an over-approximation"
            );
            let got = classes(&[false; 4]);
            assert_eq!(got.len(), 1, "{got:?}");
            assert!(got[0].contains("class 0"), "{got:?}");
        }
    }

    /// The corpus cases the planted faults below are seen on: rises (inputs and bits), silent
    /// cycles inside a class, and a nested reduction.
    const PLANT_CASES: [&str; 4] = [
        include_str!("../../../../rdra-ish-cli/tests/engine_corpus/cone_outside_monotone.rdra"),
        include_str!("../../../../rdra-ish-cli/tests/engine_corpus/cone_rise_param_on_a_stub.rdra"),
        include_str!("../../../../rdra-ish-cli/tests/engine_corpus/cone_reduced_class_loops.rdra"),
        include_str!(
            "../../../../rdra-ish-cli/tests/engine_corpus/cone_reduced_inner_class_loops.rdra"
        ),
    ];

    /// Every target and property cone of `PLANT_CASES` at `Bounds::VERIFY`, as cone systems
    /// with their seeds.
    fn plant_cones() -> Vec<(TransitionSystem, Vec<Seed>)> {
        let mut out = Vec::new();
        for src in PLANT_CASES {
            let ts = TransitionSystem::build(&model_from(src), Bounds::VERIFY, &Scope::ALL).ts;
            let targets = (ts.targets().into_iter())
                .filter(|t| !matches!(t, Target::Reach(_)))
                .map(|t| (ts.target_cone(t), ts.target_seed(t)));
            let properties =
                (0..ts.properties().len()).map(|i| (ts.property_cone(i), ts.property_seed(i)));
            for (cone, seed) in targets.chain(properties).collect::<Vec<_>>() {
                out.push(ts.cone_system(&cone, [seed]));
            }
        }
        out
    }

    /// `layout_faults` on the extended sub-lifecycle of every cone of `PLANT_CASES` with an S
    /// side: the widths `X` lays out pass, and a raw width or a length off by one fails.
    #[test]
    fn a_planted_input_layout_fails_its_check() {
        let mut checked = 0;
        for (sys, seeds) in plant_cones() {
            let split = sys.split_with(&seeds, true);
            if split.s.is_empty() || split.s.is_subset(&split.obs) {
                continue;
            }
            let ext = Extended::of(&sys, &split);
            let x = &ext.system;
            let len: usize = (ext.inputs_local.iter())
                .map(|v| x.slots_of(&x.var(*v).kind))
                .sum();
            let raw = raw_width(&ext);
            assert!(layout_faults(&ext, raw, len).is_empty());
            for (r, l) in [(raw + 1, len + 1), (raw, len + 1)] {
                let faults = layout_faults(&ext, r, l);
                assert!(faults.iter().any(|f| f.hypothesis == Hypothesis::Stage));
            }
            checked += 1;
        }
        assert!(checked > 0, "a cone of the cases has an S side");
    }

    /// Of the cones of `PLANT_CASES` that reduce, how many `plant` (a `dropped` switch) makes
    /// decide unreduced (the planted run is the cone system itself) with a fault of `want`
    /// found on the way.
    fn fall_backs(plant: &str, want: Hypothesis) -> usize {
        let caps = Caps::new(usize::MAX, 200_000);
        let faults = || CHECKED.with(|c| c.get().faults[want as usize]);
        let mut fell_back = 0;
        for (sys, seeds) in plant_cones() {
            if sys.reduced(&seeds, caps).reduction().is_none() {
                continue;
            }
            let before = faults();
            let planted = dropping(plant, || sys.reduced(&seeds, caps));
            fell_back += usize::from(planted == sys && faults() > before);
        }
        fell_back
    }

    /// Each planted fault reaches `of_graph` or `Classes::of` on a real run: the check of its
    /// hypothesis fails, which a debug build lets through only because the fault is planted,
    /// and the cone is decided unreduced. The rise table keyed by the inputs a rise leaves
    /// (`rise_table_by_source`), the raw inputs listed as a stage on every rise
    /// (`stages_raw_always`), a back edge of `divergence` that marks nothing
    /// (`divergence_back_edges`), `s_cycles` that says nothing loops (`s_cycles`), and an inner
    /// seed that reads and keeps nothing (`inner_seed_empty`).
    #[test]
    fn a_planted_fault_makes_a_reduced_cone_decide_unreduced() {
        for (plant, want) in [
            ("rise_table_by_source", Hypothesis::Table),
            ("stages_raw_always", Hypothesis::Stage),
            ("divergence_back_edges", Hypothesis::Loop),
            ("s_cycles", Hypothesis::Loop),
            ("inner_seed_empty", Hypothesis::Inner),
        ] {
            let n = fall_backs(plant, want);
            assert!(
                n > 0,
                "{plant}: no reduced cone falls back on a {want:?} fault"
            );
        }
    }
}
