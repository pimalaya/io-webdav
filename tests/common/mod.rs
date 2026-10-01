//! Shared helpers for the integration tests.
//!
//! Two families live here. The scripted-coroutine helpers ([`http_response`]
//! plus the `expect_*` steps) let the offline suites (rfc4918, rfc4791,
//! rfc5397, rfc6352, rfc6578, client) resume any I/O-free coroutine against
//! canned HTTP response bytes, following the io-imap canonical layout. The
//! provider helpers below them run live CalDAV / CardDAV flows.
//!
//! Each provider test drives [`WebdavClientStd`] against a live CalDAV /
//! CardDAV server. Call [`caldav`] for a full calendar CRUD flow and [`carddav`]
//! for a full addressbook CRUD flow.
//!
//! Providers that forbid collection creation (Google and iCloud reject both
//! `MKCALENDAR` and `MKCOL`, exposing only the collections they provision) get
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
//! connect → CURRENT-USER-PRINCIPAL → CALENDAR-HOME-SET
//!   → MKCALENDAR create   (create test calendar)
//!   ┌ guarded by with_cleanup ─────────────────────────────────────┐
//!   │ → PROPFIND list     (verify creation)                        │
//!   │ → PROPPATCH rename  (verified by a second listing)           │
//!   │ → PUT create        (create test event)                      │
//!   │ → PUT duplicate UID (refused with no-uid-conflict)           │
//!   │ → REPORT list       (verify event present)                   │
//!   │ → REPORT enum       (etag-only spine)                        │
//!   │ → REPORT multiget   (batch bodies)                           │
//!   │ → REPORT sync       (initial sync-collection)                │
//!   │ → REPORT sync       (unknown token: refused or full resync)  │
//!   │ → GET read          (fetch raw iCalendar)                    │
//!   │ → PUT update        (bump the event, If-Match)               │
//!   │ → PUT stale update  (refused with 412)                       │
//!   │ → DELETE item       (the removal the next sync reports)      │
//!   │ → REPORT sync       (incremental, reports the removal)       │
//!   └──────────────────────────────────────────────────────────────┘
//!   → DELETE collection   (teardown, runs however the guard exits)
//! ```
//!
//! The full CardDAV flow mirrors it for addressbooks and vCards, plus a query
//! REPORT filtering on the card's UID. Both exercise the sync read-side:
//! etag-only enumeration, the protocol's multiget batch fetch, and an initial
//! plus incremental `sync-collection` REPORT (RFC 6578) that must report the
//! deleted resource as vanished.
//!
//! The item-only flows run the same item steps inside the existing collection,
//! syncing from a Depth 0 checkpoint (the card flow can skip the sync, see
//! [`carddav_cards`]). They are guarded the same way, from the moment the event
//! or card is created, since the collection holding it belongs to the account
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
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use io_http::{
    coroutine::{HttpCoroutine, HttpCoroutineState, HttpYield},
    rfc6750::bearer::HttpAuthBearer,
    rfc7617::basic::HttpAuthBasic,
    rfc8615::well_known::Http11WellKnown,
};
use io_webdav::{
    client::{WebdavClientStd, WebdavClientStdError},
    coroutine::*,
    rfc4791::calendar::CaldavCalendar,
    rfc4918::coroutine::*,
    rfc4918::{SYNC_TOKEN, WebdavAuth, propfind::WebdavPropfind, send::WebdavSendError},
    rfc6352::addressbook::{CarddavAddressbook, CarddavAddressbookPatch},
    rfc6352::card::list::CarddavCardListOptions,
    rfc6352::filter::{
        CarddavFilter, CarddavFilterTest, CarddavMatchType, CarddavPropCond, CarddavPropFilter,
        CarddavTextMatch,
    },
    rfc6578::sync_collection::{WebdavSyncCollectionError, WebdavSyncDelta},
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
        scheme => panic!("unsupported base URL scheme {scheme}"),
    }
}

/// A suffix unique to one minted collection or resource id.
///
/// The epoch milliseconds date a leftover an aborted run left behind, and the
/// counter separates two ids minted in the same millisecond. Both are needed:
/// the flows run as parallel test threads, and a server answering one home-set
/// for calendars and address books (Radicale does) has the two of them minting
/// collection names into one collection.
fn unique_suffix() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{millis}-{count}")
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
    eprintln!("WARNING: could not clean up {what} {id}, remove it by hand: {err:?}");
}

