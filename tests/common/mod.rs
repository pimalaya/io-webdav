//! Shared helpers for the integration tests.
//!
//! The scripted-coroutine helpers ([`http_response`] plus the `expect_*`
//! steps) let the offline suites (rfc4918, rfc4791, rfc5397, rfc6352, rfc6578,
//! client) resume any I/O-free coroutine against canned HTTP response bytes,
//! following the io-imap canonical layout. The live provider helpers live in
//! [`live`], which needs the `client` feature.
//!
//! Each integration test compiles this module on its own and only exercises a
//! subset of these helpers, so the rest end up flagged as dead code; suppress
//! the noise at the module level.

#![allow(dead_code)]

use core::fmt::Debug;

use io_http::{rfc6750::bearer::HttpAuthBearer, rfc7617::basic::HttpAuthBasic};
use io_webdav::{
    coroutine::*,
    rfc4918::{WebdavAuth, coroutine::*},
};
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

/// Builds an HTTP Basic [`WebdavAuth`] (RFC 7617).
pub fn basic_auth(username: &str, password: &str) -> WebdavAuth {
    WebdavAuth::Basic(HttpAuthBasic::new(username, password))
}

/// Builds an HTTP Bearer [`WebdavAuth`] (RFC 6750), e.g. an OAuth2 access
/// token.
pub fn bearer_auth(token: &str) -> WebdavAuth {
    WebdavAuth::Bearer(HttpAuthBearer::new(token))
}

#[cfg(feature = "client")]
pub mod live;
