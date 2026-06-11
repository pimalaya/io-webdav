//! End-to-end CalDAV + CardDAV tests against Fastmail.
//!
//! Fastmail exposes both protocols over HTTPS behind HTTP Basic auth;
//! generate an app password (with the calendars + contacts scopes) and
//! run with:
//!
//! ```sh
//! FASTMAIL_EMAIL=test@fastmail.com \
//! FASTMAIL_APP_PASSWORD=xxx \
//! cargo test --test fastmail -- --ignored
//! ```

mod common;

use std::env;

/// Full CalDAV calendar/event CRUD against Fastmail.
#[test]
#[ignore = "requires FASTMAIL_{EMAIL,APP_PASSWORD} env vars and --ignored"]
fn caldav() {
    let email = env::var("FASTMAIL_EMAIL").expect("FASTMAIL_EMAIL not set");
    let password = env::var("FASTMAIL_APP_PASSWORD").expect("FASTMAIL_APP_PASSWORD not set");

    common::caldav(
        "https://caldav.fastmail.com/dav/",
        common::basic_auth(&email, &password),
    );
}

/// Full CardDAV addressbook/card CRUD against Fastmail.
#[test]
#[ignore = "requires FASTMAIL_{EMAIL,APP_PASSWORD} env vars and --ignored"]
fn carddav() {
    let email = env::var("FASTMAIL_EMAIL").expect("FASTMAIL_EMAIL not set");
    let password = env::var("FASTMAIL_APP_PASSWORD").expect("FASTMAIL_APP_PASSWORD not set");

    common::carddav(
        "https://carddav.fastmail.com/dav/",
        common::basic_auth(&email, &password),
    );
}
