//! The canonical form of a state: its least layout over `G_s`.
//!
//! The layout is block by block in `Group::blocks` order (parents first), each class row by
//! row in slot order, a row being every var of the class's entities, with an owner on a
//! row that is not absent written as its parent's new position. The least layout over `G_s` is found
//! block by block. In a block of one class:
//!
//! 1. The rows that move are sorted, each read with its parents' new positions, which the
//!    blocks before it have fixed. With no ties that order is the block's part of the least
//!    layout.
//! 2. Tied rows that no child row names give the same layout in any order: they keep
//!    slot order, and the run does not branch on them.
//! 3. The canonicalizer branches on the other ties, one branch per placement of the named
//!    rows, carries each branch to the later blocks, keeps the branches whose layout is
//!    least, and merges branches that place every row a later block reads alike.
//! 4. Past `BRANCH_CAP` branches it keeps the first: the result is still a member of the
//!    class, so the run stays sound, but two members may then be stored apart.
//!
//! A block whose rows name each other's (a self-owned entity, an identity pair) is
//! enumerated whole.
//!
//! The same walk counts `|Aut(s)|`, the elements of `G_s` that leave `s` as it is: every
//! placement that reaches the least layout, with the tied rows no child names in any order.
//! The orbit of `s` then has `|G_s| / |Aut(s)|` states.

use crate::lifecycle::narrow::id8;
use crate::lifecycle::types::Value;

use super::{factorial, Class, Group, BRANCH_CAP};

/// What canonicalizing one state found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Canonical {
    /// `BRANCH_CAP` cut the search: the result is a member of the class, maybe not its least.
    pub(crate) cut: bool,
    /// The states in the orbit, `|G_s| / |Aut(s)|`; `|G_s|` alone when `cut`.
    pub(crate) orbit: u128,
}

impl Canonical {
    /// What a plain run's state stands for: itself.
    pub(crate) const PLAIN: Canonical = Canonical {
        cut: false,
        orbit: 1,
    };
}

/// A worker's scratch for canonical forms: every buffer is kept between calls.
#[derive(Default)]
pub(crate) struct Canon {
    /// Per slot of every class: some present or deleted child row names it.
    named: Vec<bool>,
    /// Per slot: the row is not absent, so it moves within its class.
    movable: Vec<bool>,
    /// The branches, `Group::slots` bytes each: per class, old slot to new position.
    pos: Vec<u8>,
    /// Per branch: the placements it stands for.
    mult: Vec<u128>,
    /// The branches being built.
    pos2: Vec<u8>,
    mult2: Vec<u128>,
    /// The least block layout so far, and the one being read.
    best: Vec<u8>,
    key: Vec<u8>,
    /// One class's movable slots, and those slots in sorted order.
    slots: Vec<u8>,
    order: Vec<u8>,
    /// A tie grew more branches than `BRANCH_CAP` allows, and some were dropped.
    dropped: bool,
    /// `Group::slots` of the last call.
    width: usize,
    /// Scratch kept between calls, so that a call allocates nothing once warm: the branch
    /// being sorted, a branch being split, the ties of a sort, one tie's rows, its named
    /// and unnamed rows, the places a split has filled, and the branches a split grows.
    cur: Vec<u8>,
    split_pos: Vec<u8>,
    ties: Vec<(usize, usize)>,
    rows: Vec<u8>,
    named_rows: Vec<u8>,
    unnamed_rows: Vec<u8>,
    used: Vec<bool>,
    grown_pos: Vec<u8>,
    grown_mult: Vec<u128>,
    /// `merge`'s projections, flat, and the branches in their order.
    merge_keys: Vec<u8>,
    merge_order: Vec<usize>,
    merge_keep: Vec<(usize, u128)>,
    /// The injections of `j` rows into `k` places, flat, `j` bytes each, per `(j, k)` seen.
    placements: Vec<((usize, usize), Vec<u8>)>,
}

