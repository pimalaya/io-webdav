//! # Request bodies
//!
//! The pieces every request body parser of [`crate::server`] shares: the
//! parse error, the property selection of a `PROPFIND` or `REPORT`, the
//! property element a `PROPPATCH`, `MKCOL` or `MKCALENDAR` sets, and the
//! namespace-aware element tree they are all read from.
//!
//! The tree is built over the scanner of the multistatus parsers, with every
//! prefix resolved to its namespace URI: a server matches names by namespace,
//! not by prefix. An element kept as raw XML is cut from the body as sent and
//! carries the namespace declarations it inherited, so it stands alone.

use core::str;

use alloc::{
    borrow::Cow,
    format,
    string::{String, ToString},
    vec::Vec,
};

use thiserror::Error;

use crate::{
    rfc4791::calendar::CALDAV,
    rfc4918::{
        DAV, escape_attr,
        xml::{XmlAttrs, XmlEvent, XmlScanner, attr, decode_attr, decode_text, same_name},
    },
    rfc6352::addressbook::CARDDAV,
    server::{
        name::{WebdavName, WebdavPropName},
        report::CaldavTimeRange,
    },
};

/// Deepest element nesting a request body parser accepts.
pub const BODY_MAX_DEPTH: usize = 64;

/// Failure causes while parsing a request body.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum WebdavBodyParseError {
    /// The body is not UTF-8.
    #[error("WebDAV request body is not UTF-8")]
    NotUtf8,
    /// The body is not one whole XML document.
    #[error("WebDAV request body is not one whole XML document")]
    Malformed,
    /// The body nests elements deeper than the given depth.
    #[error("WebDAV request body nests elements deeper than {0}")]
    TooDeep(usize),
    /// The body uses a namespace prefix it never declared.
    #[error("WebDAV request body uses the undeclared prefix {0}")]
    UnboundPrefix(String),
    /// The body is rooted at another element than the method takes.
    #[error("WebDAV request body is rooted at {{{}}}{}", .0.ns, .0.local)]
    UnexpectedRoot(WebdavName),
    /// A required element is missing.
    #[error("WebDAV request body misses the {0} element")]
    Missing(&'static str),
    /// An element or attribute holds a value the RFC does not allow.
    #[error("WebDAV request body holds an invalid {element}: {value}")]
    Invalid {
        /// The element or attribute.
        element: &'static str,
        /// The value found.
        value: String,
    },
}

/// The properties a `PROPFIND` or a `REPORT` asks for (RFC 4918 §14.20).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebdavPropSelection {
    /// `DAV:allprop`, plus the properties its `DAV:include` names.
    AllProp(Vec<WebdavPropName>),
    /// `DAV:propname`: the names alone.
    PropName,
    /// `DAV:prop`: the named properties.
    Prop(Vec<WebdavPropName>),
}

/// A property element as a `PROPPATCH`, `MKCOL` or `MKCALENDAR` sets it.
///
/// A live property reads its [`text`](Self::text) or its
/// [`children`](Self::children); a dead one is stored as its
/// [`xml`](Self::xml) and served back verbatim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebdavPropElement {
    /// The property name.
    pub name: WebdavPropName,
    /// The text directly under the element, decoded, CDATA included.
    pub text: String,
    /// The whole element as sent, declaring every namespace it inherited.
    pub xml: String,
    /// The direct child elements.
    pub children: Vec<WebdavChildElement>,
}

/// A direct child of a [`WebdavPropElement`], such as a resource type or a
/// component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebdavChildElement {
    /// The element name.
    pub name: WebdavName,
    /// The `name` attribute, which carries a component type (RFC 4791
    /// §5.2.3).
    pub name_attr: Option<String>,
}

/// The `calendar-data` or `address-data` element of a `REPORT` property
/// list, which may narrow what the server returns (RFC 4791 §9.6, RFC 6352
/// §10.4).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebdavDataRequest {
    /// The `content-type` attribute.
    pub content_type: Option<String>,
    /// The `version` attribute.
    pub version: Option<String>,
    /// The `C:expand` range of a `calendar-data` (RFC 4791 §9.6.5).
    pub expand: Option<CaldavTimeRange>,
    /// The whole element as sent, declaring every namespace it inherited.
    pub xml: String,
}

/// No declaration.
const NONE: usize = usize::MAX;

/// The XML namespace, bound to the `xml` prefix without a declaration.
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// An element of a request body, its name resolved.
#[derive(Debug)]
pub(crate) struct XmlNode<'a> {
    /// Namespace URI, empty for no namespace.
    pub(crate) ns: Cow<'a, str>,
    /// Local name.
    pub(crate) local: &'a str,
    /// Raw attribute area.
    attrs: &'a str,
    /// Text directly under the element, decoded.
    pub(crate) text: String,
    /// Child elements.
    pub(crate) children: Vec<XmlNode<'a>>,
    /// Offset of the start tag.
    start: usize,
    /// Offset past the qualified name of the start tag.
    name_end: usize,
    /// Offset past the end tag.
    end: usize,
    /// Innermost namespace declaration in scope.
    scope: usize,
}

