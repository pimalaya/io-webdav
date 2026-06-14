//! Shared helpers for provider integration tests.
//!
//! Each test drives [`WebdavClientStd`] against a live CalDAV / CardDAV
//! server. Call [`caldav`] for a full calendar CRUD flow, [`carddav`]
//! for a full addressbook CRUD flow, or [`caldav_readonly`] for the
//! discovery + list subset that providers without `MKCALENDAR` support
//! (e.g. Google) still satisfy.
//!
//! Providers that forbid collection creation (iCloud rejects both
//! `MKCALENDAR` and `MKCOL` with 403, exposing only the collections it
//! provisions) get [`caldav_items`] / [`carddav_cards`]: item / card
//! CRUD inside a caller-named existing collection, with no collection
//! create or delete.
//!
//! A fresh stream is opened before every request, so the flows do not
//! depend on the server honouring HTTP keep-alive across operations.
//!
//! The full CalDAV flow exercises:
//!
//! ```text
//! CURRENT-USER-PRINCIPAL → CALENDAR-HOME-SET
//!   → MKCALENDAR create   (create test calendar)
//!   → PROPFIND list       (verify creation)
//!   → PUT create          (create test event)
//!   → REPORT list         (verify event present)
//!   → GET read            (fetch raw iCalendar)
//!   → PUT update          (bump the event)
//!   → DELETE item         (cleanup)
//!   → DELETE collection   (cleanup)
//! ```
//!
//! The full CardDAV flow mirrors it for addressbooks and vCards.
//!
//! Each integration test compiles this module on its own and only
//! exercises a subset of these helpers, so the rest end up flagged as
//! dead code; suppress the noise at the module level.

#![allow(dead_code)]

use std::{
    io::{Read, Result as IoResult, Write},
    net::TcpStream,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use io_http::{rfc6750::bearer::HttpAuthBearer, rfc7617::basic::HttpAuthBasic};
use io_webdav::{
    client::WebdavClientStd, rfc4791::calendar::Calendar, rfc4918::WebdavAuth,
    rfc6352::addressbook::Addressbook,
};
use rustls::{ClientConfig, ClientConnection, StreamOwned, pki_types::ServerName};
use rustls_platform_verifier::ConfigVerifierExt;
use url::Url;

/// A stream that is either a plain TCP connection or a TLS-wrapped one.
enum WebdavStream {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Read for WebdavStream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        match self {
            Self::Plain(s) => s.read(buf),
            Self::Tls(s) => s.read(buf),
        }
    }
}

