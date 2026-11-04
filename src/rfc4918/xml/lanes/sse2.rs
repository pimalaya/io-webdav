//! # SSE2 lanes
//!
//! [`Lanes`] on x86 and x86_64 builds enabling SSE2, a bit a lane.

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

use crate::rfc4918::xml::lanes::{LaneMask, Lanes};

pub(crate) type Vector = __m128i;
pub(crate) type Bits = u32;
pub(crate) const LANE_BITS: u32 = 1;

impl Lanes {
    #[inline(always)]
    pub(crate) fn new(block: &[u8; 16]) -> Self {
        // SAFETY: the build enables SSE2, and `block` holds the 16 bytes
        // loaded.
        Self(unsafe { _mm_loadu_si128(block.as_ptr().cast()) })
    }

    /// The lanes equal to `byte`.
    #[inline(always)]
    pub(crate) fn eq(self, byte: u8) -> Self {
        // SAFETY: the build enables SSE2.
        Self(unsafe { _mm_cmpeq_epi8(self.0, _mm_set1_epi8(byte as i8)) })
    }

    /// The lanes of a compare that hold.
    #[inline(always)]
    pub(crate) fn mask(self) -> LaneMask {
        // SAFETY: the build enables SSE2.
        LaneMask(unsafe { _mm_movemask_epi8(self.0) } as u32)
    }
}

#[inline(always)]
pub(crate) fn or(a: Vector, b: Vector) -> Vector {
    // SAFETY: the build enables SSE2.
    unsafe { _mm_or_si128(a, b) }
}
