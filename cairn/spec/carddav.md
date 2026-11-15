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
A listed address book SHALL carry its id, display name, description, color, ctag, sync token and the reports the server advertises for it.

### Requirement: Address book identity
A listed address book SHALL carry its id by the rule calendars follow, and an address book id starting with a slash SHALL address that path.

### Requirement: Card verbs
The crate SHALL provide read, create, update and delete coroutines for cards, plus list, ETag-only enumeration and batch multiget.

### Requirement: Card identity
A card SHALL be addressed by its resource id, the href's last path segment read as A listed segment is decoded (webdav-core) says. The crate SHALL NOT append nor strip the .vcf extension, so the caller owns the whole resource name and an id read from a listing addresses the resource it came from.

#### Scenario: Suffixing server
- GIVEN a server that stores cards under a .vcf name and enumerates them that way
- WHEN the caller reads, updates or deletes a listed id
- THEN every verb targets the enumerated resource, with no extension added or stripped in between

### Requirement: Listing
Listing SHALL use an addressbook-query REPORT at Depth 1, requesting the ETag and the address data, with the caller's filter and limit. The vCard payload SHALL be returned as raw bytes, parsed upstream. A listing SHALL report whether the server truncated it with a 507 row, which the self-entry skip would otherwise discard, and SHALL return the rows it could not read.

### Requirement: Match-all filter
The default filter SHALL be an empty allof. RFC 6352 section 8.6 requires the filter element, and strict servers reject a missing one with HTTP 400 while treating an empty anyof, the schema default, as matching nothing. Enumeration SHALL always use it.

### Requirement: Query filter
Listing SHALL accept a filter modelling the whole RFC 6352 §10.5 grammar: a top-level test over prop-filters, each prop-filter matching on is-not-defined or on text-matches and param-filters, each text-match carrying its value, match type, negation and optional collation. Every test SHALL be sent explicitly, never left to the schema default. Text SHALL be escaped as element content and names and collations as attribute values.

### Requirement: Query limit
Listing SHALL accept an optional result limit, sent as the RFC 6352 §8.6.1 limit element after the filter.

### Requirement: Unsupported filter refusal
A query refused with the `supported-filter` or `supported-collation` precondition (RFC 6352 §8.6) SHALL surface as the dedicated unsupported-filter error, matched on the element whatever status wraps it.

### Requirement: A listing returns the rows it could not read
A card listing and a card multiget SHALL return beside their entries every response naming a resource that carried no data: a response-level status other than 2xx, a refused `address-data` property, or empty data. Each such row SHALL carry the resource's id and href, the response-level status and the refused properties with their status. A self-entry (an href ending in `/`), a response naming no id and a 507 truncation row SHALL be neither an entry nor a row.

#### Scenario: A card the server refuses to show
- GIVEN an address book whose `addressbook-query` answer holds two cards and a row for `c3.vcf` with `<status>HTTP/1.1 403 Forbidden</status>`
- WHEN the cards are listed
- THEN the two cards are entries and `c3.vcf` is a row carrying status 403

### Requirement: A card comes back whole
A card listing and a card multiget SHALL return the whole text of `address-data` as the card's bytes, its references resolved and its line ends normalised as XML 1.0 requires, nothing trimmed: RFC 6352 §10.4 makes that PCDATA the address object itself, whitespace around the vCard included. Blank text SHALL count as empty data.

#### Scenario: A card ending in an escaped CR
- GIVEN an `addressbook-multiget` answer whose `address-data` ends `END:VCARD&#13;` and a line feed
- WHEN the card is read
- THEN its bytes end `END:VCARD` and a CRLF, as a GET of the card returns them

### Requirement: Enumeration
Enumeration SHALL use the same addressbook-query REPORT requesting the ETag only, returning id and ETag rows with no body.

### Requirement: Batch fetch
Batch fetch SHALL use an addressbook-multiget REPORT (RFC 6352 section 8.7) with Depth pinned to 0, the only value the RFC defines for it.

### Requirement: Self-entry filtering
An entry whose href ends in a slash SHALL be skipped: it is the collection echoing itself, which some servers include in a query response.

### Requirement: Preconditions
Creation SHALL send If-None-Match with a star. Update and delete SHALL accept an optional If-Match. A write refused with the CARDDAV:no-uid-conflict precondition SHALL surface as the dedicated duplicate-uid error, which is a different refusal from a resource name already taken and keeps its own signal.

### Requirement: Home set
The address book home set SHALL be discovered from the principal URL via the addressbook-home-set property (RFC 6352 section 7.1.1).