/// A sync token no server issued, which RFC 6578 section 3.2 has it refuse
/// with the `valid-sync-token` precondition.
const UNKNOWN_SYNC_TOKEN: &str = "http://io-webdav.invalid/sync/0";

/// Asserts a sync from [`UNKNOWN_SYNC_TOKEN`] did not pass for an empty delta
/// while the resource `id` exists.
///
/// RFC 6578 section 3.2 wants the `valid-sync-token` refusal, which io-webdav
/// surfaces as its own error. A server re-enumerating the whole collection
/// instead is one a consumer survives; an empty delta silently loses changes.
fn assert_unknown_token_handled(result: Result<WebdavSyncDelta, WebdavClientStdError>, id: &str) {
    match result {
        Err(WebdavClientStdError::WebdavSyncCollection(
            WebdavSyncCollectionError::InvalidSyncToken,
        )) => {}
        Ok(delta) if delta.changed.iter().any(|c| c.href.contains(id)) => {}
        other => panic!("unknown sync token neither refused nor resynced: {other:?}"),
    }
}

/// Asserts `result` is the 412 a server answers a write whose `If-Match`
/// no longer matches (RFC 9110 section 13.1.1).
fn assert_precondition_failed<T: Debug>(result: Result<T, WebdavClientStdError>, what: &str) {
    match result {
        Err(WebdavClientStdError::Send(WebdavSendError::HttpStatus { status: 412, .. })) => {}
        other => panic!("{what}: expected 412 Precondition Failed, got {other:?}"),
    }
}

/// Reads the sync token of the collection at `path` with a Depth 0
/// `PROPFIND`, the checkpoint an incremental sync starts from.
///
/// The item-only flows run inside a collection holding the account's own
/// resources, so they sync from a checkpoint rather than enumerating them all.
/// The token is read at Depth 0 because Google leaves it out of a Depth 1
/// listing, and refuses the empty-token initial sync with a 400.
fn collection_sync_token(base: &Url, auth: &WebdavAuth, path: &str) -> String {
    let mut stream = connect(base);
    let mut coroutine = WebdavPropfind::new(base, auth, "io-webdav", path, 0, &[SYNC_TOKEN]);
    let mut buf = [0u8; 16 * 1024];
    let mut arg: Option<&[u8]> = None;

    let multistatus = loop {
        match coroutine.resume(arg.take()) {
            WebdavCoroutineState::Complete(Ok(multistatus)) => break multistatus,
            WebdavCoroutineState::Complete(Err(err)) => panic!("sync token PROPFIND: {err}"),
            WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
                stream.write_all(&bytes).expect("write sync token request");
            }
            WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
                let n = stream.read(&mut buf).expect("read sync token response");
                arg = Some(&buf[..n]);
            }
        }
    };

    multistatus
        .responses
        .iter()
        .find_map(|entry| entry.text(SYNC_TOKEN))
        .unwrap_or_else(|| panic!("collection {path} has no sync token"))
        .to_owned()
}

