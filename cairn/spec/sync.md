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
