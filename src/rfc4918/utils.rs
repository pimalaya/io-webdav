//! WebDAV shared helpers: the generic DAV property vocabulary, XML
//! request-body generators (PROPFIND / PROPPATCH / MKCOL / REPORT), the
//! multistatus parser, plus the `Authorization` header emitter, request
//! path resolution and `ETag` extraction.
//!
//! Request bodies are generated from a [`Property`] selector rather than
//! hard-coded templates: callers choose the properties/values they
//! need. Each [`Property`] carries its [`Namespace`] (URI plus preferred
//! prefix), so the generators emit XML without a central namespace
//! table; every RFC layer owns the namespaces and property constants it
//! speaks.
//!
//! [`Namespace`]: crate::rfc4918::Namespace

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use base64::{Engine, prelude::BASE64_STANDARD};
use io_http::rfc9110::response::HttpResponse;
use log::trace;
use quick_xml::{Reader, escape::unescape, events::Event};
use secrecy::ExposeSecret;
use url::Url;

use crate::rfc4918::types::{
    Multistatus, Namespace, PropItem, Property, ResponseEntry, WebdavAuth,
};

/// WebDAV namespace (RFC 4918), the XML default namespace.
pub const DAV: Namespace = Namespace {
    uri: "DAV:",
    prefix: "",
};

// --- generic DAV property vocabulary

/// `DAV:displayname` (RFC 4918 §15.2).
pub const DISPLAYNAME: Property = Property {
    ns: DAV,
    local: "displayname",
};
/// `DAV:resourcetype` (RFC 4918 §15.9).
pub const RESOURCETYPE: Property = Property {
    ns: DAV,
    local: "resourcetype",
};
/// `DAV:getetag` (RFC 4918 §15.6).
pub const GETETAG: Property = Property {
    ns: DAV,
    local: "getetag",
};

