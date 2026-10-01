---
cairn: log
change: google-oauth-live-suite
landed: 2026-10-01
---

# Google live suite with minted tokens and a write round trip

tests/google.rs mints its own token from the pimalaya.org service account (domain-wide delegation, scopes `calendar` and `carddav`) acting as `google@pimalaya.org`; `GOOGLE_ACCESS_TOKEN` still short-circuits. The read-only flows are gone: Google now runs the item-only flows, in the primary calendar (`events`) and the `default` address book. A `google-tests` CI job reads `GOOGLE_SERVICE_ACCOUNT_KEY`, limited to `pimalaya/io-webdav`.

The item-only flows (shared with iCloud) gained an If-Match update already, and now a sync round trip from a checkpoint read with a Depth 0 PROPFIND, a delete inside the guarded body, and an incremental sync that must report the removal. They adopt the id a create answered with, and register teardown before any assertion.

Google quirks found live, all worked around in the tests, none in the library:

- the CalDAV principal is discovered from `/caldav/v2/`, not `/`;
- a `PUT` stores the resource under a name of Google's own;
- the sync token is served only at Depth 0, absent from a Depth 1 listing, and an empty-token initial sync is refused with a 400;
- CardDAV sync deltas report neither the creation nor the removal of a card made over CardDAV (checked over 60 s); CalDAV reports both.

Before the id fix, two failed runs leaked a card each in the subject's address book; both were swept by hand.

Verified: both Google tests green live, offline suite green, clippy clean. iCloud's flows changed with them but were not run (no credentials here).

The [packaging](../spec/packaging.md) capability moved: "Live suites" now names the minted Google token, the item round trip with sync, and the Google CardDAV exception.
