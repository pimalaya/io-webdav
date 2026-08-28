---
cairn: change
id: enumeration-without-sync-collection
status: landed
created: 2026-08-28
---

# Enumeration survives a server with no `sync-collection`

## Why

RFC 6578 is an extension and a deployment may implement none of it. A live
server was found doing exactly that: its `supported-report-set` on an address
book holds

```xml
<d:supported-report><d:report><card:addressbook-multiget/></d:report></d:supported-report>
<d:supported-report><d:report><card:addressbook-query/></d:report></d:supported-report>
```

plus the three principal reports, and no `sync-collection` anywhere. The REPORT
itself, sent both with and without a trailing slash on the collection URL,
answers

```xml
<d:error xmlns:d="DAV:"><s:exception>…ReportNotSupported</s:exception><d:supported-report/></d:error>
```

`DAV:supported-report` is the RFC 3253 section 3.6 precondition for exactly this,
and both URL spellings failing identically rules out a malformed request.

The crate already recognises one refusal and gives it a name: a rejected sync
token surfaces as `InvalidSyncToken`, "so the consumer can fall back to a full
enumeration" (spec, sync, Invalid token). A server that never had the report at
all gets no such treatment: it surfaces as an undifferentiated send failure, and
every consumer that meets one is left to sniff HTTP statuses. Neverest currently
does, and a status is the wrong thing to match on, since which status wraps the
precondition is the server's choice.

This is protocol knowledge, so it belongs here rather than in each consumer.
Every one of them enumerates the same collections and hits the same wall.

**The query is the wrong alternative, which only a live server showed.** Both
card enumerations this crate offers, `list_cards` and `enum_cards`, are
`addressbook-query` REPORTs (spec, carddav, Listing and Enumeration). A query
carries a filter, and a server evaluates a filter by parsing every card it holds.
The same server that refuses `sync-collection` answers the query with

```
HTTP 500 Sabre\VObject\ParseException
Invalid VObject, line 1 did not follow the icalendar/vcard format
```

One malformed card in the collection takes the whole enumeration down, and there
is no way to enumerate around it: every REPORT this crate can send parses. A
`PROPFIND` at Depth 1 requesting `getetag` does not, listing resources and their
ETags out of the store without reading a single body, so it is the enumeration
that survives a collection holding a card the server itself cannot parse. The
crate has `WebdavPropfind` but exposes no card enumeration built on it.

That also makes the query a poor fallback on its own terms: enumeration is
supposed to be the cheap half of a sync, and asking a server to parse every card
to hand back a list of ETags is the opposite.

**A third gap surfaces with the fallback.**
 `sync-collection` reports
truncation through its 507 row and the consumer drains it (spec, sync,
Truncation). The `addressbook-query` enumeration has no equivalent: 507 is
parsed nowhere outside `rfc6578`. A consumer that treats a query result as a
complete snapshot, which is the only way to detect removals without a token,
would read a truncated listing as a mass deletion.

## What

- A dedicated error for the refusal, `UnsupportedReport`, raised on the
  `DAV:supported-report` precondition and on the `405` and `501` statuses that
  mean the request was never going to run, alongside the existing
  `InvalidSyncToken`.
- A `PROPFIND` card enumeration at Depth 1 requesting `getetag`, built on the
  existing `WebdavPropfind`. It is the only enumeration that does not ask the
  server to parse the collection, so it is the only one that survives a card the
  server cannot parse, and it is cheaper than a query besides.
- One enumeration entry point per collection type, taking a caller-fed flag in
  its options exactly as the io-imap SORT fallback does: unset runs
  `sync-collection`, set runs the `PROPFIND`. The crate implements both paths
  and the consumer chooses, so a library never trades an incremental delta for a
  full listing behind a caller's back.
- The `supported-report-set` of a collection, read during listing and cached
  beside the existing discovery cache, so the flag can be fed from a capability
  check rather than from a failed request.
- Truncation on the query path: a 507 row sets the same truncated flag the sync
  report already carries, so an incomplete listing is never mistaken for a
  complete one.

## Not in scope

**No automatic retry.** Catching the refusal and re-running the other report
inside the crate is what makes the trade invisible, which is the thing the
io-imap precedent deliberately avoids. The error and the capability are exposed;
the choice stays with the consumer.

**No repair of the malformed card.** A collection holding a resource its own
server cannot parse is the operator's problem, and this crate has no business
rewriting it. Enumerating past it is the whole ask: the card is listed, its
body fetch fails on its own, and the rest of the collection syncs.

**No CalDAV-only or CardDAV-only shape.** `sync-collection` is protocol-neutral
and lives under `sync`, so the refusal and the capability read belong there too,
with each collection type naming its own query as the alternative.