/// Emits the `xmlns` declarations for the given namespaces (deduped by
/// URI, in order). The empty-prefix namespace becomes the default
/// namespace.
pub fn xmlns_decls(namespaces: &[Namespace]) -> String {
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

/// Emits a `<prop>` block listing each property as an empty element.
pub fn prop_block(props: &[Property]) -> String {
    let mut out = String::from("<prop>");
    for prop in props {
        out.push_str(&empty_element(*prop));
    }
    out.push_str("</prop>");
    out
}

/// Builds a `PROPFIND` request body (RFC 4918 §9.1) requesting `props`.
pub fn propfind_body(props: &[Property]) -> Vec<u8> {
    let decls = xmlns_decls(&namespaces(&[], props));
    let mut body = format!("{XML_DECL}<propfind{decls}>");
    body.push_str(&prop_block(props));
    body.push_str("</propfind>");
    body.into_bytes()
}

/// Builds a `PROPPATCH` request body (RFC 4918 §9.2) setting each
/// `(property, value)` pair.
pub fn proppatch_body(set: &[(Property, &str)]) -> Vec<u8> {
    let props: Vec<Property> = set.iter().map(|(prop, _)| *prop).collect();
    let decls = xmlns_decls(&namespaces(&[], &props));
    let mut body = format!("{XML_DECL}<propertyupdate{decls}><set><prop>");
    for (prop, value) in set {
        body.push_str(&value_element(*prop, value));
    }
    body.push_str("</prop></set></propertyupdate>");
    body.into_bytes()
}

/// Builds an extended `MKCOL` request body (RFC 5689 §3): a
/// `<resourcetype>` of `<collection/>` plus `resource_types`, and each
/// `set` property value.
pub fn mkcol_body(resource_types: &[Property], set: &[(Property, &str)]) -> Vec<u8> {
    let mut props: Vec<Property> = resource_types.to_vec();
    props.extend(set.iter().map(|(prop, _)| *prop));
    let decls = xmlns_decls(&namespaces(&[], &props));

    let mut body = format!("{XML_DECL}<mkcol{decls}><set><prop><resourcetype><collection/>");
    for resource_type in resource_types {
        body.push_str(&empty_element(*resource_type));
    }
    body.push_str("</resourcetype>");
    for (prop, value) in set {
        body.push_str(&value_element(*prop, value));
    }
    body.push_str("</prop></set></mkcol>");
    body.into_bytes()
}

/// Builds a `REPORT` query body (RFC 3253 §3.6) rooted at `root` (e.g.
/// `calendar-query`), requesting `props` and appending the raw `filter`
/// fragment. `extra_ns` declares namespaces the filter needs beyond
/// those of `root` and `props`.
pub fn report_query_body(
    root: Property,
    extra_ns: &[Namespace],
    props: &[Property],
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
/// properties under 2xx `propstat`s are kept. Malformed input yields
/// whatever was parsed before the error.
pub fn parse_multistatus(xml: &str) -> Multistatus {
    let mut reader = Reader::from_str(xml);

    let mut responses: Vec<ResponseEntry> = Vec::new();
    // (local name, accumulated descendant text, direct child names)
    let mut stack: Vec<(String, String, Vec<String>)> = Vec::new();
    let mut response: Option<ResponseEntry> = None;
    let mut propstat_props: Vec<PropItem> = Vec::new();
    let mut propstat_ok: Option<bool> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = local_name(e.local_name().as_ref());
                if let Some((_, _, children)) = stack.last_mut() {
                    children.push(name.clone());
                }
                match name.as_str() {
                    "response" => response = Some(ResponseEntry::default()),
                    "propstat" => {
                        propstat_props.clear();
                        propstat_ok = None;
                    }
                    _ => {}
                }
                stack.push((name, String::new(), Vec::new()));
            }
            Ok(Event::Empty(e)) => {
                let name = local_name(e.local_name().as_ref());
                let parent_is_prop = stack.last().is_some_and(|(n, _, _)| n == "prop");
                if parent_is_prop {
                    propstat_props.push(PropItem {
                        local: name,
                        ..Default::default()
                    });
                } else if let Some((_, _, children)) = stack.last_mut() {
                    children.push(name);
                }
            }
            Ok(Event::Text(t)) => {
                if let Ok(decoded) = t.decode() {
                    let text = match unescape(&decoded) {
                        Ok(text) => text,
                        Err(_) => decoded.clone(),
                    };
                    if let Some((_, buf, _)) = stack.last_mut() {
                        buf.push_str(&text);
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
                let Some((name, text, children)) = stack.pop() else {
                    continue;
                };
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
                        propstat_ok = Some(text.contains(" 2"));
                    }
                    "href" if parent == Some("response") => {
                        if let Some(entry) = response.as_mut() {
                            if entry.href.is_empty() {
                                entry.href = text.trim().to_string();
                            }
                        }
                    }
                    _ if parent == Some("prop") => {
                        propstat_props.push(PropItem {
                            local: name,
                            text,
                            children,
                        });
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    Multistatus { responses }
}

// --- HTTP plumbing

/// Returns the value of the HTTP `Authorization` header for the given
/// scheme, or [`None`] when no header should be emitted.
pub fn emit_header(auth: &WebdavAuth) -> Option<String> {
    match auth {
        WebdavAuth::None => None,
        WebdavAuth::Basic { username, password } => {
            let password = password.expose_secret();
            let digest = BASE64_STANDARD.encode(format!("{username}:{password}"));
            Some(format!("Basic {digest}"))
        }
        WebdavAuth::Bearer { token } => Some(format!("Bearer {}", token.expose_secret())),
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
pub fn trace_unrecognized(entry: &ResponseEntry, known: &[Property]) {
    for item in &entry.props {
        if !known.iter().any(|prop| prop.local == item.local) {
            trace!("ignoring unrecognized WebDAV property `{}`", item.local);
        }
    }
}

// --- private helpers

const XML_DECL: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>";

/// Collects `DAV:` plus `extra` plus every property namespace.
fn namespaces(extra: &[Namespace], props: &[Property]) -> Vec<Namespace> {
    let mut nss = Vec::with_capacity(1 + extra.len() + props.len());
    nss.push(DAV);
    nss.extend_from_slice(extra);
    nss.extend(props.iter().map(|prop| prop.ns));
    nss
}

fn qualified(ns: Namespace, local: &str) -> String {
    if ns.prefix.is_empty() {
        local.to_string()
    } else {
        format!("{}:{local}", ns.prefix)
    }
}

fn empty_element(prop: Property) -> String {
    format!("<{}/>", qualified(prop.ns, prop.local))
}

fn value_element(prop: Property, value: &str) -> String {
    let name = qualified(prop.ns, prop.local);
    format!("<{name}>{}</{name}>", escape_text(value))
}

fn local_name(bytes: &[u8]) -> String {
    core::str::from_utf8(bytes).unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use alloc::{string::ToString, vec};

    use secrecy::SecretString;

    use crate::rfc4918::{
        types::{Namespace, Property, WebdavAuth},
        utils::*,
    };

    const CALDAV: Namespace = Namespace {
        uri: "urn:ietf:params:xml:ns:caldav",
        prefix: "C",
    };
    const CALENDAR: Property = Property {
        ns: CALDAV,
        local: "calendar",
    };
    const CALENDAR_DATA: Property = Property {
        ns: CALDAV,
        local: "calendar-data",
    };

    #[test]
    fn propfind_body_lists_props_with_namespaces() {
        let body = propfind_body(&[DISPLAYNAME, CALENDAR_DATA]);
        let xml = core::str::from_utf8(&body).unwrap();
        assert!(xml.contains("xmlns=\"DAV:\""));
        assert!(xml.contains("xmlns:C=\"urn:ietf:params:xml:ns:caldav\""));
        assert!(xml.contains("<displayname/>"));
        assert!(xml.contains("<C:calendar-data/>"));
    }

    #[test]
    fn mkcol_body_carries_resourcetype_and_values() {
        let body = mkcol_body(&[CALENDAR], &[(DISPLAYNAME, "Personal & co")]);
        let xml = core::str::from_utf8(&body).unwrap();
        assert!(xml.contains("<resourcetype><collection/><C:calendar/></resourcetype>"));
        assert!(xml.contains("<displayname>Personal &amp; co</displayname>"));
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

        let principal = Property {
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
        let auth = WebdavAuth::Basic {
            username: "alice".into(),
            password: SecretString::from("secret".to_string()),
        };
        // NOTE: base64("alice:secret") = "YWxpY2U6c2VjcmV0"
        assert_eq!(emit_header(&auth).unwrap(), "Basic YWxpY2U6c2VjcmV0");
    }

    #[test]
    fn bearer_prepends_scheme() {
        let auth = WebdavAuth::Bearer {
            token: SecretString::from("xyz".to_string()),
        };
        assert_eq!(emit_header(&auth).unwrap(), "Bearer xyz");
    }

    #[test]
    fn getetag_uses_default_namespace() {
        assert_eq!(empty_or(GETETAG), "<getetag/>");
    }

    fn empty_or(prop: Property) -> String {
        let body = propfind_body(&[prop]);
        let xml = core::str::from_utf8(&body).unwrap().to_string();
        let start = xml.find("<prop>").unwrap() + "<prop>".len();
        let end = xml.find("</prop>").unwrap();
        xml[start..end].to_string()
    }
}
