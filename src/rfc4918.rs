//! RFC 4918: HTTP Extensions for Web Distributed Authoring and Versioning
//! (WebDAV).
//!
//! <https://www.rfc-editor.org/rfc/rfc4918>
//!
//! This module carries the WebDAV vocabulary shared across every RFC layer: the
//! authentication scheme, the namespace and property model, the parsed
//! multistatus body and the `DAV:` property constants. Next to them live the
//! helpers every coroutine reuses: the XML request-body generators, the
//! multistatus parser, the `Authorization` header emitter, request-path
//! resolution and `ETag` extraction. Each WebDAV method is its own submodule.
//!
//! Bodies are generated from [`WebdavProperty`] selectors rather than
//! hard-coded templates, each selector carrying its [`WebdavNamespace`], so the
//! generators need no central namespace table: every RFC layer owns the
//! namespaces and constants it speaks.

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

use core::fmt;

use alloc::{
    collections::BTreeSet,
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
/// Covers the three modes CalDAV and CardDAV deployments handle in practice,
/// reusing the io-http credential types. Higher-level coroutines never observe
/// a credential, only the header value [`emit_header`] formats out of it.
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

/// An XML namespace: its URI plus the prefix used when serializing request
/// bodies.
///
/// Each RFC layer owns the namespaces it speaks (`DAV:` in [`crate::rfc4918`],
/// CalDAV ones in [`crate::rfc4791`], CardDAV ones in [`crate::rfc6352`]); the
/// body generators only read these fields, so they never need to know which
/// namespaces exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebdavNamespace {
    /// Namespace URI (e.g. `DAV:`).
    pub uri: &'static str,
    /// Preferred XML prefix (`""` for the default namespace).
    pub prefix: &'static str,
}

/// A WebDAV property identifier: an XML [`WebdavNamespace`] plus a local name
/// (RFC 4918 §15).
///
/// Each RFC layer owns its vocabulary as `const` values rather than a central
/// enum. Construct an ad-hoc value for any property the constants do not cover.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebdavProperty {
    /// XML namespace.
    pub ns: WebdavNamespace,
    /// Local element name (e.g. `displayname`).
    pub local: &'static str,
}

/// Parsed `multistatus` body returned by `PROPFIND` / `REPORT` (RFC 4918
/// §14.16).
#[derive(Clone, Debug, Default)]
pub struct WebdavMultistatus {
    /// The parsed `<response>` entries.
    pub responses: Vec<WebdavResponseEntry>,
    /// The top-level `DAV:sync-token` of a `sync-collection` REPORT (RFC 6578
    /// §6.2); [`None`] outside sync responses.
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
    /// The response-level `<status>` code, when present: the 404 of a
    /// `sync-collection` removal row (RFC 6578 §3.4) or the 507 of a truncation
    /// row (RFC 6578 §3.6); [`None`] on propstat-only responses.
    pub status: Option<u16>,
    /// Properties gathered from every 2xx `<propstat>` of this response.
    pub props: Vec<WebdavPropItem>,
    /// Properties the server refused, each with the status of the `<propstat>`
    /// that carried it. A `PROPPATCH` answers this way (RFC 4918 §9.2): the
    /// request itself is a 207, and only the propstat says whether the property
    /// actually changed.
    pub failures: Vec<WebdavPropFailure>,
}

/// A property a server refused, and the status it refused it with.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebdavPropFailure {
    /// Status of the `<propstat>` that carried the property.
    pub status: u16,
    /// Local name of the refused property, e.g. `displayname`.
    pub property: String,
}

