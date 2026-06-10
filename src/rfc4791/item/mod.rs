//! CalDAV calendar object resources, a.k.a. items (RFC 4791 §4.1).

pub mod create;
pub mod delete;
pub mod list;
pub mod read;
mod types;
pub mod update;
mod utils;

#[doc(inline)]
pub use types::*;
#[doc(inline)]
pub use utils::*;
