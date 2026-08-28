//! Offline coverage of the standard blocking client: every method is run
//! against a scripted stream replaying canned HTTP responses, plus local TCP
//! listeners for the connect flow.

#![cfg(feature = "client")]

mod common;

use std::{
    collections::VecDeque,
    io::{Error as IoError, Read, Result as IoResult, Write},
    net::TcpListener,
};

use common::*;
use io_webdav::{
    client::{WebdavClientStd, WebdavClientStdError},
    rfc4791::calendar::CaldavCalendar,
    rfc4918::{WebdavAuth, send::WebdavSendError},
    rfc6352::addressbook::{CarddavAddressbook, CarddavAddressbookPatch},
    rfc6578::sync_collection::WebdavSyncCollectionOptions,
};
#[cfg(any(
    feature = "rustls-aws",
    feature = "rustls-ring",
    feature = "native-tls"
))]
use pimalaya_stream::tls::Tls;
use url::Url;

/// Stream replaying canned HTTP responses: each read pops and serves the next
/// response whole; writes are accepted and discarded.
struct ScriptedStream {
    responses: VecDeque<Vec<u8>>,
}

impl ScriptedStream {
    fn new(responses: impl IntoIterator<Item = Vec<u8>>) -> Self {
        Self {
            responses: responses.into_iter().collect(),
        }
    }
}

impl Read for ScriptedStream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        let Some(response) = self.responses.pop_front() else {
            return Ok(0);
        };
        assert!(response.len() <= buf.len(), "scripted response too large");
        buf[..response.len()].copy_from_slice(&response);
        Ok(response.len())
    }
}

impl Write for ScriptedStream {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}

/// Stream failing every read, to surface I/O errors out of the client.
struct FailingStream;

impl Read for FailingStream {
    fn read(&mut self, _: &mut [u8]) -> IoResult<usize> {
        Err(IoError::other("scripted read failure"))
    }
}

impl Write for FailingStream {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}

fn base() -> Url {
    Url::parse("https://dav.example.org/").unwrap()
}

/// Client with the whole discovery state pre-cached, so each method consumes
/// exactly one scripted response.
fn discovered_client(responses: Vec<Vec<u8>>) -> WebdavClientStd {
    WebdavClientStd::from_parts(
        ScriptedStream::new(responses),
        WebdavAuth::None,
        base(),
        Some(Url::parse("https://dav.example.org/principals/alice/").unwrap()),
        Some(Url::parse("https://dav.example.org/dav/calendars/").unwrap()),
        Some(Url::parse("https://dav.example.org/dav/books/").unwrap()),
    )
}

const PRINCIPAL_XML: &str = r#"<d:multistatus xmlns:d="DAV:">
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

const CALENDAR_HOME_XML: &str = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/principals/alice/</d:href>
    <d:propstat>
      <d:prop><c:calendar-home-set><d:href>/dav/calendars/</d:href></c:calendar-home-set></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

const ADDRESSBOOK_HOME_XML: &str = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav">
  <d:response>
    <d:href>/principals/alice/</d:href>
    <d:propstat>
      <d:prop><c:addressbook-home-set><d:href>/dav/books/</d:href></c:addressbook-home-set></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

const CALENDARS_XML: &str = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/dav/calendars/personal/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/><c:calendar/></d:resourcetype>
        <d:displayname>Personal</d:displayname>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

const ITEMS_XML: &str = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/dav/calendars/personal/event-1.ics</d:href>
    <d:propstat>
      <d:prop>
        <d:getetag>"etag-1"</d:getetag>
        <c:calendar-data>BEGIN:VCALENDAR
END:VCALENDAR</c:calendar-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

const ITEM_SYNC_XML: &str = r#"<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/dav/calendars/personal/event-1.ics</d:href>
    <d:propstat>
      <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:sync-token>http://example.org/ns/sync/7</d:sync-token>
</d:multistatus>"#;

const PROPPATCH_OK_XML: &str = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav">
  <d:response>
    <d:href>/dav/books/team/</d:href>
    <d:propstat>
      <d:prop><d:displayname/><c:calendar-description/></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

const ADDRESSBOOKS_XML: &str = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav">
  <d:response>
    <d:href>/dav/books/contacts/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/><c:addressbook/></d:resourcetype>
        <d:displayname>Contacts</d:displayname>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

