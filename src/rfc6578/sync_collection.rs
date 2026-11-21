//! # Sync collection
//!
//! Collection enumeration coroutine: the `sync-collection` REPORT
//! (RFC 6578 §3.2) against a sync token, or a `PROPFIND` listing when the
//! caller asks for it.
//!
//! An initial sync (no token) returns every member; a subsequent sync
//! returns only the members changed or removed since the given token, plus
//! the next token to checkpoint.
//!
//! A member row answered with an error (a 403, a 5xx) is neither a change nor
//! a removal: it lands in [`WebdavSyncDelta::unreadable`], so a full listing
//! holding one is known to be incomplete.
//!
//! A rejected token surfaces as
//! [`WebdavSyncCollectionError::InvalidSyncToken`] so the consumer can fall
//! back to a full enumeration.
//!
//! RFC 6578 is an extension and a deployment may implement none of it, which
//! it says with the RFC 3253 §3.6 precondition, surfacing as
//! [`WebdavSyncCollectionError::UnsupportedReport`].
//!
//! Setting [`WebdavSyncCollectionOptions::fallback`] then enumerates the
//! collection with a `PROPFIND` at Depth 1 instead, which returns every
//! member and no token.
//!
//! Which of the two runs stays the caller's decision: an incremental delta
//! traded for a full listing is not a trade a library makes behind its
//! consumer's back, and [`supported_reports`] tells which one the server has.
//!
//! [`supported_reports`]: crate::rfc4918::WebdavResponseEntry::supported_reports
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
//!     rfc6578::sync_collection::WebdavSyncCollection,
//! };
//! use url::Url;
//!
//! // Ready stream, already connected and TLS-negotiated
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine = WebdavSyncCollection::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/addressbooks/contacts/",
//!     None,
//!     &[GETETAG],
//!     Default::default(),
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
        DAV, WebdavAuth, WebdavEtagProps, WebdavProperty, XML_DECL, escape_text, has_element,
        href_path, prop_block,
        propfind::WebdavPropfind,
        report::WebdavReport,
        same_path,
        send::WebdavSendError,
        typed::{WebdavMultistatusOf, WebdavUnreadableEntry},
        unquoted_etag, xmlns_decls,
    },
    webdav_try,
};

/// `DAV:sync-collection` REPORT root (RFC 6578 §6.1), the name a collection
/// advertises it under in its
/// [`supported_reports`](crate::rfc4918::WebdavResponseEntry::supported_reports).
pub const SYNC_COLLECTION: WebdavProperty = WebdavProperty {
    ns: DAV,
    local: "sync-collection",
};

/// Delta returned by a `sync-collection` REPORT.
#[derive(Clone, Debug, Default)]
pub struct WebdavSyncDelta {
    /// Members created or updated since the request token.
    pub changed: Vec<WebdavSyncChange>,
    /// Hrefs of the members removed since the request token (404 response-level
    /// status, RFC 6578 §3.4).
    pub vanished: Vec<String>,
    /// The next checkpoint token, fed back to the following sync.
    pub sync_token: Option<String>,
    /// Whether the server truncated the result set with a 507 row (RFC 6578
    /// §3.6), leaving the consumer to run the report again from
    /// [`sync_token`](Self::sync_token) to drain the rest.
    pub truncated: bool,
    /// The member rows the server answered with another non-2xx status than
    /// 404 and 507 (a 403, a 5xx), in the order met.
    ///
    /// Such a member is neither changed nor removed: its state is unknown, so
    /// a consumer reading the delta as a full listing reads it as incomplete
    /// and infers no removal from what it lacks.
    pub unreadable: Vec<WebdavUnreadableEntry>,
}

/// A changed member reported by a `sync-collection` REPORT.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WebdavSyncChange {
    /// The member `<href>`, as returned by the server.
    pub href: String,
    /// Entity tag (RFC 9110 §8.8.3), without surrounding quotes.
    pub etag: Option<String>,
}

/// Failure causes during a collection enumeration.
#[derive(Debug, Error)]
pub enum WebdavSyncCollectionError {
    /// The server rejected the sync token; a full enumeration is needed.
    #[error("WebDAV server rejected the sync token; run a full enumeration")]
    InvalidSyncToken,
    /// The server does not implement the report at all (RFC 3253 §3.6); the
    /// [`fallback`](WebdavSyncCollectionOptions::fallback) enumerates it
    /// anyway.
    #[error("WebDAV server does not implement `sync-collection`; enumerate with the fallback")]
    UnsupportedReport,
    /// The underlying WebDAV send failed.
    #[error(transparent)]
    Send(#[from] WebdavSendError),
}

/// Options for [`WebdavSyncCollection::new`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WebdavSyncCollectionOptions {
    /// When `true`, skip the `sync-collection` REPORT and enumerate the
    /// collection with a `PROPFIND` at Depth 1; defaults to the REPORT.
    ///
    /// The consumer sets this from a
    /// [`supported_reports`](crate::rfc4918::WebdavResponseEntry::supported_reports)
    /// check, or after meeting an
    /// [`UnsupportedReport`](WebdavSyncCollectionError::UnsupportedReport). The
    /// `PROPFIND` lists every member and returns no token, so the delta is a
    /// full snapshot the consumer diffs itself.
    pub fallback: bool,
}

/// Coroutine that enumerates a collection through a `sync-collection` REPORT
/// (RFC 6578 §3.2) or, on
/// [`fallback`](WebdavSyncCollectionOptions::fallback), a `PROPFIND` at
/// Depth 1, returning the parsed [`WebdavSyncDelta`] either way.
#[derive(Debug)]
pub struct WebdavSyncCollection {
    state: State,
    /// The collection path, without a trailing slash, so its own self-entry can
    /// be told apart from member resources.
    collection: String,
}

