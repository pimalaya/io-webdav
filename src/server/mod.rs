//! # Server
//!
//! The server side of WebDAV, CalDAV and CardDAV, without a session: the
//! HTTP exchange belongs to the server that embeds it, which hands the
//! request headers and body to these parsers and sends what the writers
//! build.
//!
//! [`body`] holds what the request body parsers share, each method its own
//! module: [`propfind`], [`proppatch`], [`mkcol`] (`MKCALENDAR` too),
//! [`lock`] and [`report`]. [`header`] parses the WebDAV request headers.
//! [`multistatus`] writes the 207 body and [`precondition`] the `DAV:error`
//! body, both over the property names of [`name`].

pub mod body;
pub mod header;
pub mod lock;
pub mod mkcol;
pub mod multistatus;
pub mod name;
pub mod precondition;
pub mod propfind;
pub mod proppatch;
pub mod report;
