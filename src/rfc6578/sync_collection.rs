//! `sync-collection` REPORT coroutine (RFC 6578 §3.2): incremental
//! enumeration of a collection against a sync token.
//!
//! An initial sync (no token) returns every member; a subsequent sync
//! returns only the members changed or removed since the given token,
//! plus the next token to checkpoint. A rejected token surfaces as
//! [`SyncCollectionError::InvalidSyncToken`] so the consumer can fall
//! back to a full enumeration.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::{
//!     io::{Read, Write},
//!     net::TcpStream,
//! };
//!
//! use io_webdav::{
//!     coroutine::{WebdavCoroutine, WebdavCoroutineState, WebdavYield},
//!     rfc4918::{GETETAG, WebdavAuth},
//!     rfc6578::sync_collection::SyncCollection,
//! };
//! use url::Url;
//!
//! // Ready stream needed (TCP-connected, TLS-negociated)
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine = SyncCollection::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/addressbooks/contacts/",
//!     None,
//!     &[GETETAG],
//! );
//! let mut arg = None;
//!
//! let delta = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(delta)) => break delta,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("{} changed, {} vanished", delta.changed.len(), delta.vanished.len());
//! ```

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use log::trace;
use thiserror::Error;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        DAV, GETETAG, Multistatus, Property, WebdavAuth, XML_DECL, escape_text, prop_block,
        report::Report, send::SendError, xmlns_decls,
    },
};

/// Delta returned by a `sync-collection` REPORT.
#[derive(Clone, Debug, Default)]
pub struct SyncDelta {
    /// Members created or updated since the request token.
    pub changed: Vec<SyncChange>,

    /// Hrefs of the members removed since the request token (404
    /// response-level status, RFC 6578 §3.4).
    pub vanished: Vec<String>,

    /// The next checkpoint token, fed back to the following sync.
    pub sync_token: Option<String>,

    /// Whether the server truncated the result set (a 507 row was
    /// present, RFC 6578 §3.6); the consumer must run the report again
    /// from [`sync_token`](Self::sync_token) to drain the rest.
    pub truncated: bool,
}

/// A changed member reported by a `sync-collection` REPORT.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SyncChange {
    /// The member `<href>`, as returned by the server.
    pub href: String,

    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}

/// Failure causes during a `sync-collection` REPORT.
#[derive(Debug, Error)]
pub enum SyncCollectionError {
    /// The server rejected the sync token; a full enumeration is needed.
    #[error("WebDAV server rejected the sync token; run a full enumeration")]
    InvalidSyncToken,

    /// The underlying WebDAV send failed.
    #[error(transparent)]
    Send(#[from] SendError),
}

/// Coroutine that runs a `sync-collection` REPORT (RFC 6578 §3.2) and
/// returns the parsed [`SyncDelta`].
#[derive(Debug)]
pub struct SyncCollection {
    state: State,
    /// The collection path, without a trailing slash, so its own
    /// self-entry can be told apart from member resources.
    collection: String,
}

impl SyncCollection {
    /// Builds a new `sync-collection` coroutine against the collection
    /// at `path`, requesting `props` on each changed member. Pass
    /// [`None`] as `sync_token` for an initial sync. The `Depth` header
    /// is pinned to 0 as required by RFC 6578 §3.3; the scope is
    /// carried by the sync-level element instead.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        sync_token: Option<&str>,
        props: &[Property],
    ) -> Self {
        let body = sync_collection_body(sync_token, props);
        let report = Report::new(base_url, auth, user_agent, path, 0, body);
        Self {
            state: State::Report(report),
            collection: path.trim_end_matches('/').to_string(),
        }
    }
}

impl WebdavCoroutine for SyncCollection {
    type Yield = WebdavYield;
    type Return = Result<SyncDelta, SyncCollectionError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Report(report) => {
                let multistatus = match report.resume(arg) {
                    WebdavCoroutineState::Yielded(yielded) => {
                        return WebdavCoroutineState::Yielded(yielded);
                    }
                    WebdavCoroutineState::Complete(Err(SendError::HttpStatus(403, body)))
                        if body.contains("valid-sync-token") =>
                    {
                        let err = SyncCollectionError::InvalidSyncToken;
                        return WebdavCoroutineState::Complete(Err(err));
                    }
                    WebdavCoroutineState::Complete(Err(err)) => {
                        return WebdavCoroutineState::Complete(Err(err.into()));
                    }
                    WebdavCoroutineState::Complete(Ok(multistatus)) => multistatus,
                };

                let delta = from_multistatus(multistatus, &self.collection);
                WebdavCoroutineState::Complete(Ok(delta))
            }
        }
    }
}

/// Builds a `sync-collection` REPORT body (RFC 6578 §6.1): the request
/// token (an empty element for an initial sync), sync-level 1 and the
/// requested `props`, in DTD order.
pub fn sync_collection_body(sync_token: Option<&str>, props: &[Property]) -> Vec<u8> {
    let mut nss = vec![DAV];
    nss.extend(props.iter().map(|prop| prop.ns));
    let decls = xmlns_decls(&nss);

    let token = match sync_token {
        Some(token) => format!("<D:sync-token>{}</D:sync-token>", escape_text(token)),
        None => String::from("<D:sync-token/>"),
    };

    let mut body =
        format!("{XML_DECL}<D:sync-collection{decls}>{token}<D:sync-level>1</D:sync-level>");
    body.push_str(&prop_block(props));
    body.push_str("</D:sync-collection>");
    body.into_bytes()
}

