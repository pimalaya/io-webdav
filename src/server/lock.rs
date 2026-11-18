//! # LOCK bodies
//!
//! [`parse_lockinfo`] reads the body of a `LOCK` request (RFC 4918 §9.10)
//! into the lock it asks for. [`WebdavActiveLock`] is a lock granted, as
//! `DAV:lockdiscovery` reports it and a `LOCK` answers with it.

use alloc::{format, string::String};

use crate::{
    rfc4918::{DAV, XML_DECL, escape_text},
    server::{
        body::{WebdavBodyParseError, parse_tree},
        header::{WebdavDepth, WebdavTimeout},
    },
};

/// The `DAV:lockinfo` of a `LOCK` request (RFC 4918 §14.11), a write lock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebdavLockInfo {
    /// Whether the lock is exclusive or shared.
    pub scope: WebdavLockScope,
    /// The `DAV:owner` element as sent, declaring every namespace it
    /// inherited, served back verbatim.
    pub owner: Option<String>,
}

/// The `DAV:lockscope` of a lock (RFC 4918 §14.13).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebdavLockScope {
    /// `DAV:exclusive`.
    Exclusive,
    /// `DAV:shared`.
    Shared,
}

/// A granted lock (RFC 4918 §14.1).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebdavActiveLock {
    /// Whether the lock is exclusive or shared.
    pub scope: WebdavLockScope,
    /// How deep the lock reaches: zero or infinity.
    pub depth: WebdavDepth,
    /// The `DAV:owner` element, verbatim.
    pub owner: Option<String>,
    /// How long the lock lasts.
    pub timeout: Option<WebdavTimeout>,
    /// The lock token, a URI such as `urn:uuid:...`.
    pub token: String,
    /// The href of the locked resource.
    pub root: String,
}

impl WebdavLockScope {
    /// The element local name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Exclusive => "exclusive",
            Self::Shared => "shared",
        }
    }
}

impl WebdavActiveLock {
    /// Writes the `DAV:activelock` element.
    pub fn to_xml(&self) -> String {
        let scope = self.scope.as_str();
        let depth = self.depth.as_str();
        let mut xml = format!(
            "<D:activelock><D:locktype><D:write/></D:locktype>\
             <D:lockscope><D:{scope}/></D:lockscope><D:depth>{depth}</D:depth>"
        );

        if let Some(owner) = &self.owner {
            xml.push_str(owner);
        }

        if let Some(timeout) = &self.timeout {
            xml.push_str(&format!("<D:timeout>{timeout}</D:timeout>"));
        }

        let token = escape_text(&self.token);
        let root = escape_text(&self.root);
        xml.push_str(&format!(
            "<D:locktoken><D:href>{token}</D:href></D:locktoken>\
             <D:lockroot><D:href>{root}</D:href></D:lockroot></D:activelock>"
        ));
        xml
    }

    /// Writes the body a successful `LOCK` answers with (RFC 4918 §9.10.1):
    /// the `DAV:lockdiscovery` of this lock.
    pub fn lock_body(&self) -> String {
        format!(
            "{XML_DECL}<D:prop xmlns:D=\"DAV:\"><D:lockdiscovery>{}</D:lockdiscovery></D:prop>",
            self.to_xml()
        )
    }
}

/// Parses a `LOCK` request body, [`None`] for the empty body of a lock
/// refresh (RFC 4918 §9.10.2).
pub fn parse_lockinfo(body: &[u8]) -> Result<Option<WebdavLockInfo>, WebdavBodyParseError> {
    let Some(tree) = parse_tree(body)? else {
        return Ok(None);
    };

    tree.expect_root(DAV.uri, "lockinfo")?;
    let root = &tree.root;

    let scope = root
        .dav("lockscope")
        .and_then(|scope| scope.children.first())
        .ok_or(WebdavBodyParseError::Missing("lockscope"))?;
    let scope = match scope {
        scope if scope.is_dav("exclusive") => WebdavLockScope::Exclusive,
        scope if scope.is_dav("shared") => WebdavLockScope::Shared,
        scope => {
            return Err(WebdavBodyParseError::Invalid {
                element: "lockscope",
                value: scope.local.into(),
            });
        }
    };

    let kind = root
        .dav("locktype")
        .and_then(|kind| kind.children.first())
        .ok_or(WebdavBodyParseError::Missing("locktype"))?;
    if !kind.is_dav("write") {
        return Err(WebdavBodyParseError::Invalid {
            element: "locktype",
            value: kind.local.into(),
        });
    }

    let owner = root.dav("owner").map(|owner| tree.raw(owner));
    Ok(Some(WebdavLockInfo { scope, owner }))
}
