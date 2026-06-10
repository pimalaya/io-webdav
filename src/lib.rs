#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]

extern crate alloc;
#[cfg(feature = "client")]
extern crate std;

#[cfg(feature = "client")]
pub mod client;
pub mod coroutine;
#[cfg(feature = "rfc4791")]
pub mod rfc4791;
pub mod rfc4918;
pub mod rfc5397;
#[cfg(feature = "rfc6352")]
pub mod rfc6352;
pub mod rfc6764;
