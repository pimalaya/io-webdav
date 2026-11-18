//! # PROPPATCH bodies
//!
//! [`parse_proppatch`] reads the body of a `PROPPATCH` request (RFC 4918
//! §9.2) into its instructions, in document order, each set value kept as
//! the raw element a dead property is stored as.

use alloc::vec::Vec;

use crate::{
    rfc4918::DAV,
    server::{
        body::{WebdavBodyParseError, WebdavPropElement, XmlNode, parse_tree},
        name::WebdavPropName,
    },
};

/// One instruction of a `DAV:propertyupdate` (RFC 4918 §14.19).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebdavPropUpdate {
    /// `DAV:set`: the property takes the element's value.
    Set(WebdavPropElement),
    /// `DAV:remove`: the property is removed.
    Remove(WebdavPropName),
}

/// Parses a `PROPPATCH` request body into its instructions, to be applied in
/// order and all or none (RFC 4918 §9.2).
pub fn parse_proppatch(body: &[u8]) -> Result<Vec<WebdavPropUpdate>, WebdavBodyParseError> {
    let Some(tree) = parse_tree(body)? else {
        return Err(WebdavBodyParseError::Missing("propertyupdate"));
    };

    tree.expect_root(DAV.uri, "propertyupdate")?;

    let mut updates = Vec::new();
    for instruction in &tree.root.children {
        let set = instruction.is_dav("set");
        if !set && !instruction.is_dav("remove") {
            continue;
        }

        let props = instruction
            .children
            .iter()
            .filter(|prop| prop.is_dav("prop"))
            .flat_map(|prop| &prop.children);

        for prop in props {
            updates.push(match set {
                true => WebdavPropUpdate::Set(tree.prop_element(prop)),
                false => WebdavPropUpdate::Remove(XmlNode::prop_name(prop)),
            });
        }
    }

    Ok(updates)
}
