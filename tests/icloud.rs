//! End-to-end CalDAV + CardDAV tests against Apple iCloud.
//!
//! iCloud serves CalDAV and CardDAV over HTTPS behind HTTP Basic auth,
//! but only with an app-specific password: accounts have mandatory 2FA,
//! so the primary Apple ID password is rejected. Generate one at
//! <https://account.apple.com/account/manage>.
//!
//! iCloud forbids creating collections over DAV: `MKCALENDAR` and
//! `MKCOL` both return 403, and it exposes only the collections it
//! provisions for the account (notably a single fixed `card`
//! addressbook). So these tests exercise item / card CRUD inside an
//! existing collection rather than the full create-collection flow.
//! Point them at a throwaway calendar via `ICLOUD_CALENDAR_ID` (the
//! per-calendar UUID; list them once with the CalDAV flow logs or the
//! iCloud web UI). The addressbook defaults to `card` and is
//! overridable via `ICLOUD_ADDRESSBOOK_ID`. Run with:
//!
//! ```sh
//! ICLOUD_EMAIL=test@icloud.com \
//! ICLOUD_APP_PASSWORD=xxxx-xxxx-xxxx-xxxx \
//! ICLOUD_CALENDAR_ID=11111111-2222-3333-4444-555555555555 \
//! cargo test --test icloud -- --ignored
//! ```
//!
//! The two protocols live on separate hosts: CalDAV on
//! `caldav.icloud.com`, CardDAV on `contacts.icloud.com`. Both are the
//! generic entry points; iCloud advertises the principal and home-sets
//! on a per-account partition host (e.g. `p52-caldav.icloud.com`). The
//! shared helper keeps only the discovered path and replays it against
//! the generic host, which iCloud routes to the right partition by
//! credential. If a future iCloud change starts requiring the partition
//! host outright, swap the base URL for the advertised one (or reconnect
//! via `WebdavClientStd::set_stream`).

mod common;

use std::env;

/// CalDAV event CRUD inside an existing iCloud calendar.
#[test]
#[ignore = "requires ICLOUD_{EMAIL,APP_PASSWORD,CALENDAR_ID} env vars and --ignored"]
fn caldav() {
    let email = env::var("ICLOUD_EMAIL").expect("ICLOUD_EMAIL not set");
    let password = env::var("ICLOUD_APP_PASSWORD").expect("ICLOUD_APP_PASSWORD not set");
    let calendar_id = env::var("ICLOUD_CALENDAR_ID").expect("ICLOUD_CALENDAR_ID not set");

    common::caldav_items(
        "https://caldav.icloud.com/",
        common::basic_auth(&email, &password),
        &calendar_id,
    );
}

/// CardDAV card CRUD inside iCloud's fixed `card` addressbook.
#[test]
#[ignore = "requires ICLOUD_{EMAIL,APP_PASSWORD} env vars and --ignored"]
fn carddav() {
    let email = env::var("ICLOUD_EMAIL").expect("ICLOUD_EMAIL not set");
    let password = env::var("ICLOUD_APP_PASSWORD").expect("ICLOUD_APP_PASSWORD not set");
    let addressbook_id = env::var("ICLOUD_ADDRESSBOOK_ID").unwrap_or("card".into());

    common::carddav_cards(
        "https://contacts.icloud.com/",
        common::basic_auth(&email, &password),
        &addressbook_id,
    );
}
