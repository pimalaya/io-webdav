---
cairn: log
change: an-enumeration-returns-what-it-could-not-read
landed: 2026-11-21
---

# An enumeration returns what it could not read

The `sync-collection` REPORT and its `PROPFIND` fallback return every member row the server answered with an error, where they traced and dropped it, so a consumer reading a full listing no longer takes a 403 or 5xx member for a removed one. neverest did, and propagated the delete (its cairn/changes/connector-listings-fail-rather-than-shrink).

**The field.** `WebdavSyncDelta::unreadable: Vec<WebdavUnreadableEntry>`, the type a-listing-returns-what-it-could-not-read added for the card and item listings, in the order met. `from_multistatus` sorts a 404 into `vanished` and a 507 into `truncated` as before, skips the self-entry before looking at the status (a 403 self-entry stays skipped), and makes any other non-2xx row unreadable, one naming no member id skipped. It is the only builder of a `WebdavSyncDelta`, called by both arms of `WebdavSyncCollection::resume`; `WebdavClientStd::sync_items` and `sync_cards` return the field with the delta. Breaking for a consumer building the struct by literal.

**The getetag ruling.** A 2xx row whose `getetag` propstat the server refused stays a change with no ETag, unlike the precedent's refused data property: an enumeration returns membership, which the 2xx row states, and the ETag is a version the body read supplies; reading it unreadable would hide a member the server named.

**Left out.** `CaldavItemEnum` and `CarddavCardEnum` (the query spines) have their own projection, not `from_multistatus`: they list a 403 row as a member with no ETag and a 507 row naming the collection bare as a bogus member. No silent drop, no consumer in the family; noted in the proposal for a change of its own.

**Capabilities moved.** sync: "Delta shape" names the unreadable rows; new requirement "An enumeration returns the member rows it could not read".

**Verified.** `an_enumeration_returns_the_member_rows_it_could_not_read` (both enumerations) and `initial_sync_sorts_the_delta_rows`, which now asserts its 500 row unreadable, failed with the field added and left empty, and pass. `cargo fmt --check`, `cargo clippy --all-features --all-targets -- -D warnings` and `cargo test --all-features` clean (326 passed, 13 live tests ignored). neverest adapts when it pins the release.
