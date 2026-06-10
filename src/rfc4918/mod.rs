//! RFC 4918: HTTP Extensions for Web Distributed Authoring and
//! Versioning (WebDAV).
//!
//! <https://www.rfc-editor.org/rfc/rfc4918>

pub mod copy;
pub mod coroutine;
pub mod delete;
pub mod follow_redirects;
pub mod get;
pub mod mkcol;
pub mod move_;
pub mod options;
pub mod propfind;
pub mod proppatch;
pub mod put;
pub mod request;
pub mod send;
mod types;
mod utils;

#[doc(inline)]
pub use types::*;
#[doc(inline)]
pub use utils::*;
