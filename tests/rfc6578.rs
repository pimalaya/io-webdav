//! Offline coverage of the collection enumerations (RFC 6578): the
//! `sync-collection` REPORT and its `PROPFIND` fallback, resumed against
//! scripted HTTP response bytes.

mod common;

use common::*;
use io_webdav::{
    rfc4918::{GETETAG, WebdavAuth, send::WebdavSendError},
    rfc6578::sync_collection::{
        WebdavSyncCollection, WebdavSyncCollectionError, WebdavSyncCollectionOptions,
    },
};
use url::Url;

const UA: &str = "io-webdav/test";

fn base() -> Url {
    Url::parse("https://dav.example.org/").unwrap()
}

#[test]
fn initial_sync_sorts_the_delta_rows() {
    let mut sync = WebdavSyncCollection::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        None,
        &[GETETAG],
        Default::default(),
    );

    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/dav/books/contacts/changed.vcf</d:href>
        <d:propstat>
          <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>/dav/books/contacts/removed.vcf</d:href>
        <d:status>HTTP/1.1 404 Not Found</d:status>
      </d:response>
      <d:response>
        <d:href>/dav/books/contacts/error.vcf</d:href>
        <d:status>HTTP/1.1 500 Internal Server Error</d:status>
      </d:response>
      <d:response>
        <d:href>/dav/books/contacts/</d:href>
        <d:status>HTTP/1.1 507 Insufficient Storage</d:status>
      </d:response>
      <d:sync-token>http://example.org/ns/sync/42</d:sync-token>
    </d:multistatus>"#;

    let (request, ret) = expect_exchange(&mut sync, &multistatus_response(xml));
    assert!(request.starts_with("report /dav/books/contacts/ http/1.1\r\n"));
    assert!(request.contains("depth: 0\r\n"));
    assert!(request.contains("<d:sync-token/><d:sync-level>1</d:sync-level>"));

    let delta = ret.unwrap();
    assert_eq!(delta.changed.len(), 1);
    assert_eq!(delta.changed[0].href, "/dav/books/contacts/changed.vcf");
    assert_eq!(delta.changed[0].etag.as_deref(), Some("etag-1"));
    assert_eq!(delta.vanished, ["/dav/books/contacts/removed.vcf"]);
    assert_eq!(
        delta.sync_token.as_deref(),
        Some("http://example.org/ns/sync/42")
    );
    assert!(delta.truncated);
}

#[test]
fn incremental_sync_carries_the_request_token() {
    let mut sync = WebdavSyncCollection::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        Some("http://example.org/ns/sync/42"),
        &[GETETAG],
        Default::default(),
    );

    let reply = multistatus_response("<d:multistatus xmlns:d=\"DAV:\"/>");
    let (request, ret) = expect_exchange(&mut sync, &reply);
    assert!(request.contains("<d:sync-token>http://example.org/ns/sync/42</d:sync-token>"));

    let delta = ret.unwrap();
    assert!(delta.changed.is_empty());
    assert!(delta.vanished.is_empty());
    assert!(!delta.truncated);
}

#[test]
fn rejected_token_maps_to_invalid_sync_token() {
    let mut sync = WebdavSyncCollection::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        Some("stale"),
        &[GETETAG],
        Default::default(),
    );

    let body = r#"<d:error xmlns:d="DAV:"><d:valid-sync-token/></d:error>"#;
    let (_, ret) = expect_exchange(&mut sync, &http_response("403 Forbidden", &[], body));
    assert!(matches!(
        ret.unwrap_err(),
        WebdavSyncCollectionError::InvalidSyncToken
    ));
}

#[test]
fn other_failures_pass_through_as_send_errors() {
    let mut sync = WebdavSyncCollection::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        Some("token"),
        &[GETETAG],
        Default::default(),
    );

    let (_, ret) = expect_exchange(&mut sync, &http_response("403 Forbidden", &[], "denied"));
    assert!(matches!(
        ret.unwrap_err(),
        WebdavSyncCollectionError::Send(WebdavSendError::HttpStatus { status: 403, .. })
    ));
}

/// The body a server implementing no `sync-collection` answers with: the RFC
/// 3253 §3.6 precondition, wrapped in a 403, which is also the status of a
/// genuine permission refusal.
const REPORT_NOT_SUPPORTED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:error xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns">
  <s:exception>Sabre\DAV\Exception\ReportNotSupported</s:exception>
  <s:message/>
  <d:supported-report/>
</d:error>"#;

fn enumerate(token: Option<&str>, fallback: bool) -> WebdavSyncCollection {
    WebdavSyncCollection::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        token,
        &[GETETAG],
        WebdavSyncCollectionOptions { fallback },
    )
}

