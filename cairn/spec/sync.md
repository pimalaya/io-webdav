---
cairn: spec
capability: sync
status: current
---

# Collection synchronization

The RFC 6578 layer: an incremental delta against a collection, keyed by a sync token. It is protocol-neutral, so calendars and address books both use it.

### Requirement: Sync collection REPORT
The crate SHALL provide a sync-collection REPORT coroutine taking a collection path, an optional sync token and the properties to request on each changed member. Depth SHALL be pinned to 0 as RFC 6578 section 3.3 requires, the scope being carried by the sync-level element instead.

### Requirement: Initial sync
An absent sync token SHALL emit an empty sync-token element, which the server answers with the full member set plus a first checkpoint.

### Requirement: Delta shape
The report SHALL return the changed members (href plus ETag), the hrefs of vanished members, the next sync token, and whether the server truncated the result set.

### Requirement: Removals
A member removed since the request token SHALL be reported as vanished. On the wire it is a response row carrying an href and a 404 response-level status with no propstat at all, so the parser SHALL keep such rows rather than drop them.

### Requirement: Truncation
A 507 row (RFC 6578 section 3.6) SHALL set the truncated flag, telling the consumer to run the report again from the returned token to drain the rest.

### Requirement: Invalid token
A server rejecting the sync token with a 403 carrying the valid-sync-token precondition SHALL surface as a dedicated error, distinct from any other send failure, so the consumer can fall back to a full enumeration.

#### Scenario: Token expired server-side
- GIVEN a stored sync token the server no longer recognises
- WHEN the consumer runs the incremental report
- THEN it receives the invalid-sync-token error and re-enumerates instead of silently seeing an empty delta

### Requirement: Checkpoint properties
Both collection types SHALL carry a sync token property in their listing, alongside the CalendarServer ctag, so an incremental sync has a checkpoint to start from without a prior full pass.

### Requirement: Unsupported report
A server answering a REPORT with the RFC 3253 section 3.6 supported-report precondition SHALL surface as a dedicated error, distinct from any other send failure and from the invalid sync token, so the consumer can enumerate another way. The 405 and 501 statuses SHALL raise it too, both meaning the request was never going to run.

The precondition SHALL be what is matched, not the status carrying it: a server chooses which status wraps it, and one was found using 403, which is also the status of a genuine permission refusal. A refusal for lack of privileges SHALL NOT raise this error.

#### Scenario: A server without the extension
- GIVEN a collection whose supported-report-set omits sync-collection
- WHEN the consumer runs the incremental report against it
- THEN it receives the unsupported-report error rather than an opaque send failure

### Requirement: Supported report set
A collection listing SHALL return the reports the server advertises for that collection, read from the supported-report-set property, and the client SHALL cache it beside the principal and home set it already caches. A consumer SHALL be able to choose its enumeration from that set without paying a failed request first.

### Requirement: Caller-chosen enumeration
Each collection type SHALL expose one enumeration entry point taking a flag in its options: unset runs the sync-collection report, set runs a PROPFIND at Depth 1 and returns no token. The crate SHALL implement both and SHALL NOT choose between them, an incremental delta traded for a full listing being the consumer's decision and not a library's.

#### Scenario: A consumer keeps the choice
- GIVEN a server that implements neither the report nor a token
- WHEN the consumer enumerates with the flag set
- THEN it receives the full member set with no token, and knows it did

### Requirement: Enumeration without parsing
A fallback enumeration SHALL use a PROPFIND at Depth 1 requesting the ETag, not a query REPORT. A query carries a filter, a server evaluates a filter by parsing every member, and a collection holding one member the server cannot parse then fails to enumerate at all rather than enumerating past it. A PROPFIND reads resource names and ETags out of the store and parses nothing, so it survives such a collection and costs the server less besides.

#### Scenario: A collection holding an unparseable member still enumerates
- GIVEN a collection holding one resource the server cannot parse
- WHEN the consumer enumerates with the flag set
- THEN every member is listed, and only that member fails when its body is fetched

### Requirement: Query truncation
A 507 row in a query enumeration SHALL set the same truncated flag the sync-collection report carries. A consumer reading a query result as a complete snapshot, which is how removals are detected without a token, SHALL therefore never mistake a truncated listing for a complete one.

#### Scenario: A truncated listing is not a mass deletion
- GIVEN a server truncating a query enumeration
- WHEN the consumer enumerates
- THEN the result is flagged truncated rather than presented as the whole collection
