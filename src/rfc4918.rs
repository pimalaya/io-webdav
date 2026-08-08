//! RFC 4918: HTTP Extensions for Web Distributed Authoring and
//! Versioning (WebDAV).
//!
//! <https://www.rfc-editor.org/rfc/rfc4918>
//!
//! This module carries the WebDAV vocabulary shared across every RFC
//! layer: the authentication scheme, the namespace and property model,
//! the generic parsed multistatus body, and the generic `DAV:` property
//! constants. Alongside them live the crate-internal helpers every
//! coroutine reuses: the XML request-body generators (PROPFIND,
//! PROPPATCH, MKCOL, REPORT), the multistatus parser, and the
//! `Authorization` header emitter, request-path resolution and `ETag`
//! extraction. Each WebDAV method is its own submodule.
//!
//! Request bodies are generated from a [`WebdavProperty`] selector rather than
//! hard-coded templates: callers choose the properties and values they
//! need. Each [`WebdavProperty`] carries its [`WebdavNamespace`] (URI plus preferred
//! prefix), so the generators emit XML without a central namespace
//! table; every RFC layer owns the namespaces and property constants it
//! speaks.

pub mod copy;
pub mod coroutine;
pub mod delete;
pub mod follow_redirects;
pub mod get;
pub mod mkcol;
pub mod r#move;
pub mod options;
pub mod propfind;
pub mod proppatch;
pub mod put;
pub mod report;
pub mod request;
pub mod send;

use alloc::{
    format,
    string::{String, ToString},
    vec::{self, Vec},
};

use io_http::{
    rfc6750::bearer::HttpAuthBearer, rfc7617::basic::HttpAuthBasic, rfc9110::response::HttpResponse,
};
use log::trace;
use quick_xml::{
    Reader, XmlVersion,
    events::{BytesStart, Event},
};
use url::Url;

/// Authentication scheme used by the WebDAV client.
///
/// Covers the three modes the CalDAV/CardDAV deployments handle in
/// practice: no auth, HTTP Basic (RFC 7617) and HTTP Bearer (RFC 6750),
/// reusing the io-http credential types. Higher-level coroutines never
/// observe the credential directly; they only see the pre-formatted
/// header value from `emit_header`.
#[derive(Clone, Debug, Default)]
pub enum WebdavAuth {
    /// No authentication; no `Authorization` header is emitted.
    #[default]
    None,
    /// HTTP Basic authentication (RFC 7617).
    Basic(HttpAuthBasic),
    /// HTTP Bearer authentication (RFC 6750).
    Bearer(HttpAuthBearer),
}

/// An XML namespace: its URI plus the preferred prefix used when
/// serializing request bodies (the empty prefix means the default
/// namespace).
///
/// Each RFC layer owns the namespaces it speaks (`DAV:` in
/// [`crate::rfc4918`], CalDAV ones in [`crate::rfc4791`], CardDAV ones
/// in [`crate::rfc6352`]); the generic body generators only read these
/// fields, so they never need to know which namespaces exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebdavNamespace {
    /// WebdavNamespace URI (e.g. `DAV:`).
    pub uri: &'static str,
    /// Preferred XML prefix (`""` for the default namespace).
    pub prefix: &'static str,
}

/// A WebDAV property identifier: an XML [`WebdavNamespace`] plus a local name
/// (RFC 4918 §15).
///
/// Each RFC layer owns its own vocabulary as `const` values (generic
/// DAV properties in [`crate::rfc4918`], calendar ones in
/// [`crate::rfc4791`], card ones in [`crate::rfc6352`]); there is no
/// central enum. Construct an ad-hoc value for any property the
/// constants do not cover.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebdavProperty {
    /// XML namespace.
    pub ns: WebdavNamespace,
    /// Local element name (e.g. `displayname`).
    pub local: &'static str,
}

/// Parsed `multistatus` body returned by `PROPFIND` / `REPORT`
/// (RFC 4918 §14.16).
#[derive(Clone, Debug, Default)]
pub struct WebdavMultistatus {
    /// The parsed `<response>` entries.
    pub responses: Vec<WebdavResponseEntry>,
    /// The top-level `DAV:sync-token` returned by a `sync-collection`
    /// REPORT (RFC 6578 §6.2); [`None`] outside sync responses.
    pub sync_token: Option<String>,
}