impl Canon {
    /// Writes the canonical form of `s` into `out` (the same width) and says what it found.
    /// `perm` then holds the permutation that takes `s` to it.
    pub(crate) fn canonical(&mut self, g: &Group, s: &[u8], out: &mut [u8]) -> Canonical {
        let n = g.slots;
        self.named.clear();
        self.named.resize(n, false);
        self.movable.clear();
        self.movable.resize(n, false);
        // A class with fewer than two rows that may move has nothing to sort: when every class
        // is so, `G_s` is the identity and `s` is its own canonical form, its orbit itself
        // ("Always by default", 2(b)).
        let mut sorts = false;
        for class in &g.classes {
            let mut m = 0;
            for slot in 0..class.slots {
                let movable =
                    class.moves && class.exists.iter().all(|&e| s[e + slot] != Value::ABSENT.0);
                self.movable[class.first + slot] = movable;
                m += usize::from(movable);
            }
            sorts |= m > 1;
        }
        if !sorts {
            self.pos.clear();
            for class in &g.classes {
                self.pos.extend((0..class.slots).map(|s| id8(s)));
            }
            self.mult.clear();
            self.mult.push(1);
            self.width = n;
            out.copy_from_slice(s);
            return Canonical::PLAIN;
        }
        for (c, refs) in g.children.iter().enumerate() {
            let class = &g.classes[c];
            for r in refs {
                for slot in 0..r.slots {
                    if s[r.exists + slot] == Value::ABSENT.0 {
                        continue;
                    }
                    let v = s[r.off + slot];
                    let parent = match r.optional {
                        true if v == 0 => continue,
                        true => usize::from(v) - 1,
                        false => usize::from(v),
                    };
                    if parent < class.slots {
                        self.named[class.first + parent] = true;
                    }
                }
            }
        }
        self.pos.clear();
        for class in &g.classes {
            self.pos.extend((0..class.slots).map(|s| id8(s)));
        }
        self.mult.clear();
        self.mult.push(1);
        self.dropped = false;
        self.width = n;
        let mut cut = false;
        for block in &g.blocks {
            match block.joint {
                false => self.sorted_block(g, s, block.classes[0]),
                true => self.joint_block(g, s, &block.classes),
            }
            self.merge(g, &block.later);
            if self.mult.len() > BRANCH_CAP {
                self.mult.truncate(BRANCH_CAP);
                self.pos.truncate(BRANCH_CAP * n);
                cut = true;
            }
        }
        cut |= self.dropped;
        g.apply(&self.pos[..n], s, out);
        let size = g
            .classes
            .iter()
            .filter(|c| c.moves)
            .map(|c| {
                let m = (0..c.slots).filter(|&x| self.movable[c.first + x]).count();
                factorial(m)
            })
            .fold(1u128, u128::saturating_mul);
        let aut: u128 = self.mult.iter().fold(0u128, |a, m| a.saturating_add(*m));
        Canonical {
            cut,
            orbit: match cut {
                true => size,
                false => size / aut.max(1),
            },
        }
    }

    /// The permutation the last `canonical` took its state to its canonical form by.
    pub(crate) fn perm(&self) -> &[u8] {
        &self.pos[..self.width]
    }

    /// The cell of `class`'s row `slot` at `col`, its owner read under branch `pos`.
    #[inline]
    fn cell(g: &Group, s: &[u8], class: &Class, col: usize, slot: usize, pos: &[u8]) -> u8 {
        let col = &class.cols[col];
        let v = s[col.off + slot];
        match col.owner {
            Some((pc, optional)) if s[class.exists[col.ent] + slot] != Value::ABSENT.0 => {
                g.relabel(pos, pc, optional, v)
            }
            _ => v,
        }
    }

    /// Rows `a` and `b` of `class`, compared cell by cell under branch `pos`.
    #[inline]
    fn cmp_rows(
        g: &Group,
        s: &[u8],
        class: &Class,
        a: usize,
        b: usize,
        pos: &[u8],
    ) -> std::cmp::Ordering {
        for col in 0..class.cols.len() {
            let ord =
                Self::cell(g, s, class, col, a, pos).cmp(&Self::cell(g, s, class, col, b, pos));
            if ord.is_ne() {
                return ord;
            }
        }
        std::cmp::Ordering::Equal
    }

