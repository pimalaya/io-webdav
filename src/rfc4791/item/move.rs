//! # Move item
//!
//! `move-item` coroutine: moves a calendar item into another calendar of the
//! same server with `MOVE` (RFC 4918 §9.9), keeping its resource id.
//!
//! The server moves the resource itself, so its `UID` never lives in two
//! places at once: a server keeping one `UID` per account (Fastmail) refuses a
//! create of the item in the target while the source still holds it, never
//! this. `Overwrite: F` refuses to replace a resource already at the target,
//! and the optional `If-Match` gates the move on the last-known ETag of the
//! source (RFC 9110 §13.1.1). A target refusing the `UID` with the
//! `CALDAV:no-uid-conflict` precondition (RFC 4791 §5.3.2) surfaces as
//! [`DuplicateUid`](crate::rfc4918::send::WebdavSendError::DuplicateUid).
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
//!     rfc4791::item::r#move::CaldavItemMove,
//!     rfc4918::WebdavAuth,
//! };
//! use url::Url;
//!
//! // Ready stream, already connected and TLS-negotiated
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let mut coroutine = CaldavItemMove::new(
//!     &base_url,
//!     &auth,
//!     "io-webdav",
//!     "/dav/calendars/personal/",
//!     "task-1.ics",
//!     "/dav/calendars/work/",
//!     Some("\"abc123\""),
//! );
//! let mut arg = None;
//!
//! let moved = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(moved)) => break moved,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! println!("moved {} (etag {:?})", moved.id, moved.etag);
//! ```

use core::mem;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4791::item::join_path,
    rfc4918::{
        WebdavAuth,
        put::duplicate_uid,
        read_etag,
        request::WebdavRequest,
        send::{WebdavSendError, WebdavSendOk, WebdavSendRaw},
    },
};

/// Coroutine that moves a calendar item into another calendar.
#[derive(Debug)]
pub struct CaldavItemMove {
    id: String,
    state: State,
}

impl CaldavItemMove {
    /// Builds a new `move-item` coroutine moving `<calendar_path>/<id>` to
    /// `<target_path>/<id>`. `id` is the resource id exactly as the server
    /// returned it (`CaldavItemRef::id`), used verbatim on both sides.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        calendar_path: &str,
        id: &str,
        target_path: &str,
        if_match: Option<&str>,
    ) -> Self {
        let path = join_path(calendar_path, id);
        // NOTE: an absolute URL, which RFC 2518 required and every server
        // takes, where RFC 4918 also allows a path.
        let target = join_path(target_path, id);
        let destination = base_url.join(&target).map_or(target, |url| url.to_string());
        let mut request = WebdavRequest::r#move(base_url, auth, user_agent, &path)
            .destination(&destination)
            .overwrite(false);
        if let Some(etag) = if_match {
            request = request.if_match(etag);
        }
        Self {
            id: id.to_string(),
            state: State::Send(WebdavSendRaw::new(request.body(Vec::new()))),
        }
    }
}

impl WebdavCoroutine for CaldavItemMove {
    type Yield = WebdavYield;
    type Return = Result<CaldavItemMoveOk, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Send(send) => {
                let ok = match send.resume(arg) {
                    WebdavCoroutineState::Yielded(yielded) => {
                        return WebdavCoroutineState::Yielded(yielded);
                    }
                    WebdavCoroutineState::Complete(Err(err)) => {
                        return WebdavCoroutineState::Complete(Err(duplicate_uid(err)));
                    }
                    WebdavCoroutineState::Complete(Ok(ok)) => ok,
                };

                let WebdavSendOk { response, .. } = ok;
                let etag = read_etag(&response);
                let id = mem::take(&mut self.id);
                WebdavCoroutineState::Complete(Ok(CaldavItemMoveOk { id, etag }))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Send(WebdavSendRaw),
}

/// Outcome of a successful [`CaldavItemMove`] resume.
#[derive(Clone, Debug)]
pub struct CaldavItemMoveOk {
    /// Item resource id in the target calendar, the source's own.
    pub id: String,
    /// Entity tag of the moved item, when the server returns one: most do not,
    /// and a reader of the target learns it from its next listing.
    pub etag: Option<String>,
}
