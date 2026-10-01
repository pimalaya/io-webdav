---
cairn: log
change: live-suite-standard-refusals
landed: 2026-10-01
---

# Live suites assert the standard refusals

Every live flow now opens through `WebdavClientStd::connect` and, beyond the happy path, asserts what a conforming server refuses: a stale If-Match update (412), an unknown sync token (refused as invalid, or a full resync holding the live resource), and, in the full flows, a duplicate UID (no-uid-conflict). The full flows rename their collection with PROPPATCH; the card flows query the card by the UID the server holds. `carddav_cards` takes `CarddavCardsChecks { sync, if_match }`.

Found live:

- Google CardDAV applies a PUT whose If-Match no longer matches (204, overwrite) instead of answering 412, against its own documentation; Google CalDAV refuses it. A consumer syncing Google contacts gets no lost-update protection.
- Google CardDAV rewrites the vCard UID to its own id.
- Stalwart answers an unknown sync token with a full resync rather than the valid-sync-token refusal; Google CalDAV refuses it.
- Stalwart refuses a PROPPATCH re-sending `supported-calendar-component-set` (412), which RFC 4791 lets it protect; the flow now sends the name alone.

Live coverage of the library, Google + Stalwart + Radicale: 76.8% before, 88.4% after. Still unreached live: unsupported report and filter refusals, 507 truncation, the PROPFIND sync fallback, redirect errors, which no server here produces.

Verified: all six live tests green, offline suite green, clippy clean. Fastmail and iCloud flows changed with them but were not run (no credentials here).

The [packaging](../spec/packaging.md) capability moved: "Live suites" now names `connect`, the standard refusals, PROPPATCH, the UID query and both Google CardDAV exceptions.