impl IntoIterator for WebdavMultistatus {
    type Item = WebdavResponseEntry;
    type IntoIter = vec::IntoIter<WebdavResponseEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.responses.into_iter()
    }
}

/// A single `<response>` inside a [`WebdavMultistatus`]: its `href` plus the
/// properties returned under 2xx `propstat`s.
#[derive(Clone, Debug, Default)]
pub struct WebdavResponseEntry {
    /// The `<href>` text, as returned by the server.
    pub href: String,
    /// The response-level `<status>` code, when present. Carries the
    /// 404 of a `sync-collection` removal row (RFC 6578 §3.4) or the
    /// 507 of a truncation row (RFC 6578 §3.6); [`None`] on ordinary
    /// propstat-only responses.
    pub status: Option<u16>,
    /// Properties gathered from every 2xx `<propstat>` of this response.
    pub props: Vec<WebdavPropItem>,
}

impl WebdavResponseEntry {
    /// Returns the property matching `prop` (by local name), if present.
    pub fn prop(&self, prop: WebdavProperty) -> Option<&WebdavPropItem> {
        self.props.iter().find(|item| item.local == prop.local)
    }

    /// Returns `prop`'s trimmed text content when present and non-empty.
    pub fn text(&self, prop: WebdavProperty) -> Option<&str> {
        self.prop(prop)
            .map(|item| item.text.trim())
            .filter(|text| !text.is_empty())
    }

    /// Returns `true` when `<resourcetype>` lists `ty` as a child
    /// (e.g. `<C:calendar/>`).
    pub fn has_resource_type(&self, resourcetype: WebdavProperty, ty: WebdavProperty) -> bool {
        self.prop(resourcetype)
            .is_some_and(|item| item.children.iter().any(|child| child.local == ty.local))
    }

    /// Returns the last non-empty path segment of [`href`](Self::href),
    /// the conventional collection / resource identifier.
    pub fn id(&self) -> &str {
        self.href
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
    }
}

/// A single property returned inside a `<prop>` element.
#[derive(Clone, Debug, Default)]
pub struct WebdavPropItem {
    /// WebdavProperty local name (e.g. `displayname`, `resourcetype`).
    pub local: String,
    /// Concatenated descendant text (covers text properties and the
    /// `<href>` payload of principal / home-set properties).
    pub text: String,
    /// The direct child elements (e.g. `collection` and `calendar`
    /// under `<resourcetype>`).
    pub children: Vec<WebdavPropChild>,
}

/// A direct child element of a property, for the properties whose value
/// is markup rather than text.
#[derive(Clone, Debug, Default)]
pub struct WebdavPropChild {
    /// Child element local name (e.g. `collection`, `comp`).
    pub local: String,
    /// The child's `name` attribute, when it carries one. RFC 4791
    /// §5.2.3 spells a calendar's component types as
    /// `<C:comp name="VEVENT"/>`, putting the value in the attribute
    /// rather than in a text node.
    pub name: Option<String>,
}

/// The value a property is set to in a `PROPPATCH`, `MKCOL` or
/// `MKCALENDAR` body.
///
/// Most WebDAV properties carry text, but a few carry markup, which
/// would not survive text escaping: `supported-calendar-component-set`
/// (RFC 4791 §5.2.3) is a list of `<C:comp/>` children.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebdavPropValue<'a> {
    /// Text content, XML-escaped on the way out.
    Text(&'a str),
    /// A raw XML fragment, emitted verbatim; the caller owns its
    /// well-formedness and its escaping.
    Raw(String),
}

/// WebDAV namespace (RFC 4918), emitted with the `D` prefix the RFC
/// examples use. Never the default namespace: strict servers (iCloud,
/// Google) reject bodies mixing a prefixed CardDAV root with
/// default-namespace DAV children (their addressbook-multiget answers
/// HTTP 400), while the all-prefixed form every interoperable client
/// sends passes everywhere. The literal `D:` in the body generators
/// assumes this prefix.
pub const DAV: WebdavNamespace = WebdavNamespace {
    uri: "DAV:",
    prefix: "D",
};
/// CalendarServer extension namespace (ctag); protocol-neutral, used by
/// both CalDAV and CardDAV servers.
pub const CALENDARSERVER: WebdavNamespace = WebdavNamespace {
    uri: "http://calendarserver.org/ns/",
    prefix: "CS",
};

