//! Shared helpers for the integration tests.
//!
//! Two families live here. The scripted-coroutine helpers ([`http_response`]
//! plus the `expect_*` steps) let the offline suites (rfc4918, rfc4791,
//! rfc5397, rfc6352, rfc6578, client) resume any I/O-free coroutine against
//! canned HTTP response bytes, following the io-imap canonical layout. The
//! provider helpers below them run live CalDAV / CardDAV flows.
//!
//! Each provider test drives [`WebdavClientStd`] against a live CalDAV /
//! CardDAV server. Call [`caldav`] for a full calendar CRUD flow, [`carddav`]
//! for a full addressbook CRUD flow, or [`caldav_readonly`] for the discovery +
//! list subset that providers without `MKCALENDAR` support (e.g. Google) still
//! satisfy.
//!
//! Providers that forbid collection creation (iCloud rejects both `MKCALENDAR`
//! and `MKCOL` with 403, exposing only the collections it provisions) get
//! [`caldav_items`] / [`carddav_cards`]: item / card CRUD inside a caller-named
//! existing collection, with no collection create or delete.
//!
//! A fresh stream is opened before every request, so the flows do not depend on
//! the server honouring HTTP keep-alive across operations.
//!
//! These flows write to real production accounts, so everything a run creates
//! is torn down through [`with_cleanup`], on the failing path as much as on the
//! passing one. Read that function before adding a step that creates anything.
//!
//! The full CalDAV flow exercises:
//!
//! ```text
//! CURRENT-USER-PRINCIPAL → CALENDAR-HOME-SET
//!   → MKCALENDAR create   (create test calendar)
//!   ┌ guarded by with_cleanup ────────────────────────────────┐
//!   │ → PROPFIND list     (verify creation)                   │
//!   │ → PUT create        (create test event)                 │
//!   │ → REPORT list       (verify event present)              │
//!   │ → REPORT enum       (etag-only spine)                   │
//!   │ → REPORT multiget   (batch bodies)                      │
//!   │ → REPORT sync       (initial sync-collection)           │
//!   │ → GET read          (fetch raw iCalendar)               │
//!   │ → PUT update        (bump the event)                    │
//!   │ → DELETE item       (the removal the next sync reports) │
//!   │ → REPORT sync       (incremental, reports the removal)  │
//!   └─────────────────────────────────────────────────────────┘
//!   → DELETE collection   (teardown, runs however the guard exits)
//! ```
//!
//! The full CardDAV flow mirrors it for addressbooks and vCards. Both exercise
//! the sync read-side: etag-only enumeration, the protocol's multiget batch
//! fetch, and an initial plus incremental `sync-collection` REPORT (RFC 6578)
//! that must report the deleted resource as vanished.
//!
//! The item-only flows are guarded the same way, from the moment the event or
//! card is created, since the collection holding it belongs to the account
//! rather than to the run.
//!
//! Each integration test compiles this module on its own and only exercises a
//! subset of these helpers, so the rest end up flagged as dead code; suppress
//! the noise at the module level.

#![allow(dead_code)]

use core::fmt::Debug;

use std::{
    io::{Read, Result as IoResult, Write},
    net::TcpStream,
    panic::{self, AssertUnwindSafe},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use io_http::{
    coroutine::{HttpCoroutine, HttpCoroutineState, HttpYield},
    rfc6750::bearer::HttpAuthBearer,
    rfc7617::basic::HttpAuthBasic,
    rfc8615::well_known::Http11WellKnown,
};
use io_webdav::{
    client::WebdavClientStd, coroutine::*, rfc4791::calendar::CaldavCalendar, rfc4918::WebdavAuth,
    rfc4918::coroutine::*, rfc6352::addressbook::CarddavAddressbook,
};
use rustls::{ClientConfig, ClientConnection, StreamOwned, pki_types::ServerName};
use rustls_platform_verifier::ConfigVerifierExt;
use url::Url;

// --- scripted-coroutine helpers ---

/// Serializes an HTTP/1.1 response: the given status line, the extra headers, a
/// correct `Content-Length` and the body.
pub fn http_response(status: &str, extra: &[(&str, &str)], body: &str) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status}\r\n");
    for (name, value) in extra {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    out.into_bytes()
}

