//! States packed into bits at the store boundary (#296 item 2.3, #207 step 3). A state is one
//! byte per cell, each cell holding a value under its domain's size; the store keeps each cell
//! in the fewest bits its domain needs, `ceil(log2(size))`, end to end, so a state of 40 to 66
//! bytes is kept in 6 to 12. Everything outside the store reads the bytes, as before: `Bfs`
//! packs a state as it admits it, after #212's canonical form is computed on the bytes, and a
//! reader unpacks the states it reads.

use crate::lifecycle::TransitionSystem;

/// The bits of each cell of a state, and where each cell's bits start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Packing {
    /// Per cell: its bits, 0 to 8.
    bits: Box<[u8]>,
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

impl Packing {
    /// The packing of states whose cells have domains of `sizes` values, in order.
    pub(crate) fn new(sizes: impl IntoIterator<Item = usize>) -> Self {
        let bits: Box<[u8]> = sizes.into_iter().map(bits_for).collect();
        let total: usize = bits.iter().map(|&b| usize::from(b)).sum();
        Packing {
            bits,
            bytes: total.div_ceil(8),
        }
    }

    /// The packing of `ts`'s states: each var's slots in the bits of its domain's size.
    pub(crate) fn of(ts: &TransitionSystem) -> Self {
        let mut sizes = vec![256; ts.width];
        for v in ts.var_ids() {
            let decl = ts.var(v);
            let at = ts.offset(v);
            let size = decl.domain.size(ts);
            sizes[at..at + ts.slots_of(&decl.kind)].fill(size);
        }
        Packing::new(sizes)
    }

    /// The bytes of an unpacked state.
    pub(crate) fn width(&self) -> usize {
        self.bits.len()
    }

    /// The bytes of a packed state.
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    /// `state` packed into `out`, which is `bytes()` long. Panics, in a release build too, when
    /// a cell holds a value its bits cannot: two states would pack alike.
    pub(crate) fn pack(&self, state: &[u8], out: &mut [u8]) {
        let over = pack_into(&self.bits, state, out);
        assert!(
            over == 0,
            "a state cell holds a value outside its domain: {state:?}, bits {:?}",
            self.bits
        );
    }

    /// The state `packed` holds, into `out`, which is `width()` long.
    pub(crate) fn unpack(&self, packed: &[u8], out: &mut [u8]) {
        unpack_into(&self.bits, packed, out);
    }
}

/// `cells` packed into `out`, each in its `bits`, low bits first. Returns the OR of every
/// value's bits past its width: 0 when every value fits.
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

/// The cells `packed` holds, each in its `bits`, into `out`.
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

    /// Every state of cells of every size from 1 to 256 reads back from its packed bytes, and
    /// packs into the bytes the bits add up to.
    #[test]
    fn a_state_reads_back_at_every_domain_size() {
        for size in 1..=256usize {
            let sizes = [size, 3, size, 2, 256, size];
            let p = Packing::new(sizes);
            let state: Vec<u8> = sizes.iter().map(|&s| (s - 1) as u8).collect();
            let mut packed = vec![0u8; p.bytes()];
            p.pack(&state, &mut packed);
            let mut back = vec![0u8; p.width()];
            p.unpack(&packed, &mut back);
            assert_eq!(back, state, "size {size}");
            let bits: usize = sizes.iter().map(|&s| usize::from(bits_for(s))).sum();
            assert_eq!(p.bytes(), bits.div_ceil(8));
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

    /// For four cells of any widths from 0 to 8 bits and any values: packing reports a value
    /// past its bits exactly when there is one, writes only the bytes the widths add up to,
    /// and, when every value fits, unpacking gives the cells back. So two states pack alike
    /// only when they are equal, at every domain size.
    #[kani::proof]
    #[kani::unwind(6)]
    fn pack_then_unpack_is_the_state() {
        let bits: [u8; 4] = kani::any();
        let cells: [u8; 4] = kani::any();
        kani::assume(bits.iter().all(|&b| b <= 8));
        let fits = bits
            .iter()
            .zip(&cells)
            .all(|(&b, &v)| u32::from(v) >> b == 0);
        let bytes = (bits.iter().map(|&b| usize::from(b)).sum::<usize>() + 7) >> 3;
        let mut packed = [0u8; 4];
        let over = pack_into(&bits, &cells, &mut packed[..bytes]);
        assert_eq!(over == 0, fits);
        if fits {
            let mut back = [0u8; 4];
            unpack_into(&bits, &packed[..bytes], &mut back);
            assert_eq!(back, cells);
        }
    }
}
