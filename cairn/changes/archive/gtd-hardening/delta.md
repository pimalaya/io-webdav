---
cairn: change
change: gtd-hardening
---

# Delta

## ADDED Requirements

### Requirement: Calendar identity
A listed calendar whose collection is a direct child of the home set SHALL carry the last path segment of its href as its id. Any other listed calendar SHALL carry its href's path as its id, starting with a slash, percent-encoding kept as the server served it. The home set and the href SHALL be compared percent-decoded. A calendar id starting with a slash SHALL address that path; any other id SHALL address the home set joined with it.

#### Scenario: Calendars outside the home set
- GIVEN a home set /caldav/v2/alice/ listing /caldav/v2/alice/events/ and /caldav/v2/team/events/
- WHEN the caller lists the calendars
- THEN the first carries the id events and the second the id /caldav/v2/team/events
- AND a write into the second addresses /caldav/v2/team/events/, never the home set's events

### Requirement: Address book identity
A listed address book SHALL carry its id by the rule calendars follow, and an address book id starting with a slash SHALL address that path.

### Requirement: Item move
The crate SHALL provide a coroutine moving a calendar item into another calendar of the same server with MOVE (RFC 4918 section 9.9), sending Overwrite F and, when the caller names an ETag, If-Match. The moved item SHALL keep its resource id. A refusal naming the no-uid-conflict or unique-scheduling-object-resource precondition SHALL be the duplicate-UID error, as for a create.

#### Scenario: A task filed into another calendar
- GIVEN a server keeping one UID per account
- WHEN the caller moves an item from one calendar into another
- THEN the server moves the resource itself, and no request ever puts a second resource with that UID

### Requirement: Closed connection
A client given a way to reconnect SHALL, when the server closed the connection before any byte of the response to a request arrived, open a fresh connection and resend that request, once. The full client's connect SHALL give it that way, to the URL and options it connected with. A client without one SHALL surface the I/O error. Swapping the stream SHALL drop the way to reconnect, so a client moved to another host never reconnects to the old one.

#### Scenario: The server closed after a write
- GIVEN a server answering a PUT with Connection: close and closing
- WHEN the client sends its next request
- THEN the request reaches the server on a fresh connection and its answer is returned

## MODIFIED Requirements

### Requirement: Item verbs
The crate SHALL provide read, create, update, delete and move coroutines for calendar items, plus list, ETag-only enumeration and batch multiget.

### Requirement: Resource identity
An item or card id SHALL be the last non-empty path segment of its href. A collection's id SHALL follow its layer's identity rule (Calendar identity, Address book identity). The crate SHALL never add nor strip a file extension anywhere in that derivation.
