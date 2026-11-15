---
cairn: spec
capability: caldav
status: current
---

# CalDAV

The RFC 4791 layer: calendar collections and the calendar object resources they hold, called items throughout the crate. It is shape-for-shape the twin of the CardDAV layer.

### Requirement: Calendar collections
The crate SHALL provide list, create, update and delete coroutines for calendar collections. Creation SHALL use MKCALENDAR rather than the extended MKCOL, which CalDAV servers require for calendars.

### Requirement: Calendar properties
A listed calendar SHALL carry its id, its href as served (A served href beside the id, webdav-core), display name, description, color, component set, ctag, sync token, default time zone and the reports the server advertises for it. Every property the listing reads SHALL also be writable on create, the advertised report set excepted, which RFC 3253 section 3.1.5 protects, so a calendar round-trips through create; an update SHALL write all of them but the component set (Calendar update).

### Requirement: Calendar color
A calendar's color SHALL be written as `calendar-color` in the `http://apple.com/ns/ical/` namespace, by a create and an update alike. A listing SHALL request that property and `calendar-color` in the `http://inf-it.com/ns/ab/` namespace, and SHALL read the first, the second when the first is absent or empty, whatever order the server answers them in. An address book's color SHALL stay `addressbook-color` in the inf-it namespace.

#### Scenario: A calendar carrying both colors
- GIVEN a calendar holding an inf-it `calendar-color` and an Apple one, the server answering the inf-it one first
- WHEN the calendars are listed
- THEN the calendar's color is the Apple one

### Requirement: Calendar identity
A listed calendar whose collection is a direct child of the home set SHALL carry the last path segment of its href as its id, read as A listed segment is decoded (webdav-core) says. Any other listed calendar SHALL carry its href's path as its id, starting with a slash, percent-encoding kept as the server served it. The home set and the href SHALL be compared percent-decoded. A calendar id starting with a slash SHALL address that path; any other id SHALL address the home set joined with it.

#### Scenario: Calendars outside the home set
- GIVEN a home set /caldav/v2/alice/ listing /caldav/v2/alice/events/ and /caldav/v2/team/events/
- WHEN the caller lists the calendars
- THEN the first carries the id events and the second the id /caldav/v2/team/events
- AND a write into the second addresses /caldav/v2/team/events/, never the home set's events

### Requirement: Component set
The component types a calendar holds SHALL be read from and written to supported-calendar-component-set (RFC 4791 section 5.2.3), whose value is a list of comp children carrying a name attribute. An empty set means the server advertises no restriction, which the RFC defines as accepting any type. A server fixes the set at creation time, so setting it is only meaningful on create.

### Requirement: Calendar update
A calendar update SHALL take a partial update in which each of the display name, description, color and default time zone is left alone, removed (`DAV:remove`, RFC 4918 section 9.2) or set (`DAV:set`), all sent in one PROPPATCH. A removed property SHALL be verified as a set one is (A property update is verified, not assumed). The component set SHALL be no part of it, RFC 4791 section 5.2.3 protecting it once the calendar exists.

#### Scenario: A cleared description
- GIVEN a calendar with a name, a description and a color
- WHEN the caller updates it, setting the name and removing the description
- THEN one PROPPATCH sets the name and removes the description, and the color appears in neither

### Requirement: Item verbs
The crate SHALL provide read, create, update, delete and move coroutines for calendar items, plus list, ETag-only enumeration and batch multiget.

### Requirement: Item identity
An item SHALL be addressed by its resource id, the href's last path segment read as A listed segment is decoded (webdav-core) says. The crate SHALL NOT append nor strip the .ics extension, so the caller owns the whole resource name and an id read from a listing addresses the resource it came from.

#### Scenario: Listed id round-trips
- GIVEN a listing returning an item whose href last segment carries no extension
- WHEN the caller reads that id back
- THEN the GET targets the very resource the server enumerated

### Requirement: Item move
The crate SHALL provide a coroutine moving a calendar item into another calendar of the same server with MOVE (RFC 4918 section 9.9), sending Overwrite F and, when the caller names an ETag, If-Match. The moved item SHALL keep its resource id. A refusal naming the no-uid-conflict or unique-scheduling-object-resource precondition SHALL be the duplicate-UID error, as for a create.

#### Scenario: A task filed into another calendar
- GIVEN a server keeping one UID per account
- WHEN the caller moves an item from one calendar into another
- THEN the server moves the resource itself, and no request ever puts a second resource with that UID

### Requirement: Listing
Listing SHALL use a calendar-query REPORT at Depth 1, requesting the ETag and the calendar data. The caller SHALL pass the optional VCALENDAR child filter, an empty string listing every component type. The iCalendar payload SHALL be returned as raw bytes, parsed upstream. A listing SHALL report whether the server truncated it with a 507 row, which the self-entry skip would otherwise discard, and SHALL return the rows it could not read.

### Requirement: A listing returns the rows it could not read
An item listing and an item multiget SHALL return beside their entries every response naming a resource that carried no data: a response-level status other than 2xx, a refused `calendar-data` property, or empty data. Each such row SHALL carry the resource's id and href, the response-level status and the refused properties with their status. A self-entry (an href ending in `/`), a response naming no id and a 507 truncation row SHALL be neither an entry nor a row.

#### Scenario: An item the server refuses to show
- GIVEN a calendar whose `calendar-query` answer holds two items and a row for `e3.ics` with `<status>HTTP/1.1 403 Forbidden</status>`
- WHEN the items are listed
- THEN the two items are entries and `e3.ics` is a row carrying status 403

### Requirement: An item comes back whole
An item listing and an item multiget SHALL return the whole text of `calendar-data` as the item's bytes, its references resolved and its line ends normalised as XML 1.0 requires, nothing trimmed: RFC 4791 §9.6 makes that PCDATA the iCalendar object itself, whitespace around it included. Blank text SHALL count as empty data.

#### Scenario: An item ending in an escaped CR
- GIVEN a `calendar-multiget` answer whose `calendar-data` ends `END:VCALENDAR&#13;` and a line feed
- WHEN the item is read
- THEN its bytes end `END:VCALENDAR` and a CRLF, as a GET of the item returns them

### Requirement: Enumeration
Enumeration SHALL use the same calendar-query REPORT requesting the ETag only, returning id and ETag rows with no body, so a full spine costs no payload.

### Requirement: Batch fetch
Batch fetch SHALL use a calendar-multiget REPORT (RFC 4791 section 7.9) with Depth pinned to 0, the only value the RFC defines for it, fetching several item bodies in one round-trip instead of one GET per item.

### Requirement: Self-entry filtering
An entry whose href ends in a slash SHALL be skipped: it is the collection echoing itself, which some servers include in a query response, and it would otherwise enter the spine as an item named after the collection.

### Requirement: Preconditions
Creation SHALL send If-None-Match with a star so the server rejects the write when the resource already exists. Update and delete SHALL accept an optional If-Match so the caller can gate the write on the last-known ETag. A write refused with the CALDAV:no-uid-conflict precondition, or with CALDAV:unique-scheduling-object-resource (RFC 6638), SHALL surface as the dedicated duplicate-uid error, which is a different refusal from a resource name already taken and keeps its own signal.

### Requirement: Home set
The calendar home set SHALL be discovered from the principal URL via the calendar-home-set property (RFC 4791 section 6.2.1).