/// Shortcut for a 207 Multi-Status [`http_response`] carrying `xml`.
pub fn multistatus_response(xml: &str) -> Vec<u8> {
    http_response("207 Multi-Status", &[], xml)
}

/// Resumes a standard-shape coroutine and returns the written bytes.
pub fn expect_wants_write<C, R>(cor: &mut C, arg: Option<&[u8]>) -> Vec<u8>
where
    C: WebdavCoroutine<Yield = WebdavYield, Return = R>,
    R: Debug,
{
    match cor.resume(arg) {
        WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => bytes,
        state => panic!("expected WantsWrite, got {state:?}"),
    }
}

/// Resumes a standard-shape coroutine, expecting a read request.
pub fn expect_wants_read<C, R>(cor: &mut C)
where
    C: WebdavCoroutine<Yield = WebdavYield, Return = R>,
    R: Debug,
{
    match cor.resume(None) {
        WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {}
        state => panic!("expected WantsRead, got {state:?}"),
    }
}

/// Feeds `reply` to a standard-shape coroutine and returns its terminal value.
pub fn expect_complete<C, R>(cor: &mut C, reply: &[u8]) -> R
where
    C: WebdavCoroutine<Yield = WebdavYield, Return = R>,
    R: Debug,
{
    match cor.resume(Some(reply)) {
        WebdavCoroutineState::Complete(ret) => ret,
        state => panic!("expected Complete, got {state:?}"),
    }
}

/// Runs the canonical write/read/reply sequence against a standard-shape
/// coroutine: returns the written request bytes (lowercased for
/// case-insensitive assertions) plus the terminal value.
pub fn expect_exchange<C, R>(cor: &mut C, reply: &[u8]) -> (String, R)
where
    C: WebdavCoroutine<Yield = WebdavYield, Return = R>,
    R: Debug,
{
    let bytes = expect_wants_write(cor, None);
    let request = String::from_utf8_lossy(&bytes).to_lowercase();
    expect_wants_read(cor);
    (request, expect_complete(cor, reply))
}

/// Resumes a redirect-shape coroutine and returns the written bytes.
pub fn expect_redirect_wants_write<C, R>(cor: &mut C, arg: Option<&[u8]>) -> Vec<u8>
where
    C: WebdavCoroutine<Yield = WebdavRedirectYield, Return = R>,
    R: Debug,
{
    match cor.resume(arg) {
        WebdavCoroutineState::Yielded(WebdavRedirectYield::WantsWrite(bytes)) => bytes,
        state => panic!("expected WantsWrite, got {state:?}"),
    }
}

/// Resumes a redirect-shape coroutine, expecting a read request.
pub fn expect_redirect_wants_read<C, R>(cor: &mut C)
where
    C: WebdavCoroutine<Yield = WebdavRedirectYield, Return = R>,
    R: Debug,
{
    match cor.resume(None) {
        WebdavCoroutineState::Yielded(WebdavRedirectYield::WantsRead) => {}
        state => panic!("expected WantsRead, got {state:?}"),
    }
}

/// Feeds `reply` to a redirect-shape coroutine and returns its terminal value.
pub fn expect_redirect_complete<C, R>(cor: &mut C, reply: &[u8]) -> R
where
    C: WebdavCoroutine<Yield = WebdavRedirectYield, Return = R>,
    R: Debug,
{
    match cor.resume(Some(reply)) {
        WebdavCoroutineState::Complete(ret) => ret,
        state => panic!("expected Complete, got {state:?}"),
    }
}

