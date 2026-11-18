//! # PROPFIND bodies
//!
//! [`parse_propfind`] reads the body of a `PROPFIND` request (RFC 4918
//! §9.1) into the [`WebdavPropSelection`] it asks for.

use alloc::vec::Vec;

use crate::{
    rfc4918::DAV,
    server::body::{WebdavBodyParseError, WebdavPropSelection, parse_tree},
};

/// Parses a `PROPFIND` request body (RFC 4918 §14.20), an empty one asking
/// for every property (§9.1).
pub fn parse_propfind(body: &[u8]) -> Result<WebdavPropSelection, WebdavBodyParseError> {
    let Some(tree) = parse_tree(body)? else {
        return Ok(WebdavPropSelection::AllProp(Vec::new()));
    };

    tree.expect_root(DAV.uri, "propfind")?;
    tree.selection(&tree.root)
        .ok_or(WebdavBodyParseError::Missing("prop"))
}
