//! States packed into bits at the store boundary (#296 item 2.3, #207 step 3). A state is one
//! byte per cell, each cell holding a value under its domain's size; the store keeps each cell
//! in the fewest bits its domain needs, `ceil(log2(size))`, end to end, so a state of 30 to 60
//! bytes is kept in 6 to 12. Everything outside the store reads the bytes, as before: `Bfs`
//! packs a state as it admits it, after #212's canonical form is computed on the bytes, and a
//! reader unpacks the states it reads, or reads one cell (`Packing::cell`).
//!
//! A state of up to `FAST_CELLS` cells packs and unpacks through 64-bit words, each cell at a
//! bit offset worked out once, with no branch that depends on the widths (`pack_words`,
//! `unpack_words`). A wider one goes cell by cell (`pack_into`, `unpack_into`). Kani proves
//! both round trips at every width from 0 to 8 bits.

use crate::lifecycle::TransitionSystem;

/// The most cells a state packs through words: every sample's state is narrower.
pub(crate) const FAST_CELLS: usize = 96;

/// The 64-bit words `FAST_CELLS` cells of 8 bits take, and one more that a cell's high bits
/// spill into.
const WORDS: usize = FAST_CELLS * 8 / 64 + 1;

/// The bits of each cell of a state, and where each cell's bits start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Packing {
    /// Per cell: its bits, 0 to 8.
    bits: Box<[u8]>,
    /// Per cell: the bit its value starts at, the bits of the cells before it summed. A `u32`:
    /// a state of 255 rows of 34 Int columns already passes bit 65,535, and a `u16` that
    /// saturated there wrote every later cell to one place (P50).
    offs: Box<[u32]>,
    /// The bytes a packed state takes.
    bytes: usize,
}

/// The bits a domain of `size` values needs: 0 for one value, 8 for 256 or more.
pub(crate) fn bits_for(size: usize) -> u8 {
    match size {
        0 | 1 => 0,
        s => (usize::BITS - (s - 1).leading_zeros()).min(8) as u8,
    }
}

/// Per cell of `ts`'s states: its domain's size.
fn cell_sizes(ts: &TransitionSystem) -> Vec<usize> {
    let mut sizes = vec![256; ts.width];
    for v in ts.var_ids() {
        let decl = ts.var(v);
        let at = ts.offset(v);
        let size = decl.domain.size(ts);
        sizes[at..at + ts.slots_of(&decl.kind)].fill(size);
    }
    sizes
}

/// The bits a packed state of `ts` takes, summed in a `u64`: what the build holds to
/// `MOST_PACKED_BITS` (`StateTooWide`) before any `Packing::of` would.
pub(crate) fn packed_bits(ts: &TransitionSystem) -> u64 {
    cell_sizes(ts)
        .into_iter()
        .map(|s| u64::from(bits_for(s)))
        .sum()
}

/// The most bits a packed state addresses: each cell's bit offset is a `u32` (P50).
pub(crate) const MOST_PACKED_BITS: u64 = u32::MAX as u64;

/// Per cell of `bits`: the bit it starts at. Panics, in a release build too, when a state
/// has more bits than a `u32` counts, rather than placing two cells at one offset.
fn offsets(bits: &[u8]) -> Box<[u32]> {
    let mut at = 0u32;
    bits.iter()
        .map(|&b| {
            let o = at;
            at = at
                .checked_add(u32::from(b))
                .expect("a packed state has at most u32::MAX bits");
            o
        })
        .collect()
}

impl Packing {
    /// The packing of states whose cells have domains of `sizes` values, in order.
    pub(crate) fn new(sizes: impl IntoIterator<Item = usize>) -> Self {
        let bits: Box<[u8]> = sizes.into_iter().map(bits_for).collect();
        let total: usize = bits.iter().map(|&b| usize::from(b)).sum();
        Packing {
            offs: offsets(&bits),
            bits,
            bytes: total.div_ceil(8),
        }
    }

    /// The packing of `ts`'s states: each var's slots in the bits of its domain's size.
    pub(crate) fn of(ts: &TransitionSystem) -> Self {
        Packing::new(cell_sizes(ts))
    }

    /// The bytes of an unpacked state.
    pub(crate) fn width(&self) -> usize {
        self.bits.len()
    }

