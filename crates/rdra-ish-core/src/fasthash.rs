//! A fast hasher for the explorer's maps, whose keys are the model's own states and patterns.
//! The default `SipHash` resists chosen keys, which these are not, and costs more per byte.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// A `HashMap` that hashes with [`FastHasher`]. Its iteration order is not the default map's,
/// so nothing that reaches the output may iterate one.
pub(crate) type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

/// Multiply-rotate over 8-byte words (the FxHash scheme).
#[derive(Clone, Copy, Default)]
pub(crate) struct FastHasher(u64);

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

/// One step of the scheme: `h` with `word` mixed in, before `finish`'s rotation.
#[inline]
pub(crate) fn fx_step(h: u64, word: u64) -> u64 {
    (h.rotate_left(5) ^ word).wrapping_mul(SEED)
}

impl FastHasher {
    fn add(&mut self, word: u64) {
        self.0 = fx_step(self.0, word);
    }
}

impl Hasher for FastHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
        if !rest.is_empty() {
            let mut word = [0u8; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, n: u8) {
        self.add(u64::from(n));
    }

    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }

    /// The multiply leaves the low bits weak, and the table indexes by them.
    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }
}

#[cfg(test)]
mod tests {
    use super::FastMap;

    /// Keys that differ in one byte, at any position and of any length, stay distinct keys.
    #[test]
    fn keys_that_differ_in_one_byte_are_distinct() {
        let mut map: FastMap<Box<[u8]>, usize> = FastMap::default();
        let mut n = 0;
        for len in 0..20usize {
            for at in 0..len.max(1) {
                for value in 0..4u8 {
                    let mut key = vec![0u8; len];
                    if let Some(byte) = key.get_mut(at) {
                        *byte = value;
                    }
                    if map.insert(key.into(), n).is_none() {
                        n += 1;
                    }
                }
            }
        }
        // Per length: the all-zero key, and three more values at each position.
        let distinct: usize = (0..20usize).map(|len| 1 + 3 * len).sum();
        assert_eq!(map.len(), distinct);
        assert_eq!(map.get(&[0u8, 0, 3][..]), map.get(&[0u8, 0, 3][..]));
        assert!(map.contains_key(&[0u8; 19][..]));
        assert!(!map.contains_key(&[9u8; 19][..]));
    }
}
