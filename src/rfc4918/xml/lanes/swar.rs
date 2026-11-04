//! # SWAR lanes
//!
//! [`Lanes`] on builds without SSE2 or NEON, 32-bit ARM among them: two
//! 64-bit words compared byte by byte at once, the high bit of each byte its
//! lane. A borrow can mark the lanes past a match, never the ones before, so
//! the first lane is exact.

use crate::rfc4918::xml::lanes::{LaneMask, Lanes};

pub(crate) type Vector = [u64; 2];
pub(crate) type Bits = u128;
pub(crate) const LANE_BITS: u32 = 8;

const ONES: u64 = 0x0101_0101_0101_0101;
const HIGHS: u64 = 0x8080_8080_8080_8080;

impl Lanes {
    #[inline(always)]
    pub(crate) fn new(block: &[u8; 16]) -> Self {
        let (low, high) = block.split_at(8);
        let low = u64::from_le_bytes(low.try_into().unwrap());
        let high = u64::from_le_bytes(high.try_into().unwrap());
        Self([low, high])
    }

    /// The lanes equal to `byte`.
    #[inline(always)]
    pub(crate) fn eq(self, byte: u8) -> Self {
        let [low, high] = self.0;
        let bytes = ONES * u64::from(byte);
        Self([zero_bytes(low ^ bytes), zero_bytes(high ^ bytes)])
    }

    /// The lanes of a compare that hold.
    #[inline(always)]
    pub(crate) fn mask(self) -> LaneMask {
        LaneMask(u128::from(self.0[0]) | u128::from(self.0[1]) << 64)
    }
}

#[inline(always)]
pub(crate) fn or(a: Vector, b: Vector) -> Vector {
    [a[0] | b[0], a[1] | b[1]]
}

/// The high bit of each zero byte of `word`.
#[inline(always)]
fn zero_bytes(word: u64) -> u64 {
    word.wrapping_sub(ONES) & !word & HIGHS
}