/// Sorts the multistatus rows into a [`SyncDelta`]: 404 rows are
/// removals, a 507 row flags truncation, everything else is a change.
/// `collection` is the request-target path (trailing slash trimmed), so
/// the collection's own self-entry can be dropped rather than mistaken
/// for a member resource.
fn from_multistatus(multistatus: Multistatus, collection: &str) -> SyncDelta {
    let mut delta = SyncDelta {
        sync_token: multistatus.sync_token,
        ..Default::default()
    };

    for entry in multistatus.responses {
        match entry.status {
            Some(404) => delta.vanished.push(entry.href),
            Some(507) => delta.truncated = true,
            Some(status) if status / 100 != 2 => {
                trace!(
                    "skip sync-collection row {} with status {status}",
                    entry.href
                );
            }
            // Skip the collection self-entry: some servers (iCloud) echo
            // the collection itself in the sync report, as its own path
            // (with or without a trailing slash). It is not a member
            // resource and would otherwise enter the spine as a bogus
            // card named after the collection.
            _ if entry.href.trim_end_matches('/') == collection.trim_end_matches('/') => {
                trace!("skip sync-collection self-entry {}", entry.href);
            }
            _ => {
                let etag = entry
                    .text(GETETAG)
                    .map(|raw| raw.trim_matches('"').to_string());
                delta.changed.push(SyncChange {
                    href: entry.href,
                    etag,
                });
            }
        }
    }

    delta
}

#[derive(Debug)]
enum State {
    Report(Report),
}

#[cfg(test)]
mod tests {
    use crate::rfc4918::parse_multistatus;

    use super::*;

    #[test]
    fn body_carries_empty_token_on_initial_sync() {
        let body = sync_collection_body(None, &[GETETAG]);
        let xml = core::str::from_utf8(&body).unwrap();
        assert!(xml.contains("<D:sync-collection xmlns:D=\"DAV:\">"));
        assert!(xml.contains("<D:sync-token/><D:sync-level>1</D:sync-level>"));
        assert!(xml.contains("<D:prop><D:getetag/></D:prop>"));
        assert!(xml.ends_with("</D:sync-collection>"));
    }

    #[test]
    fn body_carries_the_given_token() {
        let body = sync_collection_body(Some("http://example.com/ns/sync/1234"), &[GETETAG]);
        let xml = core::str::from_utf8(&body).unwrap();
        assert!(xml.contains("<D:sync-token>http://example.com/ns/sync/1234</D:sync-token>"));
    }

    #[test]
    fn delta_sorts_changed_vanished_and_truncated_rows() {
        let xml = r#"<?xml version="1.0"?>
        <d:multistatus xmlns:d="DAV:">
          <d:response>
            <d:href>/dav/addressbooks/contacts/changed.vcf</d:href>
            <d:propstat>
              <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
              <d:status>HTTP/1.1 200 OK</d:status>
            </d:propstat>
          </d:response>
          <d:response>
            <d:href>/dav/addressbooks/contacts/removed.vcf</d:href>
            <d:status>HTTP/1.1 404 Not Found</d:status>
          </d:response>
          <d:response>
            <d:href>/dav/addressbooks/contacts/</d:href>
            <d:status>HTTP/1.1 507 Insufficient Storage</d:status>
          </d:response>
          <d:sync-token>http://example.com/ns/sync/1234</d:sync-token>
        </d:multistatus>"#;

        let delta = from_multistatus(parse_multistatus(xml), "/dav/addressbooks/contacts");

        assert_eq!(delta.changed.len(), 1);
        assert_eq!(
            delta.changed[0].href,
            "/dav/addressbooks/contacts/changed.vcf"
        );
        assert_eq!(delta.changed[0].etag.as_deref(), Some("etag-1"));
        assert_eq!(delta.vanished, ["/dav/addressbooks/contacts/removed.vcf"]);
        assert_eq!(
            delta.sync_token.as_deref(),
            Some("http://example.com/ns/sync/1234")
        );
        assert!(delta.truncated);
    }

    #[test]
    fn delta_skips_the_collection_self_entry() {
        // iCloud echoes the addressbook collection itself in the initial
        // sync report, as its own path with no trailing slash; it must
        // not enter the spine as a bogus card named after the collection.
        let xml = r#"<?xml version="1.0"?>
        <d:multistatus xmlns:d="DAV:">
          <d:response>
            <d:href>/17170244959/carddavhome/card</d:href>
            <d:propstat>
              <d:prop><d:getetag>"coll-etag"</d:getetag></d:prop>
              <d:status>HTTP/1.1 200 OK</d:status>
            </d:propstat>
          </d:response>
          <d:response>
            <d:href>/17170244959/carddavhome/card/5d18175a.vcf</d:href>
            <d:propstat>
              <d:prop><d:getetag>"etag-1"</d:getetag></d:prop>
              <d:status>HTTP/1.1 200 OK</d:status>
            </d:propstat>
          </d:response>
        </d:multistatus>"#;

        let delta = from_multistatus(parse_multistatus(xml), "/17170244959/carddavhome/card/");

        assert_eq!(delta.changed.len(), 1);
        assert_eq!(
            delta.changed[0].href,
            "/17170244959/carddavhome/card/5d18175a.vcf"
        );
    }
}