impl WebdavSyncCollection {
    /// Builds a new enumeration coroutine against the collection at `path`,
    /// requesting `props` on each member and taking [`None`] as `sync_token`
    /// for an initial sync.
    ///
    /// The REPORT pins its `Depth` to 0 as RFC 6578 §3.3 requires, the scope
    /// riding the sync-level element instead; the `PROPFIND` fallback takes
    /// Depth 1, where its members live, and ignores `sync_token`.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        sync_token: Option<&str>,
        props: &[WebdavProperty],
        opts: WebdavSyncCollectionOptions,
    ) -> Self {
        let state = if opts.fallback {
            trace!("using WebDAV PROPFIND enumeration fallback");
            State::WebdavPropfind(WebdavPropfind::new(
                base_url, auth, user_agent, path, 1, props,
            ))
        } else {
            let body = sync_collection_body(sync_token, props);
            State::WebdavReport(WebdavReport::new(base_url, auth, user_agent, path, 0, body))
        };

        Self {
            state,
            collection: path.trim_end_matches('/').to_string(),
        }
    }
}

impl WebdavCoroutine for WebdavSyncCollection {
    type Yield = WebdavYield;
    type Return = Result<WebdavSyncDelta, WebdavSyncCollectionError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::WebdavReport(report) => {
                let multistatus = match report.resume(arg) {
                    WebdavCoroutineState::Yielded(yielded) => {
                        return WebdavCoroutineState::Yielded(yielded);
                    }
                    WebdavCoroutineState::Complete(Err(WebdavSendError::HttpStatus {
                        body,
                        ..
                    })) if has_element(&body, &[VALID_SYNC_TOKEN]) => {
                        let err = WebdavSyncCollectionError::InvalidSyncToken;
                        return WebdavCoroutineState::Complete(Err(err));
                    }
                    WebdavCoroutineState::Complete(Err(WebdavSendError::UnsupportedReport {
                        ..
                    })) => {
                        let err = WebdavSyncCollectionError::UnsupportedReport;
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
            State::WebdavPropfind(propfind) => {
                let multistatus = webdav_try!(propfind, arg);
                let delta = from_multistatus(multistatus, &self.collection);
                WebdavCoroutineState::Complete(Ok(delta))
            }
        }
    }
}

/// Builds a `sync-collection` REPORT body (RFC 6578 §6.1): the request token
/// (an empty element for an initial sync), sync-level 1 and the requested
/// `props`, in DTD order.
pub fn sync_collection_body(sync_token: Option<&str>, props: &[WebdavProperty]) -> Vec<u8> {
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

/// Sorts the multistatus rows into a [`WebdavSyncDelta`]: 404 rows are
/// removals, a 507 row flags truncation, any other non-2xx row is unreadable,
/// everything else is a change. Both enumerations answer a multistatus, so
/// both are sorted here.
///
/// A 2xx row whose `getetag` the server refused stays a change with no ETag:
/// the member exists and changed, only its version is unknown.
///
/// `collection` is the request-target path, against which the collection's own
/// self-entry is recognised and dropped rather than taken for a member.
fn from_multistatus(
    multistatus: WebdavMultistatusOf<WebdavEtagProps>,
    collection: &str,
) -> WebdavSyncDelta {
    let mut delta = WebdavSyncDelta {
        sync_token: multistatus.sync_token,
        ..Default::default()
    };

    let collection = collection.trim_end_matches('/');

    for entry in multistatus.responses {
        match entry.status {
            Some(404) => delta.vanished.push(entry.href),
            Some(507) => delta.truncated = true,
            // NOTE: some servers (iCloud) echo the collection itself in the
            // sync report, and a PROPFIND always answers with it, neither being
            // a member resource; either would otherwise enter the spine as a
            // bogus member named after the collection.
            _ if same_path(href_path(&entry.href).trim_end_matches('/'), collection) => {
                trace!("skip enumeration self-entry {}", entry.href);
            }
            Some(status) if status / 100 != 2 => {
                let id = entry.id().into_owned();
                if id.is_empty() {
                    trace!("skip enumeration row {} naming no member", entry.href);
                    continue;
                }

                trace!("enumeration row {} unreadable: {status}", entry.href);
                delta.unreadable.push(WebdavUnreadableEntry {
                    id,
                    href: entry.href,
                    status: entry.status,
                    failures: entry.failures,
                });
            }
            _ => {
                let etag = unquoted_etag(entry.props.getetag);
                delta.changed.push(WebdavSyncChange {
                    href: entry.href,
                    etag,
                });
            }
        }
    }

    delta
}

/// Local name of the precondition refusing a sync token (RFC 6578 §3.2).
const VALID_SYNC_TOKEN: &str = "valid-sync-token";

#[derive(Debug)]
enum State {
    WebdavReport(WebdavReport<WebdavMultistatusOf<WebdavEtagProps>>),
    WebdavPropfind(WebdavPropfind<WebdavMultistatusOf<WebdavEtagProps>>),
}

#[cfg(test)]
mod tests {
    use crate::{
        rfc4918::{GETETAG, typed::parse_multistatus_as},
        rfc6578::sync_collection::*,
    };

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

        let delta = from_multistatus(
            parse_multistatus_as(xml).unwrap(),
            "/dav/addressbooks/contacts",
        );

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
        // NOTE: iCloud echoes the addressbook collection itself, as its own
        // path with no trailing slash.
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

        let delta = from_multistatus(
            parse_multistatus_as(xml).unwrap(),
            "/17170244959/carddavhome/card/",
        );

        assert_eq!(delta.changed.len(), 1);
        assert_eq!(
            delta.changed[0].href,
            "/17170244959/carddavhome/card/5d18175a.vcf"
        );
    }
}