    /// The bytes of a packed state.
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    /// Whether a state packs through words. Under Kani it never does: the `packing`
    /// harnesses prove the word path equal to the one-cell-at-a-time path, and the `States`
    /// harness over the word path took 83 GB and 9 minutes where it takes 30 s over the other.
    #[inline]
    fn by_words(&self) -> bool {
        self.width() <= FAST_CELLS && !cfg!(kani)
    }

    /// `state` packed into `out`, which is `bytes()` long. Panics, in a release build too, when
    /// a cell holds a value its bits cannot: two states would pack alike.
    #[inline]
    pub(crate) fn pack(&self, state: &[u8], out: &mut [u8]) {
        let over = match self.by_words() {
            true => pack_words(&self.bits, state, out),
            false => pack_into(&self.bits, state, out),
        };
        if over != 0 {
            self.outside(state);
        }
    }

    #[cold]
    #[inline(never)]
    fn outside(&self, state: &[u8]) -> ! {
        panic!(
            "a state cell holds a value outside its domain: {state:?}, bits {:?}",
            self.bits
        );
    }

    /// The state `packed` holds, into `out`, which is `width()` long.
    #[inline]
    pub(crate) fn unpack(&self, packed: &[u8], out: &mut [u8]) {
        match self.by_words() {
            true => unpack_words(&self.bits, packed, out),
            false => unpack_into(&self.bits, packed, out),
        }
    }

    /// `to` packed into `out`, given `from` and `from_packed`, what `pack` made of it: `out`
    /// starts as `from_packed`, and only the cells where `to` differs from `from` are written,
    /// so a move that changes a few cells costs a few writes, not a whole pack. Panics as
    /// `pack` does when a changed cell holds a value its bits cannot.
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn patch(&self, from: &[u8], from_packed: &[u8], to: &[u8], out: &mut [u8]) {
        debug_assert!(from.len() == self.width() && to.len() == self.width());
        out.copy_from_slice(from_packed);
        let mut over = 0u16;
        for (j, (a, b)) in from.chunks(8).zip(to.chunks(8)).enumerate() {
            let mut diff = word(a) ^ word(b);
            while diff != 0 {
                let c = j * 8 + (diff.trailing_zeros() >> 3) as usize;
                diff &= !(0xff << ((c & 7) * 8));
                let (bits, o) = (u32::from(self.bits[c]), self.offs[c] as usize);
                let v = u16::from(to[c]);
                over |= v >> bits;
                let (at, sh) = (o >> 3, o & 7);
                let m = ((1u16 << bits) - 1) << sh;
                let v = (v << sh) & m;
                // `m` covers at most 8 bits from `sh`; each `as u8` takes one byte of it, and the
                // byte `m >> 8` its high one.
                out[at] = (out[at] & !(m as u8)) | v as u8;
                if m > 0xff {
                    out[at + 1] = (out[at + 1] & !((m >> 8) as u8)) | (v >> 8) as u8;
                }
            }
        }
        if over != 0 {
            self.outside(to);
        }
    }

    /// Cell `c` of the state `packed` holds.
    /// The mask keeps the cell's `bits[c]` bits, at most 8, so the `as u8` drops only zeros.
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn cell(&self, packed: &[u8], c: usize) -> u8 {
        let o = self.offs[c] as usize;
        let byte = |i: usize| u16::from(packed.get(i).copied().unwrap_or(0));
        let two = byte(o >> 3) | (byte((o >> 3) + 1) << 8);
        ((two >> (o & 7)) & ((1u16 << self.bits[c]) - 1)) as u8
    }
}

/// Up to 8 bytes as a little-endian word, the missing high bytes 0.
#[inline]
fn word(bytes: &[u8]) -> u64 {
    let mut b = [0u8; 8];
    b[..bytes.len()].copy_from_slice(bytes);
    u64::from_le_bytes(b)
}

