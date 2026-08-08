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
A listed calendar SHALL carry its id, display name, description, color, component set, ctag, sync token and default time zone. Every property the listing reads SHALL also be writable, so a calendar round-trips through create and update.

### Requirement: Component set
The component types a calendar holds SHALL be read from and written to supported-calendar-component-set (RFC 4791 section 5.2.3), whose value is a list of comp children carrying a name attribute. An empty set means the server advertises no restriction, which the RFC defines as accepting any type. A server fixes the set at creation time, so setting it is only meaningful on create.

### Requirement: Item verbs
The crate SHALL provide read, create, update and delete coroutines for calendar items, plus list, ETag-only enumeration and batch multiget.

### Requirement: Item identity
An item SHALL be addressed by its resource id, the href's last path segment used verbatim. The crate SHALL NOT append nor strip the .ics extension, so the caller owns the whole resource name and an id read from a listing addresses the resource it came from.

#### Scenario: Listed id round-trips
- GIVEN a listing returning an item whose href last segment carries no extension
- WHEN the caller reads that id back
- THEN the GET targets the very resource the server enumerated

### Requirement: Listing
Listing SHALL use a calendar-query REPORT at Depth 1, requesting the ETag and the calendar data. The caller SHALL pass the optional VCALENDAR child filter, an empty string listing every component type. The iCalendar payload SHALL be returned as raw bytes, parsed upstream.

### Requirement: Enumeration
Enumeration SHALL use the same calendar-query REPORT requesting the ETag only, returning id and ETag rows with no body, so a full spine costs no payload.

### Requirement: Batch fetch
Batch fetch SHALL use a calendar-multiget REPORT (RFC 4791 section 7.9) with Depth pinned to 0, the only value the RFC defines for it, fetching several item bodies in one round-trip instead of one GET per item.

### Requirement: Self-entry filtering
An entry whose href ends in a slash SHALL be skipped: it is the collection echoing itself, which some servers include in a query response, and it would otherwise enter the spine as an item named after the collection.

### Requirement: Preconditions
Creation SHALL send If-None-Match with a star so the server rejects the write when the resource already exists. Update and delete SHALL accept an optional If-Match so the caller can gate the write on the last-known ETag.

### Requirement: Home set
The calendar home set SHALL be discovered from the principal URL via the calendar-home-set property (RFC 4791 section 6.2.1).
