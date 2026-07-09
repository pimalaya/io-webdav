//! Offline coverage of the WebDAV core layer (RFC 4918): the request
//! builder, the XML body generators, the multistatus parser, the shared
//! helpers and every generic coroutine, all resumed against scripted
//! HTTP response bytes.

mod common;

use common::*;
use io_webdav::rfc4918::{
    DAV, DISPLAYNAME, GETETAG, Namespace, PropItem, Property, RESOURCETYPE, ResponseEntry,
    WebdavAuth,
    copy::Copy as CopyResource,
    delete::Delete,
    follow_redirects::{FollowRedirects, FollowRedirectsError},
    get::Get,
    mkcol::Mkcol,
    r#move::Move,
    options::Options,
    parse_multistatus,
    propfind::Propfind,
    proppatch::Proppatch,
    put::{Put, PutArgs},
    read_etag,
    report::Report,
    report_query_body,
    request::WebdavRequest,
    resolve, resolve_href,
    send::{SendError, SendRaw},
    trace_unrecognized, xmlns_decls,
};
use url::Url;

const UA: &str = "io-webdav/test";

const CALDAV: Namespace = Namespace {
    uri: "urn:ietf:params:xml:ns:caldav",
    prefix: "C",
};
const CALENDAR: Property = Property {
    ns: CALDAV,
    local: "calendar",
};

fn base() -> Url {
    Url::parse("https://dav.example.org/dav/").unwrap()
}

// --- XML generators ------------------------------------------------------

#[test]
fn xmlns_decls_dedupes_and_supports_the_default_namespace() {
    const DEFAULT: Namespace = Namespace {
        uri: "urn:x-test:default",
        prefix: "",
    };
    let decls = xmlns_decls(&[DAV, DAV, DEFAULT, CALDAV]);
    assert_eq!(
        decls,
        " xmlns:D=\"DAV:\" xmlns=\"urn:x-test:default\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\""
    );
}

#[test]
fn report_query_body_supports_an_unprefixed_root() {
    const ROOT: Property = Property {
        ns: Namespace {
            uri: "urn:x-test:default",
            prefix: "",
        },
        local: "custom-query",
    };
    let body = report_query_body(ROOT, &[], &[GETETAG], "<x/>");
    let xml = core::str::from_utf8(&body).unwrap();
    assert!(xml.contains("<custom-query"));
    assert!(xml.ends_with("</custom-query>"));
    assert!(xml.contains("<D:prop><D:getetag/></D:prop><x/>"));
}

// --- multistatus parser --------------------------------------------------

#[test]
fn parse_multistatus_reads_cdata_and_resolves_entities() {
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href><![CDATA[/dav/books/contacts/]]></d:href>
        <d:href>/dav/ignored-second-href/</d:href>
        <d:propstat>
          <d:prop>
            <d:displayname>A &amp; B &lt;&gt; &quot;&apos; &#x21; &bogus;</d:displayname>
            <d:getetag/>
          </d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let ms = parse_multistatus(xml);
    let entry = &ms.responses[0];
    assert_eq!(entry.href, "/dav/books/contacts/");
    assert_eq!(entry.id(), "contacts");
    // NOTE: predefined and numeric references resolve; the unknown
    // entity is kept verbatim.
    assert_eq!(entry.text(DISPLAYNAME), Some("A & B <> \"' ! &bogus;"));

    let etag = entry.prop(GETETAG).expect("empty getetag kept as a prop");
    assert!(etag.text.is_empty());
    assert!(entry.text(GETETAG).is_none());
}

#[test]
fn parse_multistatus_ignores_unparsable_status_lines() {
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/dav/a/</d:href>
        <d:status>garbage</d:status>
        <d:propstat>
          <d:prop><d:displayname>A</d:displayname></d:prop>
          <d:status>HTTP/1.1 nonsense</d:status>
        </d:propstat>
      </d:response>
      <d:sync-token>   </d:sync-token>
    </d:multistatus>"#;

    let ms = parse_multistatus(xml);
    let entry = &ms.responses[0];
    // NOTE: unparsable statuses resolve to no code, so the propstat is
    // not 2xx and its props are dropped; the blank sync-token is ignored.
    assert_eq!(entry.status, None);
    assert!(entry.props.is_empty());
    assert!(ms.sync_token.is_none());
}

