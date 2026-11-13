---
cairn: delta
change: the-request-boundary
---

# Delta

## ADDED Requirements

### Requirement: Entity tags
An entity tag read from an ETag header or a getetag property SHALL be handed out as its opaque value when strong, the surrounding quotes dropped, and whole when weak, prefix and quotes kept. A conditional header SHALL quote a bare value and pass a star, a quoted value and a weak tag through unchanged, so a tag read is the tag sent back. A bare value SHALL be quoted whatever it starts with. A weak tag SHALL be sent as it is and the server's answer surfaced: the precondition is never dropped.

#### Scenario: A weak tag round-trips
- GIVEN a server answering with a weak entity tag
- WHEN the caller passes the tag it read to a conditional write
- THEN the If-Match header carries the same weak tag, prefix and both quotes

### Requirement: Member failures
A DELETE, COPY or MOVE answered 207 SHALL have its multistatus parsed (RFC 4918 sections 9.6.1, 9.8.5 and 9.9.4). It SHALL fail with a dedicated error listing every row whose response status is not 2xx, each with its href and status, a row carrying no status counting as failed; a body that is not a whole multistatus SHALL fail as any multistatus does. It SHALL succeed only when every row carries a 2xx status.

#### Scenario: A collection with a locked member
- GIVEN a DELETE on a collection one member of which is locked
- WHEN the server answers 207 naming that member with a 423
- THEN the call fails listing the member and its status, rather than report the collection deleted

### Requirement: Header values
A request whose header value carries a control character, a byte below 0x20 or 0x7F, SHALL NOT be sent: the send coroutines SHALL complete at once with a dedicated error naming the header. This covers every header of the request, the conditional tags, the destination, the content type and the user agent among them, whatever built the request.

#### Scenario: An entity tag carrying a line break
- GIVEN an entity tag holding a carriage return and a line feed
- WHEN the caller passes it to a conditional write
- THEN the coroutine fails naming If-Match and no byte is written

### Requirement: Unexpected redirect
A send that follows no redirect SHALL fail on a 3xx with an error carrying the resolved target URL, whether the connection can be kept alive and whether the target is same-origin, so the caller can reconnect there.

### Requirement: Collection paths
A request on a calendar or an address book itself, a listing, an enumeration, a multiget or a sync, SHALL address the collection path with its trailing slash (RFC 4918 section 5.2), the form a server would otherwise redirect to.

#### Scenario: A report on a calendar
- GIVEN a calendar id under the cached home set
- WHEN the client enumerates its items
- THEN the REPORT addresses the calendar path ending in a slash

## MODIFIED Requirements

### Requirement: Multistatus parsing
The parser SHALL be vocabulary-agnostic, matching by local name and ignoring namespace prefixes. It SHALL read the envelope by position: a response under the root, an href, a status and a propstat under a response, a prop and a status under a propstat, the sync-token under the root, so a property value reusing one of those names stays a value. It SHALL keep properties from 2xx propstats only, recording the properties of non-2xx propstats as failures carrying their status, while still surfacing responses carrying no 2xx propstat as entries with empty props and their response-level status. It SHALL read the top-level sync-token. Predefined and numeric character references SHALL be resolved, unknown entity references kept verbatim, as SHALL a numeric reference naming NUL or written with a sign, a DOCTYPE skipped with its internal subset (its entities left unexpanded). Element nesting deeper than 64 levels SHALL fail with a typed error, keeping the parse linear and its tree shallow enough for a 1 MiB stack.

A body SHALL parse only as one whole multistatus document: its first element is multistatus, that element closes, every end tag matches its start tag, no element follows the root and the scan reaches the end of the body. Anything else SHALL fail with a typed error, one naming a foreign root, the other carrying the count of responses read before the document stopped being whole: a listing missing a member reads as its removal. A closed multistatus holding no response SHALL parse as an empty listing. Text outside the root SHALL be ignored, hiding no response. The status carrying the body SHALL NOT be checked beyond success.

#### Scenario: A page instead of a listing
- GIVEN a PROPFIND or REPORT answered 200 with an HTML page
- WHEN the body is parsed
- THEN the parse fails naming the root it found, rather than return an empty listing

#### Scenario: A cut listing
- GIVEN a multistatus whose document stops after some of its responses
- WHEN the body is parsed
- THEN the parse fails carrying the count of responses read, rather than return them as the listing

#### Scenario: Responses nested in a property value
- GIVEN a response whose property value holds response, propstat and prop elements of its own
- WHEN the body is parsed
- THEN the outer response keeps its href and properties, and the nested elements stay children of the property

### Requirement: Resource identity
An item or card id SHALL be the last non-empty path segment of its href, percent-encoding kept as the server served it. A collection's id SHALL follow its layer's identity rule (Calendar identity, Address book identity). The crate SHALL never add nor strip a file extension anywhere in that derivation. A create, update or move SHALL return the id as the last segment of the URL it sent, percent-encoded, which is the form a listing returns, never the caller's string as given. The crate SHALL offer an encoder turning a name the caller mints into such a segment, every byte but the RFC 3986 unreserved ones escaped; an encoded name SHALL be sent as it stands.

#### Scenario: A name needing escapes
- GIVEN a create under a name holding a space and a non-ASCII letter
- WHEN the server answers without a Location header
- THEN the returned id is the percent-encoded segment the request addressed

### Requirement: Location header
When a server reports a created resource in a Location header, the id SHALL be that header's last path segment, query and fragment dropped, falling back to the last segment of the request URL when the header is absent or its segment is empty.

#### Scenario: Server names the resource itself
- GIVEN a PUT creating a resource under a caller-chosen name
- WHEN the server answers 201 with a Location pointing at a different name
- THEN the returned id is the server's name, which is what later reads address

### Requirement: Home set resolution
The per-protocol home set SHALL be discovered from the principal URL, calendar-home-set for CalDAV and addressbook-home-set for CardDAV. Each SHALL yield nothing rather than fail when the multistatus carries no matching href. The property holds any number of hrefs (RFC 4791 section 6.2.1, RFC 6352 section 7.1.1): each SHALL be read as its own element, and the first non-empty one SHALL be the home set returned. A property holding no href element at all SHALL be read as its trimmed text, for the servers that write the path bare.

#### Scenario: A principal with two home sets
- GIVEN a principal whose home set property names two hrefs
- WHEN the home set is discovered
- THEN the first href is returned, resolved against the base URL

#### Scenario: A home set written as bare text
- GIVEN a principal whose home set property holds a path and no href element
- WHEN the home set is discovered
- THEN that path is returned, resolved against the base URL

### Requirement: No redirect following
The client SHALL never follow a redirect. It owns a single connected stream, so it SHALL surface the target URL in an unexpected-redirect error and let the caller reconnect and swap the stream in. A discovery call SHALL raise the client's own unexpected-redirect error; any other call SHALL surface the send error of that name, which carries the target too.

#### Scenario: Context root moved
- GIVEN a client configured with a context root the provider redirects away from
- WHEN discovery runs
- THEN the call fails with the redirect target, and the caller reconnects there and retries

## REMOVED Requirements
