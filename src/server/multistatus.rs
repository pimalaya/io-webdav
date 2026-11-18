//! # Multistatus writer
//!
//! [`WebdavMultistatusBody`] is the `207 Multi-Status` body a server answers
//! `PROPFIND`, `PROPPATCH` and `REPORT` with (RFC 4918 §13): one
//! [`WebdavResponseBody`] per resource, its properties grouped by status
//! into propstats, or a response-level status such as the 404 of a member
//! a `sync-collection` reports removed (RFC 6578 §3.4).
//!
//! Known namespaces are declared once on the root, under the prefixes of
//! [`NAMESPACE_DECLS`]; an element of any other namespace declares its own.

use alloc::{format, string::String, vec::Vec};

use crate::{
    rfc4791::calendar::{APPLE_ICAL, CALDAV},
    rfc4918::{CALENDARSERVER, DAV, WebdavProperty, XML_DECL, escape_attr, escape_text},
    rfc6352::addressbook::CARDDAV,
    server::{
        lock::{WebdavActiveLock, WebdavLockScope},
        name::WebdavPropName,
        precondition::WebdavPrecondition,
    },
};

/// The namespace declarations of a written root element: `D` for `DAV:`,
/// `C` for CalDAV, `CR` for CardDAV, `CS` for CalendarServer and `A` for
/// Apple iCal.
pub const NAMESPACE_DECLS: &str = " xmlns:D=\"DAV:\" \
    xmlns:C=\"urn:ietf:params:xml:ns:caldav\" \
    xmlns:CR=\"urn:ietf:params:xml:ns:carddav\" \
    xmlns:CS=\"http://calendarserver.org/ns/\" \
    xmlns:A=\"http://apple.com/ns/ical/\"";

/// A `DAV:multistatus` body (RFC 4918 §14.16).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebdavMultistatusBody {
    /// One response per resource.
    pub responses: Vec<WebdavResponseBody>,
    /// The `DAV:responsedescription`.
    pub description: Option<String>,
    /// The `DAV:sync-token` a `sync-collection` answers with (RFC 6578
    /// §6.2).
    pub sync_token: Option<String>,
}

/// A `DAV:response` (RFC 4918 §14.24).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebdavResponseBody {
    /// The resource href, percent-encoded as it is served.
    pub href: String,
    /// A response-level status, which replaces the propstats.
    pub status: Option<u16>,
    /// The properties, grouped by status.
    pub propstats: Vec<WebdavPropstatBody>,
    /// The condition the request failed on for this resource.
    pub error: Option<WebdavPrecondition>,
    /// The `DAV:responsedescription`.
    pub description: Option<String>,
}

/// A `DAV:propstat` (RFC 4918 §14.22).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebdavPropstatBody {
    /// The status every property shares.
    pub status: u16,
    /// The properties.
    pub props: Vec<WebdavPropBody>,
    /// The condition the properties failed on.
    pub error: Option<WebdavPrecondition>,
}

/// A property of a propstat.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebdavPropBody {
    /// The property name.
    pub name: WebdavPropName,
    /// The value.
    pub content: WebdavPropContent,
}

/// The value of a written property.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebdavPropContent {
    /// No value: the name of a `propname` answer or of a failed property.
    Empty,
    /// Text, escaped on the way out, a carriage return as `&#13;` so an
    /// iCalendar or vCard keeps its CRLF line ends.
    Text(String),
    /// One `DAV:href`, such as `current-user-principal`.
    Href(String),
    /// Several `DAV:href`s, such as a home set.
    Hrefs(Vec<String>),
    /// A raw XML fragment written inside the property element; its
    /// prefixes are those of [`NAMESPACE_DECLS`] or declared by itself.
    Xml(String),
    /// A whole element written in place of the property element, such as
    /// the [`xml`](crate::server::body::WebdavPropElement::xml) of a dead
    /// property.
    Element(String),
    /// The elements of a `DAV:resourcetype`, such as `DAV:collection`.
    ResourceType(Vec<WebdavProperty>),
    /// The component names of a `C:supported-calendar-component-set`.
    Components(Vec<String>),
    /// The media types of a `C:supported-calendar-data`.
    CalendarDataTypes(Vec<WebdavDataType>),
    /// The media types of a `CR:supported-address-data`.
    AddressDataTypes(Vec<WebdavDataType>),
    /// The privileges of a `DAV:current-user-privilege-set`, such as
    /// `DAV:read` (RFC 3744 §5.4).
    Privileges(Vec<WebdavProperty>),
    /// The reports of a `DAV:supported-report-set`, such as
    /// `C:calendar-query`.
    Reports(Vec<WebdavProperty>),
    /// The locks of a `DAV:lockdiscovery`.
    Locks(Vec<WebdavActiveLock>),
    /// The write lock scopes of a `DAV:supportedlock`.
    LockScopes(Vec<WebdavLockScope>),
}

