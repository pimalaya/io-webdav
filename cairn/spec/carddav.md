---
cairn: spec
capability: carddav
status: current
---

# CardDAV

The RFC 6352 layer: address book collections and the address object resources they hold, called cards throughout the crate. It is shape-for-shape the twin of the CalDAV layer.

### Requirement: Address book collections
The crate SHALL provide list, create, update and delete coroutines for address book collections. Creation SHALL use the extended MKCOL.

### Requirement: Address book properties
A listed address book SHALL carry its id, display name, description, color, ctag and sync token.

### Requirement: Card verbs
The crate SHALL provide read, create, update and delete coroutines for cards, plus list, ETag-only enumeration and batch multiget.

### Requirement: Card identity
A card SHALL be addressed by its resource id, the href's last path segment used verbatim. The crate SHALL NOT append nor strip the .vcf extension, so the caller owns the whole resource name and an id read from a listing addresses the resource it came from.

#### Scenario: Suffixing server
- GIVEN a server that stores cards under a .vcf name and enumerates them that way
- WHEN the caller reads, updates or deletes a listed id
- THEN every verb targets the enumerated resource, with no extension added or stripped in between

### Requirement: Listing
Listing SHALL use an addressbook-query REPORT at Depth 1, requesting the ETag and the address data. The vCard payload SHALL be returned as raw bytes, parsed upstream.

### Requirement: Match-all filter
The query body SHALL carry an empty allof filter. RFC 6352 section 8.6 requires the filter element, and strict servers reject a missing one with HTTP 400 while treating an empty anyof, the schema default, as matching nothing.

### Requirement: Enumeration
Enumeration SHALL use the same addressbook-query REPORT requesting the ETag only, returning id and ETag rows with no body.

### Requirement: Batch fetch
Batch fetch SHALL use an addressbook-multiget REPORT (RFC 6352 section 8.7) with Depth pinned to 0, the only value the RFC defines for it.

### Requirement: Self-entry filtering
An entry whose href ends in a slash SHALL be skipped: it is the collection echoing itself, which some servers include in a query response.

### Requirement: Preconditions
Creation SHALL send If-None-Match with a star. Update and delete SHALL accept an optional If-Match.

### Requirement: Home set
The address book home set SHALL be discovered from the principal URL via the addressbook-home-set property (RFC 6352 section 7.1.1).
