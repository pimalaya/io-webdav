---
cairn: change
id: carddav-query-filter
status: active
created: 2026-09-26
---

# CardDAV query filters

> Cross-repo change, same id in io-webdav (here) and cardamum. Order: **io-webdav** (0.4.0) → **cardamum**.

## Why

cardamum#25 asks for a card search. The shared `card` API stays least-common-denominator until the 2027 query work, so search lands per protocol, and CardDAV is the one backend without it. RFC 6352 §8.6 already defines it: the `addressbook-query` REPORT this crate sends to list cards carries a filter, always the empty match-all one today.

Listing also drops a server-side truncation. A server capping the result answers a 507 row for the collection itself, which the self-entry skip discards, so a partial listing reads as complete. Enumeration already reports it.

## What

- **A typed filter covering RFC 6352 §10.5**: filter, prop-filter, param-filter, is-not-defined and text-match, with every attribute (`test`, `collation`, `negate-condition`, `match-type`). The default stays the empty `allof` match-all.
- **The RFC 6352 §8.6.1 limit** (`C:limit/C:nresults`).
- **List options and output**: `CarddavCardList::new` takes `CarddavCardListOptions { filter, limit }`, and returns `CarddavCardListOk { cards, truncated }`, the twin of `CarddavCardEnumOk`. Breaking, hence 0.4.0.
- **A named refusal**: the `supported-filter` and `supported-collation` preconditions (RFC 6352 §8.6) surface as `WebdavSendError::UnsupportedFilter`, matched on the element like `DuplicateUid`.

## Scope / non-goals

- **No filter on enumeration.** It feeds sync, which needs the whole collection.
- **No CalDAV filter.** `calendar_query_body` keeps its raw fragment; aligning it is a separate change.
- **No client-side evaluation.** The server decides what matches, and the crate reports what it answered.
- **No partial `address-data`** (RFC 6352 §10.4).
