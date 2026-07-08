//! CardDAV address object resources, a.k.a. cards (RFC 6352 §5.1).

pub mod create;
pub mod delete;
pub mod enumerate;
pub mod list;
pub mod multiget;
pub mod read;
mod types;
pub mod update;
mod utils;

#[doc(inline)]
pub use types::*;
#[doc(inline)]
pub use utils::*;