/// Runs the canonical write/read/reply sequence against a redirect-shape
/// coroutine: returns the written request bytes (lowercased) plus the terminal
/// value.
pub fn expect_redirect_exchange<C, R>(cor: &mut C, reply: &[u8]) -> (String, R)
where
    C: WebdavCoroutine<Yield = WebdavRedirectYield, Return = R>,
    R: Debug,
{
    let bytes = expect_redirect_wants_write(cor, None);
    let request = String::from_utf8_lossy(&bytes).to_lowercase();
    expect_redirect_wants_read(cor);
    (request, expect_redirect_complete(cor, reply))
}

/// Feeds `reply` to a redirect-shape coroutine and returns the surfaced
/// redirect (target URL, keep-alive flag, same-origin flag).
pub fn expect_wants_redirect<C, R>(cor: &mut C, reply: &[u8]) -> (Url, bool, bool)
where
    C: WebdavCoroutine<Yield = WebdavRedirectYield, Return = R>,
    R: Debug,
{
    match cor.resume(Some(reply)) {
        WebdavCoroutineState::Yielded(WebdavRedirectYield::WantsRedirect {
            url,
            keep_alive,
            same_origin,
        }) => (url, keep_alive, same_origin),
        state => panic!("expected WantsRedirect, got {state:?}"),
    }
}

// --- live provider helpers ---

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

/// Builds an HTTP Bearer [`WebdavAuth`] (RFC 6750), e.g. an OAuth2 access
/// token.
pub fn bearer_auth(token: &str) -> WebdavAuth {
    WebdavAuth::Bearer(HttpAuthBearer::new(token))
}

/// Opens a fresh stream to `url`'s authority: plain TCP for `http`, TLS for
/// `https` (ALPN left at the server default).
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

/// Milliseconds since the Unix epoch, used to mint unique collection and
/// resource ids per run.
fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

/// Runs `body`, then `cleanup` whichever way `body` went, and only then
/// re-raises a panic `body` may have raised.
///
/// These flows run against real production accounts. Every step panics on
/// failure, so a cleanup written as the last statements of a flow is skipped
/// the moment anything goes wrong, and each failed run leaves a collection or a
/// resource behind in the account for good. Whatever a run created has to be
/// torn down on the failing path too, which is the one where it matters.
///
/// `cleanup` is best-effort on purpose. It is caught too, because it reconnects
/// and every connection step panics on failure: a teardown that cannot reach
/// the server any more must report that and step aside, never replace the
/// failure the run was about to report.
fn with_cleanup<T, B, C>(state: &mut T, body: B, cleanup: C)
where
    B: FnOnce(&mut T),
    C: FnOnce(&mut T),
{
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| body(state)));

    if panic::catch_unwind(AssertUnwindSafe(|| cleanup(state))).is_err() {
        eprintln!("WARNING: cleanup itself failed, the account may hold leftovers");
    }

    if let Err(payload) = outcome {
        panic::resume_unwind(payload);
    }
}

/// Reports a failed teardown without panicking, naming what was left behind so
/// it can be removed by hand.
fn report_leftover(what: &str, id: &str, err: &dyn Debug) {
    eprintln!("WARNING: could not clean up {what} `{id}`, remove it by hand: {err:?}");
}

/// Full CalDAV CRUD flow against the DAV root at `base_url`.
pub fn caldav(base_url: &str, auth: WebdavAuth) {
    let _ = env_logger::try_init();
    let base = Url::parse(base_url).expect("parse base URL");
    let mut client = WebdavClientStd::new(connect(&base), auth, base.clone());

    // --- discovery ---

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
    // NOTE: the caller owns the whole resource name, extension included; the
    // same name is the item's id everywhere afterwards.
    let item_name = format!("{item_id}.ics");

    // --- MKCALENDAR create ---

    let calendar = CaldavCalendar {
        id: cal_id.clone(),
        display_name: Some("io-webdav integration test".to_owned()),
        description: Some("created by io-webdav integration tests".to_owned()),
        components: ["VEVENT".to_owned()].into(),
        ..Default::default()
    };
    client.set_stream(connect(&base));
    client.create_calendar(&calendar).expect("create calendar");

    // NOTE: from here on the account holds a real collection, so every exit
    // path has to remove it. See with_cleanup.
    with_cleanup(
        &mut client,
        |client| caldav_body(client, &base, &cal_id, &item_id, &item_name),
        |client| {
            client.set_stream(connect(&base));
            if let Err(err) = client.delete_calendar(&cal_id) {
                report_leftover("calendar", &cal_id, &err);
            }
        },
    );
}