/// `cells` packed into `out`, each in its `bits`, low bits first, through 64-bit
/// words: at most `FAST_CELLS` cells. Returns the OR of every value's bits past its width: 0
/// when every value fits.
#[inline]
// Each `as u8` keeps one byte by design: a masked cell of at most 8 bits, the low byte of
// the accumulator a store writes, or the OR of `v >> b` over cells `v` of a byte.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn pack_words(bits: &[u8], cells: &[u8], out: &mut [u8]) -> u8 {
    debug_assert!(cells.len() <= FAST_CELLS && bits.len() == cells.len());
    // The word being filled stays in a register: a store per word, not per cell.
    let mut words = [0u64; WORDS];
    let (mut acc, mut held, mut w, mut over) = (0u64, 0u32, 0usize, 0u64);
    for (&b, &v) in bits.iter().zip(cells) {
        let v = u64::from(v);
        over |= v >> b;
        acc |= v << held;
        held += u32::from(b);
        if held >= 64 {
            words[w] = acc;
            w += 1;
            held -= 64;
            // The bits of `v` past the word; `held` was at least 56 before `b` was added.
            acc = (v >> 1) >> (u32::from(b) - held - 1);
        }
    }
    words[w] = acc;
    for (chunk, w) in out.chunks_mut(8).zip(&words) {
        chunk.copy_from_slice(&w.to_le_bytes()[..chunk.len()]);
    }
    over as u8
}

/// The cells `packed` holds, each in its `bits`, low bits first, into `out`, through 64-bit
/// words: at most `FAST_CELLS` cells.
#[inline]
// Each `as u8` keeps one byte by design: a masked cell of at most 8 bits, the low byte of
// the accumulator a store writes, or the OR of `v >> b` over cells `v` of a byte.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn unpack_words(bits: &[u8], packed: &[u8], out: &mut [u8]) {
    debug_assert!(out.len() <= FAST_CELLS && bits.len() == out.len());
    let mut words = [0u64; WORDS];
    for (w, chunk) in words.iter_mut().zip(packed.chunks(8)) {
        let mut b = [0u8; 8];
        b[..chunk.len()].copy_from_slice(chunk);
        *w = u64::from_le_bytes(b);
    }
    // The word being read stays in a register, `held` of its bits not yet read.
    let (mut acc, mut held, mut w) = (words[0], 64u32, 1usize);
    for (&b, cell) in bits.iter().zip(out.iter_mut()) {
        let b = u32::from(b);
        let mask = (1u64 << b) - 1;
        if held < b {
            // The cell's high bits start the next word.
            let next = words[w];
            w += 1;
            *cell = ((acc | (next << held)) & mask) as u8;
            acc = next >> (b - held);
            held += 64 - b;
        } else {
            *cell = (acc & mask) as u8;
            acc >>= b;
            held -= b;
        }
    }
}

/// `cells` packed into `out`, each in its `bits`, low bits first, one cell at a time. Returns
/// the OR of every value's bits past its width: 0 when every value fits.
// Each `as u8` keeps one byte by design: a masked cell of at most 8 bits, the low byte of
// the accumulator a store writes, or the OR of `v >> b` over cells `v` of a byte.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn pack_into(bits: &[u8], cells: &[u8], out: &mut [u8]) -> u8 {
    debug_assert_eq!(bits.len(), cells.len());
    let (mut acc, mut held, mut at, mut over) = (0u32, 0u32, 0usize, 0u8);
    for (&b, &v) in bits.iter().zip(cells) {
        let b = u32::from(b);
        over |= (u32::from(v) >> b) as u8;
        acc |= u32::from(v) << held;
        held += b;
        while held >= 8 {
            out[at] = acc as u8;
            acc >>= 8;
            held -= 8;
            at += 1;
        }
    }
    if held > 0 {
        out[at] = acc as u8;
    }
    over
}