const CARDS_XML: &str = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav">
  <d:response>
    <d:href>/dav/books/contacts/alice.vcf</d:href>
    <d:propstat>
      <d:prop>
        <d:getetag>"etag-1"</d:getetag>
        <c:address-data>BEGIN:VCARD
END:VCARD</c:address-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

const SYNC_XML: &str = r#"<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/dav/books/contacts/alice.vcf</d:href>
    <d:propstat>
      <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:sync-token>http://example.org/ns/sync/42</d:sync-token>
</d:multistatus>"#;

// --- construction and options ---

#[test]
fn new_applies_the_default_options() {
    let client = WebdavClientStd::new(ScriptedStream::new([]), WebdavAuth::None, base());
    assert!(client.user_agent.starts_with("io-webdav/"));
    assert!(client.principal_url.is_none());
    assert!(client.calendar_home_set.is_none());
    assert!(client.addressbook_home_set.is_none());

    let debug = format!("{client:?}");
    assert!(debug.contains("https://dav.example.org/"));
    assert!(debug.contains("user_agent"));
}

#[test]
fn auth_returns_the_active_scheme() {
    let auth = basic_auth("alice", "secret");
    let client = WebdavClientStd::new(ScriptedStream::new([]), auth, base());
    assert!(matches!(client.auth(), WebdavAuth::Basic(_)));
}

#[test]
fn set_stream_swaps_the_transport() {
    let mut client = WebdavClientStd::new(FailingStream, WebdavAuth::None, base());
    client.principal_url = Some(Url::parse("https://dav.example.org/principals/alice/").unwrap());
    client.set_stream(ScriptedStream::new([multistatus_response(
        CALENDAR_HOME_XML,
    )]));

    let home = client.calendar_home_set().expect("home-set discovery");
    assert_eq!(home.as_str(), "https://dav.example.org/dav/calendars/");
}

// --- connect ---

#[cfg(any(
    feature = "rustls-aws",
    feature = "rustls-ring",
    feature = "native-tls"
))]
#[test]
fn connect_opens_plain_tcp_for_http() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local listener");
    let port = listener.local_addr().unwrap().port();
    let url = Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap();

    let client = WebdavClientStd::connect(&url, &Tls::default(), WebdavAuth::None)
        .expect("plain TCP connect");
    assert_eq!(client.base_url, url);
}

#[cfg(any(
    feature = "rustls-aws",
    feature = "rustls-ring",
    feature = "native-tls"
))]
#[test]
fn connect_rejects_hostless_urls() {
    let url = Url::parse("data:,x").unwrap();
    let err = WebdavClientStd::connect(&url, &Tls::default(), WebdavAuth::None).unwrap_err();
    assert!(matches!(err, WebdavClientStdError::UrlMissingHost(_)));
}

#[cfg(any(
    feature = "rustls-aws",
    feature = "rustls-ring",
    feature = "native-tls"
))]
#[test]
fn connect_rejects_unsupported_schemes() {
    let url = Url::parse("ftp://dav.example.org/").unwrap();
    let err = WebdavClientStd::connect(&url, &Tls::default(), WebdavAuth::None).unwrap_err();
    let WebdavClientStdError::UrlUnsupportedScheme(_, scheme) = err else {
        panic!("expected UrlUnsupportedScheme, got {err:?}");
    };
    assert_eq!(scheme, "ftp");
}

#[cfg(any(
    feature = "rustls-aws",
    feature = "rustls-ring",
    feature = "native-tls"
))]
#[test]
fn connect_surfaces_tls_failures() {
    // NOTE: bind then drop a listener to obtain a port that refuses the
    // connection, so the TLS connect fails fast instead of hanging.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local listener");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let url = Url::parse(&format!("https://127.0.0.1:{port}/")).unwrap();
    let result = WebdavClientStd::connect(&url, &Tls::default(), WebdavAuth::None);
    assert!(result.is_err(), "expected the TLS connect to fail");
}

// --- discovery ---

#[test]
fn current_user_principal_discovers_then_caches() {
    let mut client = WebdavClientStd::new(
        ScriptedStream::new([multistatus_response(PRINCIPAL_XML)]),
        WebdavAuth::None,
        base(),
    );

    let principal = client.current_user_principal().expect("discovery");
    assert_eq!(
        principal.as_str(),
        "https://dav.example.org/principals/alice/"
    );

    // NOTE: no scripted response left; a second call must hit the cache.
    let cached = client.current_user_principal().expect("cached");
    assert_eq!(cached, principal);
}

