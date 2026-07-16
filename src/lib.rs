#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! # io-webdav
//!
//! I/O-free WebDAV client coroutines built on io-http: every network
//! exchange is a resumable state machine emitting read and write
//! requests instead of performing I/O itself. The caller owns the
//! socket and pumps the coroutine with the bytes it read, whatever the
//! runtime (blocking, async, in-memory tests). The `client` feature
//! ships a ready-made std-blocking pump for callers who just want a
//! working client.
//!
//! ## Layout: one folder per RFC
//!
//! The source tree mirrors how the WebDAV specifications are split,
//! one module per RFC. [`rfc4918`] implements the WebDAV core: the
//! PROPFIND, PROPPATCH, MKCOL, COPY, MOVE, DELETE, GET, PUT, OPTIONS
//! and REPORT requests, the multistatus response parser, the
//! [`rfc4918::WebdavAuth`] modes and the low-level send coroutine every
//! higher request builds on. [`rfc4791`] covers CalDAV: calendar
//! collections and calendar object resources (items), with calendar
//! home-set discovery. [`rfc6352`] covers CardDAV: address book
//! collections and contact cards, with address book home-set
//! discovery, batch multiget and ETag-only enumeration. [`rfc5397`]
//! discovers the current user principal, the entry point of the
//! discovery flow. [`rfc6578`] adds collection synchronization: the
//! sync-collection REPORT and its sync tokens.
//!
//! Two modules span the RFC modules and therefore live at the crate
//! root: [`coroutine`] defines the coroutine contract every state
//! machine implements, and the optional [`client`] module (`client`
//! feature) is the std-blocking pump: a light client wrapping any
//! stream you opened yourself, or a full client opening the TCP/TLS
//! connection itself when one of the TLS features is enabled.
//!
//! ## The coroutine contract
//!
//! Every coroutine implements [`coroutine::WebdavCoroutine`]: a resume
//! method taking the bytes read since the last step and returning
//! either an intermediate yield or a terminal completion. Standard
//! coroutines yield the shared read and write requests of
//! [`coroutine::WebdavYield`]; the redirect-aware discovery coroutines
//! declare their own [`rfc4918::coroutine::WebdavRedirectYield`],
//! surfacing a 3xx response to the caller as a redirect request instead
//! of following it, so the caller decides whether to reconnect to the
//! new authority and retry. The [`webdav_try`] macro chains an inner
//! coroutine step inside an outer resume, re-yielding and
//! short-circuiting like the question mark operator.
//!
//! ## Conventions
//!
//! The crate is no_std with alloc; std only enters behind the `client`
//! feature. Every public item carries the bare `Webdav` prefix, the
//! protocol not being version-scoped. Logging follows the library
//! rules: state changes at debug level, in-process steps and data dumps
//! at trace level.

extern crate alloc;
#[cfg(feature = "client")]
extern crate std;

#[cfg(feature = "client")]
pub mod client;
pub mod coroutine;
pub mod rfc4791;
pub mod rfc4918;
pub mod rfc5397;
pub mod rfc6352;
pub mod rfc6578;