impl fmt::Display for WebdavPropFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.property, self.status)
    }
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

    /// Returns `true` when `<resourcetype>` lists `ty` as a child (e.g.
    /// `<C:calendar/>`).
    pub fn has_resource_type(&self, resourcetype: WebdavProperty, ty: WebdavProperty) -> bool {
        self.prop(resourcetype)
            .is_some_and(|item| item.children.iter().any(|child| child.local == ty.local))
    }

    /// Returns the local names of the reports the server advertises under
    /// `<supported-report-set>` (RFC 3253 §3.1.5), e.g. `sync-collection`.
    ///
    /// The property nests each name three levels down: one `<supported-report>`
    /// per report, each holding a `<report>` holding the report element itself.
    pub fn supported_reports(&self) -> BTreeSet<String> {
        let Some(item) = self.prop(SUPPORTED_REPORT_SET) else {
            return BTreeSet::new();
        };

        item.children
            .iter()
            .flat_map(|supported| &supported.children)
            .filter(|child| child.local == "report")
            .flat_map(|report| &report.children)
            .map(|report| report.local.clone())
            .collect()
    }

    /// Returns the last non-empty path segment of [`href`](Self::href), the
    /// conventional collection or resource identifier.
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
    /// Property local name (e.g. `displayname`, `resourcetype`).
    pub local: String,
    /// Concatenated descendant text, covering text properties as well as the
    /// `<href>` payload of principal and home-set properties.
    pub text: String,
    /// The direct child elements (e.g. `collection` and `calendar` under
    /// `<resourcetype>`).
    pub children: Vec<WebdavPropChild>,
}

/// A direct child element of a property, for the properties whose value is
/// markup rather than text.
#[derive(Clone, Debug, Default)]
pub struct WebdavPropChild {
    /// Child element local name (e.g. `collection`, `comp`).
    pub local: String,
    /// The child's `name` attribute, when it carries one: RFC 4791 §5.2.3
    /// spells a component type as `<C:comp name="VEVENT"/>`, putting the value
    /// in the attribute rather than in a text node.
    pub name: Option<String>,
    /// The child's own children, a few property values being nested markup
    /// rather than a flat list: RFC 3253 §3.1.5 spells `supported-report-set`
    /// as one `<supported-report>` per report, each holding a `<report>`
    /// holding the report element itself.
    pub children: Vec<WebdavPropChild>,
}

/// The value a property is set to in a `PROPPATCH`, `MKCOL` or `MKCALENDAR`
/// body.
///
/// Most WebDAV properties carry text, but a few carry markup, which would not
/// survive text escaping: `supported-calendar-component-set` (RFC 4791 §5.2.3)
/// is a list of `<C:comp/>` children.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebdavPropValue<'a> {
    /// Text content, XML-escaped on the way out.
    Text(&'a str),
    /// A raw XML fragment, emitted verbatim; the caller owns its
    /// well-formedness and its escaping.
    Raw(String),
}

/// WebDAV namespace (RFC 4918), emitted with the `D` prefix the RFC examples
/// use and assumed by the literal `D:` of the body generators.
///
/// Never the default namespace: strict servers (iCloud, Google) answer HTTP 400
/// to a body mixing a prefixed CardDAV root with default-namespace DAV
/// children, while the all-prefixed form passes everywhere.
pub const DAV: WebdavNamespace = WebdavNamespace {
    uri: "DAV:",
    prefix: "D",
};
/// CalendarServer extension namespace (ctag), spoken by both CalDAV and CardDAV
/// servers.
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
/// `DAV:supported-report-set` (RFC 3253 §3.1.5), the reports a collection
/// advertises.
pub const SUPPORTED_REPORT_SET: WebdavProperty = WebdavProperty {
    ns: DAV,
    local: "supported-report-set",
};
/// `CS:getctag` (CalendarServer extension), bumped on every collection change.
pub const GETCTAG: WebdavProperty = WebdavProperty {
    ns: CALENDARSERVER,
    local: "getctag",
};

/// `DAV:propertyupdate` PROPPATCH request root (RFC 4918 §9.2).
const PROPERTYUPDATE: WebdavProperty = WebdavProperty {
    ns: DAV,
    local: "propertyupdate",
};

/// Emits the `xmlns` declarations of the given namespaces, deduped by URI and
/// in order. The empty-prefix namespace becomes the default namespace.
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

