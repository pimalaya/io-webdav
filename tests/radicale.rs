//! End-to-end CalDAV + CardDAV tests against a local Radicale server.
//!
//! Start a local Radicale instance and run with:
//!
//! ```sh
//! ./tests/radicale.sh
//! cargo test --test radicale -- --ignored
//! ```
//!
//! The bootstrap script runs Radicale in a container with a single
//! htpasswd user (`test` / `test`) over plain HTTP on host port 5232.

mod common;

const BASE_URL: &str = "http://localhost:5232/";

/// Full CalDAV calendar/event CRUD against Radicale.
#[test]
#[ignore = "requires a running Radicale instance on localhost:5232 and --ignored"]
fn caldav() {
    common::caldav(BASE_URL, common::basic_auth("test", "test"));
}

/// Full CardDAV addressbook/card CRUD against Radicale.
#[test]
#[ignore = "requires a running Radicale instance on localhost:5232 and --ignored"]
fn carddav() {
    common::carddav(BASE_URL, common::basic_auth("test", "test"));
}