#[test]
fn parse_multistatus_survives_malformed_xml() {
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/dav/a/</d:href>
        <d:propstat>
          <d:prop><d:displayname>A</d:displayname></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response><d:href>/dav/b/</d:href><broken"#;

    let ms = parse_multistatus(xml);
    assert_eq!(ms.responses.len(), 1);
    assert_eq!(ms.responses[0].text(DISPLAYNAME), Some("A"));

    // NOTE: a stray entity reference before the root element must not
    // derail the parse either.
    let ms = parse_multistatus("&amp;<d:multistatus xmlns:d=\"DAV:\"/>");
    assert!(ms.responses.is_empty());
}

#[test]
fn multistatus_iterates_over_its_responses() {
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response><d:href>/a/</d:href></d:response>
      <d:response><d:href>/b/</d:href></d:response>
    </d:multistatus>"#;

    let hrefs: Vec<String> = parse_multistatus(xml)
        .into_iter()
        .map(|entry| entry.href)
        .collect();
    assert_eq!(hrefs, ["/a/", "/b/"]);
}

#[test]
fn response_entry_helpers_handle_missing_data() {
    let entry = ResponseEntry {
        href: String::new(),
        status: None,
        props: vec![PropItem {
            local: "unknown-prop".into(),
            text: "   ".into(),
            children: Vec::new(),
        }],
    };

    assert_eq!(entry.id(), "");
    assert!(entry.prop(DISPLAYNAME).is_none());
    let unknown = Property {
        ns: DAV,
        local: "unknown-prop",
    };
    assert!(entry.text(unknown).is_none(), "blank text is filtered");
    assert!(!entry.has_resource_type(RESOURCETYPE, CALENDAR));

    // NOTE: only logs; must not panic on unrecognized properties.
    trace_unrecognized(&entry, &[DISPLAYNAME]);
}

// --- request builder and path resolution ---------------------------------

#[test]
fn request_carries_host_port_auth_and_conditional_headers() {
    let base = Url::parse("https://dav.example.org:8443/dav/").unwrap();
    let auth = basic_auth("alice", "secret");
    let request = WebdavRequest::put(&base, &auth, UA, "file.ics")
        .content_type_ical()
        .if_match("etag-1")
        .if_none_match("*")
        .body(b"BEGIN:VCALENDAR".to_vec());

    let headers = &request.headers;
    let header = |name: &str| {
        headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    };

    assert_eq!(request.method, "PUT");
    assert_eq!(header("Host"), Some("dav.example.org:8443"));
    assert_eq!(header("User-Agent"), Some(UA));
    assert_eq!(header("Content-Type"), Some("text/calendar; charset=utf-8"));
    assert_eq!(header("If-Match"), Some("\"etag-1\""));
    assert_eq!(header("If-None-Match"), Some("*"));
    assert!(header("Authorization").unwrap().starts_with("Basic "));
    assert_eq!(request.body, b"BEGIN:VCALENDAR");
}

#[test]
fn request_passes_quoted_and_weak_etags_through() {
    let request = WebdavRequest::delete(&base(), &WebdavAuth::None, UA, "x")
        .if_match("\"already-quoted\"")
        .body(Vec::new());
    assert!(
        request
            .headers
            .iter()
            .any(|(n, v)| n == "If-Match" && v == "\"already-quoted\"")
    );

    let request = WebdavRequest::delete(&base(), &WebdavAuth::None, UA, "x")
        .if_match("W/\"weak\"")
        .body(Vec::new());
    assert!(
        request
            .headers
            .iter()
            .any(|(n, v)| n == "If-Match" && v == "W/\"weak\"")
    );
}