    /// Appends `class`'s layout under branch `pos` to `key`: its rows in new-position order.
    fn layout(g: &Group, s: &[u8], class: &Class, pos: &[u8], key: &mut Vec<u8>) {
        let mut at = [0u8; 256];
        for old in 0..class.slots {
            at[usize::from(pos[class.first + old])] = id8(old);
        }
        for &old in &at[..class.slots] {
            for col in 0..class.cols.len() {
                key.push(Self::cell(g, s, class, col, usize::from(old), pos));
            }
        }
    }

    /// Takes a block's layout `key` for a branch: false when it is larger than the least so
    /// far; a smaller one drops the branches built so far.
    fn least(&mut self, key_len: usize) -> bool {
        use std::cmp::Ordering;
        let key = &self.key[..key_len];
        match self.mult2.is_empty() {
            true => {
                self.best.clear();
                self.best.extend_from_slice(key);
                true
            }
            false => match key.cmp(&self.best[..]) {
                Ordering::Greater => false,
                Ordering::Equal => true,
                Ordering::Less => {
                    self.best.clear();
                    self.best.extend_from_slice(key);
                    self.pos2.clear();
                    self.mult2.clear();
                    true
                }
            },
        }
    }

    /// One class's block: each branch sorts the class's movable rows, and the branches whose
    /// layout is least go on, each split once per placement of its tied named rows.
    fn sorted_block(&mut self, g: &Group, s: &[u8], c: usize) {
        let class = &g.classes[c];
        let n = g.slots;
        self.slots.clear();
        self.slots.extend(
            (0..class.slots)
                .filter(|&x| self.movable[class.first + x])
                .map(|x| id8(x)),
        );
        let m = self.slots.len();
        // Nothing to sort, and the layout depends on the branch only through owners: one
        // branch keeps every branch.
        if m < 2 && (self.mult.len() == 1 || class.cols.iter().all(|c| c.owner.is_none())) {
            return;
        }
        let branches = self.mult.len();
        self.pos2.clear();
        self.mult2.clear();
        self.best.clear();
        let mut pos = std::mem::take(&mut self.cur);
        for b in 0..branches {
            pos.clear();
            pos.extend_from_slice(&self.pos[b * n..(b + 1) * n]);
            // Insertion sort, ties in slot order.
            self.order.clear();
            for &x in &self.slots {
                let mut i = self.order.len();
                self.order.push(x);
                while i > 0
                    && Self::cmp_rows(
                        g,
                        s,
                        class,
                        usize::from(self.order[i - 1]),
                        usize::from(x),
                        &pos,
                    )
                    .is_gt()
                {
                    self.order[i] = self.order[i - 1];
                    i -= 1;
                }
                self.order[i] = x;
            }
            for (i, &old) in self.order.iter().enumerate() {
                pos[class.first + usize::from(old)] = self.slots[i];
            }
            self.key.clear();
            if branches > 1 {
                let mut key = std::mem::take(&mut self.key);
                Self::layout(g, s, class, &pos, &mut key);
                self.key = key;
            }
            if !self.least(self.key.len()) {
                continue;
            }
            self.split(g, s, class, b, &pos);
        }
        self.cur = pos;
        std::mem::swap(&mut self.pos, &mut self.pos2);
        std::mem::swap(&mut self.mult, &mut self.mult2);
    }