/// The body of the full CalDAV flow, everything that runs once the test
/// calendar exists. Split out so [`with_cleanup`] can own the teardown.
fn caldav_body(
    client: &mut WebdavClientStd,
    base: &Url,
    cal_id: &str,
    item_id: &str,
    item_name: &str,
) {
    // --- PROPFIND list (verify creation) ---

    client.set_stream(connect(base));
    let calendars = client.list_calendars().expect("list calendars");
    let created_calendar = calendars
        .iter()
        .find(|c| c.id == cal_id)
        .unwrap_or_else(|| panic!("created calendar {cal_id} missing from list"));
    // NOTE: the component set was sent at MKCALENDAR time, so a server that
    // honours it reports it back; one that advertises nothing at all is saying
    // "any type", which is not a mismatch.
    assert!(
        created_calendar.components.is_empty() || created_calendar.components.contains("VEVENT"),
        "created calendar dropped the VEVENT component: {:?}",
        created_calendar.components
    );

    // --- PUT create event ---

    client.set_stream(connect(base));
    let created = client
        .create_item(
            cal_id,
            item_name,
            build_ics(item_id, "io-webdav event").into_bytes(),
        )
        .expect("create item");
    assert_eq!(created.id, item_name, "create item id mismatch");

    // --- REPORT list items (verify present) ---

    client.set_stream(connect(base));
    let items = client.list_items(cal_id, "").expect("list items");
    // NOTE: an item is addressed by its id, i.e. the resource name the server
    // enumerates, used verbatim: we created `<item_id>.ics`, so that is its id
    // everywhere (io-webdav never adds nor strips an extension).
    assert!(
        items.iter().any(|i| i.id == item_name),
        "created event {item_name} missing from REPORT"
    );

    // --- REPORT enum item refs (ETag-only spine) ---

    client.set_stream(connect(base));
    let refs = client.enum_items(cal_id, "").expect("enum items");
    assert!(
        refs.refs.iter().any(|r| r.id == item_name),
        "created event {item_name} missing from etag-only enumeration"
    );

    // --- REPORT multiget (batch bodies) ---

    client.set_stream(connect(base));
    let fetched = client
        .multiget_items(cal_id, &[item_name])
        .expect("multiget items");
    assert!(
        fetched
            .iter()
            .any(|i| i.id == item_name && !i.data.is_empty()),
        "multiget returned no body for event {item_name}"
    );

    // --- REPORT sync-collection (initial sync) ---

    client.set_stream(connect(base));
    let initial = client
        .sync_items(cal_id, None, Default::default())
        .expect("initial sync");
    assert!(
        initial.changed.iter().any(|c| c.href.contains(item_id)),
        "created event {item_id} missing from initial sync"
    );
    let sync_token = initial.sync_token.expect("initial sync returned no token");

    // --- GET read item ---

    client.set_stream(connect(base));
    let body = client.read_item(cal_id, item_name).expect("read item");
    assert!(!body.data.is_empty(), "read item returned empty body");

    // --- PUT update item ---

    client.set_stream(connect(base));
    client
        .update_item(
            cal_id,
            item_name,
            build_ics(item_id, "io-webdav event (updated)").into_bytes(),
            body.etag.as_deref(),
        )
        .expect("update item");

    // --- DELETE item (the removal the next sync must report) ---

    client.set_stream(connect(base));
    client
        .delete_item(cal_id, item_name, None)
        .expect("delete item");

    // --- REPORT sync-collection (incremental sync reports the removal) ---

    client.set_stream(connect(base));
    let delta = client
        .sync_items(cal_id, Some(&sync_token), Default::default())
        .expect("incremental sync");
    assert!(
        delta.vanished.iter().any(|href| href.contains(item_id)),
        "deleted event {item_id} missing from incremental sync removals"
    );
}

