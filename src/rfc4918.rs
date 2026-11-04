//! # RFC 4918: HTTP Extensions for Web Distributed Authoring and Versioning
//!
//! <https://www.rfc-editor.org/rfc/rfc4918>
//!
//! This module carries the WebDAV vocabulary shared across every RFC layer:
//! the authentication scheme, the namespace and property model, the parsed
//! multistatus body and the `DAV:` property constants.
//!
//! Next to them live the helpers every coroutine reuses: the XML
//! request-body generators, the multistatus parser, the `Authorization`
//! header emitter, request-path resolution and `ETag` extraction. Each
//! WebDAV method is its own submodule.
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
pub mod typed;
pub(crate) mod xml;

use core::{fmt, mem};

use alloc::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    format,
    string::{String, ToString},
    vec::{self, Vec},
};

use io_http::{
    rfc6750::bearer::HttpAuthBearer, rfc7617::basic::HttpAuthBasic, rfc9110::response::HttpResponse,
};
use log::trace;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use crate::rfc4918::xml::{XmlEvent, XmlScanner, attr, decode_attr, decode_text};

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
/// Each RFC layer owns the namespaces it speaks (`DAV:` in
/// [`crate::rfc4918`], CalDAV in [`crate::rfc4791`], CardDAV in
/// [`crate::rfc6352`]), so the body generators read these fields alone.
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
    /// `sync-collection` removal row (RFC 6578 §3.4) or the 507 of a
    /// truncation row (§3.6); [`None`] on propstat-only responses.
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
    /// resource identifier.
    pub fn id(&self) -> &str {
        self.href
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
    }

    /// Returns the id of the collection this entry lists under the home set
    /// `home` (see [`collection_id`]).
    pub fn collection_id(&self, home: &str) -> String {
        collection_id(home, &self.href)
    }
}

/// Returns the id of the collection at `href` listed under the home set path
/// `home`: the last path segment when the collection is a direct child of
/// `home`, the href's path otherwise, starting with a slash and kept as the
/// server served it.
///
/// RFC 4791 and RFC 6352 make a collection URL opaque: Google lists every
/// calendar as `/caldav/v2/<calendar>/events/`, outside the home set, where a
/// last segment alone would name them all `events`. Both paths are compared
/// percent-decoded, a server being free to encode either differently.
pub fn collection_id(home: &str, href: &str) -> String {
    let path = href_path(href).trim_end_matches('/');
    let (parent, last) = path.rsplit_once('/').unwrap_or(("", path));

    // NOTE: a relative href names no parent to compare; it keeps the last
    // segment, as every id did before.
    if !path.starts_with('/') || decoded(parent) == decoded(home.trim_end_matches('/')) {
        return last.to_string();
    }

    path.to_string()
}

/// Returns the path a collection id addresses, trailing slash included: the
/// id itself when it starts with a slash (a collection outside the home set,
/// see [`collection_id`]), the home set path `home` joined with it otherwise.
pub fn collection_path(home: &str, id: &str) -> String {
    if id.starts_with('/') {
        return format!("{}/", id.trim_end_matches('/'));
    }

    let home = home.trim_end_matches('/');
    let id = id.trim_matches('/');
    format!("{home}/{id}/")
}

/// Returns the path of `href`, an absolute URL or a reference, without its
/// query or fragment.
fn href_path(href: &str) -> &str {
    let href = href.split(['?', '#']).next().unwrap_or(href);
    match href.split_once("://") {
        Some((_, rest)) => rest.find('/').map_or("/", |start| &rest[start..]),
        None => href,
    }
}