/// A namespace declaration.
#[derive(Debug)]
struct XmlDecl<'a> {
    prefix: &'a str,
    uri: Cow<'a, str>,
    /// Start offset of the declaring element.
    owner: usize,
    /// The declaration it shadows the chain of.
    parent: usize,
}

/// A request body read into elements.
#[derive(Debug)]
pub(crate) struct XmlTree<'a> {
    body: &'a str,
    decls: Vec<XmlDecl<'a>>,
    pub(crate) root: XmlNode<'a>,
}

impl<'a> XmlNode<'a> {
    /// Whether the element is `local` in namespace `ns`.
    pub(crate) fn is(&self, ns: &str, local: &str) -> bool {
        self.local == local && self.ns == ns
    }

    /// Whether the element is `local` in the `DAV:` namespace.
    pub(crate) fn is_dav(&self, local: &str) -> bool {
        self.is(DAV.uri, local)
    }

    /// Whether the element is `local` in the CalDAV namespace.
    pub(crate) fn is_caldav(&self, local: &str) -> bool {
        self.is(CALDAV.uri, local)
    }

    /// Whether the element is `local` in the CardDAV namespace.
    pub(crate) fn is_carddav(&self, local: &str) -> bool {
        self.is(CARDDAV.uri, local)
    }

    /// The first child `local` in namespace `ns`.
    pub(crate) fn child(&self, ns: &str, local: &str) -> Option<&XmlNode<'a>> {
        self.children.iter().find(|child| child.is(ns, local))
    }

    /// The first child `local` in the `DAV:` namespace.
    pub(crate) fn dav(&self, local: &str) -> Option<&XmlNode<'a>> {
        self.child(DAV.uri, local)
    }

    /// The decoded value of the unprefixed attribute `name`.
    pub(crate) fn attr(&self, name: &str) -> Option<String> {
        attr(self.attrs, name).map(decode_attr)
    }

    /// The element name.
    pub(crate) fn name(&self) -> WebdavName {
        WebdavName {
            ns: self.ns.to_string(),
            local: self.local.to_string(),
        }
    }

    /// The element name, read as a property name.
    pub(crate) fn prop_name(&self) -> WebdavPropName {
        WebdavPropName::new(&self.ns, self.local)
    }

    /// The text, trimmed.
    pub(crate) fn trimmed(&self) -> &str {
        self.text.trim()
    }
}

impl<'a> XmlTree<'a> {
    /// Fails unless the root is `local` in namespace `ns`.
    pub(crate) fn expect_root(&self, ns: &str, local: &str) -> Result<(), WebdavBodyParseError> {
        if self.root.is(ns, local) {
            return Ok(());
        }

        Err(WebdavBodyParseError::UnexpectedRoot(self.root.name()))
    }

    /// The element as sent, every namespace it inherited declared on it.
    pub(crate) fn raw(&self, node: &XmlNode<'_>) -> String {
        let mut out = String::with_capacity(node.end - node.start + 64);
        out.push_str(&self.body[node.start..node.name_end]);

        let mut seen: Vec<&str> = Vec::new();
        let mut i = node.scope;

        while i != NONE {
            let decl = &self.decls[i];
            i = decl.parent;

            if seen.contains(&decl.prefix) {
                continue;
            }
            seen.push(decl.prefix);

            if decl.owner == node.start {
                continue;
            }

            let uri = escape_attr(&decl.uri)
                .replace('\t', "&#9;")
                .replace('\n', "&#10;")
                .replace('\r', "&#13;");
            match decl.prefix {
                "" => out.push_str(&format!(" xmlns=\"{uri}\"")),
                prefix => out.push_str(&format!(" xmlns:{prefix}=\"{uri}\"")),
            }
        }

        out.push_str(&self.body[node.name_end..node.end]);
        out
    }

    /// The property selection under `node`: its `allprop`, `propname` or
    /// `prop` child.
    pub(crate) fn selection(&self, node: &XmlNode<'_>) -> Option<WebdavPropSelection> {
        node.children.iter().find_map(|child| {
            if child.is_dav("prop") {
                let names = child.children.iter().map(XmlNode::prop_name).collect();
                return Some(WebdavPropSelection::Prop(names));
            }

            if child.is_dav("propname") {
                return Some(WebdavPropSelection::PropName);
            }

            if child.is_dav("allprop") {
                let include = node
                    .dav("include")
                    .map(|include| include.children.iter().map(XmlNode::prop_name).collect())
                    .unwrap_or_default();
                return Some(WebdavPropSelection::AllProp(include));
            }

            None
        })
    }

