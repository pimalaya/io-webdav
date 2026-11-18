#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]

//! # io-webdav
//!
//! I/O-free WebDAV client coroutines built on io-http: every network exchange
//! is a resumable state machine emitting read and write requests instead of
//! performing I/O itself.
//!
//! The caller owns the socket and pumps the coroutine with the bytes it read,
//! whatever the runtime (blocking, async, in-memory tests). The `std` feature
//! ships a ready-made std-blocking pump for callers who just want a working
//! client.
//!
//! ## Features
//!
//! The vocabulary (namespaces, properties, hrefs and ids, the multistatus
//! parsers and the request body builders) is always compiled. `client`, on by
//! default, adds the coroutines and io-http. `server` adds [`server`]: request
//! body and header parsers plus multistatus and error body writers, for a
//! server that owns its HTTP stack. `std` adds the blocking client, and each
//! TLS feature (`rustls-ring` by default, `rustls-aws`, `native-tls`) its
//! connector.
//!
//! ## Layout: one folder per RFC
//!
//! The source tree mirrors how the WebDAV specifications are split, one
//! module per RFC. [`rfc4918`] implements the WebDAV core: the PROPFIND,
//! PROPPATCH, MKCOL, COPY, MOVE, DELETE, GET, PUT, OPTIONS and REPORT
//! requests.
//!
//! Next to them it carries the multistatus response parsers, the
//! [`rfc4918::WebdavAuth`] modes and the low-level send coroutine every
//! higher request builds on. A multistatus reads either vocabulary-agnostic
//! or into a serde type of the caller ([`rfc4918::typed`]), over a scanner
//! built for the XML WebDAV servers send, which probes short spans 16 bytes
//! at a time with the SSE2 or NEON compares of the build.
//!
//! [`rfc4791`] covers CalDAV (calendar collections and items) and
//! [`rfc6352`] CardDAV (address book collections and cards). The two are
//! deliberately twins, same verbs under the same shapes, home-set discovery
//! and batch multiget and ETag-only enumeration included.
//!
//! [`rfc5397`] discovers the current user principal, the entry point of the
//! discovery flow. [`rfc6578`] adds collection synchronization, the
//! sync-collection REPORT and its tokens, protocol-neutral so both layers
//! use it, plus the PROPFIND enumeration for a server implementing none.
//!
//! Three modules span the RFC modules and therefore live at the crate root.
//! [`coroutine`] defines the coroutine contract every state machine
//! implements. The optional [`client`] module (`std` feature) is the
//! std-blocking pump: a light client wrapping any stream you opened yourself,
//! or a full client opening the TCP/TLS connection itself when one of the TLS
//! features is enabled. The optional [`server`] module (`server` feature) is
//! the server side, without a session.
//!
//! ## The coroutine contract
//!
//! Every coroutine implements [`coroutine::WebdavCoroutine`]: a resume method
//! taking the bytes read since the last step and returning either an
//! intermediate yield or a terminal completion. Standard coroutines yield the
//! shared read and write requests of [`coroutine::WebdavYield`].
//!
//! The redirect-aware discovery coroutines declare their own
//! [`rfc4918::coroutine::WebdavRedirectYield`], surfacing a 3xx response to the
//! caller as a redirect request instead of following it, so the caller decides
//! whether to reconnect to the new authority and retry.
//!
//! The [`webdav_try`] macro chains an inner coroutine step inside an outer
//! resume, re-yielding and short-circuiting like the question mark operator.
//!
//! ## Conventions
//!
//! The crate is no_std with alloc. std only enters behind the `std` feature.
//! Every public item carries the bare `Webdav` prefix, the protocol not being
//! version-scoped. Logging follows the library rules: state changes at debug
//! level, in-process steps and data dumps at trace level.
//!
//! The crate stays byte-oriented. An iCalendar or vCard payload is carried as
//! raw bytes in both directions, the parse belonging to ical and vcard
//! upstream, so neither is a dependency here. A REPORT returns the whole text
//! of `calendar-data` or `address-data`, untrimmed: it is the object itself.
//!
//! A coroutine joins the ids it is given, percent-encoding the bytes a path
//! segment cannot hold raw. A listing decodes the segment it reads an id from
//! only when that encoding gives the segment back, keeping it as served
//! otherwise, so an id round-trips through create, list and address.
//! [`rfc4918::check_collection_id`] and [`rfc4918::check_resource_id`] refuse
//! one addressing another resource, and the client runs them first.
//!
//! Beside its id, every listed, fetched or written resource carries its href
//! as served, percent-encoding kept: the key for a store keeping the server's
//! spelling, two spellings decoding to one id.
//!
//! The living spec and the development history are in the cairn/ folder, and
//! the crate's own conventions and test matrix are in CONTRIBUTING.md.

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
pub mod client;
#[cfg(feature = "client")]
pub mod coroutine;
pub mod rfc4791;
pub mod rfc4918;
#[cfg(feature = "client")]
pub mod rfc5397;
pub mod rfc6352;
#[cfg(feature = "client")]
pub mod rfc6578;
#[cfg(feature = "server")]
pub mod server;
