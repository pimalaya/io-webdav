//! # NEON lanes
//!
//! [`Lanes`] on aarch64 builds enabling NEON, four bits a lane: the narrowing
//! shift that makes the mask is one instruction where a bit a lane takes two
//! horizontal adds.

use core::arch::aarch64::*;

use crate::rfc4918::xml::lanes::{LaneMask, Lanes};

pub(crate) type Vector = uint8x16_t;
pub(crate) type Bits = u64;
pub(crate) const LANE_BITS: u32 = 4;

impl Lanes {
    #[inline(always)]
    pub(crate) fn new(block: &[u8; 16]) -> Self {
        // SAFETY: the build enables NEON, and `block` holds the 16 bytes
        // loaded.
        Self(unsafe { vld1q_u8(block.as_ptr()) })
    }

    /// The lanes equal to `byte`.
    #[inline(always)]
    pub(crate) fn eq(self, byte: u8) -> Self {
        // SAFETY: the build enables NEON.
        Self(unsafe { vceqq_u8(self.0, vdupq_n_u8(byte)) })
    }

    /// The lanes of a compare that hold.
    #[inline(always)]
    pub(crate) fn mask(self) -> LaneMask {
        // SAFETY: the build enables NEON.
        let nibbles = unsafe { vshrn_n_u16::<4>(vreinterpretq_u16_u8(self.0)) };
        // SAFETY: as above.
        LaneMask(unsafe { vget_lane_u64::<0>(vreinterpret_u64_u8(nibbles)) })
    }
}

#[inline(always)]
pub(crate) fn or(a: Vector, b: Vector) -> Vector {
    // SAFETY: the build enables NEON.
    unsafe { vorrq_u8(a, b) }
}