    /// The `calendar-data` or `address-data` element of the `prop` under
    /// `node`.
    pub(crate) fn data_request(
        &self,
        node: &XmlNode<'_>,
    ) -> Result<Option<WebdavDataRequest>, WebdavBodyParseError> {
        let Some(data) = node.dav("prop").and_then(|prop| {
            prop.children
                .iter()
                .find(|child| child.is_caldav("calendar-data") || child.is_carddav("address-data"))
        }) else {
            return Ok(None);
        };

        if data.children.is_empty() && data.attrs.trim_ascii().is_empty() {
            return Ok(None);
        }

        let expand = match data.child(CALDAV.uri, "expand") {
            Some(expand) => Some(CaldavTimeRange::from_node(expand)?),
            None => None,
        };

        Ok(Some(WebdavDataRequest {
            content_type: data.attr("content-type"),
            version: data.attr("version"),
            expand,
            xml: self.raw(data),
        }))
    }

    /// The property elements under the `prop` of every `set` child of
    /// `node`.
    pub(crate) fn set_props(&self, node: &XmlNode<'_>) -> Vec<WebdavPropElement> {
        node.children
            .iter()
            .filter(|child| child.is_dav("set"))
            .flat_map(|set| &set.children)
            .filter(|prop| prop.is_dav("prop"))
            .flat_map(|prop| &prop.children)
            .map(|element| self.prop_element(element))
            .collect()
    }

    /// Reads `node` as a property element.
    pub(crate) fn prop_element(&self, node: &XmlNode<'_>) -> WebdavPropElement {
        let children = node
            .children
            .iter()
            .map(|child| WebdavChildElement {
                name: child.name(),
                name_attr: child.attr("name"),
            })
            .collect();

        WebdavPropElement {
            name: node.prop_name(),
            text: node.text.clone(),
            xml: self.raw(node),
            children,
        }
    }
}

/// Reads a request body into elements, [`None`] when it is empty or blank.
pub(crate) fn parse_tree(body: &[u8]) -> Result<Option<XmlTree<'_>>, WebdavBodyParseError> {
    let body = str::from_utf8(body).map_err(|_| WebdavBodyParseError::NotUtf8)?;
    if body.trim_ascii().is_empty() {
        return Ok(None);
    }

    let base = body.as_ptr() as usize;
    let offset = |s: &str| s.as_ptr() as usize - base;

    let mut events = XmlScanner::new(body);
    let mut decls: Vec<XmlDecl> = Vec::new();
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut root: Option<XmlNode> = None;
    let mut scope = NONE;

    while let Some(event) = events.next() {
        match event {
            XmlEvent::Start {
                prefix,
                local,
                attrs,
                empty,
            } => {
                if root.is_some() {
                    return Err(WebdavBodyParseError::Malformed);
                }

                if !is_name(local) || !(prefix.is_empty() || is_name(prefix)) {
                    return Err(WebdavBodyParseError::Malformed);
                }

                if stack.len() == BODY_MAX_DEPTH {
                    return Err(WebdavBodyParseError::TooDeep(BODY_MAX_DEPTH));
                }

                let name_end = offset(local) + local.len();
                let start = match body.as_bytes()[offset(local) - 1] {
                    b':' => offset(local) - prefix.len() - 2,
                    _ => offset(local) - 1,
                };

                for (name, value) in XmlAttrs(attrs) {
                    let prefix = match name.strip_prefix("xmlns") {
                        Some("") => "",
                        Some(rest) => match rest.strip_prefix(':') {
                            Some(prefix) if !is_name(prefix) => {
                                return Err(WebdavBodyParseError::Malformed);
                            }
                            Some(prefix) => prefix,
                            None => continue,
                        },
                        None => continue,
                    };

                    let uri = match value.contains(['&', '\t', '\n', '\r']) {
                        true => Cow::Owned(decode_attr(value)),
                        false => Cow::Borrowed(value),
                    };

                    decls.push(XmlDecl {
                        prefix,
                        uri,
                        owner: start,
                        parent: scope,
                    });
                    scope = decls.len() - 1;
                }

                let ns = resolve(&decls, scope, prefix)?;
                let node = XmlNode {
                    ns,
                    local,
                    attrs,
                    text: String::new(),
                    children: Vec::new(),
                    start,
                    name_end,
                    end: events.offset(),
                    scope,
                };

                if !empty {
                    stack.push(node);
                    continue;
                }

                scope = stack.last().map_or(NONE, |parent| parent.scope);
                match stack.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None => root = Some(node),
                }
            }
            XmlEvent::End(close) => {
                let Some(mut node) = stack.pop_if(|open| same_name(open.local, close)) else {
                    return Err(WebdavBodyParseError::Malformed);
                };

                node.end = events.offset();
                scope = stack.last().map_or(NONE, |parent| parent.scope);
                match stack.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None => root = Some(node),
                }
            }
            XmlEvent::Text { raw, clean } => match stack.last_mut() {
                Some(open) => decode_text(raw, clean, &mut open.text),
                None if raw.trim_ascii().is_empty() => {}
                None => return Err(WebdavBodyParseError::Malformed),
            },
            XmlEvent::Cdata(raw) => match stack.last_mut() {
                Some(open) => open.text.push_str(raw),
                None => return Err(WebdavBodyParseError::Malformed),
            },
        }
    }

    match root {
        Some(root) if events.is_done() => Ok(Some(XmlTree { body, decls, root })),
        _ => Err(WebdavBodyParseError::Malformed),
    }
}