/// Builds a `PROPPATCH` request body (RFC 4918 §9.2) setting each `(property,
/// value)` pair and removing each property in `remove`.
///
/// A `DAV:propertyupdate` carries both instruction kinds, applied in document
/// order, and only a `DAV:remove` deletes a property: omitting a property from
/// `set` leaves it as it was. An empty instruction block is left out entirely.
pub fn proppatch_body(
    set: &[(WebdavProperty, WebdavPropValue<'_>)],
    remove: &[WebdavProperty],
) -> Vec<u8> {
    let mut props: Vec<WebdavProperty> = set.iter().map(|(prop, _)| *prop).collect();
    props.extend_from_slice(remove);
    let mut nss = namespaces(&[], &props);
    nss.push(PROPERTYUPDATE.ns);
    let decls = xmlns_decls(&nss);
    let open = qualified(PROPERTYUPDATE.ns, PROPERTYUPDATE.local);

    let mut body = format!("{XML_DECL}<{open}{decls}>");
    if !set.is_empty() {
        body.push_str("<D:set><D:prop>");
        for (prop, value) in set {
            body.push_str(&value_element(*prop, value));
        }
        body.push_str("</D:prop></D:set>");
    }
    if !remove.is_empty() {
        body.push_str("<D:remove>");
        body.push_str(&prop_block(remove));
        body.push_str("</D:remove>");
    }
    body.push_str(&format!("</{open}>"));
    body.into_bytes()
}

/// Builds a `<root><set><prop>...</prop></set></root>` body setting each
/// `(property, value)` pair, rooted at `root`.
///
/// Backs the creation requests, which have nothing to remove: extended `MKCOL`
/// (RFC 5689 §3) and CalDAV `MKCALENDAR` (RFC 4791 §5.3.1). [`proppatch_body`]
/// builds the update counterpart, which also carries `DAV:remove`.
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

/// Extracts a resource id from a `Location` header: its last path segment,
/// query and fragment dropped, or [`None`] when that segment is empty.
///
/// A server may store a created resource under a name of its own and report it
/// here (Google does), in which case that name, not the one the caller sent, is
/// what addresses the resource afterwards.
pub fn id_from_location(location: &str) -> Option<String> {
    let path = location
        .split(['?', '#'])
        .next()
        .unwrap_or(location)
        .trim_end_matches('/');
    let segment = path.rsplit('/').next().unwrap_or_default();
    (!segment.is_empty()).then(|| segment.to_string())
}

/// Builds an extended `MKCOL` request body (RFC 5689 §3): a `<resourcetype>` of
/// `<collection/>` plus `resource_types`, and each `set` property value.
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
/// fragment.
///
/// `extra_ns` declares the namespaces the filter needs beyond those of `root`
/// and `props`.
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
/// Matching is by local name, namespace prefixes ignored, and only properties
/// under 2xx `propstat`s land in `props`. A response without any 2xx propstat
/// still survives as an entry carrying its response-level status, which is what
/// `sync-collection` removal and truncation rows are. Predefined and numeric
/// character references are resolved, unknown ones kept verbatim, and malformed
/// input yields whatever was parsed before the error.
pub fn parse_multistatus(xml: &str) -> WebdavMultistatus {
    let mut reader = Reader::from_str(xml);

    let mut responses: Vec<WebdavResponseEntry> = Vec::new();
    let mut sync_token: Option<String> = None;
    // NOTE: local name, accumulated descendant text, direct children.
    let mut stack: Vec<(String, String, Vec<WebdavPropChild>)> = Vec::new();
    let mut response: Option<WebdavResponseEntry> = None;
    let mut propstat_props: Vec<WebdavPropItem> = Vec::new();
    let mut propstat_status: Option<u16> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let local = e.local_name().as_ref().to_string();
                if let Some((_, _, children)) = stack.last_mut() {
                    children.push(WebdavPropChild {
                        local: local.clone(),
                        name: name_attribute(&e),
                        ..Default::default()
                    });
                }
                match local.as_str() {
                    "response" => response = Some(WebdavResponseEntry::default()),
                    "propstat" => {
                        propstat_props.clear();
                        propstat_status = None;
                    }
                    _ => {}
                }
                stack.push((local, String::new(), Vec::new()));
            }
            Ok(Event::Empty(e)) => {
                let local = e.local_name().as_ref().to_string();
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
                        ..Default::default()
                    });
                }
            }
            Ok(Event::Text(t)) => {
                if let Some((_, buf, _)) = stack.last_mut() {
                    buf.push_str(&t.xml_content(XmlVersion::Implicit1_0));
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if let Some((_, buf, _)) = stack.last_mut() {
                    if let Ok(Some(ch)) = r.resolve_char_ref() {
                        buf.push(ch);
                    } else {
                        match r.as_ref() {
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
                if let Some((_, buf, _)) = stack.last_mut() {
                    buf.push_str(&t.into_inner());
                }
            }
            Ok(Event::End(_)) => {
                if let Some((name, text, children)) = stack.pop() {
                    let parent = stack.last().map(|(n, _, _)| n.clone());
                    if let Some((_, parent_text, parent_children)) = stack.last_mut() {
                        parent_text.push_str(&text);
                        // NOTE: the element being popped is the last child its
                        // parent pushed, so handing it its own children there
                        // is what keeps nested markup readable at any depth.
                        if let Some(entry) = parent_children.last_mut() {
                            entry.children.clone_from(&children);
                        }
                    }
                    let parent = parent.as_deref();

                    match name.as_str() {
                        "response" => {
                            if let Some(entry) = response.take() {
                                responses.push(entry);
                            }
                        }
                        "propstat" => {
                            if let Some(entry) = response.as_mut() {
                                match propstat_status {
                                    Some(status) if status / 100 == 2 => {
                                        entry.props.append(&mut propstat_props)
                                    }
                                    // NOTE: a refused propstat is where a
                                    // PROPPATCH says it changed nothing, so its
                                    // properties are kept as failures.
                                    Some(status) => entry.failures.extend(
                                        propstat_props.drain(..).map(|item| WebdavPropFailure {
                                            status,
                                            property: item.local,
                                        }),
                                    ),
                                    None => {}
                                }
                            }
                            propstat_props.clear();
                            propstat_status = None;
                        }
                        "status" if parent == Some("propstat") => {
                            propstat_status = status_code(&text);
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

/// Returns the value of the HTTP `Authorization` header for the given scheme,
/// or [`None`] when no header should be emitted.
pub fn emit_header(auth: &WebdavAuth) -> Option<String> {
    match auth {
        WebdavAuth::None => None,
        WebdavAuth::Basic(credentials) => Some(credentials.to_authorization()),
        WebdavAuth::Bearer(token) => Some(token.to_authorization()),
    }
}

/// Resolves `path` against `base_url`.
///
/// An empty path returns `base_url` unchanged, an absolute path replaces the
/// base path and a relative one is appended to it. Falls back to `base_url`
/// when the join fails.
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

/// Reads the `ETag` header (RFC 9110 §8.8.3) out of an HTTP response, stripping
/// the surrounding double quotes when present.
pub fn read_etag(response: &HttpResponse) -> Option<String> {
    response
        .header("etag")
        .map(|raw| raw.trim_matches('"').into())
}

/// Resolves an `<href>` value against `base_url`, joining when the href is
/// relative. Returns [`None`] when the href cannot be parsed.
pub fn resolve_href(base_url: &Url, href: &str) -> Option<Url> {
    match Url::parse(href) {
        Ok(url) => Some(url),
        Err(url::ParseError::RelativeUrlWithoutBase) => base_url.join(href).ok(),
        Err(_) => None,
    }
}

/// Trace-logs every property of `entry` whose local name is not in `known`, so
/// a `from_props` mapper surfaces what it ignores without failing.
pub fn trace_unrecognized(entry: &WebdavResponseEntry, known: &[WebdavProperty]) {
    for item in &entry.props {
        if !known.iter().any(|prop| prop.local == item.local) {
            trace!("ignoring unrecognized WebDAV property {}", item.local);
        }
    }
}

/// Extracts the numeric code out of an HTTP status line (e.g. `HTTP/1.1 404 Not
/// Found`).
fn status_code(text: &str) -> Option<u16> {
    text.split_whitespace().nth(1)?.parse().ok()
}

/// Maximum length of a summarised error body, in characters.
const SUMMARY_LEN: usize = 200;

/// Boils an error response body down to one readable line.
///
/// Servers answer an error with anything from a DAV XML condition to a full
/// HTML page (Fastmail) to nothing at all (iCloud). The DAV
/// `responsedescription` is the one part written for a human, so it wins, then
/// an HTML `title`; failing both, the markup is stripped, the whitespace
/// collapsed and the result capped. An empty body summarises to nothing, which
/// lets a caller drop the separator rather than end its message on a colon.
pub fn summarize_body(body: &str) -> String {
    let text = element_text(body, "responsedescription")
        .or_else(|| element_text(body, "title"))
        .unwrap_or_else(|| strip_markup(body));
    let text = text.trim();

    match text.char_indices().nth(SUMMARY_LEN) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

/// Returns the trimmed text of the first `<local>` element found, whatever its
/// namespace prefix, when it carries any.
fn element_text(body: &str, local: &str) -> Option<String> {
    let open = format!("{local}>");
    let start = body.find(&open)? + open.len();
    let rest = &body[start..];
    let end = rest.find("</")?;
    let text = rest[..end].trim();

    (!text.is_empty()).then(|| text.to_string())
}

/// Drops every `<...>` tag and collapses the remaining whitespace.
fn strip_markup(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut in_tag = false;

    for ch in body.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag => continue,
            ch if ch.is_whitespace() => {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            ch => out.push(ch),
        }
    }

    out
}

/// Renders a summarised body as an error-message suffix, or nothing at all when
/// there is no summary to show.
pub fn summarized(body: &str) -> String {
    match summarize_body(body) {
        summary if summary.is_empty() => String::new(),
        summary => format!(": {summary}"),
    }
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

/// Reads an element's `name` attribute, ignoring a malformed or non-UTF-8
/// value.
fn name_attribute(element: &BytesStart) -> Option<String> {
    let attribute = element.try_get_attribute("name").ok()??;
    // NOTE: the parser does not track the XML declaration, and a reader seeing
    // no version is told to assume XML 1.0, which every WebDAV body is.
    let value = attribute.normalized_value(XmlVersion::Implicit1_0).ok()?;
    Some(value.into_owned())
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
        let body = proppatch_body(&[(DISPLAYNAME, WebdavPropValue::Text("Renamed"))], &[]);
        let xml = core::str::from_utf8(&body).unwrap();
        assert!(xml.contains("<D:propertyupdate xmlns:D=\"DAV:\">"));
        assert!(
            xml.contains("<D:set><D:prop><D:displayname>Renamed</D:displayname></D:prop></D:set>")
        );
        assert!(!xml.contains("<D:remove>"));
        assert!(xml.ends_with("</D:propertyupdate>"));
    }

    #[test]
    fn proppatch_body_removes_the_given_properties() {
        const DESCRIPTION: WebdavProperty = WebdavProperty {
            ns: CALDAV,
            local: "calendar-description",
        };
        let body = proppatch_body(
            &[(DISPLAYNAME, WebdavPropValue::Text("Renamed"))],
            &[DESCRIPTION],
        );
        let xml = core::str::from_utf8(&body).unwrap();

        // NOTE: a set-only body would leave the removed property untouched,
        // which is the whole point of the second instruction block.
        assert!(xml.contains("xmlns:C=\"urn:ietf:params:xml:ns:caldav\""));
        assert!(
            xml.contains("<D:set><D:prop><D:displayname>Renamed</D:displayname></D:prop></D:set>")
        );
        assert!(xml.contains("<D:remove><D:prop><C:calendar-description/></D:prop></D:remove>"));
    }

    #[test]
    fn proppatch_body_leaves_out_empty_instruction_blocks() {
        let body = proppatch_body(&[], &[DISPLAYNAME]);
        let xml = core::str::from_utf8(&body).unwrap();
        assert!(!xml.contains("<D:set>"));
        assert!(xml.contains("<D:remove><D:prop><D:displayname/></D:prop></D:remove>"));

        let body = proppatch_body(&[], &[]);
        let xml = core::str::from_utf8(&body).unwrap();
        assert!(xml.contains("<D:propertyupdate xmlns:D=\"DAV:\"></D:propertyupdate>"));
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
        let body = proppatch_body(&[(COMPONENT_SET, comps)], &[]);
        let xml = core::str::from_utf8(&body).unwrap();
        // NOTE: escaping raw markup would turn the children into text and lose
        // the property's whole value.
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
        // NOTE: the value lives in the attribute, not in a text node, so a
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
