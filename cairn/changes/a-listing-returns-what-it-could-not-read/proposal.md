---
cairn: change
id: a-listing-returns-what-it-could-not-read
status: active
created: 2026-11-14
---

# A listing returns what it could not read

## Why

Himalaya's "a result carries its errors" (2026-11-14) has a listing name every item it could not read, never dropping one silently. io-maildir (a-listing-returns-what-it-could-not-read) and io-jmap (a-get-keeps-what-it-misses) gave Himalaya what it needed to do so. cardamum and calendula follow with their own change of Himalaya's name, and their CardDAV and CalDAV listings cannot: io-webdav drops the rows itself.

`card_from_response` (src/rfc6352/card.rs) and `item_from_response` (src/rfc4791/item.rs) map a multistatus response to an entry and answer `None` for any response without address data or calendar data. That covers the self-entry (an href ending in `/`, rightly skipped) and also every row the server refused: a per-href `<status>` 403 or 404, an `address-data` or `calendar-data` propstat answered 403 or 404 (kept in `WebdavResponseOf::failures` and discarded here), a resource whose data came back empty. `CarddavCardList`, `CaldavItemList`, `CarddavCardMultiget` and `CaldavItemMultiget` all `filter_map` through them, so a caller reads a shorter listing as a complete one. `CaldavItemList` also has no `truncated` flag, where `CarddavCardList` reports a 507 row (RFC 6352 §8.6.1) and a calendar-query answer may truncate the same way.

## What

- A new public type, `WebdavUnreadRow { id: String, href: String, status: Option<u16>, failures: Vec<WebdavPropFailure> }`: a response naming a resource (not a self-entry, a non-empty id) that carried no data, with the response-level status and the refused properties as the server stated them.
- The two projections answer three ways: an entry, an unread row, or nothing (a self-entry, or an id-less href).
- `CarddavCardListOk` gains `unread: Vec<WebdavUnreadRow>`, in the order met.
- `CaldavItemList` returns `CaldavItemListOk { items, truncated, unread }` where it returned a bare `BTreeSet<CaldavItemEntry>`, `truncated` read as `CarddavCardList` reads it (a 507 row). **Breaking.**
- `CarddavCardMultiget` and `CaldavItemMultiget` return their entries beside `unread`, so a multiget naming a card or an item the server answers 404 says so. **Breaking** for each.
- The std client's methods returning these outputs follow (src/client.rs).
- CHANGELOG Unreleased: the type, the two list outputs, the multigets, each breaking change marked as io-webdav marks them.

## Decisions

- **Rows, not errors**: io-webdav reports what the server answered; the code a row takes (`permission-denied`, an unreadable item) is the app's.
- **A refused data property counts as unread** even under a 2xx response status, since the resource exists and its data did not come.
- **Same id as io-maildir's change**, for the family's history.

## Scope / non-goals

- The sync-collection listing (src/rfc6578) and the enumeration spine (src/rfc4791/item/enumerate.rs) keep their own handling.
- No version bump; the release (0.9.0, with the stream 0.4 bump of 185ab63) is the owner's.

## Order

Independent of the apps' Phases 1 to 3, which run first and in parallel. cardamum's and calendula's Phase 4 pin this change's commit.