/// Full CalDAV CRUD flow against the DAV root at `base_url`.
pub fn caldav(base_url: &str, auth: WebdavAuth) {
    let _ = env_logger::try_init();
    let base = Url::parse(base_url).expect("parse base URL");
    // NOTE: the client opens its own first stream, the way consumers
    // connect; later requests each get a fresh one.
    let mut client = WebdavClientStd::connect(&base, auth, Default::default()).expect("connect");

    // --- discovery ---

    let principal = client
        .current_user_principal()
        .expect("current-user-principal discovery");
    assert!(!principal.path().is_empty(), "empty principal path");

    client.set_stream(connect(&base));
    let home = client
        .calendar_home_set()
        .expect("calendar-home-set discovery");
    assert!(!home.path().is_empty(), "empty calendar home-set path");

    let ts = unique_suffix();
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

    // --- PROPPATCH rename (verified by a second listing) ---

    // NOTE: only the name goes out: the component set is protected once
    // the calendar exists (RFC 4791 section 5.2.3).
    let renamed = CaldavCalendar {
        id: cal_id.to_owned(),
        display_name: Some("io-webdav integration test (renamed)".to_owned()),
        ..Default::default()
    };
    client.set_stream(connect(base));
    client.update_calendar(&renamed).expect("update calendar");

    client.set_stream(connect(base));
    let calendars = client.list_calendars().expect("list calendars");
    let renamed = calendars.iter().find(|c| c.id == cal_id);
    assert_eq!(
        renamed.and_then(|c| c.display_name.as_deref()),
        Some("io-webdav integration test (renamed)"),
        "PROPPATCH did not rename calendar {cal_id}"
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

    // --- PUT a second event under the same UID (refused, RFC 4791 5.3.2.1) ---

    client.set_stream(connect(base));
    let duplicate = client.create_item(
        cal_id,
        &format!("duplicate-{item_name}"),
        build_ics(item_id, "io-webdav event (duplicate)").into_bytes(),
    );
    assert!(
        duplicate.as_ref().is_err_and(|err| err.is_duplicate_uid()),
        "duplicate event UID not refused with no-uid-conflict: {duplicate:?}"
    );

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

    // --- REPORT sync-collection (unknown token, RFC 6578 section 3.2) ---

    client.set_stream(connect(base));
    let unknown = client.sync_items(cal_id, Some(UNKNOWN_SYNC_TOKEN), Default::default());
    assert_unknown_token_handled(unknown, item_id);

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

    // --- PUT update with the stale ETag (refused, RFC 9110 If-Match) ---

    if let Some(stale) = body.etag.as_deref() {
        client.set_stream(connect(base));
        let refused = client.update_item(
            cal_id,
            item_name,
            build_ics(item_id, "io-webdav event (stale)").into_bytes(),
            Some(stale),
        );
        assert_precondition_failed(refused, "stale event update");
    }

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
    // NOTE: the client opens its own first stream, the way consumers
    // connect; later requests each get a fresh one.
    let mut client = WebdavClientStd::connect(&base, auth, Default::default()).expect("connect");

    // --- discovery ---

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
        "target calendar {calendar_id} missing from home-set: {:?}",
        calendars.iter().map(|c| &c.id).collect::<Vec<_>>()
    );

    let path = format!("{}{calendar_id}/", home.path());
    let checkpoint = collection_sync_token(&base, client.auth(), &path);

    let item_id = format!("event-{}", unique_suffix());
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
    // NOTE: a server may store the event under a name of its own (Google
    // does), so from here on the event is the id it answered with.
    let id = created.id;

    // NOTE: the event now exists in a calendar this flow does not own, so every
    // exit path has to remove it, unless the body already did. See
    // with_cleanup.
    with_cleanup(
        &mut (client, false),
        |(client, deleted)| {
            caldav_items_body(
                client,
                deleted,
                &base,
                calendar_id,
                &item_id,
                &id,
                &checkpoint,
            )
        },
        |(client, deleted)| {
            if *deleted {
                return;
            }

            client.set_stream(connect(&base));
            if let Err(err) = client.delete_item(calendar_id, &id, None) {
                report_leftover("event", &id, &err);
            }
        },
    );
}

