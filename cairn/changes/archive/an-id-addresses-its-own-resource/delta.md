---
cairn: delta
change: an-id-addresses-its-own-resource
---

# Delta

## ADDED Requirements

### Requirement: An id stays inside what it names
(webdav-core) The crate SHALL offer a check for collection ids and one for item and card ids, refusing with a dedicated error carrying the id. A collection id SHALL be refused when, its trailing slashes dropped and then the leading slash of an id starting with one, it is empty or holds an empty, `.` or `..` segment. An item or card id SHALL be refused when, its leading slashes dropped, it is empty or holds such a segment. Segments SHALL be split on `/` and `\` and compared percent-decoded, the forms a URL parser or a server resolves as dot segments. The path builders and the coroutines SHALL join an id as given, checking it being their caller's part.

#### Scenario: The parent of the home set
- GIVEN the collection id `..`, `%2e%2e` or `a/../..`
- WHEN the caller checks it
- THEN it is refused, where joined under the home set it would address the home set's parent

#### Scenario: A collection outside the home set
- GIVEN the collection id /caldav/v2/team%40g/events
- WHEN the caller checks it
- THEN it passes, and addresses /caldav/v2/team%40g/events/ as before

### Requirement: A refused id sends nothing
(client) Every client method taking a collection, item or card id, the own id of a calendar or address book to create or update, a move target and each id of a multiget included, SHALL check it once the home set is known and, refused, SHALL fail with an invalid-input I/O error carrying the check's error, writing no byte.

#### Scenario: Deleting an address book named ..
- GIVEN a client with its address book home set cached
- WHEN the caller deletes the address book `..`
- THEN the call fails with an invalid-input error naming `..` and no DELETE is written

## MODIFIED Requirements

None.

## REMOVED Requirements

None.
