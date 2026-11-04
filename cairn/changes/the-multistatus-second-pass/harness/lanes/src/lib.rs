//! # lanes
//!
//! The scanner and its vector backends (src/rfc4918/xml.rs and xml/lanes),
//! mounted through symlinks to run their unit tests on other targets and
//! under Miri, the crate's dev-dependencies linking for the host only.

#![no_std]
#![cfg_attr(not(test), allow(dead_code))]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod rfc4918 {
    pub mod xml;
}