/// The cells `packed` holds, each in its `bits`, into `out`, one cell at a time.
// Each `as u8` keeps one byte by design: a masked cell of at most 8 bits, the low byte of
// the accumulator a store writes, or the OR of `v >> b` over cells `v` of a byte.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn unpack_into(bits: &[u8], packed: &[u8], out: &mut [u8]) {
    debug_assert_eq!(bits.len(), out.len());
    let (mut acc, mut held, mut at) = (0u32, 0u32, 0usize);
    for (&b, cell) in bits.iter().zip(out.iter_mut()) {
        let b = u32::from(b);
        while held < b {
            acc |= u32::from(packed[at]) << held;
            at += 1;
            held += 8;
        }
        *cell = (acc & ((1u32 << b) - 1)) as u8;
        acc >>= b;
        held -= b;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every state of cells of every size from 1 to 256 reads back from its packed bytes, whole
    /// and cell by cell, by words and one cell at a time alike, and packs into the bytes the
    /// bits add up to.
    #[test]
    fn a_state_reads_back_at_every_domain_size() {
        for size in 1..=256usize {
            let sizes: Vec<usize> = (0..40).map(|i| [size, 3, size, 2, 256, 1][i % 6]).collect();
            let p = Packing::new(sizes.iter().copied());
            let state: Vec<u8> = sizes.iter().map(|&s| (s - 1) as u8).collect();
            let mut packed = vec![0u8; p.bytes()];
            p.pack(&state, &mut packed);
            let mut slow = vec![0u8; p.bytes()];
            assert_eq!(pack_into(&p.bits, &state, &mut slow), 0);
            assert_eq!(packed, slow, "size {size}");
            let mut back = vec![0u8; p.width()];
            p.unpack(&packed, &mut back);
            assert_eq!(back, state, "size {size}");
            for (c, &v) in state.iter().enumerate() {
                assert_eq!(p.cell(&packed, c), v, "size {size}, cell {c}");
            }
            // Patching from another state of the domains gives what packing gives.
            let other: Vec<u8> = state
                .iter()
                .enumerate()
                .map(|(c, &v)| v / (c as u8 % 3 + 1))
                .collect();
            let mut want = vec![0u8; p.bytes()];
            p.pack(&other, &mut want);
            let mut got = vec![0u8; p.bytes()];
            p.patch(&state, &packed, &other, &mut got);
            assert_eq!(got, want, "size {size}");
            p.patch(&other, &want, &state, &mut got);
            assert_eq!(got, packed, "size {size}");
            let bits: usize = sizes.iter().map(|&s| usize::from(bits_for(s))).sum();
            assert_eq!(p.bytes(), bits.div_ceil(8));
        }
    }

    /// A state of more than `FAST_CELLS` cells packs one cell at a time and reads back.
    #[test]
    fn a_wide_state_reads_back() {
        let sizes: Vec<usize> = (0..FAST_CELLS + 9).map(|i| i % 7 + 1).collect();
        let p = Packing::new(sizes.iter().copied());
        let state: Vec<u8> = sizes.iter().map(|&s| (s - 1) as u8).collect();
        let mut packed = vec![0u8; p.bytes()];
        p.pack(&state, &mut packed);
        let mut back = vec![0u8; p.width()];
        p.unpack(&packed, &mut back);
        assert_eq!(back, state);
    }

    /// Past bit 65,535 (P50): a cell starts where the bits before it end, and patching and
    /// reading one cell agree with packing and unpacking the whole state, on either side of
    /// the first cell that starts past `u16::MAX`. 8,192 cells of 8 bits end at bit 65,536,
    /// so the 8,193rd is the first past it; the cells of 3 bits after the 8-bit ones start at
    /// offsets that are not byte-aligned.
    #[test]
    fn a_state_past_bit_65535_packs_patches_and_reads_back() {
        for (eights, threes) in [(8_191, 0), (8_192, 0), (8_192, 1), (8_193, 0), (8_190, 9)] {
            let sizes: Vec<usize> = (0..eights)
                .map(|_| 256)
                .chain((0..threes).map(|_| 8))
                .collect();
            let p = Packing::new(sizes.iter().copied());
            let from: Vec<u8> = sizes
                .iter()
                .enumerate()
                .map(|(c, &s)| (c % s) as u8)
                .collect();
            // Every cell changes, the last ones included.
            let to: Vec<u8> = sizes
                .iter()
                .enumerate()
                .map(|(c, &s)| ((c + 1) % s) as u8)
                .collect();
            let (mut a, mut b, mut got) = (
                vec![0u8; p.bytes()],
                vec![0u8; p.bytes()],
                vec![0u8; p.bytes()],
            );
            p.pack(&from, &mut a);
            p.pack(&to, &mut b);
            p.patch(&from, &a, &to, &mut got);
            assert!(got == b, "{eights} + {threes} cells: a patch is the pack");
            let mut back = vec![0u8; p.width()];
            p.unpack(&got, &mut back);
            assert!(
                back == to,
                "{eights} + {threes} cells: the patched state reads back"
            );
            for (c, &v) in to.iter().enumerate() {
                assert_eq!(p.cell(&b, c), v, "{eights} + {threes} cells: cell {c}");
            }
            let mut at = 0usize;
            for (c, &b) in p.bits.iter().enumerate() {
                assert_eq!(
                    p.offs[c] as usize, at,
                    "cell {c} starts where the cells before end"
                );
                at += usize::from(b);
            }
        }
    }

    /// A value outside its domain's bits is caught at pack time.
    #[test]
    #[should_panic(expected = "outside its domain")]
    fn a_value_past_its_bits_panics() {
        let p = Packing::new([3, 3]);
        let mut packed = vec![0u8; p.bytes()];
        p.pack(&[1, 4], &mut packed);
    }
}

