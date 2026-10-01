---
cairn: change
id: google-oauth-live-suite
status: landed
created: 2026-10-01
---

# Google live suite with minted tokens and a write round trip

## Why

The Google suite (tests/google.rs) is read-only and manual: it takes a hand-minted `GOOGLE_ACCESS_TOKEN`, which dies within the hour, so it was kept out of CI. It never writes, so the Bearer path is untested for `PUT`, `If-Match` and `DELETE`, which is what a consumer syncing a Google account does (MOA step 5a: CalDAV/CardDAV with OAuth against Google).

Pimalaya now owns a Workspace on pimalaya.org with a service account holding domain-wide delegation, so a test can mint its own token for `google@pimalaya.org`, unattended, in CI too.

## What

- tests/google.rs: `token()` takes `GOOGLE_ACCESS_TOKEN`, or mints one from `GOOGLE_SERVICE_ACCOUNT_KEY{,_FILE}` acting as `GOOGLE_SERVICE_ACCOUNT_SUBJECT` (default `google@pimalaya.org`), scopes `calendar` and `carddav`. Google still refuses `MKCALENDAR`/`MKCOL`, so the tests run the item-only flows inside the subject's primary calendar and `default` address book (overridable with `GOOGLE_CALENDAR_ID` / `GOOGLE_ADDRESSBOOK_ID`).
- tests/common: the item-only flows (`caldav_items`, `carddav_cards`, shared with iCloud) gain the sync round trip the full flows already have: initial `sync-collection`, `DELETE` inside the guarded body, incremental sync reporting the removal. Teardown deletes the item only if the body did not.
- CI: a `google-tests` job reading the `GOOGLE_SERVICE_ACCOUNT_KEY` secret, limited to `pimalaya/io-webdav` since forks hold no secret.

No library code changes.