    /// Adds branch `b`, sorted into `pos`, to the next branches: one per placement of the
    /// named rows of each tie, the unnamed ones in slot order, each standing for the orders
    /// of its unnamed rows.
    fn split(&mut self, g: &Group, s: &[u8], class: &Class, b: usize, pos: &[u8]) {
        let n = g.slots;
        let m = self.order.len();
        let mut mult = self.mult[b];
        // The tie groups, as ranges of `order`, with their named members.
        let mut ties = std::mem::take(&mut self.ties);
        ties.clear();
        let mut lo = 0;
        while lo < m {
            let mut hi = lo + 1;
            while hi < m
                && Self::cmp_rows(
                    g,
                    s,
                    class,
                    usize::from(self.order[lo]),
                    usize::from(self.order[hi]),
                    pos,
                )
                .is_eq()
            {
                hi += 1;
            }
            let named = self.order[lo..hi]
                .iter()
                .filter(|&&x| self.named[class.first + usize::from(x)])
                .count();
            let k = hi - lo;
            match named {
                0 => mult = mult.saturating_mul(factorial(k)),
                j if k > 1 => {
                    mult = mult.saturating_mul(factorial(k - j));
                    ties.push((lo, hi));
                }
                _ => {}
            }
            lo = hi;
        }
        let start = self.mult2.len();
        self.pos2.extend_from_slice(pos);
        self.mult2.push(mult);
        for &(lo, hi) in &ties {
            // The rows of the tie in slot order, and the positions it fills.
            self.rows.clear();
            self.rows.extend_from_slice(&self.order[lo..hi]);
            self.rows.sort_unstable();
            self.named_rows.clear();
            self.unnamed_rows.clear();
            for &x in &self.rows {
                match self.named[class.first + usize::from(x)] {
                    true => self.named_rows.push(x),
                    false => self.unnamed_rows.push(x),
                }
            }
            let (j, k) = (self.named_rows.len(), hi - lo);
            let at = placements(&mut self.placements, j, k);
            let flat = &self.placements[at].1;
            let places = &self.slots[lo..hi];
            let end = self.mult2.len();
            self.grown_pos.clear();
            self.grown_mult.clear();
            for at in start..end {
                if self.grown_mult.len() >= BRANCH_CAP * 2 {
                    self.dropped = true;
                    break;
                }
                for placement in flat.chunks_exact(j) {
                    let p = &mut self.split_pos;
                    p.clear();
                    p.extend_from_slice(&self.pos2[at * n..(at + 1) * n]);
                    self.used.clear();
                    self.used.resize(k, false);
                    for (row, &i) in self.named_rows.iter().zip(placement) {
                        p[class.first + usize::from(*row)] = places[usize::from(i)];
                        self.used[usize::from(i)] = true;
                    }
                    let mut free = places
                        .iter()
                        .zip(&self.used)
                        .filter(|(_, u)| !**u)
                        .map(|(x, _)| *x);
                    for row in &self.unnamed_rows {
                        p[class.first + usize::from(*row)] =
                            free.next().expect("as many places as rows");
                    }
                    self.grown_pos.extend_from_slice(p);
                    self.grown_mult.push(self.mult2[at]);
                }
            }
            self.pos2.truncate(start * n);
            self.mult2.truncate(start);
            self.pos2.extend_from_slice(&self.grown_pos);
            self.mult2.extend_from_slice(&self.grown_mult);
        }
        self.ties = ties;
    }

    /// A block whose rows name each other's: every element of the block's group, per branch,
    /// and the least layouts.
    fn joint_block(&mut self, g: &Group, s: &[u8], classes: &[usize]) {
        let n = g.slots;
        let branches = self.mult.len();
        // Per class: its movable slots and every arrangement of them.
        let movable: Vec<Vec<u8>> = classes
            .iter()
            .map(|&c| {
                let class = &g.classes[c];
                (0..class.slots)
                    .filter(|&x| self.movable[class.first + x])
                    .map(|x| id8(x))
                    .collect()
            })
            .collect();
        let arrangements: Vec<Vec<Vec<u8>>> =
            movable.iter().map(|m| permutations(m.len())).collect();
        let total: usize = arrangements.iter().map(Vec::len).product();
        self.pos2.clear();
        self.mult2.clear();
        self.best.clear();
        for b in 0..branches {
            for mut k in 0..total {
                let mut pos: Vec<u8> = self.pos[b * n..(b + 1) * n].to_vec();
                for (i, &c) in classes.iter().enumerate() {
                    let class = &g.classes[c];
                    let arr = &arrangements[i][k % arrangements[i].len()];
                    k /= arrangements[i].len();
                    for (j, &old) in movable[i].iter().enumerate() {
                        pos[class.first + usize::from(old)] = movable[i][usize::from(arr[j])];
                    }
                }
                let mut key = std::mem::take(&mut self.key);
                key.clear();
                for &c in classes {
                    Self::layout(g, s, &g.classes[c], &pos, &mut key);
                }
                self.key = key;
                if self.least(self.key.len()) {
                    self.pos2.extend_from_slice(&pos);
                    self.mult2.push(self.mult[b]);
                }
            }
        }
        std::mem::swap(&mut self.pos, &mut self.pos2);
        std::mem::swap(&mut self.mult, &mut self.mult2);
    }

