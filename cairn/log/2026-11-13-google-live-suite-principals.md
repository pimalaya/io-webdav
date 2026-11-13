---
cairn: log
change: google-live-suite-principals
landed: 2026-11-13
---

# Google live suite mints one token per protocol

Since 2026-11-07 the pimalaya.org domain-wide delegation of the test service account is narrowed on purpose: for `google@pimalaya.org` it grants `contacts`, `carddav`, `calendar.events`, `calendar.calendarlist.readonly` and `calendar.calendars`, and no longer the full `calendar` scope. tests/google.rs minted a single token asking `calendar` and `carddav` together, and one refused scope fails the whole request, so both tests died at the token endpoint with `unauthorized_client` before reaching a DAV server.

The suite now mints one token per protocol, each asking only what that protocol needs:

- `carddav` acts as the delegated user with `contacts` and `carddav`, and runs its card flow unchanged in the `default` address book.
- `caldav` and `caldav_secondary_calendars` act as the service account itself (no subject in the assertion) with the full `calendar` scope, which a service account gets for itself without delegation. `caldav` runs its item flow unchanged, in the service account's own primary calendar (`events`) instead of the test user's.

Found live, on 2026-11-13:

- Google CalDAV refuses a delegated token of the three narrowed calendar scopes from the principal discovery on: the PROPFIND on `/caldav/v2/` answers 403 with the GData error `insufficientPermissions`.
- The service account acting as itself has a principal, a home set and a primary calendar over CalDAV, and the whole item flow passes there: checkpoint sync, unknown sync token refused, stale If-Match refused with 412, removal reported.
- `MKCALENDAR` as the service account answers 404, so the run cannot make a throwaway calendar over CalDAV and keeps to the item flow.

Not covered any more: CalDAV as a delegated Workspace user. Granting the full `calendar` scope to the delegation would bring it back. `caldav_secondary_calendars` was not run with scratch calendars; under a key they now have to be the service account's own.

Verified: the Google suite green live twice in a row with the key, and once more with the feature set CI uses; offline suite green, clippy clean. No change under src/.

The [packaging](../spec/packaging.md) capability moved: "Live suites" now names the two Google principals and their scopes.
