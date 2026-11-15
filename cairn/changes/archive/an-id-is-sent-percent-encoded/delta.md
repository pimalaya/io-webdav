---
cairn: delta
change: an-id-is-sent-percent-encoded
---

# Delta

## ADDED Requirements

### Requirement: A relative id is sent percent-encoded
(webdav-core) The collection path builder and the item and card path builders SHALL percent-encode each byte of a relative id that an RFC 3986 path segment cannot hold raw, any byte but the unreserved ones, the sub-delimiters, `:` and `@`, keeping `/` as the segment separator and an escape already there (`%` and two hex digits) as it stands. An id read from an href SHALL be joined byte-identical, and the id's percent-decoded form SHALL stay unchanged, so the id checks judge the path sent. A collection id starting with a slash SHALL be taken as the server gave it.

#### Scenario: An address book named with a space
- GIVEN the collection id `My Book`
- WHEN the client creates the address book
- THEN the MKCOL addresses `<home>/My%20Book/`, the path the id `My%20Book` read from a listing addresses too

#### Scenario: A backslash in an id
- GIVEN the collection id `a\b`
- WHEN the client deletes the address book
- THEN the DELETE addresses `<home>/a%5Cb/`, not the nested collection `<home>/a/b/`

## MODIFIED Requirements

### Requirement: An id stays inside what it names
(webdav-core) The crate SHALL offer a check for collection ids and one for item and card ids, refusing with a dedicated error carrying the id. A collection id SHALL be refused when, its trailing slashes dropped and then the leading slash of an id starting with one, it is empty or holds an empty, `.` or `..` segment. An item or card id SHALL be refused when, its leading slashes dropped, it is empty or holds such a segment. Segments SHALL be split on `/` and `\` and compared percent-decoded, the forms a URL parser or a server resolves as dot segments. The path builders and the coroutines SHALL NOT check an id, checking it being their caller's part, and SHALL join it as A relative id is sent percent-encoded says.

#### Scenario: The parent of the home set
- GIVEN the collection id `..`, `%2e%2e` or `a/../..`
- WHEN the caller checks it
- THEN it is refused, where joined under the home set it would address the home set's parent

#### Scenario: A collection outside the home set
- GIVEN the collection id /caldav/v2/team%40g/events
- WHEN the caller checks it
- THEN it passes, and addresses /caldav/v2/team%40g/events/ as before

## REMOVED Requirements

None.
