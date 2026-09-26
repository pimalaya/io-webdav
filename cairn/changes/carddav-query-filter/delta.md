---
cairn: change
change: carddav-query-filter
---

# Delta

## ADDED Requirements

### Requirement: Query filter
Listing SHALL accept a filter modelling the whole RFC 6352 §10.5 grammar: a top-level test over prop-filters, each prop-filter matching on is-not-defined or on text-matches and param-filters, each text-match carrying its value, match type, negation and optional collation. Every test SHALL be sent explicitly, never left to the schema default. Text SHALL be escaped as element content and names and collations as attribute values.

### Requirement: Query limit
Listing SHALL accept an optional result limit, sent as the RFC 6352 §8.6.1 limit element after the filter.

### Requirement: Unsupported filter refusal
A query refused with the `supported-filter` or `supported-collation` precondition (RFC 6352 §8.6) SHALL surface as the dedicated unsupported-filter error, matched on the element whatever status wraps it.

## MODIFIED Requirements

### Requirement: Listing
Listing SHALL use an addressbook-query REPORT at Depth 1, requesting the ETag and the address data, with the caller's filter and limit. The vCard payload SHALL be returned as raw bytes, parsed upstream. A listing SHALL report whether the server truncated it with a 507 row, which the self-entry skip would otherwise discard.

### Requirement: Match-all filter
The default filter SHALL be an empty allof. RFC 6352 section 8.6 requires the filter element, and strict servers reject a missing one with HTTP 400 while treating an empty anyof, the schema default, as matching nothing. Enumeration SHALL always use it.