#[test]
fn request_against_a_hostless_url_omits_the_host_header() {
    let base = Url::parse("urn:example:dav").unwrap();
    let request = WebdavRequest::options(&base, &WebdavAuth::None, UA, "").body(Vec::new());
    assert!(request.headers.iter().all(|(n, _)| n != "Host"));
}

#[test]
fn content_type_shortcuts_set_the_expected_values() {
    let request = WebdavRequest::put(&base(), &WebdavAuth::None, UA, "x.vcf")
        .content_type_vcard()
        .body(Vec::new());
    assert!(
        request
            .headers
            .iter()
            .any(|(n, v)| n == "Content-Type" && v == "text/vcard; charset=utf-8")
    );
}

#[test]
fn resolve_appends_a_slash_to_slashless_bases() {
    let base = Url::parse("https://dav.example.org/dav").unwrap();
    let url = resolve(&base, "personal");
    assert_eq!(url.as_str(), "https://dav.example.org/dav/personal");
}

#[test]
fn resolve_falls_back_to_the_base_on_unjoinable_paths() {
    let url = resolve(&base(), "http://exa mple.org/x");
    assert_eq!(url, base());
}

#[test]
fn resolve_href_joins_relative_and_rejects_invalid() {
    let absolute = resolve_href(&base(), "https://other.example.org/p/").unwrap();
    assert_eq!(absolute.as_str(), "https://other.example.org/p/");

    let relative = resolve_href(&base(), "/principals/alice/").unwrap();
    assert_eq!(
        relative.as_str(),
        "https://dav.example.org/principals/alice/"
    );

    assert!(resolve_href(&base(), "http://exa mple.org/x").is_none());
}

// --- send coroutines ------------------------------------------------------

#[test]
fn send_raw_returns_the_response_body() {
    let request = WebdavRequest::get(&base(), &WebdavAuth::None, UA, "file.txt").body(Vec::new());
    let mut send = SendRaw::new(request);

    let (request, ok) = expect_exchange(&mut send, &http_response("200 OK", &[], "hello"));
    let ok = ok.unwrap();
    assert!(request.starts_with("get /dav/file.txt http/1.1\r\n"));
    assert_eq!(ok.body, b"hello");
    assert_eq!(*ok.response.status, 200);
    assert!(ok.keep_alive);
}

#[test]
fn send_raw_maps_failure_statuses_to_http_status() {
    let request = WebdavRequest::get(&base(), &WebdavAuth::None, UA, "x").body(Vec::new());
    let mut send = SendRaw::new(request);

    let (_, ret) = expect_exchange(&mut send, &http_response("404 Not Found", &[], "nope"));
    let err = ret.unwrap_err();
    let SendError::HttpStatus(status, body) = err else {
        panic!("expected HttpStatus, got {err:?}");
    };
    assert_eq!(status, 404);
    assert_eq!(body, "nope");
}

#[test]
fn send_raw_rejects_redirects() {
    let request = WebdavRequest::get(&base(), &WebdavAuth::None, UA, "x").body(Vec::new());
    let mut send = SendRaw::new(request);

    let reply = http_response("301 Moved Permanently", &[("Location", "/new")], "");
    let (_, ret) = expect_exchange(&mut send, &reply);
    assert!(matches!(ret.unwrap_err(), SendError::UnexpectedRedirect));
}

#[test]
fn send_raw_surfaces_transport_errors() {
    let request = WebdavRequest::get(&base(), &WebdavAuth::None, UA, "x").body(Vec::new());
    let mut send = SendRaw::new(request);

    // NOTE: an immediate EOF while reading the response head surfaces
    // the underlying HTTP/1.1 send error.
    let (_, ret) = expect_exchange(&mut send, b"");
    assert!(matches!(ret.unwrap_err(), SendError::Send(_)));
}

#[test]
fn follow_redirects_returns_the_success_body() {
    let request = WebdavRequest::propfind(&base(), &WebdavAuth::None, UA, "").body(Vec::new());
    let mut send = FollowRedirects::new(request);

    let (_, ret) = expect_redirect_exchange(&mut send, &http_response("200 OK", &[], "body"));
    assert_eq!(ret.unwrap().body, b"body");
}

