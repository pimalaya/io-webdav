//! # MKCOL and MKCALENDAR bodies
//!
//! [`parse_mkcol`] reads the body of an extended `MKCOL` (RFC 5689 §3) and
//! [`parse_mkcalendar`] the body of a `MKCALENDAR` (RFC 4791 §5.3.1), both
//! into the properties the new collection is created with.

use alloc::vec::Vec;

use crate::{
    rfc4791::calendar::CALDAV,
    rfc4918::DAV,
    server::body::{WebdavBodyParseError, WebdavPropElement, parse_tree},
};

/// Parses a `MKCOL` request body into the properties it sets, the
/// `DAV:resourcetype` included; an empty body sets none.
pub fn parse_mkcol(body: &[u8]) -> Result<Vec<WebdavPropElement>, WebdavBodyParseError> {
    let Some(tree) = parse_tree(body)? else {
        return Ok(Vec::new());
    };

    tree.expect_root(DAV.uri, "mkcol")?;
    Ok(tree.set_props(&tree.root))
}

/// Parses a `MKCALENDAR` request body into the properties it sets; an empty
/// body sets none.
pub fn parse_mkcalendar(body: &[u8]) -> Result<Vec<WebdavPropElement>, WebdavBodyParseError> {
    let Some(tree) = parse_tree(body)? else {
        return Ok(Vec::new());
    };

    tree.expect_root(CALDAV.uri, "mkcalendar")?;
    Ok(tree.set_props(&tree.root))
}