/// Read-only CalDAV flow for providers without `MKCALENDAR` support (e.g.
/// Google): discover the home-set and list calendars.
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

/// Read-only CardDAV flow for providers without `MKCOL` support (e.g. Google):
/// discover the home-set and list addressbooks.
pub fn carddav_readonly(base_url: &str, auth: WebdavAuth) {
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
        .addressbook_home_set()
        .expect("addressbook-home-set discovery");
    assert!(!home.path().is_empty(), "empty addressbook home-set path");

    client.set_stream(connect(&base));
    let addressbooks = client.list_addressbooks().expect("list addressbooks");
    assert!(
        !addressbooks.is_empty(),
        "expected at least one addressbook in the home-set"
    );
}

/// Resolves Google's CardDAV context root by issuing an authenticated PROPFIND
/// to `https://www.googleapis.com/.well-known/carddav` and returning the
/// `Location` it 301-redirects to.
///
/// Google's `.well-known` only redirects for an authenticated PROPFIND; a plain
/// GET (or an unauthenticated request) 404s. So this reuses the HTTP well-known
/// request builder, swaps the method to PROPFIND, and adds the OAuth2 bearer.
pub fn google_carddav_base(token: &str) -> Url {
    let origin = "https://www.googleapis.com/";

    let mut request =
        Http11WellKnown::prepare_request(origin, "carddav").expect("prepare well-known request");
    request.method = "PROPFIND".into();
    let request = request
        .header(
            "Authorization",
            HttpAuthBearer::new(token).to_authorization(),
        )
        .header("Depth", "0");

    let mut stream = connect(&Url::parse(origin).expect("parse well-known origin"));
    let mut coroutine = Http11WellKnown::new(request);
    let mut buf = [0u8; 8 * 1024];
    let mut arg: Option<&[u8]> = None;

    let output = loop {
        match coroutine.resume(arg.take()) {
            HttpCoroutineState::Complete(Ok(output)) => break output,
            HttpCoroutineState::Complete(Err(err)) => panic!("well-known PROPFIND failed: {err}"),
            HttpCoroutineState::Yielded(HttpYield::WantsWrite(bytes)) => {
                stream.write_all(&bytes).expect("write well-known request");
            }
            HttpCoroutineState::Yielded(HttpYield::WantsRead) => {
                let n = stream.read(&mut buf).expect("read well-known response");
                arg = Some(&buf[..n]);
            }
        }
    };

    output
        .redirect_url
        .expect("well-known should 301 to a context root")
}