/// The body of the item-only CalDAV flow, everything that runs once the test
/// event exists. Split out so [`with_cleanup`] can own the teardown, which
/// `deleted` tells whether the event is already gone.
fn caldav_items_body(
    client: &mut WebdavClientStd,
    deleted: &mut bool,
    base: &Url,
    calendar_id: &str,
    item_id: &str,
    item_name: &str,
    checkpoint: &str,
) {
    // --- REPORT list items (verify present) ---

    client.set_stream(connect(base));
    let items = client.list_items(calendar_id, "").expect("list items");
    // NOTE: an item is addressed by its id, i.e. the resource name the server
    // enumerates, used verbatim: the id the create answered with, whatever name
    // we asked for (io-webdav never adds nor strips an extension).
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

    // --- REPORT sync-collection (from the checkpoint, reports the creation) ---

    client.set_stream(connect(base));
    let created = client
        .sync_items(calendar_id, Some(checkpoint), Default::default())
        .expect("checkpoint sync");
    assert!(
        created.changed.iter().any(|c| c.href.contains(item_name)),
        "created event {item_name} missing from checkpoint sync"
    );
    let sync_token = created
        .sync_token
        .expect("checkpoint sync returned no token");

    // --- REPORT sync-collection (unknown token, RFC 6578 section 3.2) ---

    client.set_stream(connect(base));
    let unknown = client.sync_items(calendar_id, Some(UNKNOWN_SYNC_TOKEN), Default::default());
    assert_unknown_token_handled(unknown, item_name);

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

    // --- PUT update with the stale ETag (refused, RFC 9110 If-Match) ---

    if let Some(stale) = body.etag.as_deref() {
        client.set_stream(connect(base));
        let refused = client.update_item(
            calendar_id,
            item_name,
            build_ics(item_id, "io-webdav event (stale)").into_bytes(),
            Some(stale),
        );
        assert_precondition_failed(refused, "stale event update");
    }

    // --- DELETE item (the removal the next sync must report) ---

    client.set_stream(connect(base));
    client
        .delete_item(calendar_id, item_name, None)
        .expect("delete item");
    *deleted = true;

    // --- REPORT sync-collection (incremental sync reports the removal) ---

    client.set_stream(connect(base));
    let delta = client
        .sync_items(calendar_id, Some(&sync_token), Default::default())
        .expect("incremental sync");
    assert!(
        delta.vanished.iter().any(|href| href.contains(item_name)),
        "deleted event {item_name} missing from incremental sync removals"
    );
}