/// Whether `name` can be a prefix or a local name: a non-empty run of
/// characters neither blank, control nor markup, a colon included.
fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| !c.is_whitespace() && !c.is_control() && !"<>\"'=/&:".contains(c))
}

/// Resolves `prefix` against the declarations in scope.
fn resolve<'a>(
    decls: &[XmlDecl<'a>],
    mut scope: usize,
    prefix: &str,
) -> Result<Cow<'a, str>, WebdavBodyParseError> {
    while scope != NONE {
        let decl = &decls[scope];
        if decl.prefix == prefix {
            return Ok(decl.uri.clone());
        }
        scope = decl.parent;
    }

    match prefix {
        "" => Ok(Cow::Borrowed("")),
        "xml" => Ok(Cow::Borrowed(XML_NS)),
        prefix => Err(WebdavBodyParseError::UnboundPrefix(prefix.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use crate::server::body::*;

    fn tree(xml: &str) -> XmlTree<'_> {
        parse_tree(xml.as_bytes()).unwrap().unwrap()
    }

    #[test]
    fn names_resolve_by_namespace_not_prefix() {
        let xml = "<x:a xmlns:x='DAV:' xmlns='urn:d'><b/><y:c xmlns:y='DAV:'/></x:a>";
        let tree = tree(xml);
        assert!(tree.root.is_dav("a"));
        assert!(tree.root.children[0].is("urn:d", "b"));
        assert!(tree.root.children[1].is_dav("c"));
    }

    #[test]
    fn raw_elements_carry_their_inherited_namespaces() {
        let xml = "<D:a xmlns:D=\"DAV:\" xmlns:Z=\"urn:z&amp;\"><D:b>\
                   <Z:c xmlns:D=\"urn:shadow\">t<D:d/></Z:c></D:b></D:a>";
        let first = tree(xml);
        let c = &first.root.children[0].children[0];
        assert_eq!(
            first.raw(c),
            "<Z:c xmlns:Z=\"urn:z&amp;\" xmlns:D=\"urn:shadow\">t<D:d/></Z:c>"
        );
        assert!(c.children[0].is("urn:shadow", "d"));

        let other = tree("<a xmlns='urn:d'><:b>t</:b></a>");
        let b = &other.root.children[0];
        assert_eq!(other.raw(b), "<:b xmlns=\"urn:d\">t</:b>");
    }

    #[test]
    fn broken_bodies_fail() {
        let fails = |xml: &str| parse_tree(xml.as_bytes()).unwrap_err();
        assert_eq!(fails("<a><b></a>"), WebdavBodyParseError::Malformed);
        assert_eq!(fails("<a/><b/>"), WebdavBodyParseError::Malformed);
        assert_eq!(fails("<a/>tail"), WebdavBodyParseError::Malformed);
        assert_eq!(fails("<a xmlns:p<q='u'/>"), WebdavBodyParseError::Malformed);
        assert_eq!(fails("<a\u{c}/>"), WebdavBodyParseError::Malformed);
        assert_eq!(fails("<a:b:c/>"), WebdavBodyParseError::Malformed);
        assert_eq!(fails("<a>"), WebdavBodyParseError::Malformed);
        assert_eq!(
            fails("<p:a/>"),
            WebdavBodyParseError::UnboundPrefix("p".into())
        );
        assert_eq!(
            parse_tree(b"<a>\xff</a>").unwrap_err(),
            WebdavBodyParseError::NotUtf8
        );

        let deep = "<a>".repeat(BODY_MAX_DEPTH + 1);
        assert_eq!(fails(&deep), WebdavBodyParseError::TooDeep(BODY_MAX_DEPTH));
        assert!(parse_tree(b" \r\n").unwrap().is_none());
    }
}
