//! Read-only CalDAV/CardDAV tests against Google.
//!
//! Google speaks CalDAV/CardDAV over HTTPS but only behind an OAuth2
//! Bearer token (no app passwords), and it rejects collection creation
//! (`MKCALENDAR`/`MKCOL`), so these tests only cover the read-only
//! subset: discovery plus listing.
//!
//! Mint an access token out of band (the OAuth2 dance is the caller's
//! job; this crate only forwards the token as `Authorization: Bearer`),
//! then run with:
//!
//! ```sh
//! GOOGLE_ACCESS_TOKEN="ya29...." \
//! cargo test --test google -- --ignored
//! ```
//!
//! These tests are intentionally not wired into CI: Google access
//! tokens expire after about an hour, so a stored secret would be stale
//! by the time CI runs. Running them unattended would mean a
//! refresh-token exchange step on every push; they are kept manual.
//!
//! Note on Google's non-standard discovery: only the `.well-known`
//! entry behaves oddly. `.well-known/carddav` 301-redirects, but only
//! for an *authenticated PROPFIND* (a plain GET 404s), so the CardDAV
//! test starts at that entry via [`common::google_carddav_base`].
//! `.well-known/caldav` does not redirect at all, so the CalDAV test
//! targets the resolved API host directly.

mod common;

use std::env;

/// Discovery + calendar listing against Google Calendar.
#[test]
#[ignore = "requires GOOGLE_ACCESS_TOKEN env var and --ignored"]
fn caldav() {
    let token = env::var("GOOGLE_ACCESS_TOKEN").expect("GOOGLE_ACCESS_TOKEN not set");

    common::caldav_readonly(
        "https://apidata.googleusercontent.com/",
        common::bearer_auth(&token),
    );
}

/// Discovery (via the authenticated `.well-known/carddav` redirect) +
/// addressbook listing against Google Contacts.
#[test]
#[ignore = "requires GOOGLE_ACCESS_TOKEN env var and --ignored"]
fn carddav() {
    let token = env::var("GOOGLE_ACCESS_TOKEN").expect("GOOGLE_ACCESS_TOKEN not set");

    let base = common::google_carddav_base(&token);
    common::carddav_readonly(base.as_str(), common::bearer_auth(&token));
}