/// A media type a collection accepts (RFC 4791 §9.6, RFC 6352 §10.4).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebdavDataType {
    /// The media type, such as `text/vcard`.
    pub content_type: String,
    /// Its version, such as `4.0`.
    pub version: String,
}

impl WebdavMultistatusBody {
    /// Writes the body.
    pub fn to_xml(&self) -> String {
        let mut xml = format!("{XML_DECL}<D:multistatus{NAMESPACE_DECLS}>");

        for response in &self.responses {
            response.write(&mut xml);
        }

        if let Some(description) = &self.description {
            let description = escape_text(description);
            xml.push_str(&format!(
                "<D:responsedescription>{description}</D:responsedescription>"
            ));
        }

        if let Some(token) = &self.sync_token {
            let token = escape_text(token);
            xml.push_str(&format!("<D:sync-token>{token}</D:sync-token>"));
        }

        xml.push_str("</D:multistatus>");
        xml
    }
}

impl WebdavResponseBody {
    /// Adds a property to the propstat of `status`, opening it if needed.
    pub fn push(&mut self, status: u16, prop: WebdavPropBody) {
        match self.propstats.iter_mut().find(|p| p.status == status) {
            Some(propstat) => propstat.props.push(prop),
            None => self.propstats.push(WebdavPropstatBody {
                status,
                props: Vec::from([prop]),
                error: None,
            }),
        }
    }

    fn write(&self, xml: &mut String) {
        let href = escape_text(&self.href);
        xml.push_str(&format!("<D:response><D:href>{href}</D:href>"));

        match self.status {
            Some(status) => push_status(xml, status),
            None => {
                for propstat in &self.propstats {
                    propstat.write(xml);
                }
            }
        }

        if let Some(error) = &self.error {
            xml.push_str(&format!("<D:error>{}</D:error>", error.to_xml()));
        }

        if let Some(description) = &self.description {
            let description = escape_text(description);
            xml.push_str(&format!(
                "<D:responsedescription>{description}</D:responsedescription>"
            ));
        }

        xml.push_str("</D:response>");
    }
}

impl WebdavPropstatBody {
    fn write(&self, xml: &mut String) {
        xml.push_str("<D:propstat><D:prop>");
        for prop in &self.props {
            prop.write(xml);
        }
        xml.push_str("</D:prop>");

        push_status(xml, self.status);

        if let Some(error) = &self.error {
            xml.push_str(&format!("<D:error>{}</D:error>", error.to_xml()));
        }

        xml.push_str("</D:propstat>");
    }
}

