---
cairn: log
change: enumeration-without-sync-collection
landed: 2026-08-28
---

# Enumeration survives a server with no `sync-collection`

RFC 6578 is an extension, and a live CardDAV server was found implementing none of it: its `supported-report-set` on an address book advertises `addressbook-multiget` and `addressbook-query` plus the three principal reports, and the `sync-collection` REPORT, sent both with and without a trailing slash, answers `ReportNotSupported` with the RFC 3253 §3.6 `DAV:supported-report` precondition wrapped in a 403. Until now that surfaced as an undifferentiated send failure, leaving every consumer to sniff HTTP statuses, which neverest had started doing. This lands the protocol knowledge here, where every consumer meets the same wall.

**The refusal has a name.** `WebdavReport` classifies its send failure: the precondition in the body, whatever status carries it, plus `405` and `501` on the status alone, become `WebdavSendError::UnsupportedReport`. The precondition is what is matched, not the status, since a `403` is equally the answer to a genuine permission refusal. `WebdavSyncCollection` maps it onto its own `UnsupportedReport`, a peer of the `InvalidSyncToken` it already named, and the client layer recognises both through `WebdavClientStdError::is_unsupported_report`, the enumeration nesting its send one level deeper than a plain request.

**The alternative is a `PROPFIND`, not the query.** Both card enumerations this crate offers are `addressbook-query` REPORTs, and a query carries a filter a server evaluates by parsing every card it holds. The same server answers the query with an HTTP 500 `Sabre\VObject\ParseException`: one malformed card takes the whole enumeration down, and no REPORT here can enumerate around it. `WebdavSyncCollectionOptions::fallback` therefore runs a `PROPFIND` at Depth 1 requesting the ETag, which reads names and ETags out of the store, parses nothing, and returns the same `WebdavSyncDelta` with no token. Both paths live in the same coroutine, as the io-imap SORT fallback does; the crate never chooses between them, an incremental delta traded for a full listing being the consumer's call.

**The capability is read while listing.** `SUPPORTED_REPORT_SET` joined both `LIST_PROPS` and `SYNC_COLLECTION` names the report a consumer looks for there, so `CaldavCalendar` and `CarddavAddressbook` carry the reports their server advertises, and the client caches them per collection in `calendar_reports` and `addressbook_reports`, beside the principal and home sets. The flag can be fed from that rather than from a request that has already failed. Reading it needed the multistatus parser to keep property markup deeper than one level: `WebdavPropChild` gained its own `children`, the report names sitting three levels under the property.

**Truncation reached the query path.** A 507 row (RFC 6578 §3.6) was parsed only by the sync report. `CaldavItemEnum` and `CarddavCardEnum` now return a `CaldavItemEnumOk` and a `CarddavCardEnumOk` carrying the same `truncated` flag, since a full enumeration is how removals are detected without a token and a truncated one read as a snapshot looks like a mass deletion.

Capabilities moved: sync (five new requirements), caldav and carddav (the collection listings carry the advertised report set), client (the discovery cache holds it). Offline coverage grew by nine tests, the refusal being told apart from a permission refusal, a credential failure, a server fault and a stale token, using the body the real server answers with.
