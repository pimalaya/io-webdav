---
cairn: delta
change: a-report-body-comes-back-whole
---

# Delta

## ADDED Requirements

### Requirement: A card comes back whole
(carddav) A card listing and a card multiget SHALL return the whole text of `address-data` as the card's bytes, its references resolved and its line ends normalised as XML 1.0 requires, nothing trimmed: RFC 6352 §10.4 makes that PCDATA the address object itself, whitespace around the vCard included. Blank text SHALL count as empty data.

#### Scenario: A card ending in an escaped CR
- GIVEN an `addressbook-multiget` answer whose `address-data` ends `END:VCARD&#13;` and a line feed
- WHEN the card is read
- THEN its bytes end `END:VCARD` and a CRLF, as a GET of the card returns them

### Requirement: An item comes back whole
(caldav) An item listing and an item multiget SHALL return the whole text of `calendar-data` as the item's bytes, its references resolved and its line ends normalised as XML 1.0 requires, nothing trimmed: RFC 4791 §9.6 makes that PCDATA the iCalendar object itself, whitespace around it included. Blank text SHALL count as empty data.

#### Scenario: An item ending in an escaped CR
- GIVEN a `calendar-multiget` answer whose `calendar-data` ends `END:VCALENDAR&#13;` and a line feed
- WHEN the item is read
- THEN its bytes end `END:VCALENDAR` and a CRLF, as a GET of the item returns them

### Requirement: A parsed property keeps its whole text
(webdav-core) The vocabulary-agnostic parser SHALL keep each property's whole text, references resolved and line ends normalised, on the parsed property; its text helper SHALL return it trimmed.

## MODIFIED Requirements

None.

## REMOVED Requirements

None.