#[test]
fn current_user_principal_fails_on_an_empty_multistatus() {
    let empty = multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>");
    let mut client = WebdavClientStd::new(ScriptedStream::new([empty]), WebdavAuth::None, base());
    let err = client.current_user_principal().unwrap_err();
    assert!(matches!(err, WebdavClientStdError::MissingPrincipal));
}

#[test]
fn discovery_redirects_surface_as_unexpected_with_the_target() {
    let redirect = http_response("301 Moved Permanently", &[("Location", "/dav/")], "");
    let mut client =
        WebdavClientStd::new(ScriptedStream::new([redirect]), WebdavAuth::None, base());
    let err = client.current_user_principal().unwrap_err();
    let WebdavClientStdError::UnexpectedRedirect(url) = err else {
        panic!("expected UnexpectedRedirect, got {err:?}");
    };
    assert_eq!(url.as_str(), "https://dav.example.org/dav/");
}

#[test]
fn home_set_discovery_redirects_surface_as_unexpected() {
    let redirect = || http_response("301 Moved Permanently", &[("Location", "/dav/")], "");

    let mut client = WebdavClientStd::new(
        ScriptedStream::new([multistatus_response(PRINCIPAL_XML), redirect()]),
        WebdavAuth::None,
        base(),
    );
    let err = client.calendar_home_set().unwrap_err();
    assert!(matches!(err, WebdavClientStdError::UnexpectedRedirect(_)));

    let mut client = WebdavClientStd::new(
        ScriptedStream::new([multistatus_response(PRINCIPAL_XML), redirect()]),
        WebdavAuth::None,
        base(),
    );
    let err = client.addressbook_home_set().unwrap_err();
    assert!(matches!(err, WebdavClientStdError::UnexpectedRedirect(_)));
}

#[test]
fn calendar_home_set_runs_the_full_discovery_then_caches() {
    let mut client = WebdavClientStd::new(
        ScriptedStream::new([
            multistatus_response(PRINCIPAL_XML),
            multistatus_response(CALENDAR_HOME_XML),
        ]),
        WebdavAuth::None,
        base(),
    );

    let home = client.calendar_home_set().expect("discovery");
    assert_eq!(home.as_str(), "https://dav.example.org/dav/calendars/");
    assert_eq!(client.calendar_home_set().expect("cached"), home);
}

#[test]
fn calendar_home_set_fails_on_an_empty_multistatus() {
    let mut client = WebdavClientStd::new(
        ScriptedStream::new([
            multistatus_response(PRINCIPAL_XML),
            multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>"),
        ]),
        WebdavAuth::None,
        base(),
    );
    let err = client.calendar_home_set().unwrap_err();
    assert!(matches!(err, WebdavClientStdError::MissingCalendarHomeSet));
}

#[test]
fn addressbook_home_set_runs_the_full_discovery_then_caches() {
    let mut client = WebdavClientStd::new(
        ScriptedStream::new([
            multistatus_response(PRINCIPAL_XML),
            multistatus_response(ADDRESSBOOK_HOME_XML),
        ]),
        WebdavAuth::None,
        base(),
    );

    let home = client.addressbook_home_set().expect("discovery");
    assert_eq!(home.as_str(), "https://dav.example.org/dav/books/");
    assert_eq!(client.addressbook_home_set().expect("cached"), home);
}

#[test]
fn addressbook_home_set_fails_on_an_empty_multistatus() {
    let mut client = WebdavClientStd::new(
        ScriptedStream::new([
            multistatus_response(PRINCIPAL_XML),
            multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>"),
        ]),
        WebdavAuth::None,
        base(),
    );
    let err = client.addressbook_home_set().unwrap_err();
    assert!(matches!(
        err,
        WebdavClientStdError::MissingAddressbookHomeSet
    ));
}

