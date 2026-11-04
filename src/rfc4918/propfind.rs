//! # PROPFIND
//!
//! Generic `PROPFIND` coroutine (RFC 4918 §9.1).
//!
//! Requests `props` against `path` at the given `Depth`; the request body is
//! generated from the selector and the response is parsed into `M`: a
//! [`WebdavMultistatus`], or a [`WebdavMultistatusOf`] reading the properties
//! into a serde type.
//!
//! [`WebdavMultistatusOf`]: crate::rfc4918::typed::WebdavMultistatusOf
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
//!     rfc4918::{
//!         DISPLAYNAME, RESOURCETYPE, WebdavAuth, propfind::WebdavPropfind,
//!         typed::WebdavMultistatusOf,
//!     },
//! };
//! use serde::Deserialize;
//! use url::Url;
//!
//! #[derive(Debug, Deserialize)]
//! struct Props {
//!     displayname: Option<String>,
//! }
//!
//! // Ready stream, already connected and TLS-negotiated
//! let mut stream = TcpStream::connect("dav.example.org:443").unwrap();
//! let mut buf = [0u8; 4096];
//!
//! let base_url: Url = "https://dav.example.org/".parse().unwrap();
//! let auth = WebdavAuth::None;
//! let props = [RESOURCETYPE, DISPLAYNAME];
//! let mut coroutine =
//!     WebdavPropfind::<WebdavMultistatusOf<Props>>::new(&base_url, &auth, "io-webdav", "/dav/", 1, &props);
//! let mut arg = None;
//!
//! let multistatus = loop {
//!     match coroutine.resume(arg.take()) {
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsWrite(bytes)) => {
//!             stream.write_all(&bytes).unwrap();
//!         }
//!         WebdavCoroutineState::Yielded(WebdavYield::WantsRead) => {
//!             let n = stream.read(&mut buf).unwrap();
//!             arg = Some(&buf[..n]);
//!         }
//!         WebdavCoroutineState::Complete(Ok(multistatus)) => break multistatus,
//!         WebdavCoroutineState::Complete(Err(err)) => panic!("{err}"),
//!     }
//! };
//!
//! for response in &multistatus.responses {
//!     println!("{}: {:?}", response.href, response.props.displayname);
//! }
//! ```

use core::marker::PhantomData;

use log::trace;
use url::Url;

use crate::{
    coroutine::*,
    rfc4918::{
        WebdavAuth, WebdavMultistatus, WebdavMultistatusParse, WebdavProperty, body_text,
        propfind_body,
        request::WebdavRequest,
        send::{WebdavSendError, WebdavSendRaw},
    },
    webdav_try,
};

/// Coroutine that runs a `PROPFIND` and parses the multistatus body into `M`.
#[derive(Debug)]
pub struct WebdavPropfind<M = WebdavMultistatus> {
    state: State,
    multistatus: PhantomData<fn() -> M>,
}

impl<M: WebdavMultistatusParse> WebdavPropfind<M> {
    /// Builds a new `PROPFIND` coroutine requesting `props` against `path`
    /// (relative to `base_url`) with the given `depth`.
    pub fn new(
        base_url: &Url,
        auth: &WebdavAuth,
        user_agent: &str,
        path: &str,
        depth: u8,
        props: &[WebdavProperty],
    ) -> Self {
        let request = WebdavRequest::propfind(base_url, auth, user_agent, path)
            .depth(depth)
            .content_type_xml()
            .body(propfind_body(props));
        Self {
            state: State::Send(WebdavSendRaw::new(request)),
            multistatus: PhantomData,
        }
    }
}

impl<M: WebdavMultistatusParse> WebdavCoroutine for WebdavPropfind<M> {
    type Yield = WebdavYield;
    type Return = Result<M, WebdavSendError>;

    fn resume(&mut self, arg: Option<&[u8]>) -> WebdavCoroutineState<Self::Yield, Self::Return> {
        trace!("sending request");
        match &mut self.state {
            State::Send(send) => {
                let ok = webdav_try!(send, arg);
                let xml = body_text(&ok.body);
                trace!("received multistatus body {xml}");
                WebdavCoroutineState::Complete(M::parse(&xml).map_err(Into::into))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Send(WebdavSendRaw),
}
