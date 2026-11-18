//! Offline coverage of the WebDAV core layer (RFC 4918): the request builder,
//! the XML body generators, the multistatus parser, the shared helpers and
//! every generic coroutine, all resumed against scripted HTTP response bytes.

#![cfg(feature = "client")]

mod common;

use common::*;
use io_webdav::{
    coroutine::{WebdavCoroutine, WebdavCoroutineState},
    rfc4918::{
        DAV, DISPLAYNAME, GETETAG, MULTISTATUS_MAX_DEPTH, RESOURCETYPE, WebdavAuth, WebdavIdError,
        WebdavMemberFailure, WebdavMultistatusParseError, WebdavNamespace, WebdavPropFailure,
        WebdavPropItem, WebdavPropValue, WebdavProperty, WebdavResponseEntry, check_collection_id,
        check_resource_id, collection_id, collection_path,
        copy::WebdavCopy as CopyResource,
        delete::WebdavDelete,
        encode_id,
        follow_redirects::{WebdavFollowRedirects, WebdavFollowRedirectsError},
        get::WebdavGet,
        id_from_location,
        mkcol::WebdavMkcol,
        r#move::WebdavMove,
        options::WebdavOptions,
        parse_multistatus,
        propfind::WebdavPropfind,
        proppatch::WebdavProppatch,
        put::{WebdavPut, WebdavPutArgs},
        read_etag,
        report::WebdavReport,
        report_query_body,
        request::WebdavRequest,
        resolve, resolve_href,
        send::{WebdavSendError, WebdavSendRaw},
        summarize_body, trace_unrecognized, xmlns_decls,
    },
    rfc6352::addressbook::ADDRESS_DATA,
};
use url::Url;

const UA: &str = "io-webdav/test";

const CALDAV: WebdavNamespace = WebdavNamespace {
    uri: "urn:ietf:params:xml:ns:caldav",
    prefix: "C",
};
const CALENDAR: WebdavProperty = WebdavProperty {
    ns: CALDAV,
    local: "calendar",
};

fn base() -> Url {
    Url::parse("https://dav.example.org/dav/").unwrap()
}

// --- XML generators ---

