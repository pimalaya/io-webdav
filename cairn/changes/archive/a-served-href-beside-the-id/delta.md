---
cairn: delta
change: a-served-href-beside-the-id
---

# Delta

## ADDED Requirements

### Requirement: A served href beside the id
(webdav-core) Every listed, enumerated or multiget item and card, every unreadable row, every listed calendar and address book, and the outcome of an item or card create, update or move SHALL carry beside its id its href as served: the `<href>` of its response, or for a write the `Location` header when its last segment names the resource, the path of the URL sent otherwise, the `Destination` URL for a move. The href SHALL keep the server's spelling, escapes and raw bytes alike, so two segments decoding to one id stay apart. A collection given to a create SHALL be addressed by its id, its href ignored.

#### Scenario: Two spellings of one name
- GIVEN a home set listing `My%20Cal/` and, from a non-conforming server, `My Cal/`
- WHEN the calendars are listed
- THEN both carry the id `My Cal`, and each its own href, `My%20Cal/` and `My Cal/`

#### Scenario: A create the server does not relocate
- GIVEN an item created as `My Cal.ics` and answered without a `Location` header
- WHEN the create completes
- THEN it carries the id `My Cal.ics` and the href `<calendar>/My%20Cal.ics`, the path sent

## MODIFIED Requirements

### Requirement: Calendar properties
(caldav) A listed calendar SHALL carry its id, its href as served (A served href beside the id, webdav-core), display name, description, color, component set, ctag, sync token, default time zone and the reports the server advertises for it. Every property the listing reads SHALL also be writable on create, the advertised report set excepted, which RFC 3253 section 3.1.5 protects, so a calendar round-trips through create; an update SHALL write all of them but the component set (Calendar update).

### Requirement: Address book properties
(carddav) A listed address book SHALL carry its id, its href as served (A served href beside the id, webdav-core), display name, description, color, ctag, sync token and the reports the server advertises for it.

## REMOVED Requirements

None.