impl Write for WebdavStream {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        match self {
            Self::Plain(s) => s.write(buf),
            Self::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> IoResult<()> {
        match self {
            Self::Plain(s) => s.flush(),
            Self::Tls(s) => s.flush(),
        }
    }
}

/// Builds an HTTP Basic [`WebdavAuth`] (RFC 7617).
pub fn basic_auth(username: &str, password: &str) -> WebdavAuth {
    WebdavAuth::Basic(HttpAuthBasic::new(username, password))
}

/// Builds an HTTP Bearer [`WebdavAuth`] (RFC 6750), e.g. an OAuth2
/// access token.
pub fn bearer_auth(token: &str) -> WebdavAuth {
    WebdavAuth::Bearer(HttpAuthBearer::new(token))
}

/// Opens a fresh stream to `url`'s authority: plain TCP for `http`,
/// TLS for `https` (ALPN left at the server default).
fn connect(url: &Url) -> WebdavStream {
    let host = url.host_str().expect("base URL has a host").to_owned();

    match url.scheme() {
        "http" => {
            let port = url.port().unwrap_or(80);
            let tcp = TcpStream::connect((host.as_str(), port)).expect("TCP connect");
            WebdavStream::Plain(tcp)
        }
        "https" => {
            let port = url.port().unwrap_or(443);
            let server_name = ServerName::try_from(host.clone()).expect("valid server name");
            let config = ClientConfig::with_platform_verifier().expect("TLS config");
            let conn = ClientConnection::new(Arc::new(config), server_name).expect("TLS handshake");
            let tcp = TcpStream::connect((host.as_str(), port)).expect("TCP connect");
            WebdavStream::Tls(Box::new(StreamOwned::new(conn, tcp)))
        }
        scheme => panic!("unsupported base URL scheme `{scheme}`"),
    }
}

/// Milliseconds since the Unix epoch, used to mint unique collection
/// and resource ids per run.
fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

/// Full CalDAV CRUD flow against the DAV root at `base_url`.
pub fn caldav(base_url: &str, auth: WebdavAuth) {
    let _ = env_logger::try_init();
    let base = Url::parse(base_url).expect("parse base URL");
    let mut client = WebdavClientStd::new(connect(&base), auth, base.clone());

    // ── DISCOVERY ─────────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    let principal = client
        .current_user_principal()
        .expect("current-user-principal discovery");
    assert!(!principal.path().is_empty(), "empty principal path");

    client.set_stream(connect(&base));
    let home = client
        .calendar_home_set()
        .expect("calendar-home-set discovery");
    assert!(!home.path().is_empty(), "empty calendar home-set path");

    let ts = unix_millis();
    let cal_id = format!("io-webdav-test-{ts}");
    let item_id = format!("event-{ts}");

    // ── MKCALENDAR create ───────────────────────────────────────────────────────

    let calendar = Calendar {
        id: cal_id.clone(),
        display_name: Some("io-webdav integration test".to_owned()),
        description: Some("created by io-webdav integration tests".to_owned()),
        ..Default::default()
    };
    client.set_stream(connect(&base));
    client.create_calendar(&calendar).expect("create calendar");

    // ── PROPFIND list (verify creation) ─────────────────────────────────────────

    client.set_stream(connect(&base));
    let calendars = client.list_calendars().expect("list calendars");
    assert!(
        calendars.iter().any(|c| c.id == cal_id),
        "created calendar {cal_id} missing from list"
    );

    // ── PUT create event ────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    let created = client
        .create_item(
            &cal_id,
            &item_id,
            build_ics(&item_id, "io-webdav event").into_bytes(),
        )
        .expect("create item");
    assert_eq!(created.id, item_id, "create item id mismatch");

    // ── REPORT list items (verify present) ──────────────────────────────────────

    client.set_stream(connect(&base));
    let items = client.list_items(&cal_id, "").expect("list items");
    assert!(
        items.iter().any(|i| i.id == item_id),
        "created event {item_id} missing from REPORT"
    );

    // ── GET read item ───────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    let body = client.read_item(&cal_id, &item_id).expect("read item");
    assert!(!body.data.is_empty(), "read item returned empty body");

    // ── PUT update item ─────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    client
        .update_item(
            &cal_id,
            &item_id,
            build_ics(&item_id, "io-webdav event (updated)").into_bytes(),
            body.etag.as_deref(),
        )
        .expect("update item");

    // ── CLEANUP: delete item then collection ────────────────────────────────────

    client.set_stream(connect(&base));
    client
        .delete_item(&cal_id, &item_id, None)
        .expect("delete item");

    client.set_stream(connect(&base));
    client.delete_calendar(&cal_id).expect("delete calendar");
}

/// Read-only CalDAV flow for providers without `MKCALENDAR` support
/// (e.g. Google): discover the home-set and list calendars.
pub fn caldav_readonly(base_url: &str, auth: WebdavAuth) {
    let _ = env_logger::try_init();
    let base = Url::parse(base_url).expect("parse base URL");
    let mut client = WebdavClientStd::new(connect(&base), auth, base.clone());

    client.set_stream(connect(&base));
    let principal = client
        .current_user_principal()
        .expect("current-user-principal discovery");
    assert!(!principal.path().is_empty(), "empty principal path");

    client.set_stream(connect(&base));
    let home = client
        .calendar_home_set()
        .expect("calendar-home-set discovery");
    assert!(!home.path().is_empty(), "empty calendar home-set path");

    client.set_stream(connect(&base));
    let calendars = client.list_calendars().expect("list calendars");
    assert!(
        !calendars.is_empty(),
        "expected at least one calendar in the home-set"
    );
}

/// CalDAV item CRUD inside the existing calendar `calendar_id`, for
/// providers that reject `MKCALENDAR` (e.g. iCloud): discover, confirm
/// the calendar is present, then create/list/read/update/delete an
/// event. The collection itself is never created nor deleted.
pub fn caldav_items(base_url: &str, auth: WebdavAuth, calendar_id: &str) {
    let _ = env_logger::try_init();
    let base = Url::parse(base_url).expect("parse base URL");
    let mut client = WebdavClientStd::new(connect(&base), auth, base.clone());

    // ── DISCOVERY ─────────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    let principal = client
        .current_user_principal()
        .expect("current-user-principal discovery");
    assert!(!principal.path().is_empty(), "empty principal path");

    client.set_stream(connect(&base));
    let home = client
        .calendar_home_set()
        .expect("calendar-home-set discovery");
    assert!(!home.path().is_empty(), "empty calendar home-set path");

    // ── PROPFIND list (confirm the target calendar exists) ──────────────────────

    client.set_stream(connect(&base));
    let calendars = client.list_calendars().expect("list calendars");
    assert!(
        calendars.iter().any(|c| c.id == calendar_id),
        "target calendar {calendar_id} missing from home-set"
    );

    let item_id = format!("event-{}", unix_millis());

    // ── PUT create event ────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    let created = client
        .create_item(
            calendar_id,
            &item_id,
            build_ics(&item_id, "io-webdav event").into_bytes(),
        )
        .expect("create item");
    assert_eq!(created.id, item_id, "create item id mismatch");

    // ── REPORT list items (verify present) ──────────────────────────────────────

    client.set_stream(connect(&base));
    let items = client.list_items(calendar_id, "").expect("list items");
    assert!(
        items.iter().any(|i| i.id == item_id),
        "created event {item_id} missing from REPORT"
    );

    // ── GET read item ───────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    let body = client.read_item(calendar_id, &item_id).expect("read item");
    assert!(!body.data.is_empty(), "read item returned empty body");

    // ── PUT update item ─────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    client
        .update_item(
            calendar_id,
            &item_id,
            build_ics(&item_id, "io-webdav event (updated)").into_bytes(),
            body.etag.as_deref(),
        )
        .expect("update item");

    // ── CLEANUP: delete the item only ───────────────────────────────────────────

    client.set_stream(connect(&base));
    client
        .delete_item(calendar_id, &item_id, None)
        .expect("delete item");
}

/// Full CardDAV CRUD flow against the DAV root at `base_url`.
pub fn carddav(base_url: &str, auth: WebdavAuth) {
    let _ = env_logger::try_init();
    let base = Url::parse(base_url).expect("parse base URL");
    let mut client = WebdavClientStd::new(connect(&base), auth, base.clone());

    // ── DISCOVERY ─────────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    let principal = client
        .current_user_principal()
        .expect("current-user-principal discovery");
    assert!(!principal.path().is_empty(), "empty principal path");

    client.set_stream(connect(&base));
    let home = client
        .addressbook_home_set()
        .expect("addressbook-home-set discovery");
    assert!(!home.path().is_empty(), "empty addressbook home-set path");

    let ts = unix_millis();
    let book_id = format!("io-webdav-test-{ts}");
    let card_id = format!("card-{ts}");

    // ── MKCOL create ────────────────────────────────────────────────────────────

    let addressbook = Addressbook {
        id: book_id.clone(),
        display_name: Some("io-webdav integration test".to_owned()),
        description: Some("created by io-webdav integration tests".to_owned()),
        ..Default::default()
    };
    client.set_stream(connect(&base));
    client
        .create_addressbook(&addressbook)
        .expect("create addressbook");

    // ── PROPFIND list (verify creation) ─────────────────────────────────────────

    client.set_stream(connect(&base));
    let addressbooks = client.list_addressbooks().expect("list addressbooks");
    assert!(
        addressbooks.iter().any(|b| b.id == book_id),
        "created addressbook {book_id} missing from list"
    );

    // ── PUT create card ─────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    let created = client
        .create_card(
            &book_id,
            &card_id,
            build_vcf(&card_id, "io-webdav Test").into_bytes(),
        )
        .expect("create card");
    assert_eq!(created.id, card_id, "create card id mismatch");

    // ── REPORT list cards (verify present) ──────────────────────────────────────

    client.set_stream(connect(&base));
    let cards = client.list_cards(&book_id).expect("list cards");
    assert!(
        cards.iter().any(|c| c.id == card_id),
        "created card {card_id} missing from REPORT"
    );

    // ── GET read card ───────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    let body = client.read_card(&book_id, &card_id).expect("read card");
    assert!(!body.data.is_empty(), "read card returned empty body");

    // ── PUT update card ─────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    client
        .update_card(
            &book_id,
            &card_id,
            build_vcf(&card_id, "io-webdav Test (updated)").into_bytes(),
            body.etag.as_deref(),
        )
        .expect("update card");

    // ── CLEANUP: delete card then collection ────────────────────────────────────

    client.set_stream(connect(&base));
    client
        .delete_card(&book_id, &card_id, None)
        .expect("delete card");

    client.set_stream(connect(&base));
    client
        .delete_addressbook(&book_id)
        .expect("delete addressbook");
}

/// CardDAV card CRUD inside the existing addressbook `addressbook_id`,
/// for providers that reject `MKCOL` (e.g. iCloud, which exposes a
/// single fixed `card` addressbook): discover, confirm the addressbook
/// is present, then create/list/read/update/delete a vCard. The
/// collection itself is never created nor deleted.
pub fn carddav_cards(base_url: &str, auth: WebdavAuth, addressbook_id: &str) {
    let _ = env_logger::try_init();
    let base = Url::parse(base_url).expect("parse base URL");
    let mut client = WebdavClientStd::new(connect(&base), auth, base.clone());

    // ── DISCOVERY ─────────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    let principal = client
        .current_user_principal()
        .expect("current-user-principal discovery");
    assert!(!principal.path().is_empty(), "empty principal path");

    client.set_stream(connect(&base));
    let home = client
        .addressbook_home_set()
        .expect("addressbook-home-set discovery");
    assert!(!home.path().is_empty(), "empty addressbook home-set path");

    // ── PROPFIND list (confirm the target addressbook exists) ───────────────────

    client.set_stream(connect(&base));
    let addressbooks = client.list_addressbooks().expect("list addressbooks");
    assert!(
        addressbooks.iter().any(|b| b.id == addressbook_id),
        "target addressbook {addressbook_id} missing from home-set"
    );

    let card_id = format!("card-{}", unix_millis());

    // ── PUT create card ─────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    let created = client
        .create_card(
            addressbook_id,
            &card_id,
            build_vcf(&card_id, "io-webdav Test").into_bytes(),
        )
        .expect("create card");
    assert_eq!(created.id, card_id, "create card id mismatch");

    // ── REPORT list cards (verify present) ──────────────────────────────────────

    client.set_stream(connect(&base));
    let cards = client.list_cards(addressbook_id).expect("list cards");
    assert!(
        cards.iter().any(|c| c.id == card_id),
        "created card {card_id} missing from REPORT"
    );

    // ── GET read card ───────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    let body = client
        .read_card(addressbook_id, &card_id)
        .expect("read card");
    assert!(!body.data.is_empty(), "read card returned empty body");

    // ── PUT update card ─────────────────────────────────────────────────────────

    client.set_stream(connect(&base));
    client
        .update_card(
            addressbook_id,
            &card_id,
            build_vcf(&card_id, "io-webdav Test (updated)").into_bytes(),
            body.etag.as_deref(),
        )
        .expect("update card");

    // ── CLEANUP: delete the card only ───────────────────────────────────────────

    client.set_stream(connect(&base));
    client
        .delete_card(addressbook_id, &card_id, None)
        .expect("delete card");
}

/// Builds a minimal single-event iCalendar object (CRLF line endings,
/// as required by RFC 5545 §3.1).
fn build_ics(uid: &str, summary: &str) -> String {
    [
        "BEGIN:VCALENDAR",
        "VERSION:2.0",
        "PRODID:-//Pimalaya//io-webdav integration test//EN",
        "BEGIN:VEVENT",
        &format!("UID:{uid}"),
        "DTSTAMP:20260101T000000Z",
        "DTSTART:20260101T120000Z",
        "DTEND:20260101T130000Z",
        &format!("SUMMARY:{summary}"),
        "END:VEVENT",
        "END:VCALENDAR",
    ]
    .join("\r\n")
}

/// Builds a minimal vCard 3.0 object (CRLF line endings, as required by
/// RFC 6350 §3.2).
fn build_vcf(uid: &str, name: &str) -> String {
    [
        "BEGIN:VCARD",
        "VERSION:3.0",
        &format!("UID:{uid}"),
        &format!("FN:{name}"),
        &format!("N:{name};;;;"),
        "EMAIL:io-webdav-test@pimalaya.org",
        "END:VCARD",
    ]
    .join("\r\n")
}