/// Percent-decodes `path` (RFC 3986 §2.1), leaving a malformed escape as it
/// stands.
fn decoded(path: &str) -> Vec<u8> {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))
            .and_then(|hex| u8::from_str_radix(core::str::from_utf8(hex).ok()?, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    out
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

/// Escapes a double-quoted XML attribute value (`&`, `<`, `>`, `"`).
pub fn escape_attr(value: &str) -> String {
    escape_text(value).replace('"', "&quot;")
}

/// Whether `body` holds an element named one of `locals`, whatever its
/// namespace prefix.
///
/// Read as XML rather than searched for as a substring, so a body merely
/// quoting the name is not one. Preconditions are matched this way, the RFCs
/// naming the element and only recommending the status that wraps it.
pub(crate) fn has_element(body: &str, locals: &[&str]) -> bool {
    XmlScanner::new(body)
        .any(|event| matches!(event, XmlEvent::Start { local, .. } if locals.contains(&local)))
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
/// Backs the creation requests, which have nothing to remove: extended
/// `MKCOL` (RFC 5689 §3) and CalDAV `MKCALENDAR` (RFC 4791 §5.3.1).
/// [`proppatch_body`] is the update counterpart, carrying `DAV:remove`.
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

/// Deepest element nesting [`parse_multistatus`] accepts.
///
/// RFC 4918 bodies nest about eight levels at most, the property values
/// included; the bound keeps the parse linear and the recursive drop of
/// [`WebdavPropChild`] off the end of the stack.
pub const MULTISTATUS_MAX_DEPTH: usize = 64;

/// Elements whose descendant text is never read.
const STRUCTURAL: [&str; 4] = ["multistatus", "response", "propstat", "prop"];

/// Failure causes while parsing a multistatus body.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum WebdavMultistatusParseError {
    /// The body nests elements deeper than the given depth.
    #[error("WebDAV multistatus nests elements deeper than {0}")]
    TooDeep(usize),
    /// The properties of a response do not fit the requested type.
    #[error("WebDAV multistatus response {href} has unreadable properties: {reason}")]
    InvalidProps {
        /// The href of the response.
        href: String,
        /// Why the properties did not deserialize.
        reason: String,
    },
}

/// A representation [`WebdavPropfind`](propfind::WebdavPropfind) and
/// [`WebdavReport`](report::WebdavReport) parse a multistatus body into.
///
/// Implemented by [`WebdavMultistatus`], vocabulary-agnostic, and by
/// [`WebdavMultistatusOf`](typed::WebdavMultistatusOf), typed by the caller.
pub trait WebdavMultistatusParse: Sized + sealed::Sealed {
    /// Parses a `multistatus` body.
    fn parse(xml: &str) -> Result<Self, WebdavMultistatusParseError>;
}

impl sealed::Sealed for WebdavMultistatus {}

impl WebdavMultistatusParse for WebdavMultistatus {
    fn parse(xml: &str) -> Result<Self, WebdavMultistatusParseError> {
        parse_multistatus(xml)
    }
}

pub(crate) mod sealed {
    pub trait Sealed {}
}

/// The `getetag` of a response, the one property enumerations read.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct WebdavEtagProps {
    pub getetag: Option<String>,
}

/// Returns an entity tag without its surrounding quotes, when non-empty.
pub(crate) fn unquoted_etag(etag: Option<String>) -> Option<String> {
    etag.filter(|etag| !etag.is_empty())
        .map(|etag| etag.trim_matches('"').to_string())
}

/// The local names of a property's children, e.g. the types a
/// `resourcetype` lists.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct WebdavChildNames(pub BTreeMap<String, ()>);

/// A `supported-report-set` (RFC 3253 §3.1.5): one `supported-report` per
/// report, each holding a `report` holding the report element itself.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct WebdavReportSet {
    #[serde(default)]
    pub supported_report: Vec<WebdavSupportedReport>,
}

/// One `supported-report` of a [`WebdavReportSet`].
#[derive(Debug, Default, Deserialize)]
pub(crate) struct WebdavSupportedReport {
    #[serde(default)]
    pub report: WebdavChildNames,
}

impl WebdavReportSet {
    /// Returns the local names of the advertised reports.
    pub(crate) fn reports(self) -> BTreeSet<String> {
        self.supported_report
            .into_iter()
            .flat_map(|supported| supported.report.0.into_keys())
            .collect()
    }
}

/// Returns `text` unless it is empty.
pub(crate) fn non_empty(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.is_empty())
}

/// An element open while parsing a multistatus body.
struct OpenElement<'a> {
    /// The local name.
    local: &'a str,
    /// The accumulated text, borrowed until a second piece joins it.
    text: Cow<'a, str>,
    /// The direct children.
    children: Vec<WebdavPropChild>,
    /// Whether the children surface, the element sitting in a property.
    keeps: bool,
    /// Whether the text is read.
    wants: bool,
}