impl WebdavPropBody {
    fn write(&self, xml: &mut String) {
        if let WebdavPropContent::Element(element) = &self.content {
            xml.push_str(element);
            return;
        }

        let (open, close) = tags(self.name.ns(), self.name.local());

        if self.content == WebdavPropContent::Empty {
            xml.push_str(&format!("<{open}/>"));
            return;
        }

        xml.push_str(&format!("<{open}>"));

        match &self.content {
            WebdavPropContent::Text(text) => {
                xml.push_str(&escape_text(text).replace('\r', "&#13;"))
            }
            WebdavPropContent::Href(href) => push_href(xml, href),
            WebdavPropContent::Hrefs(hrefs) => {
                for href in hrefs {
                    push_href(xml, href);
                }
            }
            WebdavPropContent::Xml(fragment) => xml.push_str(fragment),
            WebdavPropContent::ResourceType(types) => {
                for ty in types {
                    push_empty(xml, *ty);
                }
            }
            WebdavPropContent::Components(names) => {
                for name in names {
                    let name = escape_attr(name);
                    xml.push_str(&format!("<C:comp name=\"{name}\"/>"));
                }
            }
            WebdavPropContent::CalendarDataTypes(types) => {
                push_data_types(xml, "C:calendar-data", types)
            }
            WebdavPropContent::AddressDataTypes(types) => {
                push_data_types(xml, "CR:address-data-type", types)
            }
            WebdavPropContent::Privileges(privileges) => {
                for privilege in privileges {
                    xml.push_str("<D:privilege>");
                    push_empty(xml, *privilege);
                    xml.push_str("</D:privilege>");
                }
            }
            WebdavPropContent::Reports(reports) => {
                for report in reports {
                    xml.push_str("<D:supported-report><D:report>");
                    push_empty(xml, *report);
                    xml.push_str("</D:report></D:supported-report>");
                }
            }
            WebdavPropContent::Locks(locks) => {
                for lock in locks {
                    xml.push_str(&lock.to_xml());
                }
            }
            WebdavPropContent::LockScopes(scopes) => {
                for scope in scopes {
                    let scope = scope.as_str();
                    xml.push_str(&format!(
                        "<D:lockentry><D:lockscope><D:{scope}/></D:lockscope>\
                         <D:locktype><D:write/></D:locktype></D:lockentry>"
                    ));
                }
            }
            WebdavPropContent::Empty | WebdavPropContent::Element(_) => {}
        }

        xml.push_str(&format!("</{close}>"));
    }
}

/// The prefix [`NAMESPACE_DECLS`] binds `uri` to.
fn prefix(uri: &str) -> Option<&'static str> {
    match uri {
        uri if uri == DAV.uri => Some("D"),
        uri if uri == CALDAV.uri => Some("C"),
        uri if uri == CARDDAV.uri => Some("CR"),
        uri if uri == CALENDARSERVER.uri => Some("CS"),
        uri if uri == APPLE_ICAL.uri => Some("A"),
        _ => None,
    }
}

/// The open tag content and the close tag name of element `local` in
/// namespace `ns`, the namespace declared when the root does not.
fn tags(ns: &str, local: &str) -> (String, String) {
    match prefix(ns) {
        Some(prefix) => {
            let name = format!("{prefix}:{local}");
            (name.clone(), name)
        }
        None if ns.is_empty() => (local.into(), local.into()),
        None => {
            let ns = escape_attr(ns);
            (format!("X:{local} xmlns:X=\"{ns}\""), format!("X:{local}"))
        }
    }
}

fn push_empty(xml: &mut String, element: WebdavProperty) {
    let (open, _) = tags(element.ns.uri, element.local);
    xml.push_str(&format!("<{open}/>"));
}

fn push_href(xml: &mut String, href: &str) {
    xml.push_str(&format!("<D:href>{}</D:href>", escape_text(href)));
}

fn push_data_types(xml: &mut String, element: &str, types: &[WebdavDataType]) {
    for ty in types {
        let content_type = escape_attr(&ty.content_type);
        let version = escape_attr(&ty.version);
        xml.push_str(&format!(
            "<{element} content-type=\"{content_type}\" version=\"{version}\"/>"
        ));
    }
}

fn push_status(xml: &mut String, status: u16) {
    let reason = reason(status);
    xml.push_str(&format!("<D:status>HTTP/1.1 {status} {reason}</D:status>"));
}

/// The reason phrase of a status code (RFC 9110 §15, RFC 4918 §11).
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        207 => "Multi-Status",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        412 => "Precondition Failed",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Content",
        423 => "Locked",
        424 => "Failed Dependency",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        507 => "Insufficient Storage",
        _ => "Status",
    }
}