#[test]
fn methods_require_the_home_set_cache() {
    let mut client = WebdavClientStd::new(ScriptedStream::new([]), WebdavAuth::None, base());

    let calendar = CaldavCalendar::default();
    assert!(matches!(
        client.list_calendars().unwrap_err(),
        WebdavClientStdError::MissingCalendarHomeSet
    ));
    assert!(matches!(
        client.create_calendar(&calendar).unwrap_err(),
        WebdavClientStdError::MissingCalendarHomeSet
    ));
    assert!(matches!(
        client.update_calendar(&calendar).unwrap_err(),
        WebdavClientStdError::MissingCalendarHomeSet
    ));
    assert!(matches!(
        client.delete_calendar("personal").unwrap_err(),
        WebdavClientStdError::MissingCalendarHomeSet
    ));
    assert!(matches!(
        client.list_items("personal", "").unwrap_err(),
        WebdavClientStdError::MissingCalendarHomeSet
    ));

    let addressbook = CarddavAddressbook::default();
    assert!(matches!(
        client.list_addressbooks().unwrap_err(),
        WebdavClientStdError::MissingAddressbookHomeSet
    ));
    assert!(matches!(
        client.create_addressbook(&addressbook).unwrap_err(),
        WebdavClientStdError::MissingAddressbookHomeSet
    ));
    assert!(matches!(
        client
            .update_addressbook(&CarddavAddressbookPatch::default())
            .unwrap_err(),
        WebdavClientStdError::MissingAddressbookHomeSet
    ));
    assert!(matches!(
        client.delete_addressbook("contacts").unwrap_err(),
        WebdavClientStdError::MissingAddressbookHomeSet
    ));
    assert!(matches!(
        client.list_cards("contacts").unwrap_err(),
        WebdavClientStdError::MissingAddressbookHomeSet
    ));
}

#[test]
fn http_failures_surface_as_send_errors() {
    let mut client = discovered_client(vec![http_response("403 Forbidden", &[], "denied")]);
    let err = client.list_calendars().unwrap_err();
    assert!(matches!(
        err,
        WebdavClientStdError::Send(WebdavSendError::HttpStatus { status: 403, .. })
    ));
}

#[test]
fn discovery_http_failures_surface_as_follow_redirects_errors() {
    let forbidden = http_response("403 Forbidden", &[], "denied");
    let mut client =
        WebdavClientStd::new(ScriptedStream::new([forbidden]), WebdavAuth::None, base());
    let err = client.current_user_principal().unwrap_err();
    assert!(matches!(
        err,
        WebdavClientStdError::WebdavFollowRedirects(_)
    ));
}

#[test]
fn io_failures_surface_as_io_errors() {
    let mut client = WebdavClientStd::new(FailingStream, WebdavAuth::None, base());
    let err = client.current_user_principal().unwrap_err();
    assert!(matches!(err, WebdavClientStdError::Io(_)));

    client.calendar_home_set = Some(Url::parse("https://dav.example.org/dav/calendars/").unwrap());
    let err = client.list_calendars().unwrap_err();
    assert!(matches!(err, WebdavClientStdError::Io(_)));
}

// --- CalDAV methods ---

#[test]
fn calendar_methods_run_their_coroutines() {
    let mut client = discovered_client(vec![
        multistatus_response(CALENDARS_XML),
        http_response("201 Created", &[], ""),
        multistatus_response(PROPPATCH_OK_XML),
        http_response("204 No Content", &[], ""),
    ]);

    let calendars = client.list_calendars().expect("list calendars");
    assert_eq!(calendars.first().unwrap().id, "personal");

    let calendar = CaldavCalendar {
        id: "work".into(),
        display_name: Some("Work".into()),
        ..Default::default()
    };
    client.create_calendar(&calendar).expect("create calendar");
    client.update_calendar(&calendar).expect("update calendar");
    client.delete_calendar("work").expect("delete calendar");
}

#[test]
fn item_methods_run_their_coroutines() {
    let mut client = discovered_client(vec![
        multistatus_response(ITEMS_XML),
        multistatus_response(ITEMS_XML),
        multistatus_response(ITEMS_XML),
        multistatus_response(ITEM_SYNC_XML),
        http_response("200 OK", &[("ETag", "\"etag-1\"")], "BEGIN:VCALENDAR"),
        http_response("201 Created", &[("ETag", "\"etag-1\"")], ""),
        http_response("204 No Content", &[], ""),
        http_response("204 No Content", &[], ""),
    ]);

    let items = client.list_items("personal", "").expect("list items");
    assert_eq!(items.first().unwrap().id, "event-1.ics");

    let refs = client.enum_items("personal", "").expect("enum items");
    assert_eq!(refs.refs.first().unwrap().id, "event-1.ics");

    let fetched = client
        .multiget_items("personal", &["event-1.ics"])
        .expect("multiget items");
    assert_eq!(fetched.len(), 1);

    let delta = client
        .sync_items("personal", None, Default::default())
        .expect("initial sync");
    assert_eq!(delta.changed.len(), 1);
    assert_eq!(
        delta.sync_token.as_deref(),
        Some("http://example.org/ns/sync/7")
    );

    let body = client
        .read_item("personal", "event-1.ics")
        .expect("read item");
    assert_eq!(body.etag.as_deref(), Some("etag-1"));

    let created = client
        .create_item("personal", "event-1.ics", b"BEGIN:VCALENDAR".to_vec())
        .expect("create item");
    assert_eq!(created.id, "event-1.ics");

    client
        .update_item(
            "personal",
            "event-1.ics",
            b"BEGIN:VCALENDAR".to_vec(),
            Some("etag-1"),
        )
        .expect("update item");
    client
        .delete_item("personal", "event-1.ics", None)
        .expect("delete item");
}

