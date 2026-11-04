//! # Lanes
//!
//! The 16-byte compares the [scanner](crate::rfc4918::xml) probes with: SSE2
//! on x86 and x86_64 and NEON on aarch64 when the build enables them, as
//! every hosted target does, two 64-bit words elsewhere. Each compare is
//! written out at its call site, so the probes stay branch-free whatever the
//! optimisation level.
//!
//! A mask is only ever read for its first lane, the one every backend reports
//! exactly.

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
mod neon;
#[cfg(all(
    any(target_arch = "x86", target_arch = "x86_64"),
    target_feature = "sse2"
))]
mod sse2;
#[cfg(not(any(
    all(target_arch = "aarch64", target_feature = "neon"),
    all(
        any(target_arch = "x86", target_arch = "x86_64"),
        target_feature = "sse2"
    )
)))]
mod swar;

use core::ops::BitOr;

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
use crate::rfc4918::xml::lanes::neon::{Bits, LANE_BITS, Vector, or};
#[cfg(all(
    any(target_arch = "x86", target_arch = "x86_64"),
    target_feature = "sse2"
))]
use crate::rfc4918::xml::lanes::sse2::{Bits, LANE_BITS, Vector, or};
#[cfg(not(any(
    all(target_arch = "aarch64", target_feature = "neon"),
    all(
        any(target_arch = "x86", target_arch = "x86_64"),
        target_feature = "sse2"
    )
)))]
use crate::rfc4918::xml::lanes::swar::{Bits, LANE_BITS, Vector, or};

/// Sixteen bytes, compared lane by lane.
#[derive(Clone, Copy)]
pub(crate) struct Lanes(Vector);

/// The lanes of a compare that hold, `LANE_BITS` bits each, lowest lane
/// first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LaneMask(Bits);

impl LaneMask {
    /// The first lane that holds.
    #[inline(always)]
    pub(crate) fn first(self) -> Option<usize> {
        (self.0 != 0).then(|| (self.0.trailing_zeros() / LANE_BITS) as usize)
    }

    /// The lanes before the first lane of `end`, all of them when it has
    /// none.
    #[inline(always)]
    pub(crate) fn before(self, end: Self) -> Self {
        Self(self.0 & end.0.wrapping_sub(1) & !end.0)
    }
}

impl BitOr for Lanes {
    type Output = Self;

    #[inline(always)]
    fn bitor(self, other: Self) -> Self {
        Self(or(self.0, other.0))
    }
}

#[cfg(test)]
mod tests {
    use crate::rfc4918::xml::lanes::*;

    #[test]
    fn masks_find_every_byte_at_every_lane() {
        for byte in 0..=255 {
            for lane in 0..16 {
                let mut block = [b'x'; 16];
                block[lane] = byte;
                block[15 - lane] = b':';

                let lanes = Lanes::new(&block);
                let found = (lanes.eq(b'<') | lanes.eq(byte) | lanes.eq(0xff)).mask();
                let want = block
                    .iter()
                    .position(|&b| b == b'<' || b == byte || b == 0xff);
                assert_eq!(found.first(), want, "{byte} at {lane}");

                let colons = lanes.eq(b':').mask().before(lanes.eq(byte).mask());
                let end = block.iter().position(|&b| b == byte).unwrap_or(16);
                let want = block[..end].iter().position(|&b| b == b':');
                assert_eq!(colons.first(), want, "{byte} at {lane}");
            }
        }
    }
}
