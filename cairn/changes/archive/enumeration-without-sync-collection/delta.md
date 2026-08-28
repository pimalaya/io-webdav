---
cairn: change
change: enumeration-without-sync-collection
---

# Delta

## ADDED Requirements

### Requirement: Unsupported report
A server answering a REPORT with the RFC 3253 section 3.6 supported-report
precondition SHALL surface as a dedicated error, distinct from any other send
failure and from the invalid sync token, so the consumer can enumerate another
way. The `405` and `501` statuses SHALL raise it too, both meaning the request
was never going to run.

The precondition SHALL be what is matched, not the status carrying it: a server
chooses which status wraps it, and one was found using `403`, which is also the
status of a genuine permission refusal. A refusal for lack of privileges SHALL
NOT raise this error.

#### Scenario: A server without the extension
- GIVEN a collection whose supported-report-set omits sync-collection
- WHEN the consumer runs the incremental report against it
- THEN it receives the unsupported-report error rather than an opaque send failure

### Requirement: Supported report set
A collection listing SHALL return the reports the server advertises for that
collection, read from the supported-report-set property, and the client SHALL
cache it beside the principal and home set it already caches. A consumer SHALL
be able to choose its enumeration from that set without paying a failed request
first.

### Requirement: Caller-chosen enumeration
Each collection type SHALL expose one enumeration entry point taking a flag in
its options: unset runs the sync-collection report, set runs a `PROPFIND` at Depth 1 and
returns no token. The crate SHALL implement both and SHALL
NOT choose between them, an incremental delta traded for a full listing being
the consumer's decision and not a library's.

#### Scenario: A consumer keeps the choice
- GIVEN a server that implements neither the report nor a token
- WHEN the consumer enumerates with the flag set
- THEN it receives the full member set with no token, and knows it did

### Requirement: Enumeration without parsing
A fallback enumeration SHALL use a `PROPFIND` at Depth 1 requesting the ETag,
not a query REPORT. A query carries a filter, a server evaluates a filter by
parsing every member, and a collection holding one member the server cannot
parse then fails to enumerate at all rather than enumerating past it. A
`PROPFIND` reads resource names and ETags out of the store and parses nothing,
so it survives such a collection and costs the server less besides.

#### Scenario: A collection holding an unparseable member still enumerates
- GIVEN a collection holding one resource the server cannot parse
- WHEN the consumer enumerates with the flag set
- THEN every member is listed, and only that member fails when its body is fetched

### Requirement: Query truncation
A 507 row in a query enumeration SHALL set the same truncated flag the
sync-collection report carries. A consumer reading a query result as a complete
snapshot, which is how removals are detected without a token, SHALL therefore
never mistake a truncated listing for a complete one.

#### Scenario: A truncated listing is not a mass deletion
- GIVEN a server truncating a query enumeration
- WHEN the consumer enumerates
- THEN the result is flagged truncated rather than presented as the whole collection

## MODIFIED Requirements

### Requirement: Address book properties
A listed address book SHALL carry its id, display name, description, color, ctag, sync token and the reports the server advertises for it.

### Requirement: Calendar properties
A listed calendar SHALL carry its id, display name, description, color, component set, ctag, sync token, default time zone and the reports the server advertises for it. Every property the listing reads SHALL also be writable, the advertised report set excepted, which RFC 3253 section 3.1.5 protects, so a calendar round-trips through create and update.

### Requirement: Discovery cache
The client SHALL cache the principal URL and both home sets, plus the reports each listed collection advertises. Each discovery step SHALL resolve the previous one when it is not cached. A method needing a home set that was never resolved SHALL fail with a dedicated missing-cache error rather than guess a path.
