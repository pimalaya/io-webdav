---
cairn: change
id: an-enumeration-returns-what-it-could-not-read
status: landed
created: 2026-11-21
---

# An enumeration returns what it could not read

## Why

`from_multistatus` (src/rfc6578/sync_collection.rs) sorts the rows of both enumerations, the `sync-collection` REPORT and its `PROPFIND` fallback: a 404 row is a removal, a 507 row a truncation, and any other non-2xx row was only traced and dropped. In a full listing (an initial sync, the `PROPFIND` fallback) a member the server answered 403 or 5xx was then simply absent, and a consumer reading the listing as a whole collection infers its removal. neverest does, and propagates the delete to the other endpoint (neverest cairn/changes/connector-listings-fail-rather-than-shrink).

a-listing-returns-what-it-could-not-read closed the same class for the card and item listings and multigets, and left this one out of scope.

## What

- `WebdavSyncDelta` gains `unreadable: Vec<WebdavUnreadableEntry>`, in the order met: every member row answered with a non-2xx status other than 404 (vanished) and 507 (truncated), with its id, href, status and refused properties. **Breaking**: a consumer building the struct by literal names the field.
- `from_multistatus` checks the self-entry before the error statuses, so a self-entry stays skipped whatever its status; a row naming no member id is traced and skipped, as the precedent's projections skip it.
- `WebdavClientStd::sync_items` and `sync_cards` return the field through the delta, unchanged in shape.
- CHANGELOG Unreleased, marked breaking.

## Decisions

- **The precedent's type**: `WebdavUnreadableEntry`, not a sync-specific one; a consumer handling both listings matches one shape.
- **A 507 row stays truncation only**, even when it names the collection without its trailing slash, as before: the 507 arm runs before the self-entry and error arms.
- **A 404 row stays a removal** (RFC 6578 §3.4); it is not unreadable.
- **A refused `getetag` under a 2xx row stays a change, with no ETag.** The precedent counts a refused `address-data` or `calendar-data` as unreadable because that property is the payload a listing exists to return: without it the entry carries nothing. An enumeration returns membership, and a 2xx row states that the member exists (and, in a delta, that it changed); the ETag is the version, which the consumer gets from the body read that follows. Making it unreadable would hide a member the server named, turning a known change into an unknown one; dropping it would be the defect this change fixes. The propstat's status is not carried on `WebdavSyncChange`: no consumer needs it, and adding it is breaking for no reader.
- **Rows, not errors**, as the precedent: the enumeration still succeeds, and what an unreadable member means (an incomplete listing) is the consumer's call.

## Scope / non-goals

- The query spines `CaldavItemEnum` (src/rfc4791/item/enumerate.rs) and `CarddavCardEnum` (src/rfc6352/card/enumerate.rs) do not go through `from_multistatus`: their own `from_response` keeps every row whose href does not end in `/`, so a 403 row is listed as a member with no ETag and a 507 row naming the collection bare as a bogus member. That is no silent drop (nothing is absent), but it is the same misreading the precedent fixed for the card listing; left for a change of its own, no consumer of this family calling them (neverest enumerates through `sync_items` and `sync_cards`).
- A 404 self-entry still lands in `vanished`, as before.
- No version bump; the release is the owner's.

## Order

io-webdav first, released; neverest then pins the release (its cairn/changes/connector-listings-fail-rather-than-shrink).