#[test]
fn xmlns_decls_dedupes_and_supports_the_default_namespace() {
    const DEFAULT: WebdavNamespace = WebdavNamespace {
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
    const ROOT: WebdavProperty = WebdavProperty {
        ns: WebdavNamespace {
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

// --- multistatus parser ---

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

    let ms = parse_multistatus(xml).unwrap();
    let entry = &ms.responses[0];
    assert_eq!(entry.href, "/dav/books/contacts/");
    assert_eq!(entry.id(), "contacts");
    assert_eq!(entry.text(DISPLAYNAME), Some("A & B <> \"' ! &bogus;"));

    let etag = entry.prop(GETETAG).expect("empty getetag kept as a prop");
    assert!(etag.text.is_empty());
    assert!(entry.text(GETETAG).is_none());
}

#[test]
fn parse_multistatus_normalises_text_and_attributes() {
    let xml = "<d:multistatus xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\">\
      <d:response><d:href>/cal/</d:href><d:propstat><d:prop>\
      <d:displayname>a\r\nb\rc &#65; &#xZZ; &#1114112; &amp</d:displayname>\
      <c:supported-calendar-component-set>\
      <c:comp name=\"V\tEV&#x45;NT\r\n\"/><c:comp x:name=\"ignored\"></c:comp>\
      </c:supported-calendar-component-set>\
      </d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>\
      </d:multistatus>";

    let ms = parse_multistatus(xml).unwrap();
    let entry = &ms.responses[0];
    assert_eq!(
        entry.text(DISPLAYNAME),
        Some("a\nb\nc A &#xZZ; &#1114112; &amp")
    );

    let set = entry
        .props
        .iter()
        .find(|item| item.local == "supported-calendar-component-set")
        .expect("component set parsed");
    assert_eq!(set.children[0].name.as_deref(), Some("V EVENT "));
    assert_eq!(set.children[1].name, None);
}
#[test]
fn parse_multistatus_fails_past_the_max_depth() {
    // NOTE: 6000 levels, a 42 KB body that overflowed a 1 MiB stack
    // through the recursive clone of the children; run on a stack the
    // size of an Android thread.
    let xml = nested(6000);
    let err = on_small_stack(move || parse_multistatus(&xml).unwrap_err());
    assert_eq!(
        err,
        WebdavMultistatusParseError::TooDeep(MULTISTATUS_MAX_DEPTH)
    );
}

#[test]
fn parse_multistatus_fails_past_the_max_depth_on_a_large_body() {
    let xml = nested(200_000);
    let err = on_small_stack(move || parse_multistatus(&xml).unwrap_err());
    assert_eq!(
        err,
        WebdavMultistatusParseError::TooDeep(MULTISTATUS_MAX_DEPTH)
    );
}

#[test]
fn parse_multistatus_keeps_the_children_at_the_max_depth() {
    // NOTE: multistatus, response, propstat, prop and the property
    // itself take five levels.
    let levels = MULTISTATUS_MAX_DEPTH - 5;
    let xml = nested(levels);
    let ms = on_small_stack(move || parse_multistatus(&xml).unwrap());

    let mut depth = 0;
    let mut children = &ms.responses[0].props[0].children;
    while let Some(child) = children.first() {
        depth += 1;
        children = &child.children;
    }

    assert_eq!(depth, levels);
    assert_eq!(ms.responses[0].props[0].text, "x");
}

#[test]
fn parse_multistatus_is_linear_in_wide_bodies() {
    // NOTE: 20000 responses, each with nested property markup; the old
    // parser copied every subtree into each ancestor on close.
    let row = "<D:response><D:href>/a/</D:href><D:propstat><D:prop>\
               <D:supported-report-set><D:supported-report><D:report>\
               <D:sync-collection/></D:report></D:supported-report>\
               </D:supported-report-set></D:prop>\
               <D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>";
    let xml = format!(
        "<D:multistatus xmlns:D=\"DAV:\">{}</D:multistatus>",
        row.repeat(20_000)
    );
    let ms = on_small_stack(move || parse_multistatus(&xml).unwrap());
    assert_eq!(ms.responses.len(), 20_000);
    assert!(
        ms.responses[19_999]
            .supported_reports()
            .contains("sync-collection")
    );
}

/// A multistatus whose single property nests `levels` elements.
fn nested(levels: usize) -> String {
    format!(
        "<multistatus><response><href>/a/</href><propstat><prop><p>{}x{}</p></prop>\
         <status>HTTP/1.1 200 OK</status></propstat></response></multistatus>",
        "<a>".repeat(levels),
        "</a>".repeat(levels),
    )
}

/// Runs `f` on a 1 MiB stack, the size of an ordinary Android thread.
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn parse_multistatus_fails_at_a_mismatched_close() {
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response><d:href>/a/</d:href></d:response>
      <d:response><d:href>/b/</d:wrong></d:response>
    </d:multistatus>"#;

    assert_eq!(
        parse_multistatus(xml).unwrap_err(),
        WebdavMultistatusParseError::Incomplete(1)
    );
}

const ROW: &str = "<d:response><d:href>/a.ics</d:href></d:response>";

#[test]
fn parse_multistatus_fails_on_a_body_that_is_not_a_whole_multistatus() {
    let html = "<html><head><title>Sign in</title></head><body>Please log in</body></html>";
    let cut = format!("<d:multistatus xmlns:d=\"DAV:\">{ROW}<d:response><d:href>/b.ics</d:hr");
    let open = format!("<d:multistatus xmlns:d=\"DAV:\">{ROW}");
    let twice = format!("<d:multistatus xmlns:d=\"DAV:\">{ROW}</d:multistatus>{ROW}");

    for (body, err) in [
        (
            html,
            WebdavMultistatusParseError::UnexpectedRoot("html".into()),
        ),
        (&cut, WebdavMultistatusParseError::Incomplete(1)),
        (&open, WebdavMultistatusParseError::Incomplete(1)),
        (&twice, WebdavMultistatusParseError::Incomplete(1)),
        ("", WebdavMultistatusParseError::Incomplete(0)),
    ] {
        assert_eq!(parse_multistatus(body).unwrap_err(), err, "{body}");
    }
}

#[test]
fn parse_multistatus_reads_the_envelope_by_position() {
    // NOTE: the shape of an expand-property answer (RFC 3253 §3.8).
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/principals/alice/</d:href>
        <d:propstat>
          <d:prop>
            <d:group-membership>
              <d:response>
                <d:href>/principals/staff/</d:href>
                <d:propstat>
                  <d:prop><d:displayname>Staff</d:displayname></d:prop>
                  <d:status>HTTP/1.1 200 OK</d:status>
                </d:propstat>
              </d:response>
            </d:group-membership>
            <d:displayname>Alice</d:displayname>
          </d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let ms = parse_multistatus(xml).unwrap();
    assert_eq!(ms.responses.len(), 1);

    let entry = &ms.responses[0];
    assert_eq!(entry.href, "/principals/alice/");

    let locals: Vec<&str> = entry.props.iter().map(|item| &*item.local).collect();
    assert_eq!(locals, ["group-membership", "displayname"]);
    assert_eq!(entry.text(DISPLAYNAME), Some("Alice"));
    assert_eq!(entry.props[0].children[0].local, "response");
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

    let ms = parse_multistatus(xml).unwrap();
    let entry = &ms.responses[0];
    assert_eq!(entry.status, None);
    assert!(entry.props.is_empty());
    assert!(ms.sync_token.is_none());
}

#[test]
fn parse_multistatus_keeps_refused_properties_as_failures() {
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav">
      <d:response>
        <d:href>/dav/books/contacts/</d:href>
        <d:propstat>
          <d:prop><d:displayname/></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
        <d:propstat>
          <d:prop><c:addressbook-description/><d:getetag/></d:prop>
          <d:status>HTTP/1.1 403 Forbidden</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let ms = parse_multistatus(xml).unwrap();
    let entry = &ms.responses[0];

    assert_eq!(entry.props.len(), 1);
    assert_eq!(entry.props[0].local, "displayname");

    let failures: Vec<String> = entry
        .failures
        .iter()
        .map(WebdavPropFailure::to_string)
        .collect();
    assert_eq!(failures, ["addressbook-description (403)", "getetag (403)"]);
}

#[test]
fn summarize_body_prefers_the_response_description() {
    let body = r#"<?xml version="1.0"?>
      <d:error xmlns:d="DAV:">
        <c:valid-address-data xmlns:c="urn:ietf:params:xml:ns:carddav"/>
        <d:responsedescription>Missing mandatory UID property</d:responsedescription>
      </d:error>"#;
    assert_eq!(summarize_body(body), "Missing mandatory UID property");
}

#[test]
fn summarize_body_strips_markup_and_caps_the_rest() {
    let html = "<html>\n  <head><title>404 Not Found</title></head>\n  <body><h1>Not Found</h1></body>\n</html>";
    assert_eq!(summarize_body(html), "404 Not Found");

    assert_eq!(
        summarize_body("<p>Denied</p>  <p>twice</p>"),
        "Denied twice"
    );

    assert!(summarize_body("").is_empty());
    assert!(summarize_body("   \n ").is_empty());

    let long = "x".repeat(500);
    let summary = summarize_body(&long);
    assert!(summary.ends_with('…'));
    assert_eq!(summary.chars().count(), 201);
}

#[test]
fn parse_multistatus_reads_the_supported_report_set() {
    let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
      <d:response>
        <d:href>/dav/books/contacts/</d:href>
        <d:propstat>
          <d:prop>
            <d:supported-report-set>
              <d:supported-report><d:report><card:addressbook-multiget/></d:report></d:supported-report>
              <d:supported-report><d:report><card:addressbook-query/></d:report></d:supported-report>
              <d:supported-report><d:report><d:sync-collection/></d:report></d:supported-report>
            </d:supported-report-set>
          </d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>/dav/books/legacy/</d:href>
        <d:propstat>
          <d:prop><d:displayname>Legacy</d:displayname></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let ms = parse_multistatus(xml).unwrap();
    let reports = ms.responses[0].supported_reports();

    assert_eq!(reports.len(), 3);
    assert!(reports.contains("sync-collection"));
    assert!(reports.contains("addressbook-multiget"));
    assert!(reports.contains("addressbook-query"));

    assert!(ms.responses[1].supported_reports().is_empty());
}

#[test]
fn parse_multistatus_fails_on_malformed_xml() {
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/dav/a/</d:href>
        <d:propstat>
          <d:prop><d:displayname>A</d:displayname></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response><d:href>/dav/b/</d:href><broken"#;

    assert_eq!(
        parse_multistatus(xml).unwrap_err(),
        WebdavMultistatusParseError::Incomplete(1)
    );
}

#[test]
fn parse_multistatus_ignores_text_outside_the_root() {
    let ms = parse_multistatus("&amp;<d:multistatus xmlns:d=\"DAV:\"/>").unwrap();
    assert!(ms.responses.is_empty());

    let xml = format!("\u{feff}<d:multistatus xmlns:d=\"DAV:\">{ROW}</d:multistatus>\nnotice\n");
    let ms = parse_multistatus(&xml).unwrap();
    assert_eq!(ms.responses.len(), 1);
}

#[test]
fn multistatus_iterates_over_its_responses() {
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response><d:href>/a/</d:href></d:response>
      <d:response><d:href>/b/</d:href></d:response>
    </d:multistatus>"#;

    let hrefs: Vec<String> = parse_multistatus(xml)
        .unwrap()
        .into_iter()
        .map(|entry| entry.href)
        .collect();
    assert_eq!(hrefs, ["/a/", "/b/"]);
}

#[test]
fn response_entry_helpers_handle_missing_data() {
    let entry = WebdavResponseEntry {
        href: String::new(),
        status: None,
        failures: Vec::new(),
        props: vec![WebdavPropItem {
            local: "unknown-prop".into(),
            text: "   ".into(),
            children: Vec::new(),
        }],
    };

    assert_eq!(entry.id(), "");
    assert!(entry.prop(DISPLAYNAME).is_none());
    let unknown = WebdavProperty {
        ns: DAV,
        local: "unknown-prop",
    };
    assert!(entry.text(unknown).is_none(), "blank text is filtered");
    assert!(!entry.has_resource_type(RESOURCETYPE, CALENDAR));

    trace_unrecognized(&entry, &[DISPLAYNAME]);
}

#[test]
fn a_parsed_property_keeps_its_whole_text_beside_the_trimmed_one() {
    let xml = "<d:multistatus xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:carddav\">
      <d:response><d:href>/dav/books/contacts/a.vcf</d:href><d:propstat><d:prop>
        <c:address-data>BEGIN:VCARD&#13;\nEND:VCARD&#13;\n</c:address-data>
      </d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>
    </d:multistatus>";
    let ms = parse_multistatus(xml).unwrap();
    let entry = &ms.responses[0];
    let whole = &entry.prop(ADDRESS_DATA).unwrap().text;
    assert_eq!(whole, "BEGIN:VCARD\r\nEND:VCARD\r\n");
    assert_eq!(entry.text(ADDRESS_DATA), Some("BEGIN:VCARD\r\nEND:VCARD"));
}

// --- request builder and path resolution ---

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

// --- send coroutines ---

#[test]
fn send_raw_returns_the_response_body() {
    let request = WebdavRequest::get(&base(), &WebdavAuth::None, UA, "file.txt").body(Vec::new());
    let mut send = WebdavSendRaw::new(request);

    let (request, ok) = expect_exchange(&mut send, &http_response("200 OK", &[], "hello"));
    let ok = ok.unwrap();
    assert!(request.starts_with("get /dav/file.txt http/1.1\r\n"));
    assert_eq!(ok.body, b"hello");
    assert!(ok.response.body.is_empty());
    assert_eq!(*ok.response.status, 200);
    assert!(ok.keep_alive);
}

#[test]
fn send_raw_maps_failure_statuses_to_http_status() {
    let request = WebdavRequest::get(&base(), &WebdavAuth::None, UA, "x").body(Vec::new());
    let mut send = WebdavSendRaw::new(request);

    let (_, ret) = expect_exchange(&mut send, &http_response("404 Not Found", &[], "nope"));
    let err = ret.unwrap_err();
    let WebdavSendError::HttpStatus { status, body } = err else {
        panic!("expected HttpStatus, got {err:?}");
    };
    assert_eq!(status, 404);
    assert_eq!(body, "nope");
}

#[test]
fn send_raw_rejects_redirects() {
    let request = WebdavRequest::get(&base(), &WebdavAuth::None, UA, "x").body(Vec::new());
    let mut send = WebdavSendRaw::new(request);

    let reply = http_response("301 Moved Permanently", &[("Location", "/new")], "");
    let (_, ret) = expect_exchange(&mut send, &reply);
    let WebdavSendError::UnexpectedRedirect {
        url,
        keep_alive,
        same_origin,
    } = ret.unwrap_err()
    else {
        panic!("expected an unexpected redirect");
    };
    assert_eq!(url.as_str(), "https://dav.example.org/new");
    assert!(keep_alive);
    assert!(same_origin);
}

#[test]
fn send_raw_surfaces_transport_errors() {
    let request = WebdavRequest::get(&base(), &WebdavAuth::None, UA, "x").body(Vec::new());
    let mut send = WebdavSendRaw::new(request);

    let (_, ret) = expect_exchange(&mut send, b"");
    assert!(matches!(ret.unwrap_err(), WebdavSendError::Send(_)));
}

#[test]
fn follow_redirects_returns_the_success_body() {
    let request = WebdavRequest::propfind(&base(), &WebdavAuth::None, UA, "").body(Vec::new());
    let mut send = WebdavFollowRedirects::new(request);

    let (_, ret) = expect_redirect_exchange(&mut send, &http_response("200 OK", &[], "body"));
    let ok = ret.unwrap();
    assert_eq!(ok.body, b"body");
    assert!(ok.response.body.is_empty());
}

#[test]
fn follow_redirects_surfaces_the_redirect() {
    let request = WebdavRequest::propfind(&base(), &WebdavAuth::None, UA, "").body(Vec::new());
    let mut send = WebdavFollowRedirects::new(request);

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
    let mut send = WebdavFollowRedirects::new(request);
    let (_, ret) = expect_redirect_exchange(&mut send, &http_response("403 Forbidden", &[], "no"));
    let err = ret.unwrap_err();
    let WebdavFollowRedirectsError::HttpStatus { status, body } = err else {
        panic!("expected HttpStatus, got {err:?}");
    };
    assert_eq!(status, 403);
    assert_eq!(body, "no");

    let request = WebdavRequest::propfind(&base(), &WebdavAuth::None, UA, "").body(Vec::new());
    let mut send = WebdavFollowRedirects::new(request);
    let (_, ret) = expect_redirect_exchange(&mut send, b"");
    assert!(matches!(
        ret.unwrap_err(),
        WebdavFollowRedirectsError::Send(_)
    ));
}

// --- generic method coroutines ---

#[test]
fn get_returns_the_raw_body() {
    let mut get = WebdavGet::new(&base(), &WebdavAuth::None, UA, "calendars/personal/e.ics");
    let (request, ret) = expect_exchange(&mut get, &http_response("200 OK", &[], "ICS"));
    assert!(request.starts_with("get /dav/calendars/personal/e.ics http/1.1\r\n"));
    assert_eq!(ret.unwrap().body, b"ICS");
}

#[test]
fn put_carries_preconditions_and_content_type() {
    let mut put = WebdavPut::new(WebdavPutArgs {
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
    let mut delete = WebdavDelete::new(&base(), &WebdavAuth::None, UA, "x.ics", Some("etag-1"));
    let (request, ret) = expect_exchange(&mut delete, &http_response("204 No Content", &[], ""));
    assert!(request.starts_with("delete /dav/x.ics http/1.1\r\n"));
    assert!(request.contains("if-match: \"etag-1\"\r\n"));
    ret.unwrap();

    let mut delete = WebdavDelete::new(&base(), &WebdavAuth::None, UA, "x.ics", None);
    let (request, ret) = expect_exchange(&mut delete, &http_response("204 No Content", &[], ""));
    assert!(!request.contains("if-match"));
    ret.unwrap();
}

#[test]
fn a_weak_etag_is_read_whole_and_sent_back_whole() {
    let request = WebdavRequest::get(&base(), &WebdavAuth::None, UA, "x.ics").body(Vec::new());
    let mut get = WebdavSendRaw::new(request);
    let reply = http_response("200 OK", &[("ETag", "W/\"etag-2\"")], "");
    let (_, ret) = expect_exchange(&mut get, &reply);

    let etag = read_etag(&ret.unwrap().response).unwrap();
    assert_eq!(etag, "W/\"etag-2\"");

    let mut delete = WebdavDelete::new(&base(), &WebdavAuth::None, UA, "x.ics", Some(&etag));
    let (request, _) = expect_exchange(&mut delete, &http_response("204 No Content", &[], ""));
    assert!(request.contains("if-match: w/\"etag-2\"\r\n"));
}

#[test]
fn a_strong_etag_starting_like_a_weak_one_is_quoted() {
    let mut delete = WebdavDelete::new(&base(), &WebdavAuth::None, UA, "x.ics", Some("W/abc"));
    let (request, _) = expect_exchange(&mut delete, &http_response("204 No Content", &[], ""));
    assert!(request.contains("if-match: \"w/abc\"\r\n"));
}

/// The answer to a request that failed on a member (RFC 4918 §9.6.1).
const LOCKED_MEMBER: &str = r#"<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/dav/personal/locked.ics</d:href>
    <d:status>HTTP/1.1 423 Locked</d:status>
  </d:response>
</d:multistatus>"#;

#[test]
fn delete_copy_and_move_fail_on_a_multistatus_naming_a_failed_member() {
    let reply = multistatus_response(LOCKED_MEMBER);
    let locked = [WebdavMemberFailure {
        href: "/dav/personal/locked.ics".into(),
        status: Some(423),
    }];

    let mut delete = WebdavDelete::new(&base(), &WebdavAuth::None, UA, "personal/", None);
    let (_, ret) = expect_exchange(&mut delete, &reply);
    let err = ret.expect_err("DELETE answered 207 passed for a success");
    assert!(matches!(&err, WebdavSendError::MemberFailures(failures) if *failures == locked));
    assert_eq!(
        err.to_string(),
        "WebDAV server failed on /dav/personal/locked.ics (423)"
    );

    let mut copy = CopyResource::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "personal/",
        "/dav/copy/",
        true,
        0,
    );
    let (_, ret) = expect_exchange(&mut copy, &reply);
    let err = ret.expect_err("COPY answered 207 passed for a success");
    assert!(matches!(err, WebdavSendError::MemberFailures(failures) if failures == locked));

    let mut mv = WebdavMove::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "personal/",
        "/dav/moved/",
        false,
    );
    let (_, ret) = expect_exchange(&mut mv, &reply);
    let err = ret.expect_err("MOVE answered 207 passed for a success");
    assert!(matches!(err, WebdavSendError::MemberFailures(failures) if failures == locked));
}

#[test]
fn delete_tells_a_multistatus_it_cannot_read_from_a_success() {
    let mut delete = WebdavDelete::new(&base(), &WebdavAuth::None, UA, "personal/", None);
    let (_, ret) = expect_exchange(&mut delete, &multistatus_response(""));
    assert!(matches!(
        ret.unwrap_err(),
        WebdavSendError::Multistatus(WebdavMultistatusParseError::Incomplete(0))
    ));

    let done = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/dav/personal/</d:href>
        <d:status>HTTP/1.1 204 No Content</d:status>
      </d:response>
    </d:multistatus>"#;
    let mut delete = WebdavDelete::new(&base(), &WebdavAuth::None, UA, "personal/", None);
    let (_, ret) = expect_exchange(&mut delete, &multistatus_response(done));
    ret.unwrap();
}

#[test]
fn a_header_value_carrying_a_control_byte_is_never_sent() {
    let injected = "x\r\nDepth: infinity";

    let delete = WebdavRequest::delete(&base(), &WebdavAuth::None, UA, "x")
        .if_match(injected)
        .body(Vec::new());
    let put = WebdavRequest::put(&base(), &WebdavAuth::None, UA, "x")
        .if_none_match(injected)
        .body(Vec::new());
    let mv = WebdavRequest::r#move(&base(), &WebdavAuth::None, UA, "x")
        .destination("/dav/y\n")
        .body(Vec::new());
    let typed = WebdavRequest::put(&base(), &WebdavAuth::None, UA, "x")
        .content_type("text/calendar\u{7f}")
        .body(Vec::new());
    let agent = WebdavRequest::get(&base(), &WebdavAuth::None, "ua\0", "x").body(Vec::new());

    for (request, header) in [
        (delete, "If-Match"),
        (put, "If-None-Match"),
        (mv, "Destination"),
        (typed, "Content-Type"),
        (agent, "User-Agent"),
    ] {
        match WebdavSendRaw::new(request).resume(None) {
            WebdavCoroutineState::Complete(Err(WebdavSendError::InvalidHeader(name))) => {
                assert_eq!(name, header);
            }
            state => panic!("expected a refusal, got {state:?}"),
        }
    }
}

#[test]
fn a_discovery_request_carrying_a_control_byte_is_never_sent() {
    let request =
        WebdavRequest::propfind(&base(), &WebdavAuth::None, "ua\r\nX: y", "").body(Vec::new());

    match WebdavFollowRedirects::new(request).resume(None) {
        WebdavCoroutineState::Complete(Err(WebdavFollowRedirectsError::InvalidHeader(name))) => {
            assert_eq!(name, "User-Agent");
        }
        state => panic!("expected a refusal, got {state:?}"),
    }
}

#[test]
fn an_encoded_id_is_sent_and_returned_as_it_stands() {
    let id = encode_id("50% off/été?.ics");
    assert_eq!(id, "50%25%20off%2F%C3%A9t%C3%A9%3F.ics");

    let request = WebdavRequest::put(&base(), &WebdavAuth::None, UA, &format!("/dav/{id}"));
    assert_eq!(request.body(Vec::new()).url.path(), format!("/dav/{id}"));
}

#[test]
fn options_exposes_the_dav_header() {
    let mut options = WebdavOptions::new(&base(), &WebdavAuth::None, UA, "");
    let reply = http_response("200 OK", &[("DAV", "1, 3, calendar-access")], "");
    let (request, ret) = expect_exchange(&mut options, &reply);
    assert!(request.starts_with("options /dav/ http/1.1\r\n"));
    let ok = ret.unwrap();
    assert_eq!(ok.response.header("dav"), Some("1, 3, calendar-access"));
}

#[test]
fn mkcol_sends_the_extended_body() {
    let mut mkcol = WebdavMkcol::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "books/contacts/",
        &[CALENDAR],
        &[(DISPLAYNAME, WebdavPropValue::Text("Contacts"))],
    );
    let (request, ret) = expect_exchange(&mut mkcol, &http_response("201 Created", &[], ""));
    assert!(request.starts_with("mkcol /dav/books/contacts/ http/1.1\r\n"));
    assert!(request.contains("<d:resourcetype><d:collection/><c:calendar/></d:resourcetype>"));
    assert!(request.contains("<d:displayname>contacts</d:displayname>"));
    ret.unwrap();
}

#[test]
fn propfind_parses_the_multistatus() {
    let mut propfind: WebdavPropfind =
        WebdavPropfind::new(&base(), &WebdavAuth::None, UA, "", 1, &[DISPLAYNAME]);
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
    let mut proppatch = WebdavProppatch::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "personal/",
        &[(DISPLAYNAME, WebdavPropValue::Text("Renamed"))],
        &[GETETAG],
    );
    let (request, ret) = expect_exchange(
        &mut proppatch,
        &multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>"),
    );
    assert!(request.starts_with("proppatch /dav/personal/ http/1.1\r\n"));
    assert!(request.contains("<d:propertyupdate"));
    assert!(request.contains("<d:displayname>renamed</d:displayname>"));
    assert!(request.contains("<d:remove><d:prop><d:getetag/></d:prop></d:remove>"));
    ret.unwrap();
}

#[test]
fn report_sends_the_query_body_and_parses_the_multistatus() {
    let body = report_query_body(CALENDAR, &[], &[GETETAG], "");
    let mut report: WebdavReport =
        WebdavReport::new(&base(), &WebdavAuth::None, UA, "personal/", 1, body);
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
fn report_names_an_unimplemented_report_and_leaves_the_rest_alone() {
    let body = report_query_body(CALENDAR, &[], &[GETETAG], "");
    let mut report: WebdavReport =
        WebdavReport::new(&base(), &WebdavAuth::None, UA, "personal/", 1, body);
    let refusal = r#"<d:error xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns">
      <s:exception>Sabre\DAV\Exception\ReportNotSupported</s:exception>
      <d:supported-report/>
    </d:error>"#;
    let (_, ret) = expect_exchange(&mut report, &http_response("403 Forbidden", &[], refusal));

    let err = ret.unwrap_err();
    assert!(matches!(
        err,
        WebdavSendError::UnsupportedReport { status: 403, .. }
    ));
    let message = err.to_string();
    assert!(message.contains("does not implement the report"));
    assert!(message.contains("ReportNotSupported"));

    let body = report_query_body(CALENDAR, &[], &[GETETAG], "");
    let mut report: WebdavReport =
        WebdavReport::new(&base(), &WebdavAuth::None, UA, "personal/", 1, body);
    let (_, ret) = expect_exchange(
        &mut report,
        &http_response("405 Method Not Allowed", &[], ""),
    );
    assert!(ret.unwrap_err().to_string().ends_with("(HTTP 405)"));

    let body = report_query_body(CALENDAR, &[], &[GETETAG], "");
    let mut report: WebdavReport =
        WebdavReport::new(&base(), &WebdavAuth::None, UA, "personal/", 1, body);
    let redirect = http_response(
        "302 Found",
        &[("Location", "https://elsewhere.example.org/")],
        "",
    );
    let (_, ret) = expect_exchange(&mut report, &redirect);

    assert!(matches!(
        ret.unwrap_err(),
        WebdavSendError::UnexpectedRedirect { .. }
    ));
}

#[test]
fn report_leaves_a_refusal_quoting_the_precondition_alone() {
    let body = report_query_body(CALENDAR, &[], &[GETETAG], "");
    let mut report: WebdavReport =
        WebdavReport::new(&base(), &WebdavAuth::None, UA, "personal/", 1, body);
    let refusal = r#"<d:error xmlns:d="DAV:">
      <d:need-privileges/>
      <d:responsedescription>supported-report-set is protected</d:responsedescription>
    </d:error>"#;
    let (_, ret) = expect_exchange(&mut report, &http_response("403 Forbidden", &[], refusal));

    let err = ret.unwrap_err();
    assert!(
        matches!(err, WebdavSendError::HttpStatus { status: 403, .. }),
        "unexpected error {err:?}",
    );
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

    let mut mv = WebdavMove::new(&base(), &WebdavAuth::None, UA, "a.ics", "/dav/c.ics", false);
    let (request, ret) = expect_exchange(&mut mv, &http_response("201 Created", &[], ""));
    assert!(request.starts_with("move /dav/a.ics http/1.1\r\n"));
    assert!(request.contains("destination: /dav/c.ics\r\n"));
    assert!(request.contains("overwrite: f\r\n"));
    ret.unwrap();
}

#[test]
fn a_collection_under_the_home_set_keeps_its_last_segment() {
    assert_eq!(collection_id("/dav/cal/", "/dav/cal/work/"), "work");
    assert_eq!(
        collection_id("/dav/cal", "https://dav.example.org/dav/cal/work"),
        "work"
    );
    // NOTE: the two sides may encode differently.
    assert_eq!(collection_id("/u/a%40b/", "/u/a@b/events/"), "events");
    assert_eq!(collection_id("/u/a@b/", "/u/a%40b/events/"), "events");
    // NOTE: a relative href names no parent, as before.
    assert_eq!(collection_id("/dav/cal/", "work/"), "work");
}

#[test]
fn a_collection_elsewhere_is_named_by_its_path() {
    assert_eq!(
        collection_id("/caldav/v2/alice/", "/caldav/v2/team%40g/events/?x=1#f"),
        "/caldav/v2/team%40g/events"
    );
    assert_eq!(
        collection_id("/dav/cal/", "/dav/cal/sub/work/"),
        "/dav/cal/sub/work"
    );
    // NOTE: a malformed escape is compared as it stands.
    assert_eq!(collection_id("/u/%zz/", "/u/%zz/events/"), "events");
    assert_eq!(collection_id("/u/%4/", "/u/%41/events/"), "/u/%41/events");
    // NOTE: a root href (or a URL without a path) is no collection: empty.
    assert_eq!(collection_id("/dav/cal/", "https://dav.example.org"), "");
    assert_eq!(collection_id("/dav/cal/", "/"), "");
}

#[test]
fn a_listed_segment_is_decoded_when_its_encoding_gives_it_back() {
    let listed = [
        ("work", "work"),
        ("a@b", "a@b"),
        ("My%20Book", "My Book"),
        ("%C3%A9t%C3%A9", "été"),
        ("50%25off", "50%off"),
        ("a%3Fb%23c", "a?b#c"),
        ("a%5Cb", "a\\b"),
        // NOTE: kept as served: a decoded slash, an escape left in the
        // decoding, escapes the encoder would not write (a pchar, lowercase
        // hex), bytes that are no UTF-8, an id the checks refuse.
        ("a%2Fb", "a%2Fb"),
        ("a%2541", "a%2541"),
        ("a%40b", "a%40b"),
        ("card%2D1", "card%2D1"),
        ("caf%c3%a9", "caf%c3%a9"),
        ("%FF", "%FF"),
        ("%2E%2E", "%2E%2E"),
        ("a%5C..", "a%5C.."),
    ];

    for (segment, id) in listed {
        let href = format!("/dav/cal/{segment}/");
        assert_eq!(collection_id("/dav/cal/", &href), id, "{segment:?}");

        let location = format!("https://dav.example.org/dav/cal/work/{segment}?x");
        assert_eq!(
            id_from_location(&location).as_deref(),
            Some(id),
            "{segment:?}"
        );

        // NOTE: the id addresses the very segment it was read from.
        assert_eq!(collection_path("/dav/cal/", id), href, "{segment:?}");
    }

    // NOTE: the path of a collection elsewhere stays as served.
    assert_eq!(
        collection_id("/dav/cal/", "/other/My%20Book/"),
        "/other/My%20Book"
    );
}

#[test]
fn a_collection_path_takes_an_id_starting_with_a_slash_as_is() {
    assert_eq!(collection_path("/dav/cal/", "work"), "/dav/cal/work/");
    assert_eq!(collection_path("/dav/cal", "/work/"), "/work/");
    assert_eq!(
        collection_path("/dav/cal/", "/caldav/v2/x/events"),
        "/caldav/v2/x/events/"
    );
}

#[test]
fn a_collection_id_climbing_out_of_the_home_set_is_refused() {
    let climbing = [
        "", "/", "//", ".", "..", "./x", "x/.", "a/../..", "a//b", "//x", "/..", "/a/../b", "%2e",
        "%2e%2E", "%2E.", ".%2e", "a\\..", "a%2F..", "a%5C..",
    ];

    for id in climbing {
        assert_eq!(
            check_collection_id(id),
            Err(WebdavIdError(id.to_string())),
            "{id:?}"
        );
    }

    let err = check_collection_id("..").unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"WebDAV id ".." is empty or holds an empty, . or .. segment"#
    );
}

#[test]
fn a_clean_collection_id_keeps_its_path() {
    let clean = [
        ("work", "/dav/cal/work/"),
        ("work/", "/dav/cal/work/"),
        ("a..b", "/dav/cal/a..b/"),
        ("...", "/dav/cal/.../"),
        ("/caldav/v2/team%40g/events", "/caldav/v2/team%40g/events/"),
        ("/caldav/v2/x/events/", "/caldav/v2/x/events/"),
    ];

    for (id, path) in clean {
        assert_eq!(check_collection_id(id), Ok(()), "{id:?}");
        assert_eq!(collection_path("/dav/cal/", id), path);
    }
}

#[test]
fn a_relative_collection_id_is_percent_encoded() {
    let encoded = [
        ("My Book", "/dav/cal/My%20Book/"),
        ("My%20Book", "/dav/cal/My%20Book/"),
        ("été", "/dav/cal/%C3%A9t%C3%A9/"),
        ("a?b#c", "/dav/cal/a%3Fb%23c/"),
        ("a\\b", "/dav/cal/a%5Cb/"),
        ("a[b]|c^d", "/dav/cal/a%5Bb%5D%7Cc%5Ed/"),
        ("50%off", "/dav/cal/50%25off/"),
        ("%4", "/dav/cal/%254/"),
        ("a%2fb", "/dav/cal/a%2fb/"),
        ("sub/My Book/", "/dav/cal/sub/My%20Book/"),
        ("a-._~!$&'()*+,;=:@b", "/dav/cal/a-._~!$&'()*+,;=:@b/"),
        // NOTE: an id starting with a slash is the server's path, as given.
        ("/caldav/v2/x y/events", "/caldav/v2/x y/events/"),
    ];

    for (id, path) in encoded {
        assert_eq!(collection_path("/dav/cal/", id), path, "{id:?}");
    }
}

#[test]
fn an_encoded_id_is_judged_as_the_id_it_came_from() {
    let ids = [
        "", ".", "..", "a/../..", "a//b", "%2e%2E", "a\\..", "a%2F..", "a%5C..", "x/", "My Book",
        "a\\b", "50%off", "%4", "%%2e", "a..b", "été",
    ];

    // NOTE: the encoding keeps every escape and adds only ones decoding back
    // to the byte they replace, so the checks judge the path sent.
    for id in ids {
        let path = collection_path("/c/", id);
        let sent = path.strip_prefix("/c/").unwrap();
        let judged = check_collection_id(sent).is_ok();
        assert_eq!(
            check_collection_id(id).is_ok(),
            judged,
            "{id:?} as {sent:?}"
        );

        let path = io_webdav::rfc4791::item::join_path("/c", id);
        let sent = path.strip_prefix("/c/").unwrap();
        let judged = check_resource_id(sent).is_ok();
        assert_eq!(check_resource_id(id).is_ok(), judged, "{id:?} as {sent:?}");
    }
}

#[test]
fn a_resource_id_climbing_out_of_its_collection_is_refused() {
    let climbing = [
        "",
        "/",
        ".",
        "..",
        "../x.ics",
        "a/../../x.ics",
        "x.ics/",
        "a//x.ics",
        "%2E%2E",
        "x\\..\\..",
    ];

    for id in climbing {
        assert_eq!(
            check_resource_id(id),
            Err(WebdavIdError(id.to_string())),
            "{id:?}"
        );
    }

    for id in [
        "event-1.ics",
        "/event-1.ics",
        "a%40b.vcf",
        "sub/x.ics",
        "..ics",
    ] {
        assert_eq!(check_resource_id(id), Ok(()), "{id:?}");
    }
}