// --- CardDAV methods ---

#[test]
fn addressbook_methods_run_their_coroutines() {
    let mut client = discovered_client(vec![
        multistatus_response(ADDRESSBOOKS_XML),
        http_response("201 Created", &[], ""),
        multistatus_response(PROPPATCH_OK_XML),
        http_response("204 No Content", &[], ""),
    ]);

    let addressbooks = client.list_addressbooks().expect("list addressbooks");
    assert_eq!(addressbooks.first().unwrap().id, "contacts");

    let addressbook = CarddavAddressbook {
        id: "team".into(),
        display_name: Some("Team".into()),
        ..Default::default()
    };
    client
        .create_addressbook(&addressbook)
        .expect("create addressbook");

    let patch = CarddavAddressbookPatch {
        id: "team".into(),
        display_name: Some(Some("Team".into())),
        ..Default::default()
    };
    client
        .update_addressbook(&patch)
        .expect("update addressbook");
    client
        .delete_addressbook("team")
        .expect("delete addressbook");
}

#[test]
fn a_refused_property_fails_the_update() {
    // NOTE: the request itself is a 207, so only the propstat says the property
    // never changed. iCloud answers this way for a collection that does not
    // exist.
    let refused = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/dav/books/nope/</d:href>
        <d:propstat>
          <d:prop><d:displayname/></d:prop>
          <d:status>HTTP/1.1 403 Forbidden</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;
    let mut client = discovered_client(vec![
        multistatus_response(refused),
        multistatus_response(refused),
    ]);

    let patch = CarddavAddressbookPatch {
        id: "nope".into(),
        display_name: Some(Some("Renamed".into())),
        ..Default::default()
    };
    let err = client.update_addressbook(&patch).unwrap_err();
    assert!(matches!(
        err,
        WebdavClientStdError::PropertiesRejected(ref failures) if failures.len() == 1
    ));
    assert!(err.to_string().contains("displayname (403)"));

    let calendar = CaldavCalendar {
        id: "nope".into(),
        display_name: Some("Renamed".into()),
        ..Default::default()
    };
    assert!(matches!(
        client.update_calendar(&calendar).unwrap_err(),
        WebdavClientStdError::PropertiesRejected(_)
    ));
}

#[test]
fn a_property_the_server_never_mentions_fails_the_update() {
    // NOTE: iCloud answers a PROPPATCH on a collection that does not exist with
    // a 200 propstat carrying an empty prop, naming nothing, where RFC 4918
    // §9.2.1 wants a propstat per requested property.
    let silent = r#"<multistatus xmlns="DAV:">
      <response>
        <href>/dav/books/nope/</href>
        <propstat>
          <prop></prop>
          <status>HTTP/1.1 200 OK</status>
        </propstat>
      </response>
    </multistatus>"#;
    let mut client = discovered_client(vec![multistatus_response(silent)]);

    let patch = CarddavAddressbookPatch {
        id: "nope".into(),
        display_name: Some(Some("Renamed".into())),
        description: Some(None),
        ..Default::default()
    };
    let err = client.update_addressbook(&patch).unwrap_err();

    assert!(matches!(
        err,
        WebdavClientStdError::PropertiesIgnored(ref ignored) if ignored.len() == 2
    ));
    assert!(err.to_string().contains("displayname"));
    assert!(err.to_string().contains("addressbook-description"));
}

