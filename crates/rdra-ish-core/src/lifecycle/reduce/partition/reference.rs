//! The reference twin of `quotient_blocks` (#252, #275): the partition as it was computed
//! before the worklist, every block a splitter in each sweep, sweeps until one splits
//! nothing, and the rises' signatures as sets. A test build holds every quotient the tests
//! compute to it, and the random graphs of `the_worklist_partition_is_the_partition_of_
//! repeated_sweeps`. A `shadow` build holds the quotients of up to `SHADOW_CAP` states.

use std::collections::{BTreeSet, HashMap};

use super::{distinct, Adj};

/// The most states of a quotient a `shadow` build runs the sweeps on; larger ones pass
/// unchecked, and the certificate of #275's stage 2 is to check them. A test build checks
/// every quotient.
///
/// The cap sits just above the largest quotient `verify` computes on the samples at their
/// header flags (2,153,635 states), measured on main at eca31dbf with the twins added, on an
/// 18-core machine, release build: the sweeps took 105 ms on 633,787 states, 369 ms on 2,153,635
/// and 1.2 s on 1,028,160 states with 388,928 blocks, against 135, 312 and 396 ms for the
/// worklist. Their time follows the number of sweeps, which the state count does not
/// bound, so nothing past the measured sizes is known to be cheap.
#[cfg(all(feature = "shadow", not(test)))]
const SHADOW_CAP: usize = 2_500_000;

/// Holds `blocks`, what `quotient_blocks` returned for these arguments, to the sweeps, and
/// panics naming the call when the two partitions differ beyond the numbering of blocks.
pub(super) fn check(
    n: usize,
    silent: &[(u32, u32)],
    rises: &[(u32, u32)],
    initial: &[u32],
    blocks: &[u32],
) {
    #[cfg(all(feature = "shadow", not(test)))]
    if n > SHADOW_CAP {
        crate::shadow::skipped(crate::shadow::Twin::Partition);
        return;
    }
    let want = quotient_blocks(n, silent, rises, initial);
    assert!(
        same_partition(blocks, &want),
        "shadow: quotient_blocks({n} states, {} silent edges, {} rises): the worklist's {} blocks are not the reference's {}",
        silent.len(),
        rises.len(),
        distinct(blocks),
        distinct(&want)
    );
    #[cfg(feature = "shadow")]
    crate::shadow::compared(crate::shadow::Twin::Partition);
}

/// The partition of a run of the extended sub-lifecycle: the coarsest one that refines
/// `initial` (the states' `obs` and inputs), is divergence-blind stuttering stable under the
/// silent edges, and is strongly stable under the rise edges: two states of a block reach the
/// same blocks by a rise. The blocks refine the inputs, so the block a rise enters names the
/// inputs it set, and the second condition says that two equivalent states stay equivalent
/// under every rise. Returns each state's block.
pub(super) fn quotient_blocks(
    n: usize,
    silent: &[(u32, u32)],
    rises: &[(u32, u32)],
    initial: &[u32],
) -> Vec<u32> {
    let mut block = stuttering_blocks(n, silent, initial);
    if rises.is_empty() {
        return block;
    }
    loop {
        let mut signature: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); n];
        for &(x, y) in rises {
            signature[x as usize].insert(block[y as usize]);
        }
        let mut index: HashMap<(u32, &BTreeSet<u32>), u32> = HashMap::new();
        let refined: Vec<u32> = (0..n)
            .map(|i| {
                let len = index.len() as u32;
                *index.entry((block[i], &signature[i])).or_insert(len)
            })
            .collect();
        let before = block.iter().collect::<BTreeSet<_>>().len();
        if index.len() == before {
            return block;
        }
        block = stuttering_blocks(n, silent, &refined);
    }
}

/// Divergence-blind stuttering equivalence of a graph whose moves are all silent, refining
/// `initial`, the block of each state: the coarsest partition in which, for any two blocks
/// B and B', either every state of B reaches B' through B or none does. Returns each state's
/// block.
pub(super) fn stuttering_blocks(n: usize, edges: &[(u32, u32)], initial: &[u32]) -> Vec<u32> {
    let mut block: Vec<u32> = initial.to_vec();
    let mut nblocks = block.iter().max().map_or(0, |b| *b as usize + 1);
    let inter: Vec<(u32, u32)> = edges.iter().copied().filter(|(a, b)| a != b).collect();
    let pred_pairs: Vec<(u32, u32)> = inter.iter().map(|&(a, b)| (b, a)).collect();
    let pred = Adj::new(n, &pred_pairs);
    let mut members: Vec<Vec<u32>> = vec![Vec::new(); nblocks];
    for (i, &b) in block.iter().enumerate() {
        members[b as usize].push(i as u32);
    }
    let mut marked = vec![false; n];
    loop {
        let mut changed = false;
        let mut splitter = 0;
        while splitter < nblocks {
            let mut stack: Vec<u32> = Vec::new();
            let mut touched: Vec<u32> = Vec::new();
            for &t in &members[splitter] {
                for &p in pred.of(t) {
                    if block[p as usize] != splitter as u32 && !marked[p as usize] {
                        marked[p as usize] = true;
                        stack.push(p);
                        touched.push(p);
                    }
                }
            }
            while let Some(x) = stack.pop() {
                for &p in pred.of(x) {
                    if block[p as usize] == block[x as usize] && !marked[p as usize] {
                        marked[p as usize] = true;
                        stack.push(p);
                        touched.push(p);
                    }
                }
            }
            let mut by_block: HashMap<u32, Vec<u32>> = HashMap::new();
            for &x in &touched {
                by_block.entry(block[x as usize]).or_default().push(x);
            }
            let mut split: Vec<(u32, Vec<u32>)> = by_block.into_iter().collect();
            split.sort_unstable();
            for (b, xs) in split {
                if xs.len() < members[b as usize].len() {
                    let nb = nblocks as u32;
                    nblocks += 1;
                    for &x in &xs {
                        block[x as usize] = nb;
                    }
                    members.push(xs);
                    members[b as usize].retain(|m| block[*m as usize] == b);
                    changed = true;
                }
            }
            for &x in &touched {
                marked[x as usize] = false;
            }
            splitter += 1;
        }
        if !changed {
            return block;
        }
    }
}

/// Two partitions are the same up to the numbering of their blocks.
pub(super) fn same_partition(a: &[u32], b: &[u32]) -> bool {
    let mut ab: HashMap<u32, u32> = HashMap::new();
    let mut ba: HashMap<u32, u32> = HashMap::new();
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| *ab.entry(*x).or_insert(*y) == *y && *ba.entry(*y).or_insert(*x) == *x)
}
