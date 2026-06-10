//! CardDAV addressbook collections (RFC 6352 §5).

pub mod create;
pub mod delete;
pub mod home_set;
pub mod list;
mod types;
pub mod update;
mod utils;

#[doc(inline)]
pub use types::*;
#[doc(inline)]
pub use utils::*;
