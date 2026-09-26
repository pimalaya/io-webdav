---
cairn: log
change: carddav-query-filter
landed: 2026-09-26
---

# CardDAV query filters

cardamum#25 asked for a card search. The shared API waits for the 2027 query work, so search lands per protocol, and CardDAV was the backend without one: the `addressbook-query` REPORT this crate sends always carried the empty match-all filter.

**The filter is modelled whole.** `rfc6352::filter` covers RFC 6352 §10.5: filter, prop-filter, param-filter, is-not-defined and text-match, with every attribute. Every `test` is sent explicitly, so the schema default never applies, and the default filter stays the empty `allof`. Text is escaped as content, names and collations through the new `escape_attr`.

**Listing takes options.** `CarddavCardList::new` and `list_cards` take `CarddavCardListOptions { filter, limit }`, the limit sent as `C:limit` (§8.6.1). Enumeration keeps the match-all filter, since it feeds sync.

**A truncated listing says so.** The server's 507 row names the collection itself, which the self-entry skip discarded, so a capped listing read as complete. `CarddavCardListOk` now carries `truncated`, the twin of `CarddavCardEnumOk`.

**A refused filter is named.** `supported-filter` and `supported-collation` surface as `WebdavSendError::UnsupportedFilter`, matched on the element. The element reader moved into `has_element`, shared with `DuplicateUid`.

Released in 0.4.0, breaking. Capabilities moved: carddav (MODIFIED: Listing, Match-all filter; ADDED: Query filter, Query limit, Unsupported filter refusal). Offline coverage grew by three tests: the exact XML of every condition with escaping and the limit, a truncated listing, and both preconditions beside a bare 403 and a `supported-report` refusal keeping their own errors.