#[test]
fn an_unimplemented_report_is_told_apart_from_a_refused_request() {
    // NOTE: the precondition is what says the server has no such report, since
    // the status it arrives under is the server's own choice.
    for reply in [
        http_response("403 Forbidden", &[], REPORT_NOT_SUPPORTED),
        http_response("405 Method Not Allowed", &[], ""),
        http_response("501 Not Implemented", &[], ""),
    ] {
        let mut sync = enumerate(None, false);
        let (_, ret) = expect_exchange(&mut sync, &reply);
        assert!(matches!(
            ret.unwrap_err(),
            WebdavSyncCollectionError::UnsupportedReport
        ));
    }

    for reply in [
        http_response(
            "403 Forbidden",
            &[],
            r#"<d:error xmlns:d="DAV:"><d:need-privileges/></d:error>"#,
        ),
        http_response("401 Unauthorized", &[], ""),
        http_response("500 Internal Server Error", &[], ""),
    ] {
        let mut sync = enumerate(None, false);
        let (_, ret) = expect_exchange(&mut sync, &reply);
        assert!(
            matches!(
                ret.unwrap_err(),
                WebdavSyncCollectionError::Send(WebdavSendError::HttpStatus { .. })
            ),
            "a permission, credential or server failure must surface as itself",
        );
    }
}

#[test]
fn the_fallback_enumerates_with_a_propfind_and_no_token() {
    let mut sync = enumerate(Some("http://example.org/ns/sync/42"), true);

    // NOTE: the malformed card is listed like any other member: a PROPFIND
    // reads names and ETags out of the store and parses nothing, where the
    // query REPORT the server evaluates dies on it and enumerates nothing.
    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/dav/books/contacts/</d:href>
        <d:propstat>
          <d:prop><d:getetag>"coll-etag"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>/dav/books/contacts/alice.vcf</d:href>
        <d:propstat>
          <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>/dav/books/contacts/unparseable.vcf</d:href>
        <d:propstat>
          <d:prop><d:getetag>"etag-2"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let (request, ret) = expect_exchange(&mut sync, &multistatus_response(xml));
    assert!(request.starts_with("propfind /dav/books/contacts/ http/1.1\r\n"));
    assert!(request.contains("depth: 1\r\n"));
    assert!(request.contains("<d:getetag/>"));
    assert!(!request.contains("sync-collection"));

    let delta = ret.unwrap();
    assert_eq!(delta.changed.len(), 2);
    assert_eq!(delta.changed[0].href, "/dav/books/contacts/alice.vcf");
    assert_eq!(delta.changed[1].href, "/dav/books/contacts/unparseable.vcf");
    assert!(delta.vanished.is_empty());
    // NOTE: no token means the consumer holds a full listing, not a delta.
    assert!(delta.sync_token.is_none());
    assert!(!delta.truncated);
}

#[test]
fn the_fallback_flags_a_truncated_listing() {
    // NOTE: without the flag, a truncated listing read as a full snapshot makes
    // every member the server left out look deleted.
    let mut sync = enumerate(None, true);

    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>/dav/books/contacts/alice.vcf</d:href>
        <d:propstat>
          <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>/dav/books/contacts/</d:href>
        <d:status>HTTP/1.1 507 Insufficient Storage</d:status>
      </d:response>
    </d:multistatus>"#;

    let (_, ret) = expect_exchange(&mut sync, &multistatus_response(xml));

    let delta = ret.unwrap();
    assert_eq!(delta.changed.len(), 1);
    assert!(delta.truncated);
}

#[test]
fn the_self_entry_is_skipped_however_the_server_spells_it() {
    // NOTE: RFC 4918 §14.7 allows an absolute href, and a PROPFIND always
    // answers with the collection itself, so a comparison against the raw href
    // would let the collection into the spine as a member named after itself.
    let mut sync = enumerate(None, true);

    let xml = r#"<d:multistatus xmlns:d="DAV:">
      <d:response>
        <d:href>https://dav.example.org/dav/books/contacts/</d:href>
        <d:propstat>
          <d:prop><d:getetag>"coll-etag"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
      <d:response>
        <d:href>https://dav.example.org/dav/books/contacts/alice.vcf</d:href>
        <d:propstat>
          <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
          <d:status>HTTP/1.1 200 OK</d:status>
        </d:propstat>
      </d:response>
    </d:multistatus>"#;

    let (_, ret) = expect_exchange(&mut sync, &multistatus_response(xml));

    let delta = ret.unwrap();
    assert_eq!(delta.changed.len(), 1);
    assert_eq!(
        delta.changed[0].href,
        "https://dav.example.org/dav/books/contacts/alice.vcf"
    );
}
