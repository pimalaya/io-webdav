---
cairn: delta
change: caldav-alignment
---

## ADDED Requirements

### Requirement: Enumeration
Enumeration SHALL use the same calendar-query REPORT requesting the ETag only, returning id and ETag rows with no body, so a full spine costs no payload.

### Requirement: Batch fetch
Batch fetch SHALL use a calendar-multiget REPORT (RFC 4791 section 7.9) with Depth pinned to 0, the only value the RFC defines for it, fetching several item bodies in one round-trip instead of one GET per item.

### Requirement: Self-entry filtering
An entry whose href ends in a slash SHALL be skipped: it is the collection echoing itself, which some servers include in a query response, and it would otherwise enter the spine as an item named after the collection.

### Requirement: Component set
The component types a calendar holds SHALL be read from and written to supported-calendar-component-set (RFC 4791 section 5.2.3), whose value is a list of comp children carrying a name attribute. An empty set means the server advertises no restriction, which the RFC defines as accepting any type. A server fixes the set at creation time, so setting it is only meaningful on create.

### Requirement: Property values
A property set pair SHALL carry a PropValue, either text that is XML-escaped on the way out, or a raw markup fragment emitted verbatim. Raw exists for properties whose value is child elements rather than text, which escaping would destroy.

### Requirement: Property children
A parsed property SHALL expose its direct child elements, each carrying its local name and its name attribute when it has one. Attribute-valued properties like supported-calendar-component-set are unreadable otherwise.

### Requirement: Checkpoint properties
Both collection types SHALL carry a sync token property in their listing, alongside the CalendarServer ctag, so an incremental sync has a checkpoint to start from without a prior full pass.

## MODIFIED Requirements

### Requirement: Item identity
An item SHALL be addressed by its resource id, the href's last path segment used verbatim. The crate SHALL NOT append nor strip the .ics extension, so the caller owns the whole resource name and an id read from a listing addresses the resource it came from.

#### Scenario: Listed id round-trips
- GIVEN a listing returning an item whose href last segment carries no extension
- WHEN the caller reads that id back
- THEN the GET targets the very resource the server enumerated

### Requirement: Item verbs
The crate SHALL provide read, create, update and delete coroutines for calendar items, plus list, ETag-only enumeration and batch multiget.

### Requirement: Calendar properties
A listed calendar SHALL carry its id, display name, description, color, component set, ctag, sync token and default time zone. Every property the listing reads SHALL also be writable, so a calendar round-trips through create and update.