/// CalDAV item CRUD inside the existing calendar `calendar_id`, for providers
/// that reject `MKCALENDAR` (e.g. iCloud): discover, confirm the calendar is
/// present, then create/list/read/update/delete an event. The collection itself
/// is never created nor deleted.
pub fn caldav_items(base_url: &str, auth: WebdavAuth, calendar_id: &str) {
    let _ = env_logger::try_init();
    let base = Url::parse(base_url).expect("parse base URL");
    let mut client = WebdavClientStd::new(connect(&base), auth, base.clone());

    // --- discovery ---

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

    // --- PROPFIND list (confirm the target calendar exists) ---

    client.set_stream(connect(&base));
    let calendars = client.list_calendars().expect("list calendars");
    assert!(
        calendars.iter().any(|c| c.id == calendar_id),
        "target calendar {calendar_id} missing from home-set"
    );

    let item_id = format!("event-{}", unix_millis());
    // NOTE: the caller owns the whole resource name, extension included; the
    // same name is the item's id everywhere afterwards.
    let item_name = format!("{item_id}.ics");

    // --- PUT create event ---

    client.set_stream(connect(&base));
    let created = client
        .create_item(
            calendar_id,
            &item_name,
            build_ics(&item_id, "io-webdav event").into_bytes(),
        )
        .expect("create item");
    assert_eq!(created.id, item_name, "create item id mismatch");

    // NOTE: the event now exists in a calendar this flow does not own, so every
    // exit path has to remove it. See with_cleanup.
    with_cleanup(
        &mut client,
        |client| caldav_items_body(client, &base, calendar_id, &item_id, &item_name),
        |client| {
            client.set_stream(connect(&base));
            if let Err(err) = client.delete_item(calendar_id, &item_name, None) {
                report_leftover("event", &item_name, &err);
            }
        },
    );
}

/// The body of the item-only CalDAV flow, everything that runs once the test
/// event exists. Split out so [`with_cleanup`] can own the teardown.
fn caldav_items_body(
    client: &mut WebdavClientStd,
    base: &Url,
    calendar_id: &str,
    item_id: &str,
    item_name: &str,
) {
    // --- REPORT list items (verify present) ---

    client.set_stream(connect(base));
    let items = client.list_items(calendar_id, "").expect("list items");
    // NOTE: an item is addressed by its id, i.e. the resource name the server
    // enumerates, used verbatim: we created `<item_id>.ics`, so that is its id
    // everywhere (io-webdav never adds nor strips an extension).
    assert!(
        items.iter().any(|i| i.id == item_name),
        "created event {item_name} missing from REPORT"
    );

    // --- REPORT enum item refs (ETag-only spine) ---

    client.set_stream(connect(base));
    let refs = client.enum_items(calendar_id, "").expect("enum items");
    assert!(
        refs.refs.iter().any(|r| r.id == item_name),
        "created event {item_name} missing from etag-only enumeration"
    );

    // --- REPORT multiget (batch bodies) ---

    client.set_stream(connect(base));
    let fetched = client
        .multiget_items(calendar_id, &[item_name])
        .expect("multiget items");
    assert!(
        fetched
            .iter()
            .any(|i| i.id == item_name && !i.data.is_empty()),
        "multiget returned no body for event {item_name}"
    );

    // --- GET read item ---

    client.set_stream(connect(base));
    let body = client.read_item(calendar_id, item_name).expect("read item");
    assert!(!body.data.is_empty(), "read item returned empty body");

    // --- PUT update item ---

    client.set_stream(connect(base));
    client
        .update_item(
            calendar_id,
            item_name,
            build_ics(item_id, "io-webdav event (updated)").into_bytes(),
            body.etag.as_deref(),
        )
        .expect("update item");
}

/// Full CardDAV CRUD flow against the DAV root at `base_url`.
pub fn carddav(base_url: &str, auth: WebdavAuth) {
    let _ = env_logger::try_init();
    let base = Url::parse(base_url).expect("parse base URL");
    let mut client = WebdavClientStd::new(connect(&base), auth, base.clone());

    // --- discovery ---

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
    // NOTE: the caller owns the whole resource name, extension included; the
    // same name is the card's id everywhere afterwards.
    let card_name = format!("{card_id}.vcf");

    // --- MKCOL create ---

    let addressbook = CarddavAddressbook {
        id: book_id.clone(),
        display_name: Some("io-webdav integration test".to_owned()),
        description: Some("created by io-webdav integration tests".to_owned()),
        ..Default::default()
    };
    client.set_stream(connect(&base));
    client
        .create_addressbook(&addressbook)
        .expect("create addressbook");

    // NOTE: from here on the account holds a real collection, so every exit
    // path has to remove it. See with_cleanup.
    with_cleanup(
        &mut client,
        |client| carddav_body(client, &base, &book_id, &card_id, &card_name),
        |client| {
            client.set_stream(connect(&base));
            if let Err(err) = client.delete_addressbook(&book_id) {
                report_leftover("addressbook", &book_id, &err);
            }
        },
    );
}