/// Kani harnesses (`cargo kani -p rdra-ish-core`).
#[cfg(kani)]
mod proofs {
    use super::*;

    /// Packs `cells` in `bits` by words and one cell at a time, and checks the two alike, a
    /// value past its bits reported exactly when there is one, only the bytes the widths add
    /// up to written, and, when every value fits, the cells read back whole either way and
    /// one at a time.
    fn round_trip<const N: usize>(bits: [u8; N], cells: [u8; N]) {
        kani::assume(bits.iter().all(|&b| b <= 8));
        let fits = bits
            .iter()
            .zip(&cells)
            .all(|(&b, &v)| u32::from(v) >> b == 0);
        let bytes = (bits.iter().map(|&b| usize::from(b)).sum::<usize>() + 7) >> 3;
        let mut slow = [0u8; N];
        let mut fast = [0u8; N];
        assert_eq!(pack_into(&bits, &cells, &mut slow[..bytes]) == 0, fits);
        assert_eq!(pack_words(&bits, &cells, &mut fast[..bytes]) == 0, fits);
        if fits {
            assert_eq!(slow, fast);
            let mut back = [0u8; N];
            unpack_into(&bits, &slow[..bytes], &mut back);
            assert_eq!(back, cells);
            let mut back = [0u8; N];
            unpack_words(&bits, &fast[..bytes], &mut back);
            assert_eq!(back, cells);
            let p = Packing {
                offs: offsets(&bits),
                bits: Box::new(bits),
                bytes,
            };
            let c: usize = kani::any();
            kani::assume(c < N);
            assert_eq!(p.cell(&fast[..bytes], c), cells[c]);
        }
    }

    /// For four cells of any widths from 0 to 8 bits and any values, the round trip holds:
    /// two states pack alike only when they are equal, at every domain size.
    #[kani::proof]
    #[kani::unwind(15)]
    fn pack_then_unpack_is_the_state() {
        round_trip::<4>(kani::any(), kani::any());
    }

    /// For two states of seven 8-bit cells and four of any widths from 0 to 8 bits, both
    /// fitting: patching the first's packed bytes with the cells where the second differs
    /// gives the second's packed bytes, across bit 64 too.
    #[kani::proof]
    #[kani::unwind(15)]
    fn a_patch_is_the_pack() {
        let mut bits = [8u8; 11];
        for b in &mut bits[7..] {
            *b = kani::any();
        }
        kani::assume(bits.iter().all(|&b| b <= 8));
        let (from, to): ([u8; 11], [u8; 11]) = (kani::any(), kani::any());
        let fits = |s: &[u8; 11]| bits.iter().zip(s).all(|(&b, &v)| u32::from(v) >> b == 0);
        kani::assume(fits(&from) && fits(&to));
        let bytes = (bits.iter().map(|&b| usize::from(b)).sum::<usize>() + 7) >> 3;
        let p = Packing {
            offs: offsets(&bits),
            bits: Box::new(bits),
            bytes,
        };
        let (mut a, mut b, mut got) = ([0u8; 11], [0u8; 11], [0u8; 11]);
        p.pack(&from, &mut a[..bytes]);
        p.pack(&to, &mut b[..bytes]);
        p.patch(&from, &a[..bytes], &to, &mut got[..bytes]);
        assert_eq!(got, b);
    }

    /// Past the first 64-bit word: seven 8-bit cells and four of any widths from 0 to 8 bits,
    /// so a cell starts, ends or straddles bit 64 at every place it can, and the round trip
    /// holds there too.
    #[kani::proof]
    #[kani::unwind(15)]
    fn a_cell_across_a_word_reads_back() {
        let mut bits = [8u8; 11];
        for b in &mut bits[7..] {
            *b = kani::any();
        }
        round_trip::<11>(bits, kani::any());
    }
}