/// Parses a `multistatus` body into vocabulary-agnostic entries.
///
/// Matching is by local name, prefixes ignored, and only properties under a
/// 2xx `propstat` land in `props`. A response carrying none survives as an
/// entry (removal and truncation rows); malformed input yields its prefix.
/// Nesting past [`MULTISTATUS_MAX_DEPTH`] fails.
pub fn parse_multistatus(xml: &str) -> Result<WebdavMultistatus, WebdavMultistatusParseError> {
    let mut responses: Vec<WebdavResponseEntry> = Vec::new();
    let mut sync_token: Option<String> = None;
    let mut stack: Vec<OpenElement> = Vec::with_capacity(16);
    let mut response: Option<WebdavResponseEntry> = None;
    let mut propstat_props: Vec<WebdavPropItem> = Vec::new();
    let mut propstat_status: Option<u16> = None;

    for event in XmlScanner::new(xml) {
        match event {
            XmlEvent::Start {
                local,
                attrs,
                empty,
            } => {
                let depth = stack.len();
                let parent = stack.last_mut();
                let child = |local: &str| WebdavPropChild {
                    local: local.to_string(),
                    name: attr(attrs, "name").map(decode_attr),
                    ..Default::default()
                };

                if empty {
                    match parent {
                        Some(open) if open.local == "prop" => {
                            propstat_props.push(WebdavPropItem {
                                local: local.to_string(),
                                ..Default::default()
                            });
                        }
                        Some(open) if open.keeps => open.children.push(child(local)),
                        _ => {}
                    }
                    continue;
                }

                if depth == MULTISTATUS_MAX_DEPTH {
                    let err = WebdavMultistatusParseError::TooDeep(MULTISTATUS_MAX_DEPTH);
                    return Err(err);
                }

                // NOTE: an element's children only surface inside a property,
                // and its text only when read or rising into a property, so
                // the envelope gathers neither.
                let read = matches!(local, "href" | "status" | "sync-token");
                let (keeps, wants) = match parent {
                    Some(open) => {
                        if open.keeps {
                            open.children.push(child(local));
                        }

                        let in_prop = open.local == "prop";
                        let rises = !in_prop && !STRUCTURAL.contains(&open.local) && open.wants;
                        (in_prop || open.keeps, in_prop || read || rises)
                    }
                    None => (false, read),
                };

                match local {
                    "response" => response = Some(WebdavResponseEntry::default()),
                    "propstat" => {
                        propstat_props.clear();
                        propstat_status = None;
                    }
                    _ => {}
                }

                stack.push(OpenElement {
                    local,
                    text: Cow::Borrowed(""),
                    children: Vec::new(),
                    keeps,
                    wants,
                });
            }
            XmlEvent::Text { raw, clean } => {
                if let Some(open) = stack.last_mut()
                    && open.wants
                {
                    if clean {
                        push_text(&mut open.text, Cow::Borrowed(raw));
                    } else {
                        decode_text(raw, open.text.to_mut());
                    }
                }
            }
            XmlEvent::Cdata(raw) => {
                if let Some(open) = stack.last_mut()
                    && open.wants
                {
                    push_text(&mut open.text, Cow::Borrowed(raw));
                }
            }
            XmlEvent::End(close) => {
                let Some(OpenElement {
                    local: name,
                    mut text,
                    mut children,
                    ..
                }) = stack.pop_if(|open| open.local == close)
                else {
                    break;
                };

                let parent = stack.last().map(|parent| parent.local);

                // NOTE: the popped element is its parent's last child, and
                // takes its children along. A property keeps them instead,
                // below.
                if let Some(open) = stack.last_mut()
                    && parent != Some("prop")
                {
                    if !STRUCTURAL.contains(&open.local) && open.wants {
                        let piece = match name {
                            "href" | "status" | "sync-token" => text.clone(),
                            _ => mem::take(&mut text),
                        };
                        push_text(&mut open.text, piece);
                    }

                    if open.keeps
                        && let Some(entry) = open.children.last_mut()
                    {
                        entry.children = mem::take(&mut children);
                    }
                }

                match name {
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
                                Some(status) => {
                                    entry.failures.extend(propstat_props.drain(..).map(|item| {
                                        WebdavPropFailure {
                                            status,
                                            property: item.local,
                                        }
                                    }))
                                }
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
                        if let Some(entry) = response.as_mut()
                            && entry.href.is_empty()
                        {
                            entry.href = text.trim().to_string();
                        }
                    }
                    _ if parent == Some("prop") => {
                        propstat_props.push(WebdavPropItem {
                            local: name.to_string(),
                            text: text.into_owned(),
                            children,
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(WebdavMultistatus {
        responses,
        sync_token,
    })
}

/// Appends a text piece, keeping the first one borrowed.
fn push_text<'a>(text: &mut Cow<'a, str>, piece: Cow<'a, str>) {
    if text.is_empty() {
        *text = piece;
    } else if !piece.is_empty() {
        text.to_mut().push_str(&piece);
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

    if path.starts_with('/')
        && let Ok(mut url) = Url::parse(base_url.as_str())
    {
        url.set_path(path);
        return url;
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
pub(crate) fn status_code(text: &str) -> Option<u16> {
    text.split_whitespace().nth(1)?.parse().ok()
}

/// Maximum length of a summarised error body, in characters.
const SUMMARY_LEN: usize = 200;

/// Boils an error response body down to one readable line.
///
/// Servers answer with a DAV condition, an HTML page (Fastmail) or nothing
/// (iCloud), so the human-written `responsedescription` wins, then an HTML
/// `title`, then stripped markup. An empty body summarises to nothing.
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
        let ms = parse_multistatus(xml).unwrap();
        let children = &ms.responses[0].prop(COMPONENT_SET).unwrap().children;
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

        let ms = parse_multistatus(xml).unwrap();
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

        let ms = parse_multistatus(xml).unwrap();
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
        let ms = parse_multistatus(xml).unwrap();
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
