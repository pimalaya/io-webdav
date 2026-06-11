//! End-to-end CalDAV + CardDAV tests against a local Stalwart server.
//!
//! Start a local Stalwart instance and run with:
//!
//! ```sh
//! ./tests/stalwart.sh
//! cargo test --test stalwart -- --ignored
//! ```
//!
//! The bootstrap script provisions one domain (`pimalaya.org`) and one
//! user (`test@pimalaya.org`) with a strong password (Stalwart enforces
//! a zxcvbn-style strength check). Stalwart serves CalDAV / CardDAV on
//! the same HTTP listener as JMAP, bound to host port 8080. It routes
//! DAV by resource type under `/dav/` (`/dav/cal/`, `/dav/card/`), so
//! the bare `/dav/` root is not a resource: discovery targets the
//! per-type root, which resolves the shared principal.

mod common;

const CALDAV_BASE_URL: &str = "http://localhost:8080/dav/cal/";
const CARDDAV_BASE_URL: &str = "http://localhost:8080/dav/card/";

/// Full CalDAV calendar/event CRUD against Stalwart.
#[test]
#[ignore = "requires a running Stalwart instance on localhost:8080 and --ignored"]
fn caldav() {
    common::caldav(
        CALDAV_BASE_URL,
        common::basic_auth("test@pimalaya.org", "P!malaya-test-2026"),
    );
}

/// Full CardDAV addressbook/card CRUD against Stalwart.
#[test]
#[ignore = "requires a running Stalwart instance on localhost:8080 and --ignored"]
fn carddav() {
    common::carddav(
        CARDDAV_BASE_URL,
        common::basic_auth("test@pimalaya.org", "P!malaya-test-2026"),
    );
}
