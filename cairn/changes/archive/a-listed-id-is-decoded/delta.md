---
cairn: delta
change: a-listed-id-is-decoded
---

# Delta

## ADDED Requirements

### Requirement: A listed segment is decoded
(webdav-core) An id read from a path segment, the last segment of a listed or multiget href, of a Location header or of the URL a create, update or move sent, SHALL be that segment percent-decoded when the decoding is UTF-8, passes the item and card id check, and percent-encodes, as A relative id is sent percent-encoded says, back to the very segment; any other segment SHALL be the id as the server served it. A segment whose decoding holds a `/` or an escape, one escaping a byte the encoder writes raw (an unreserved byte, a sub-delimiter, `:` or `@`), one spelled with lowercase hex digits and one whose decoding the check refuses SHALL therefore stay as served. Either way the id addresses the segment it was read from, as does the segment itself.

#### Scenario: An address book named with a space
- GIVEN an address book created as `My Book`, which the server lists as `My%20Book`
- WHEN the address books are listed
- THEN it carries the id `My Book`, which addresses `<home>/My%20Book/` as the create did

#### Scenario: A decoded slash
- GIVEN a listed card whose href ends in `a%2Fb.vcf`
- WHEN the cards are listed
- THEN the card carries the id `a%2Fb.vcf`, where `a/b.vcf` would address another path

## MODIFIED Requirements

### Requirement: Resource identity
(webdav-core) An item or card id SHALL be the last non-empty path segment of its href, read as A listed segment is decoded says. A collection's id SHALL follow its layer's identity rule (Calendar identity, Address book identity). The crate SHALL never add nor strip a file extension anywhere in that derivation. A create, update or move SHALL return the id the last segment of the URL it sent gives, read the same way, which is the form a listing returns, never the caller's string as given. The crate SHALL offer an encoder turning a name the caller mints into such a segment, every byte but the RFC 3986 unreserved ones escaped; an encoded name SHALL be sent as it stands.

#### Scenario: A name needing escapes
- GIVEN a create under a name holding a space and a non-ASCII letter
- WHEN the server answers without a Location header
- THEN the request addresses the percent-encoded segment, and the returned id is the name itself, as a listing of that segment returns it

### Requirement: A relative id is sent percent-encoded
(webdav-core) The collection path builder and the item and card path builders SHALL percent-encode each byte of a relative id that an RFC 3986 path segment cannot hold raw, any byte but the unreserved ones, the sub-delimiters, `:` and `@`, keeping `/` as the segment separator and an escape already there (`%` and two hex digits) as it stands. An id read from an href SHALL be joined into the very segment it was read from, and the id's percent-decoded form SHALL stay unchanged, so the id checks judge the path sent. A collection id starting with a slash SHALL be taken as the server gave it.

#### Scenario: An address book named with a space
- GIVEN the collection id `My Book`
- WHEN the client creates the address book
- THEN the MKCOL addresses `<home>/My%20Book/`, the path the listed id `My Book` addresses too, as does `My%20Book`

#### Scenario: A backslash in an id
- GIVEN the collection id `a\b`
- WHEN the client deletes the address book
- THEN the DELETE addresses `<home>/a%5Cb/`, not the nested collection `<home>/a/b/`

### Requirement: Calendar identity
(caldav) A listed calendar whose collection is a direct child of the home set SHALL carry the last path segment of its href as its id, read as A listed segment is decoded (webdav-core) says. Any other listed calendar SHALL carry its href's path as its id, starting with a slash, percent-encoding kept as the server served it. The home set and the href SHALL be compared percent-decoded. A calendar id starting with a slash SHALL address that path; any other id SHALL address the home set joined with it.

#### Scenario: Calendars outside the home set
- GIVEN a home set /caldav/v2/alice/ listing /caldav/v2/alice/events/ and /caldav/v2/team/events/
- WHEN the caller lists the calendars
- THEN the first carries the id events and the second the id /caldav/v2/team/events
- AND a write into the second addresses /caldav/v2/team/events/, never the home set's events

### Requirement: Item identity
(caldav) An item SHALL be addressed by its resource id, the href's last path segment read as A listed segment is decoded (webdav-core) says. The crate SHALL NOT append nor strip the .ics extension, so the caller owns the whole resource name and an id read from a listing addresses the resource it came from.

#### Scenario: Listed id round-trips
- GIVEN a listing returning an item whose href last segment carries no extension
- WHEN the caller reads that id back
- THEN the GET targets the very resource the server enumerated

### Requirement: Card identity
(carddav) A card SHALL be addressed by its resource id, the href's last path segment read as A listed segment is decoded (webdav-core) says. The crate SHALL NOT append nor strip the .vcf extension, so the caller owns the whole resource name and an id read from a listing addresses the resource it came from.

#### Scenario: Suffixing server
- GIVEN a server that stores cards under a .vcf name and enumerates them that way
- WHEN the caller reads, updates or deletes a listed id
- THEN every verb targets the enumerated resource, with no extension added or stripped in between

## REMOVED Requirements

None.