/// Standard XML declaration prepended to every request body.
pub const XML_DECL: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>";

/// `DAV:displayname` (RFC 4918 §15.2).
pub const DISPLAYNAME: WebdavProperty = WebdavProperty {
    ns: DAV,
    local: "displayname",
};
/// `DAV:resourcetype` (RFC 4918 §15.9).
pub const RESOURCETYPE: WebdavProperty = WebdavProperty {
    ns: DAV,
    local: "resourcetype",
};
/// `DAV:getetag` (RFC 4918 §15.6).
pub const GETETAG: WebdavProperty = WebdavProperty {
    ns: DAV,
    local: "getetag",
};
/// `DAV:sync-token` (RFC 6578 §4), the collection checkpoint property.
pub const SYNC_TOKEN: WebdavProperty = WebdavProperty {
    ns: DAV,
    local: "sync-token",
};
/// `CS:getctag` (CalendarServer extension); bumped on every change to
/// the collection.
pub const GETCTAG: WebdavProperty = WebdavProperty {
    ns: CALENDARSERVER,
    local: "getctag",
};

/// `DAV:propertyupdate` PROPPATCH request root (RFC 4918 §9.2).
const PROPERTYUPDATE: WebdavProperty = WebdavProperty {
    ns: DAV,
    local: "propertyupdate",
};

/// Emits the `xmlns` declarations for the given namespaces (deduped by
/// URI, in order). The empty-prefix namespace becomes the default
/// namespace.
pub fn xmlns_decls(namespaces: &[WebdavNamespace]) -> String {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = String::new();

    for ns in namespaces {
        if seen.contains(&ns.uri) {
            continue;
        }
        seen.push(ns.uri);

        if ns.prefix.is_empty() {
            out.push_str(&format!(" xmlns=\"{}\"", ns.uri));
        } else {
            out.push_str(&format!(" xmlns:{}=\"{}\"", ns.prefix, ns.uri));
        }
    }

    out
}

/// Escapes XML text content (`&`, `<`, `>`).
pub fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Emits a `D:prop` block listing each property as an empty element.
pub fn prop_block(props: &[WebdavProperty]) -> String {
    let mut out = String::from("<D:prop>");
    for prop in props {
        out.push_str(&empty_element(*prop));
    }
    out.push_str("</D:prop>");
    out
}

/// Builds a `PROPFIND` request body (RFC 4918 §9.1) requesting `props`.
pub fn propfind_body(props: &[WebdavProperty]) -> Vec<u8> {
    let decls = xmlns_decls(&namespaces(&[], props));
    let mut body = format!("{XML_DECL}<D:propfind{decls}>");
    body.push_str(&prop_block(props));
    body.push_str("</D:propfind>");
    body.into_bytes()
}

