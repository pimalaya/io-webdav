//! Read-only CalDAV test against Google Calendar.
//!
//! Google speaks CalDAV over HTTPS but only behind an OAuth2 Bearer
//! token (no app passwords), and it rejects `MKCALENDAR`, so this test
//! only covers the read-only subset: discovery plus listing calendars.
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
//! This test is intentionally not wired into CI: Google access tokens
//! expire after about an hour, so a stored secret would be stale by the
//! time CI runs. Running it unattended would mean a refresh-token
//! exchange step on every push; it is kept manual instead.
//!
//! Note: Google's discovery is non-standard (the documented
//! `.well-known/caldav` entry point 301-redirects, which the client
//! surfaces rather than follows). The base URL below targets the
//! resolved API host directly; adjust it if Google moves the endpoint.

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