/// The body of the full CardDAV flow, everything that runs once the test
/// addressbook exists. Split out so [`with_cleanup`] can own the teardown.
fn carddav_body(
    client: &mut WebdavClientStd,
    base: &Url,
    book_id: &str,
    card_id: &str,
    card_name: &str,
) {
    // --- PROPFIND list (verify creation) ---

    client.set_stream(connect(base));
    let addressbooks = client.list_addressbooks().expect("list addressbooks");
    assert!(
        addressbooks.iter().any(|b| b.id == book_id),
        "created addressbook {book_id} missing from list"
    );

    // --- PUT create card ---

    client.set_stream(connect(base));
    let created = client
        .create_card(
            book_id,
            card_name,
            build_vcf(card_id, "io-webdav Test").into_bytes(),
        )
        .expect("create card");
    assert_eq!(created.id, card_name, "create card id mismatch");

    // --- REPORT list cards (verify present) ---

    client.set_stream(connect(base));
    let cards = client.list_cards(book_id).expect("list cards");
    // NOTE: a card is addressed by its id, i.e. the resource name the server
    // enumerates, used verbatim: we created `<card_id>.vcf`, so that is its id
    // everywhere (io-webdav never adds nor strips an extension).
    assert!(
        cards.iter().any(|c| c.id == card_name),
        "created card {card_name} missing from REPORT"
    );

    // --- REPORT enum card refs (ETag-only spine) ---

    client.set_stream(connect(base));
    let refs = client.enum_cards(book_id).expect("enum cards");
    assert!(
        refs.refs.iter().any(|r| r.id == card_name),
        "created card {card_name} missing from etag-only enumeration"
    );

    // --- REPORT multiget (batch bodies) ---

    client.set_stream(connect(base));
    let fetched = client
        .multiget_cards(book_id, &[card_name])
        .expect("multiget cards");
    assert!(
        fetched
            .iter()
            .any(|c| c.id == card_name && !c.data.is_empty()),
        "multiget returned no body for card {card_name}"
    );

    // --- REPORT sync-collection (initial sync) ---

    client.set_stream(connect(base));
    let initial = client
        .sync_cards(book_id, None, Default::default())
        .expect("initial sync");
    assert!(
        initial.changed.iter().any(|c| c.href.contains(card_id)),
        "created card {card_id} missing from initial sync"
    );
    let sync_token = initial.sync_token.expect("initial sync returned no token");

    // --- GET read card ---

    client.set_stream(connect(base));
    let body = client.read_card(book_id, card_name).expect("read card");
    assert!(!body.data.is_empty(), "read card returned empty body");

    // --- PUT update card ---

    client.set_stream(connect(base));
    client
        .update_card(
            book_id,
            card_name,
            build_vcf(card_id, "io-webdav Test (updated)").into_bytes(),
            body.etag.as_deref(),
        )
        .expect("update card");

    // --- cleanup: DELETE card then collection ---

    client.set_stream(connect(base));
    client
        .delete_card(book_id, card_name, None)
        .expect("delete card");

    // --- REPORT sync-collection (incremental sync reports the removal) ---

    client.set_stream(connect(base));
    let delta = client
        .sync_cards(book_id, Some(&sync_token), Default::default())
        .expect("incremental sync");
    assert!(
        delta.vanished.iter().any(|href| href.contains(card_id)),
        "deleted card {card_id} missing from incremental sync removals"
    );
}