/// Full CardDAV CRUD flow against the DAV root at `base_url`.
pub fn carddav(base_url: &str, auth: WebdavAuth) {
    let _ = env_logger::try_init();
    let base = Url::parse(base_url).expect("parse base URL");
    // NOTE: the client opens its own first stream, the way consumers
    // connect; later requests each get a fresh one.
    let mut client = WebdavClientStd::connect(&base, auth, Default::default()).expect("connect");

    // --- discovery ---

    let principal = client
        .current_user_principal()
        .expect("current-user-principal discovery");
    assert!(!principal.path().is_empty(), "empty principal path");

    client.set_stream(connect(&base));
    let home = client
        .addressbook_home_set()
        .expect("addressbook-home-set discovery");
    assert!(!home.path().is_empty(), "empty addressbook home-set path");

    let ts = unique_suffix();
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

    // --- PROPPATCH rename (verified by a second listing) ---

    let patch = CarddavAddressbookPatch {
        id: book_id.to_owned(),
        display_name: Some(Some("io-webdav integration test (renamed)".to_owned())),
        ..Default::default()
    };
    client.set_stream(connect(base));
    client
        .update_addressbook(&patch)
        .expect("update addressbook");

    client.set_stream(connect(base));
    let addressbooks = client.list_addressbooks().expect("list addressbooks");
    let renamed = addressbooks.iter().find(|b| b.id == book_id);
    assert_eq!(
        renamed.and_then(|b| b.display_name.as_deref()),
        Some("io-webdav integration test (renamed)"),
        "PROPPATCH did not rename addressbook {book_id}"
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

    // --- PUT a second card under the same UID (refused, RFC 6352 5.1) ---

    client.set_stream(connect(base));
    let duplicate = client.create_card(
        book_id,
        &format!("duplicate-{card_name}"),
        build_vcf(card_id, "io-webdav Test (duplicate)").into_bytes(),
    );
    assert!(
        duplicate.as_ref().is_err_and(|err| err.is_duplicate_uid()),
        "duplicate card UID not refused with no-uid-conflict: {duplicate:?}"
    );

    // --- REPORT list cards (verify present) ---

    client.set_stream(connect(base));
    let cards = client
        .list_cards(book_id, &CarddavCardListOptions::default())
        .expect("list cards")
        .cards;
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

    // --- REPORT sync-collection (unknown token, RFC 6578 section 3.2) ---

    client.set_stream(connect(base));
    let unknown = client.sync_cards(book_id, Some(UNKNOWN_SYNC_TOKEN), Default::default());
    assert_unknown_token_handled(unknown, card_id);

    // --- GET read card ---

    client.set_stream(connect(base));
    let body = client.read_card(book_id, card_name).expect("read card");
    assert!(!body.data.is_empty(), "read card returned empty body");

    // --- REPORT query by the UID the server holds (RFC 6352 section 10.5) ---

    // NOTE: the UID is read back rather than reused, a server being free to
    // rewrite it (Google does).
    let uid = vcard_uid(&body.data);
    client.set_stream(connect(base));
    let matching = client
        .list_cards(book_id, &uid_filter(&uid))
        .expect("query cards by UID")
        .cards;
    assert!(
        matching.len() == 1 && matching.iter().all(|c| c.id == card_name),
        "UID query did not return only card {card_name}: {matching:?}"
    );

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

    // --- PUT update with the stale ETag (refused, RFC 9110 If-Match) ---

    if let Some(stale) = body.etag.as_deref() {
        client.set_stream(connect(base));
        let refused = client.update_card(
            book_id,
            card_name,
            build_vcf(card_id, "io-webdav Test (stale)").into_bytes(),
            Some(stale),
        );
        assert_precondition_failed(refused, "stale card update");
    }

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

/// The standard behaviours [`carddav_cards`] asserts.
///
/// Google's CardDAV deviates from both: its sync deltas report neither the
/// creation nor the removal of a card made over CardDAV, and it applies a
/// `PUT` whose `If-Match` no longer matches instead of answering 412.
#[derive(Clone, Copy, Debug)]
pub struct CarddavCardsChecks {
    /// Sync from a checkpoint and expect the creation and the removal.
    pub sync: bool,
    /// Expect a stale `If-Match` update to be refused with 412.
    pub if_match: bool,
}

/// CardDAV card CRUD inside the existing addressbook `addressbook_id`, for
/// providers that reject `MKCOL` (e.g. iCloud, which exposes a single fixed
/// `card` addressbook): discover, confirm the addressbook is present, then
/// create/list/read/update/delete a vCard. The collection itself is never
/// created nor deleted.
///
/// `checks` selects the standard behaviours the flow asserts, so a provider
/// known to deviate skips one rather than failing the run.
pub fn carddav_cards(
    base_url: &str,
    auth: WebdavAuth,
    addressbook_id: &str,
    checks: CarddavCardsChecks,
) {
    let _ = env_logger::try_init();
    let base = Url::parse(base_url).expect("parse base URL");
    // NOTE: the client opens its own first stream, the way consumers
    // connect; later requests each get a fresh one.
    let mut client = WebdavClientStd::connect(&base, auth, Default::default()).expect("connect");

    // --- discovery ---

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

    let path = format!("{}{addressbook_id}/", home.path());
    let checkpoint = checks
        .sync
        .then(|| collection_sync_token(&base, client.auth(), &path));

    let card_id = format!("card-{}", unique_suffix());
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
    // NOTE: a server may store the card under a name of its own (Google
    // does), so from here on the card is the id it answered with.
    let id = created.id;

    // NOTE: the card now exists in an addressbook this flow does not own, so
    // every exit path has to remove it, unless the body already did. See
    // with_cleanup.
    with_cleanup(
        &mut (client, false),
        |(client, deleted)| {
            let body = CarddavCardsBody {
                base: &base,
                addressbook_id,
                card_id: &card_id,
                card_name: &id,
                checkpoint: checkpoint.as_deref(),
                if_match: checks.if_match,
            };
            body.run(client, deleted)
        },
        |(client, deleted)| {
            if *deleted {
                return;
            }

            client.set_stream(connect(&base));
            if let Err(err) = client.delete_card(addressbook_id, &id, None) {
                report_leftover("card", &id, &err);
            }
        },
    );
}

/// The body of the card-only CardDAV flow, everything that runs once the test
/// card exists. Split out so [`with_cleanup`] can own the teardown.
#[derive(Clone, Copy)]
struct CarddavCardsBody<'a> {
    base: &'a Url,
    addressbook_id: &'a str,
    card_id: &'a str,
    card_name: &'a str,
    checkpoint: Option<&'a str>,
    if_match: bool,
}

impl CarddavCardsBody<'_> {
    /// Runs the body; `deleted` tells the teardown whether the card is
    /// already gone.
    fn run(self, client: &mut WebdavClientStd, deleted: &mut bool) {
        let Self {
            base,
            addressbook_id,
            card_id,
            card_name,
            checkpoint,
            if_match,
        } = self;

        // --- REPORT list cards (verify present) ---

        client.set_stream(connect(base));
        let cards = client
            .list_cards(addressbook_id, &CarddavCardListOptions::default())
            .expect("list cards")
            .cards;
        // NOTE: a card is addressed by its id, i.e. the resource name the server
        // enumerates, used verbatim: the id the create answered with, whatever name
        // we asked for (io-webdav never adds nor strips an extension).
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

        // --- REPORT sync-collection (from the checkpoint, reports the creation) ---

        let sync_token = checkpoint.map(|checkpoint| {
            client.set_stream(connect(base));
            let created = client
                .sync_cards(addressbook_id, Some(checkpoint), Default::default())
                .expect("checkpoint sync");
            assert!(
                created.changed.iter().any(|c| c.href.contains(card_name)),
                "created card {card_name} missing from checkpoint sync: {created:?}"
            );

            // --- REPORT sync-collection (unknown token, RFC 6578 section 3.2) ---

            client.set_stream(connect(base));
            let unknown =
                client.sync_cards(addressbook_id, Some(UNKNOWN_SYNC_TOKEN), Default::default());
            assert_unknown_token_handled(unknown, card_name);
            created
                .sync_token
                .expect("checkpoint sync returned no token")
        });

        // --- GET read card ---

        client.set_stream(connect(base));
        let body = client
            .read_card(addressbook_id, card_name)
            .expect("read card");
        assert!(!body.data.is_empty(), "read card returned empty body");

        // --- REPORT query by the UID the server holds (RFC 6352 section 10.5) ---

        // NOTE: the UID is read back rather than reused, a server being free to
        // rewrite it (Google does).
        let uid = vcard_uid(&body.data);
        client.set_stream(connect(base));
        let matching = client
            .list_cards(addressbook_id, &uid_filter(&uid))
            .expect("query cards by UID")
            .cards;
        assert!(
            matching.len() == 1 && matching.iter().all(|c| c.id == card_name),
            "UID query did not return only card {card_name}: {matching:?}"
        );

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

        // --- PUT update with the stale ETag (refused, RFC 9110 If-Match) ---

        if let Some(stale) = body.etag.as_deref().filter(|_| if_match) {
            client.set_stream(connect(base));
            let refused = client.update_card(
                addressbook_id,
                card_name,
                build_vcf(card_id, "io-webdav Test (stale)").into_bytes(),
                Some(stale),
            );
            assert_precondition_failed(refused, "stale card update");
        }

        // --- DELETE card (the removal the next sync must report) ---

        client.set_stream(connect(base));
        client
            .delete_card(addressbook_id, card_name, None)
            .expect("delete card");
        *deleted = true;

        // --- REPORT sync-collection (incremental sync reports the removal) ---

        if let Some(sync_token) = sync_token {
            client.set_stream(connect(base));
            let delta = client
                .sync_cards(addressbook_id, Some(&sync_token), Default::default())
                .expect("incremental sync");
            assert!(
                delta.vanished.iter().any(|href| href.contains(card_name)),
                "deleted card {card_name} missing from incremental sync removals"
            );
        }
    }
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

/// Query options matching the card whose `UID` equals `uid`.
fn uid_filter(uid: &str) -> CarddavCardListOptions {
    let text = CarddavTextMatch {
        value: uid.to_owned(),
        match_type: CarddavMatchType::Equals,
        negate: false,
        collation: None,
    };

    CarddavCardListOptions {
        filter: CarddavFilter {
            test: CarddavFilterTest::AnyOf,
            props: vec![CarddavPropFilter {
                name: String::from("UID"),
                test: CarddavFilterTest::AnyOf,
                cond: CarddavPropCond::Match {
                    texts: vec![text],
                    params: Vec::new(),
                },
            }],
        },
        limit: None,
    }
}

/// Reads the `UID` property out of a vCard.
fn vcard_uid(data: &[u8]) -> String {
    String::from_utf8_lossy(data)
        .lines()
        .find_map(|line| line.strip_prefix("UID:"))
        .expect("card has a UID")
        .trim()
        .to_owned()
}