#[test]
fn follow_redirects_surfaces_the_redirect() {
    let request = WebdavRequest::propfind(&base(), &WebdavAuth::None, UA, "").body(Vec::new());
    let mut send = FollowRedirects::new(request);

    expect_redirect_wants_write(&mut send, None);
    expect_redirect_wants_read(&mut send);

    let reply = http_response("301 Moved Permanently", &[("Location", "/dav2/")], "");
    let (url, keep_alive, same_origin) = expect_wants_redirect(&mut send, &reply);
    assert_eq!(url.as_str(), "https://dav.example.org/dav2/");
    assert!(keep_alive);
    assert!(same_origin);
}

#[test]
fn follow_redirects_maps_failure_statuses_and_transport_errors() {
    let request = WebdavRequest::propfind(&base(), &WebdavAuth::None, UA, "").body(Vec::new());
    let mut send = FollowRedirects::new(request);
    let (_, ret) = expect_redirect_exchange(&mut send, &http_response("403 Forbidden", &[], "no"));
    let err = ret.unwrap_err();
    let FollowRedirectsError::HttpStatus(status, body) = err else {
        panic!("expected HttpStatus, got {err:?}");
    };
    assert_eq!(status, 403);
    assert_eq!(body, "no");

    let request = WebdavRequest::propfind(&base(), &WebdavAuth::None, UA, "").body(Vec::new());
    let mut send = FollowRedirects::new(request);
    let (_, ret) = expect_redirect_exchange(&mut send, b"");
    assert!(matches!(ret.unwrap_err(), FollowRedirectsError::Send(_)));
}

// --- generic method coroutines ---------------------------------------------

#[test]
fn get_returns_the_raw_body() {
    let mut get = Get::new(&base(), &WebdavAuth::None, UA, "calendars/personal/e.ics");
    let (request, ret) = expect_exchange(&mut get, &http_response("200 OK", &[], "ICS"));
    assert!(request.starts_with("get /dav/calendars/personal/e.ics http/1.1\r\n"));
    assert_eq!(ret.unwrap().body, b"ICS");
}

#[test]
fn put_carries_preconditions_and_content_type() {
    let mut put = Put::new(PutArgs {
        base_url: &base(),
        auth: &WebdavAuth::None,
        user_agent: UA,
        path: "calendars/personal/e.ics",
        content_type: "text/calendar; charset=utf-8",
        body: b"BEGIN:VCALENDAR".to_vec(),
        if_match: Some("etag-1"),
        if_none_match: Some("*"),
    });

    let reply = http_response("201 Created", &[("ETag", "\"etag-2\"")], "");
    let (request, ret) = expect_exchange(&mut put, &reply);
    assert!(request.starts_with("put /dav/calendars/personal/e.ics http/1.1\r\n"));
    assert!(request.contains("content-type: text/calendar; charset=utf-8\r\n"));
    assert!(request.contains("if-match: \"etag-1\"\r\n"));
    assert!(request.contains("if-none-match: *\r\n"));
    assert!(request.ends_with("begin:vcalendar"));

    let ok = ret.unwrap();
    assert_eq!(read_etag(&ok.response).as_deref(), Some("etag-2"));
}

#[test]
fn delete_carries_the_optional_if_match() {
    let mut delete = Delete::new(&base(), &WebdavAuth::None, UA, "x.ics", Some("etag-1"));
    let (request, ret) = expect_exchange(&mut delete, &http_response("204 No Content", &[], ""));
    assert!(request.starts_with("delete /dav/x.ics http/1.1\r\n"));
    assert!(request.contains("if-match: \"etag-1\"\r\n"));
    ret.unwrap();

    let mut delete = Delete::new(&base(), &WebdavAuth::None, UA, "x.ics", None);
    let (request, ret) = expect_exchange(&mut delete, &http_response("204 No Content", &[], ""));
    assert!(!request.contains("if-match"));
    ret.unwrap();
}