/// CardDAV card CRUD inside the existing addressbook `addressbook_id`, for
/// providers that reject `MKCOL` (e.g. iCloud, which exposes a single fixed
/// `card` addressbook): discover, confirm the addressbook is present, then
/// create/list/read/update/delete a vCard. The collection itself is never
/// created nor deleted.
pub fn carddav_cards(base_url: &str, auth: WebdavAuth, addressbook_id: &str) {
    let _ = env_logger::try_init();
    let base = Url::parse(base_url).expect("parse base URL");
    let mut client = WebdavClientStd::new(connect(&base), auth, base.clone());

    // --- discovery ---

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

    // --- PROPFIND list (confirm the target addressbook exists) ---

    client.set_stream(connect(&base));
    let addressbooks = client.list_addressbooks().expect("list addressbooks");
    assert!(
        addressbooks.iter().any(|b| b.id == addressbook_id),
        "target addressbook {addressbook_id} missing from home-set"
    );

    let card_id = format!("card-{}", unix_millis());
    // NOTE: the caller owns the whole resource name, extension included; the
    // same name is the card's id everywhere afterwards.
    let card_name = format!("{card_id}.vcf");

    // --- PUT create card ---

    client.set_stream(connect(&base));
    let created = client
        .create_card(
            addressbook_id,
            &card_name,
            build_vcf(&card_id, "io-webdav Test").into_bytes(),
        )
        .expect("create card");
    assert_eq!(created.id, card_name, "create card id mismatch");

    // NOTE: the card now exists in an addressbook this flow does not own, so
    // every exit path has to remove it. See with_cleanup.
    with_cleanup(
        &mut client,
        |client| carddav_cards_body(client, &base, addressbook_id, &card_id, &card_name),
        |client| {
            client.set_stream(connect(&base));
            if let Err(err) = client.delete_card(addressbook_id, &card_name, None) {
                report_leftover("card", &card_name, &err);
            }
        },
    );
}

/// The body of the card-only CardDAV flow, everything that runs once the test
/// card exists. Split out so [`with_cleanup`] can own the teardown.
fn carddav_cards_body(
    client: &mut WebdavClientStd,
    base: &Url,
    addressbook_id: &str,
    card_id: &str,
    card_name: &str,
) {
    // --- REPORT list cards (verify present) ---

    client.set_stream(connect(base));
    let cards = client.list_cards(addressbook_id).expect("list cards");
    // NOTE: a card is addressed by its id, i.e. the resource name the server
    // enumerates, used verbatim: we created `<card_id>.vcf`, so that is its id
    // everywhere (io-webdav never adds nor strips an extension).
    assert!(
        cards.iter().any(|c| c.id == card_name),
        "created card {card_name} missing from REPORT"
    );

    // --- REPORT enum card refs (ETag-only spine) ---

    client.set_stream(connect(base));
    let refs = client.enum_cards(addressbook_id).expect("enum cards");
    assert!(
        refs.refs.iter().any(|r| r.id == card_name),
        "created card {card_name} missing from etag-only enumeration"
    );

    // --- REPORT multiget (batch bodies) ---

    client.set_stream(connect(base));
    let fetched = client
        .multiget_cards(addressbook_id, &[card_name])
        .expect("multiget cards");
    assert!(
        fetched
            .iter()
            .any(|c| c.id == card_name && !c.data.is_empty()),
        "multiget returned no body for card {card_name}"
    );

    // --- GET read card ---

    client.set_stream(connect(base));
    let body = client
        .read_card(addressbook_id, card_name)
        .expect("read card");
    assert!(!body.data.is_empty(), "read card returned empty body");

    // --- PUT update card ---

    client.set_stream(connect(base));
    client
        .update_card(
            addressbook_id,
            card_name,
            build_vcf(card_id, "io-webdav Test (updated)").into_bytes(),
            body.etag.as_deref(),
        )
        .expect("update card");
}

/// Builds a minimal single-event iCalendar object (CRLF line endings, as
/// required by RFC 5545 §3.1).
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

/// Builds a minimal vCard 3.0 object, with the CRLF line endings RFC 6350 §3.2
/// requires.
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
