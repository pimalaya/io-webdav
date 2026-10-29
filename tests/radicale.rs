//! End-to-end CalDAV + CardDAV tests against a local Radicale server.
//!
//! Start a local Radicale instance and run with:
//!
//! ```sh
//! ./tests/radicale.sh
//! cargo test --test radicale -- --ignored
//! ```
//!
//! `RADICALE_URL` points the tests at another instance.
//!
//! The bootstrap script runs Radicale in a container with a single htpasswd
//! user (`test` / `test`) over plain HTTP on host port 5232.

mod common;

const BASE_URL: &str = "http://localhost:5232/";

/// The server under test: [`BASE_URL`] unless `RADICALE_URL` names another.
fn base_url() -> String {
    std::env::var("RADICALE_URL").unwrap_or_else(|_| BASE_URL.to_owned())
}

/// Full CalDAV calendar/event CRUD against Radicale.
#[test]
#[ignore = "requires a running Radicale instance on localhost:5232 and --ignored"]
fn caldav() {
    common::caldav(&base_url(), common::basic_auth("test", "test"));
}

/// An item moved between two calendars, on a client reconnecting itself.
#[test]
#[ignore = "requires a running Radicale instance on localhost:5232 and --ignored"]
fn caldav_move() {
    common::caldav_move(&base_url(), common::basic_auth("test", "test"));
}

/// Full CardDAV addressbook/card CRUD against Radicale.
#[test]
#[ignore = "requires a running Radicale instance on localhost:5232 and --ignored"]
fn carddav() {
    common::carddav(&base_url(), common::basic_auth("test", "test"));
}