#[test]
fn options_exposes_the_dav_header() {
    let mut options = Options::new(&base(), &WebdavAuth::None, UA, "");
    let reply = http_response("200 OK", &[("DAV", "1, 3, calendar-access")], "");
    let (request, ret) = expect_exchange(&mut options, &reply);
    assert!(request.starts_with("options /dav/ http/1.1\r\n"));
    let ok = ret.unwrap();
    assert_eq!(ok.response.header("dav"), Some("1, 3, calendar-access"));
}

#[test]
fn mkcol_sends_the_extended_body() {
    let mut mkcol = Mkcol::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "books/contacts/",
        &[CALENDAR],
        &[(DISPLAYNAME, "Contacts")],
    );
    let (request, ret) = expect_exchange(&mut mkcol, &http_response("201 Created", &[], ""));
    assert!(request.starts_with("mkcol /dav/books/contacts/ http/1.1\r\n"));
    assert!(request.contains("<d:resourcetype><d:collection/><c:calendar/></d:resourcetype>"));
    assert!(request.contains("<d:displayname>contacts</d:displayname>"));
    ret.unwrap();
}

#[test]
fn propfind_parses_the_multistatus() {
    let mut propfind = Propfind::new(&base(), &WebdavAuth::None, UA, "", 1, &[DISPLAYNAME]);
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/dav/personal/</d:href>
        <d:propstat>
          <d:prop><d:displayname>Personal</d:displayname></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let (request, ret) = expect_exchange(&mut propfind, &multistatus_response(xml));
    assert!(request.starts_with("propfind /dav/ http/1.1\r\n"));
    assert!(request.contains("depth: 1\r\n"));
    assert!(request.contains("<d:displayname/>"));

    let ms = ret.unwrap();
    assert_eq!(ms.responses[0].text(DISPLAYNAME), Some("Personal"));
}

#[test]
fn proppatch_sends_the_propertyupdate_body() {
    let mut proppatch = Proppatch::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "personal/",
        &[(DISPLAYNAME, "Renamed")],
    );
    let (request, ret) = expect_exchange(
        &mut proppatch,
        &multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>"),
    );
    assert!(request.starts_with("proppatch /dav/personal/ http/1.1\r\n"));
    assert!(request.contains("<d:propertyupdate"));
    assert!(request.contains("<d:displayname>renamed</d:displayname>"));
    ret.unwrap();
}

#[test]
fn report_sends_the_query_body_and_parses_the_multistatus() {
    let body = report_query_body(CALENDAR, &[], &[GETETAG], "");
    let mut report = Report::new(&base(), &WebdavAuth::None, UA, "personal/", 1, body);
    let (request, ret) = expect_exchange(
        &mut report,
        &multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>"),
    );
    assert!(request.starts_with("report /dav/personal/ http/1.1\r\n"));
    assert!(request.contains("depth: 1\r\n"));
    assert!(request.contains("<c:calendar"));
    assert!(ret.unwrap().responses.is_empty());
}

#[test]
fn copy_and_move_carry_destination_overwrite_and_depth() {
    let mut copy = CopyResource::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "a.ics",
        "/dav/b.ics",
        true,
        0,
    );
    let (request, ret) = expect_exchange(&mut copy, &http_response("201 Created", &[], ""));
    assert!(request.starts_with("copy /dav/a.ics http/1.1\r\n"));
    assert!(request.contains("destination: /dav/b.ics\r\n"));
    assert!(request.contains("overwrite: t\r\n"));
    assert!(request.contains("depth: 0\r\n"));
    ret.unwrap();

    let mut mv = Move::new(&base(), &WebdavAuth::None, UA, "a.ics", "/dav/c.ics", false);
    let (request, ret) = expect_exchange(&mut mv, &http_response("201 Created", &[], ""));
    assert!(request.starts_with("move /dav/a.ics http/1.1\r\n"));
    assert!(request.contains("destination: /dav/c.ics\r\n"));
    assert!(request.contains("overwrite: f\r\n"));
    ret.unwrap();
}
