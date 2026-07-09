//! Offline coverage of the `sync-collection` REPORT (RFC 6578), resumed
//! against scripted HTTP response bytes.

mod common;

use common::*;
use io_webdav::{
    rfc4918::{GETETAG, WebdavAuth, send::SendError},
    rfc6578::sync_collection::{SyncCollection, SyncCollectionError},
};
use url::Url;

const UA: &str = "io-webdav/test";

fn base() -> Url {
    Url::parse("https://dav.example.org/").unwrap()
}

#[test]
fn initial_sync_sorts_the_delta_rows() {
    let mut sync = SyncCollection::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        None,
        &[GETETAG],
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
    let mut sync = SyncCollection::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        Some("http://example.org/ns/sync/42"),
        &[GETETAG],
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
    let mut sync = SyncCollection::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        Some("stale"),
        &[GETETAG],
    );

    let body = r#"<d:error xmlns:d="DAV:"><d:valid-sync-token/></d:error>"#;
    let (_, ret) = expect_exchange(&mut sync, &http_response("403 Forbidden", &[], body));
    assert!(matches!(
        ret.unwrap_err(),
        SyncCollectionError::InvalidSyncToken
    ));
}

#[test]
fn other_failures_pass_through_as_send_errors() {
    let mut sync = SyncCollection::new(
        &base(),
        &WebdavAuth::None,
        UA,
        "/dav/books/contacts/",
        Some("token"),
        &[GETETAG],
    );

    let (_, ret) = expect_exchange(&mut sync, &http_response("403 Forbidden", &[], "denied"));
    assert!(matches!(
        ret.unwrap_err(),
        SyncCollectionError::Send(SendError::HttpStatus(403, _))
    ));
}