/// Builds a `PROPPATCH` request body (RFC 4918 §9.2) setting each
/// `(property, value)` pair.
pub fn proppatch_body(set: &[(WebdavProperty, WebdavPropValue<'_>)]) -> Vec<u8> {
    prop_set_body(PROPERTYUPDATE, set)
}

/// Builds a `<root><set><prop>...</prop></set></root>` body setting each
/// `(property, value)` pair, rooted at `root`. Backs both
/// [`proppatch_body`] (`DAV:propertyupdate`) and CalDAV `MKCALENDAR`
/// (`C:mkcalendar`, RFC 4791 §5.3.1).
pub fn prop_set_body(
    root: WebdavProperty,
    set: &[(WebdavProperty, WebdavPropValue<'_>)],
) -> Vec<u8> {
    let props: Vec<WebdavProperty> = set.iter().map(|(prop, _)| *prop).collect();
    let mut nss = namespaces(&[], &props);
    nss.push(root.ns);
    let decls = xmlns_decls(&nss);
    let open = qualified(root.ns, root.local);

    let mut body = format!("{XML_DECL}<{open}{decls}><D:set><D:prop>");
    for (prop, value) in set {
        body.push_str(&value_element(*prop, value));
    }
    body.push_str(&format!("</D:prop></D:set></{open}>"));
    body.into_bytes()
}

/// Extracts a resource id from a `Location` header: its last path
/// segment (query and fragment dropped), matching how a listed
/// resource's id is derived from its href. [`None`] for an empty
/// segment.
///
/// A server may store a created resource under a name of its own and
/// report it here (Google does), in which case that name, not the one
/// the caller sent, is what addresses the resource afterwards.
pub fn id_from_location(location: &str) -> Option<String> {
    let path = location
        .split(['?', '#'])
        .next()
        .unwrap_or(location)
        .trim_end_matches('/');
    let segment = path.rsplit('/').next().unwrap_or_default();
    (!segment.is_empty()).then(|| segment.to_string())
}

/// Builds an extended `MKCOL` request body (RFC 5689 §3): a
/// `<resourcetype>` of `<collection/>` plus `resource_types`, and each
/// `set` property value.
pub fn mkcol_body(
    resource_types: &[WebdavProperty],
    set: &[(WebdavProperty, WebdavPropValue<'_>)],
) -> Vec<u8> {
    let mut props: Vec<WebdavProperty> = resource_types.to_vec();
    props.extend(set.iter().map(|(prop, _)| *prop));
    let decls = xmlns_decls(&namespaces(&[], &props));

    let mut body =
        format!("{XML_DECL}<D:mkcol{decls}><D:set><D:prop><D:resourcetype><D:collection/>");
    for resource_type in resource_types {
        body.push_str(&empty_element(*resource_type));
    }
    body.push_str("</D:resourcetype>");
    for (prop, value) in set {
        body.push_str(&value_element(*prop, value));
    }
    body.push_str("</D:prop></D:set></D:mkcol>");
    body.into_bytes()
}

/// Builds a `REPORT` query body (RFC 3253 §3.6) rooted at `root` (e.g.
/// `calendar-query`), requesting `props` and appending the raw `filter`
/// fragment. `extra_ns` declares namespaces the filter needs beyond
/// those of `root` and `props`.
pub fn report_query_body(
    root: WebdavProperty,
    extra_ns: &[WebdavNamespace],
    props: &[WebdavProperty],
    filter: &str,
) -> Vec<u8> {
    let mut nss = namespaces(extra_ns, props);
    nss.push(root.ns);
    let decls = xmlns_decls(&nss);

    let open = qualified(root.ns, root.local);

    let mut body = format!("{XML_DECL}<{open}{decls}>");
    body.push_str(&prop_block(props));
    body.push_str(filter);
    body.push_str(&format!("</{open}>"));
    body.into_bytes()
}

/// Parses a `multistatus` body into vocabulary-agnostic entries.
///
/// Matching is by local name (namespace prefixes are ignored), and only
/// properties under 2xx `propstat`s are kept. Responses without any 2xx
/// propstat still survive as entries with empty props, carrying their
/// response-level status (`sync-collection` removal and truncation
/// rows). Predefined and numeric character references are resolved;
/// unknown entity references are kept verbatim. Malformed input yields
/// whatever was parsed before the error.
pub fn parse_multistatus(xml: &str) -> WebdavMultistatus {
    let mut reader = Reader::from_str(xml);

    let mut responses: Vec<WebdavResponseEntry> = Vec::new();
    let mut sync_token: Option<String> = None;
    // (local name, accumulated descendant text, direct children)
    let mut stack: Vec<(String, String, Vec<WebdavPropChild>)> = Vec::new();
    let mut response: Option<WebdavResponseEntry> = None;
    let mut propstat_props: Vec<WebdavPropItem> = Vec::new();
    let mut propstat_ok: Option<bool> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let local = local_name(e.local_name().as_ref());
                if let Some((_, _, children)) = stack.last_mut() {
                    children.push(WebdavPropChild {
                        local: local.clone(),
                        name: name_attribute(&e),
                    });
                }
                match local.as_str() {
                    "response" => response = Some(WebdavResponseEntry::default()),
                    "propstat" => {
                        propstat_props.clear();
                        propstat_ok = None;
                    }
                    _ => {}
                }
                stack.push((local, String::new(), Vec::new()));
            }
            Ok(Event::Empty(e)) => {
                let local = local_name(e.local_name().as_ref());
                let parent_is_prop = stack.last().is_some_and(|(n, _, _)| n == "prop");
                if parent_is_prop {
                    propstat_props.push(WebdavPropItem {
                        local,
                        ..Default::default()
                    });
                } else if let Some((_, _, children)) = stack.last_mut() {
                    children.push(WebdavPropChild {
                        local,
                        name: name_attribute(&e),
                    });
                }
            }
            Ok(Event::Text(t)) => {
                if let Ok(decoded) = t.decode() {
                    if let Some((_, buf, _)) = stack.last_mut() {
                        buf.push_str(&decoded);
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if let Some((_, buf, _)) = stack.last_mut() {
                    if let Ok(Some(ch)) = r.resolve_char_ref() {
                        buf.push(ch);
                    } else if let Ok(name) = r.decode() {
                        match name.as_ref() {
                            "amp" => buf.push('&'),
                            "lt" => buf.push('<'),
                            "gt" => buf.push('>'),
                            "quot" => buf.push('"'),
                            "apos" => buf.push('\''),
                            name => {
                                // NOTE: unknown entity, kept verbatim.
                                buf.push('&');
                                buf.push_str(name);
                                buf.push(';');
                            }
                        }
                    }
                }
            }
            Ok(Event::CData(t)) => {
                let bytes = t.into_inner();
                if let Ok(text) = core::str::from_utf8(&bytes) {
                    if let Some((_, buf, _)) = stack.last_mut() {
                        buf.push_str(text);
                    }
                }
            }
            Ok(Event::End(_)) => {
                if let Some((name, text, children)) = stack.pop() {
                    let parent = stack.last().map(|(n, _, _)| n.clone());
                    if let Some((_, parent_text, _)) = stack.last_mut() {
                        parent_text.push_str(&text);
                    }
                    let parent = parent.as_deref();

                    match name.as_str() {
                        "response" => {
                            if let Some(entry) = response.take() {
                                responses.push(entry);
                            }
                        }
                        "propstat" => {
                            if propstat_ok == Some(true) {
                                if let Some(entry) = response.as_mut() {
                                    entry.props.append(&mut propstat_props);
                                }
                            }
                            propstat_props.clear();
                            propstat_ok = None;
                        }
                        "status" if parent == Some("propstat") => {
                            propstat_ok =
                                Some(status_code(&text).is_some_and(|code| code / 100 == 2));
                        }
                        "status" if parent == Some("response") => {
                            if let Some(entry) = response.as_mut() {
                                entry.status = status_code(&text);
                            }
                        }
                        "sync-token" if parent == Some("multistatus") => {
                            let text = text.trim();
                            if !text.is_empty() {
                                sync_token = Some(text.to_string());
                            }
                        }
                        "href" if parent == Some("response") => {
                            if let Some(entry) = response.as_mut() {
                                if entry.href.is_empty() {
                                    entry.href = text.trim().to_string();
                                }
                            }
                        }
                        _ if parent == Some("prop") => {
                            propstat_props.push(WebdavPropItem {
                                local: name,
                                text,
                                children,
                            });
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    WebdavMultistatus {
        responses,
        sync_token,
    }
}

/// Returns the value of the HTTP `Authorization` header for the given
/// scheme, or [`None`] when no header should be emitted.
pub fn emit_header(auth: &WebdavAuth) -> Option<String> {
    match auth {
        WebdavAuth::None => None,
        WebdavAuth::Basic(credentials) => Some(credentials.to_authorization()),
        WebdavAuth::Bearer(token) => Some(token.to_authorization()),
    }
}

/// Resolves `path` against `base_url`.
///
/// Empty paths return `base_url` unchanged. Absolute paths (starting
/// with `/`) replace the base path. Relative paths are appended to the
/// base path. Falls back to `base_url` when the join fails.
pub fn resolve(base_url: &Url, path: &str) -> Url {
    if path.is_empty() {
        return base_url.clone();
    }

    if path.starts_with('/') {
        if let Ok(mut url) = Url::parse(base_url.as_str()) {
            url.set_path(path);
            return url;
        }
    }

    let mut base = base_url.clone();
    if !base.path().ends_with('/') {
        let mut new_path = base.path().to_string();
        new_path.push('/');
        base.set_path(&new_path);
    }

    base.join(path).unwrap_or_else(|_| base_url.clone())
}

/// Reads the `ETag` header (RFC 9110 §8.8.3) out of an HTTP response,
/// stripping the surrounding double quotes when present.
pub fn read_etag(response: &HttpResponse) -> Option<String> {
    response
        .header("etag")
        .map(|raw| raw.trim_matches('"').into())
}

/// Resolves an `<href>` value against `base_url`, joining when the href
/// is relative. Returns [`None`] when the href cannot be parsed.
pub fn resolve_href(base_url: &Url, href: &str) -> Option<Url> {
    match Url::parse(href) {
        Ok(url) => Some(url),
        Err(url::ParseError::RelativeUrlWithoutBase) => base_url.join(href).ok(),
        Err(_) => None,
    }
}

/// Trace-logs every property of `entry` whose local name is not in
/// `known`. Lets `from_props` mappers surface ignored properties
/// without failing.
pub fn trace_unrecognized(entry: &WebdavResponseEntry, known: &[WebdavProperty]) {
    for item in &entry.props {
        if !known.iter().any(|prop| prop.local == item.local) {
            trace!("ignoring unrecognized WebDAV property `{}`", item.local);
        }
    }
}

/// Extracts the numeric code out of an HTTP status line
/// (e.g. `HTTP/1.1 404 Not Found`).
fn status_code(text: &str) -> Option<u16> {
    text.split_whitespace().nth(1)?.parse().ok()
}

/// Collects `DAV:` plus `extra` plus every property namespace.
fn namespaces(extra: &[WebdavNamespace], props: &[WebdavProperty]) -> Vec<WebdavNamespace> {
    let mut nss = Vec::with_capacity(1 + extra.len() + props.len());
    nss.push(DAV);
    nss.extend_from_slice(extra);
    nss.extend(props.iter().map(|prop| prop.ns));
    nss
}

fn qualified(ns: WebdavNamespace, local: &str) -> String {
    if ns.prefix.is_empty() {
        local.to_string()
    } else {
        format!("{}:{local}", ns.prefix)
    }
}

fn empty_element(prop: WebdavProperty) -> String {
    format!("<{}/>", qualified(prop.ns, prop.local))
}

fn value_element(prop: WebdavProperty, value: &WebdavPropValue<'_>) -> String {
    let name = qualified(prop.ns, prop.local);
    let inner = match value {
        WebdavPropValue::Text(text) => escape_text(text),
        WebdavPropValue::Raw(xml) => xml.clone(),
    };
    format!("<{name}>{inner}</{name}>")
}

/// Reads an element's `name` attribute, ignoring a malformed or
/// non-UTF-8 value.
fn name_attribute(element: &BytesStart) -> Option<String> {
    let attribute = element.try_get_attribute("name").ok()??;
    // NOTE: the parser does not track the XML declaration, and every
    // WebDAV body in practice is XML 1.0, which is also what the
    // specification tells a reader to assume when it saw no version.
    let value = attribute.normalized_value(XmlVersion::Implicit1_0).ok()?;
    Some(value.into_owned())
}

fn local_name(bytes: &[u8]) -> String {
    core::str::from_utf8(bytes).unwrap_or("").to_string()
}
#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use io_http::{rfc6750::bearer::HttpAuthBearer, rfc7617::basic::HttpAuthBasic};

    use crate::rfc4918::*;

    const CALDAV: WebdavNamespace = WebdavNamespace {
        uri: "urn:ietf:params:xml:ns:caldav",
        prefix: "C",
    };
    const CALENDAR: WebdavProperty = WebdavProperty {
        ns: CALDAV,
        local: "calendar",
    };
    const CALENDAR_DATA: WebdavProperty = WebdavProperty {
        ns: CALDAV,
        local: "calendar-data",
    };

    #[test]
    fn propfind_body_lists_props_with_namespaces() {
        let body = propfind_body(&[DISPLAYNAME, CALENDAR_DATA]);
        let xml = core::str::from_utf8(&body).unwrap();
        assert!(xml.contains("xmlns:D=\"DAV:\""));
        assert!(xml.contains("xmlns:C=\"urn:ietf:params:xml:ns:caldav\""));
        assert!(xml.contains("<D:displayname/>"));
        assert!(xml.contains("<C:calendar-data/>"));
    }

    #[test]
    fn mkcol_body_carries_resourcetype_and_values() {
        let body = mkcol_body(
            &[CALENDAR],
            &[(DISPLAYNAME, WebdavPropValue::Text("Personal & co"))],
        );
        let xml = core::str::from_utf8(&body).unwrap();
        assert!(xml.contains("<D:resourcetype><D:collection/><C:calendar/></D:resourcetype>"));
        assert!(xml.contains("<D:displayname>Personal &amp; co</D:displayname>"));
    }

    #[test]
    fn proppatch_body_wraps_values_in_propertyupdate() {
        let body = proppatch_body(&[(DISPLAYNAME, WebdavPropValue::Text("Renamed"))]);
        let xml = core::str::from_utf8(&body).unwrap();
        assert!(xml.contains("<D:propertyupdate xmlns:D=\"DAV:\">"));
        assert!(
            xml.contains("<D:set><D:prop><D:displayname>Renamed</D:displayname></D:prop></D:set>")
        );
        assert!(xml.ends_with("</D:propertyupdate>"));
    }

    #[test]
    fn prop_set_body_roots_at_the_given_element() {
        const MKCALENDAR: WebdavProperty = WebdavProperty {
            ns: CALDAV,
            local: "mkcalendar",
        };
        let body = prop_set_body(MKCALENDAR, &[(DISPLAYNAME, WebdavPropValue::Text("Work"))]);
        let xml = core::str::from_utf8(&body).unwrap();
        assert!(xml.contains("<C:mkcalendar "));
        assert!(xml.contains("xmlns:C=\"urn:ietf:params:xml:ns:caldav\""));
        assert!(
            xml.contains("<D:set><D:prop><D:displayname>Work</D:displayname></D:prop></D:set>")
        );
        assert!(xml.ends_with("</C:mkcalendar>"));
    }

    #[test]
    fn a_raw_prop_value_is_emitted_as_markup() {
        const COMPONENT_SET: WebdavProperty = WebdavProperty {
            ns: CALDAV,
            local: "supported-calendar-component-set",
        };
        let comps = WebdavPropValue::Raw("<C:comp name=\"VEVENT\"/>".to_string());
        let body = proppatch_body(&[(COMPONENT_SET, comps)]);
        let xml = core::str::from_utf8(&body).unwrap();
        // Raw markup survives verbatim; escaping it would turn the
        // children into text and lose the property's whole value.
        assert!(xml.contains(
            "<C:supported-calendar-component-set><C:comp name=\"VEVENT\"/></C:supported-calendar-component-set>"
        ));
    }

    #[test]
    fn parse_multistatus_reads_child_name_attributes() {
        let xml = r#"<?xml version="1.0"?>
        <d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
          <d:response>
            <d:href>/dav/calendars/personal/</d:href>
            <d:propstat>
              <d:prop>
                <c:supported-calendar-component-set>
                  <c:comp name="VEVENT"/>
                  <c:comp name="VTODO"/>
                </c:supported-calendar-component-set>
              </d:prop>
              <d:status>HTTP/1.1 200 OK</d:status>
            </d:propstat>
          </d:response>
        </d:multistatus>"#;

        const COMPONENT_SET: WebdavProperty = WebdavProperty {
            ns: CALDAV,
            local: "supported-calendar-component-set",
        };
        let ms = parse_multistatus(xml);
        let children = &ms.responses[0].prop(COMPONENT_SET).unwrap().children;
        // The value lives in the attribute, not in a text node, so a
        // text-only reading of this property returns nothing at all.
        let names: Vec<_> = children
            .iter()
            .filter_map(|child| child.name.clone())
            .collect();
        assert_eq!(names, ["VEVENT", "VTODO"]);
        assert_eq!(ms.responses[0].text(COMPONENT_SET), None);
    }

    #[test]
    fn id_from_location_takes_the_last_segment() {
        let location = "https://dav.example.org/dav/books/contacts/server-9f8e7d?v=2#frag";
        assert_eq!(
            id_from_location(location),
            Some("server-9f8e7d".to_string())
        );
        assert_eq!(
            id_from_location("/dav/books/contacts/"),
            Some("contacts".to_string())
        );
        assert_eq!(id_from_location(""), None);
        assert_eq!(id_from_location("/"), None);
    }

    #[test]
    fn parse_multistatus_collects_2xx_props() {
        let xml = r#"<?xml version="1.0"?>
        <d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
          <d:response>
            <d:href>/dav/calendars/personal/</d:href>
            <d:propstat>
              <d:prop>
                <d:displayname>Personal</d:displayname>
                <d:resourcetype><d:collection/><c:calendar/></d:resourcetype>
              </d:prop>
              <d:status>HTTP/1.1 200 OK</d:status>
            </d:propstat>
          </d:response>
          <d:response>
            <d:href>/dav/calendars/other/</d:href>
            <d:propstat>
              <d:prop><d:displayname>Hidden</d:displayname></d:prop>
              <d:status>HTTP/1.1 404 Not Found</d:status>
            </d:propstat>
          </d:response>
        </d:multistatus>"#;

        let ms = parse_multistatus(xml);
        assert_eq!(ms.responses.len(), 2);

        let first = &ms.responses[0];
        assert_eq!(first.id(), "personal");
        assert_eq!(first.text(DISPLAYNAME), Some("Personal"));
        assert!(first.has_resource_type(RESOURCETYPE, CALENDAR));

        // 404 propstat is ignored
        assert_eq!(ms.responses[1].text(DISPLAYNAME), None);
    }

    #[test]
    fn parse_multistatus_reads_sync_collection_rows() {
        let xml = r#"<?xml version="1.0"?>
        <d:multistatus xmlns:d="DAV:">
          <d:response>
            <d:href>/dav/addressbooks/contacts/changed.vcf</d:href>
            <d:propstat>
              <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
              <d:status>HTTP/1.1 200 OK</d:status>
            </d:propstat>
          </d:response>
          <d:response>
            <d:href>/dav/addressbooks/contacts/removed.vcf</d:href>
            <d:status>HTTP/1.1 404 Not Found</d:status>
          </d:response>
          <d:response>
            <d:href>/dav/addressbooks/contacts/</d:href>
            <d:status>HTTP/1.1 507 Insufficient Storage</d:status>
          </d:response>
          <d:sync-token>http://example.com/ns/sync/1234</d:sync-token>
        </d:multistatus>"#;

        let ms = parse_multistatus(xml);
        assert_eq!(
            ms.sync_token.as_deref(),
            Some("http://example.com/ns/sync/1234")
        );
        assert_eq!(ms.responses.len(), 3);

        let changed = &ms.responses[0];
        assert_eq!(changed.status, None);
        assert_eq!(changed.text(GETETAG), Some("\"etag-1\""));

        let removed = &ms.responses[1];
        assert_eq!(removed.status, Some(404));
        assert!(removed.props.is_empty());

        let truncated = &ms.responses[2];
        assert_eq!(truncated.status, Some(507));
        assert!(truncated.props.is_empty());
    }

    #[test]
    fn parse_multistatus_reads_nested_href() {
        let xml = r#"<d:multistatus xmlns:d="DAV:">
          <d:response>
            <d:href>/</d:href>
            <d:propstat>
              <d:prop>
                <d:current-user-principal><d:href>/principals/alice/</d:href></d:current-user-principal>
              </d:prop>
              <d:status>HTTP/1.1 200 OK</d:status>
            </d:propstat>
          </d:response>
        </d:multistatus>"#;

        let principal = WebdavProperty {
            ns: DAV,
            local: "current-user-principal",
        };
        let ms = parse_multistatus(xml);
        let entry = &ms.responses[0];
        assert_eq!(entry.text(principal), Some("/principals/alice/"));
    }

    #[test]
    fn none_emits_nothing() {
        assert!(emit_header(&WebdavAuth::None).is_none());
    }

    #[test]
    fn basic_encodes_credentials() {
        let auth = WebdavAuth::Basic(HttpAuthBasic::new("alice", "secret"));
        // NOTE: base64("alice:secret") = "YWxpY2U6c2VjcmV0"
        assert_eq!(emit_header(&auth).unwrap(), "Basic YWxpY2U6c2VjcmV0");
    }

    #[test]
    fn bearer_prepends_scheme() {
        let auth = WebdavAuth::Bearer(HttpAuthBearer::new("xyz"));
        assert_eq!(emit_header(&auth).unwrap(), "Bearer xyz");
    }

    #[test]
    fn getetag_uses_the_dav_prefix() {
        assert_eq!(empty_or(GETETAG), "<D:getetag/>");
    }

    fn empty_or(prop: WebdavProperty) -> String {
        let body = propfind_body(&[prop]);
        let xml = core::str::from_utf8(&body).unwrap().to_string();
        let start = xml.find("<D:prop>").unwrap() + "<D:prop>".len();
        let end = xml.find("</D:prop>").unwrap();
        xml[start..end].to_string()
    }
}
