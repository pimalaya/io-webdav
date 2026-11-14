---
cairn: delta
change: a-listing-returns-what-it-could-not-read
---

# Delta

## ADDED Requirements

### Requirement: A listing returns the rows it could not read
(carddav, caldav) A card or item listing, and a card or item multiget, SHALL return beside its entries every response naming a resource that carried no data: a response-level status other than 2xx, a refused `address-data` or `calendar-data` property, or empty data. Each such row SHALL carry the resource's id and href, the response-level status and the refused properties with their status. A self-entry (an href ending in `/`), a response naming no id and a 507 truncation row SHALL be neither an entry nor a row.

#### Scenario: A card the server refuses to show
- GIVEN an address book whose `addressbook-query` answer holds two cards and a row for `c3.vcf` with `<status>HTTP/1.1 403 Forbidden</status>`
- WHEN the cards are listed
- THEN the two cards are entries and `c3.vcf` is a row carrying status 403

## MODIFIED Requirements

### Requirement: Listing
(caldav) Listing SHALL use a calendar-query REPORT at Depth 1, requesting the ETag and the calendar data. The caller SHALL pass the optional VCALENDAR child filter, an empty string listing every component type. The iCalendar payload SHALL be returned as raw bytes, parsed upstream. A listing SHALL report whether the server truncated it with a 507 row, which the self-entry skip would otherwise discard, and SHALL return the rows it could not read.

### Requirement: Listing
(carddav) Listing SHALL use an addressbook-query REPORT at Depth 1, requesting the ETag and the address data, with the caller's filter and limit. The vCard payload SHALL be returned as raw bytes, parsed upstream. A listing SHALL report whether the server truncated it with a 507 row, which the self-entry skip would otherwise discard, and SHALL return the rows it could not read.

## REMOVED Requirements

None.