    /// Merges the branches that place every named row of `later`'s classes alike: no later
    /// block can tell them apart. The first of each keeps the placements of all.
    fn merge(&mut self, g: &Group, later: &[usize]) {
        let n = g.slots;
        let branches = self.mult.len();
        if branches < 2 {
            return;
        }
        let width: usize = later.iter().map(|&c| g.classes[c].slots).sum();
        let mut keys = std::mem::take(&mut self.merge_keys);
        keys.clear();
        for b in 0..branches {
            let pos = &self.pos[b * n..(b + 1) * n];
            for &c in later {
                let class = &g.classes[c];
                for slot in 0..class.slots {
                    keys.push(match self.named[class.first + slot] {
                        true => pos[class.first + slot],
                        false => u8::MAX,
                    });
                }
            }
        }
        let key = |b: usize| &keys[b * width..(b + 1) * width];
        let mut order = std::mem::take(&mut self.merge_order);
        order.clear();
        order.extend(0..branches);
        // By projection, then by branch, so the first of each run is its earliest branch.
        order.sort_unstable_by(|&a, &b| key(a).cmp(key(b)).then(a.cmp(&b)));
        let mut keep = std::mem::take(&mut self.merge_keep);
        keep.clear();
        let mut i = 0;
        while i < order.len() {
            let mut j = i;
            let mut sum = 0u128;
            while j < order.len() && key(order[j]) == key(order[i]) {
                sum = sum.saturating_add(self.mult[order[j]]);
                j += 1;
            }
            keep.push((order[i], sum));
            i = j;
        }
        keep.sort_unstable();
        self.merge_keys = keys;
        self.merge_order = order;
        if keep.len() < branches {
            self.pos2.clear();
            self.mult2.clear();
            for &(b, m) in &keep {
                self.pos2.extend_from_slice(&self.pos[b * n..(b + 1) * n]);
                self.mult2.push(m);
            }
            std::mem::swap(&mut self.pos, &mut self.pos2);
            std::mem::swap(&mut self.mult, &mut self.mult2);
        }
        self.merge_keep = keep;
    }
}

/// The index in `memo` of the injections of `j` items into `k` places, flat, `j` bytes
/// each; computed on the first call for `(j, k)`.
fn placements(memo: &mut Vec<((usize, usize), Vec<u8>)>, j: usize, k: usize) -> usize {
    if let Some(at) = memo.iter().position(|(jk, _)| *jk == (j, k)) {
        return at;
    }
    let flat = injections(j, k)
        .into_iter()
        .flat_map(|p| p.into_iter().map(|x| id8(x)))
        .collect();
    memo.push(((j, k), flat));
    memo.len() - 1
}

/// Every injective map of `j` items into `k` places, as the place of each item.
fn injections(j: usize, k: usize) -> Vec<Vec<usize>> {
    fn go(
        j: usize,
        k: usize,
        used: &mut Vec<bool>,
        cur: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
    ) {
        if cur.len() == j {
            out.push(cur.clone());
            return;
        }
        for i in 0..k {
            if !used[i] {
                used[i] = true;
                cur.push(i);
                go(j, k, used, cur, out);
                cur.pop();
                used[i] = false;
            }
        }
    }
    let mut out = Vec::new();
    go(j, k, &mut vec![false; k], &mut Vec::new(), &mut out);
    out
}

/// Every permutation of `0..m`.
pub(super) fn permutations(m: usize) -> Vec<Vec<u8>> {
    injections(m, m)
        .into_iter()
        .map(|p| p.into_iter().map(|x| id8(x)).collect())
        .collect()
}