#[test]
fn card_methods_run_their_coroutines() {
    let mut client = discovered_client(vec![
        multistatus_response(CARDS_XML),
        multistatus_response(CARDS_XML),
        multistatus_response(CARDS_XML),
        multistatus_response(SYNC_XML),
        http_response("200 OK", &[("ETag", "\"etag-1\"")], "BEGIN:VCARD"),
        http_response("201 Created", &[("ETag", "\"etag-1\"")], ""),
        http_response("204 No Content", &[], ""),
        http_response("204 No Content", &[], ""),
    ]);

    let cards = client.list_cards("contacts").expect("list cards");
    assert_eq!(cards.first().unwrap().id, "alice.vcf");

    let refs = client.enum_cards("contacts").expect("enum cards");
    assert_eq!(refs.refs.first().unwrap().id, "alice.vcf");

    let fetched = client
        .multiget_cards("contacts", &["alice.vcf"])
        .expect("multiget cards");
    assert_eq!(fetched.len(), 1);

    let delta = client
        .sync_cards("contacts", None, Default::default())
        .expect("initial sync");
    assert_eq!(delta.changed.len(), 1);
    assert_eq!(
        delta.sync_token.as_deref(),
        Some("http://example.org/ns/sync/42")
    );

    let body = client
        .read_card("contacts", "alice.vcf")
        .expect("read card");
    assert_eq!(body.etag.as_deref(), Some("etag-1"));

    let created = client
        .create_card("contacts", "alice", b"BEGIN:VCARD".to_vec())
        .expect("create card");
    assert_eq!(created.id, "alice");

    client
        .update_card(
            "contacts",
            "alice.vcf",
            b"BEGIN:VCARD".to_vec(),
            Some("etag-1"),
        )
        .expect("update card");
    client
        .delete_card("contacts", "alice.vcf", None)
        .expect("delete card");
}

#[test]
fn listing_caches_the_reports_each_collection_advertises() {
    let books = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav">
      <d:response>
        <d:href>/dav/books/contacts/</d:href>
        <d:propstat>
          <d:prop>
            <d:resourcetype><d:collection/><c:addressbook/></d:resourcetype>
            <d:supported-report-set>
              <d:supported-report><d:report><c:addressbook-query/></d:report></d:supported-report>
            </d:supported-report-set>
          </d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;
    let calendars = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
      <d:response>
        <d:href>/dav/calendars/personal/</d:href>
        <d:propstat>
          <d:prop>
            <d:resourcetype><d:collection/><c:calendar/></d:resourcetype>
            <d:supported-report-set>
              <d:supported-report><d:report><d:sync-collection/></d:report></d:supported-report>
            </d:supported-report-set>
          </d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;
    let mut client = discovered_client(vec![
        multistatus_response(books),
        multistatus_response(calendars),
    ]);

    client.list_addressbooks().expect("list addressbooks");
    client.list_calendars().expect("list calendars");

    // NOTE: this is what a consumer reads to pick its enumeration, instead of
    // paying a failed REPORT to learn the same thing.
    let books = &client.addressbook_reports["contacts"];
    assert!(!books.contains("sync-collection"));
    assert!(books.contains("addressbook-query"));
    assert!(client.calendar_reports["personal"].contains("sync-collection"));
}

#[test]
fn sync_cards_enumerates_with_the_fallback_when_asked() {
    let listing = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/dav/books/contacts/alice.vcf</d:href>
        <d:propstat>
          <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;
    let mut client = discovered_client(vec![multistatus_response(listing)]);

    let delta = client
        .sync_cards(
            "contacts",
            None,
            WebdavSyncCollectionOptions { fallback: true },
        )
        .expect("fallback enumeration");

    assert_eq!(delta.changed.len(), 1);
    assert!(delta.sync_token.is_none());
}

#[test]
fn an_unimplemented_report_is_recognised_through_either_path() {
    let body = r#"<d:error xmlns:d="DAV:"><d:supported-report/></d:error>"#;
    let refusal = || http_response("403 Forbidden", &[], body);
    let mut client = discovered_client(vec![refusal(), refusal(), refusal()]);

    // NOTE: the enumeration nests its send one level deeper than the query, so
    // a consumer matching one variant alone misses half the refusals.
    let enumerated = client
        .sync_cards("contacts", None, Default::default())
        .unwrap_err();
    assert!(enumerated.is_unsupported_report());

    let queried = client.enum_cards("contacts").unwrap_err();
    assert!(queried.is_unsupported_report());

    let denied = client.list_addressbooks().unwrap_err();
    assert!(
        !denied.is_unsupported_report(),
        "a PROPFIND refusal is not a missing report",
    );
}
