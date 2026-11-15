---
cairn: spec
capability: webdav-core
status: current
---

# WebDAV core

The RFC 4918 layer: the HTTP methods WebDAV adds, the property model every higher layer speaks, the multistatus parser, and the authentication modes. CalDAV and CardDAV are built entirely on top of it.

### Requirement: Methods
The crate SHALL provide one coroutine per WebDAV method: PROPFIND, PROPPATCH, MKCOL, COPY, MOVE, DELETE, GET, PUT, OPTIONS and REPORT, plus the low-level send coroutine they all build on. The send coroutine SHALL hand a successful response's body over once, apart from the response head, so no body is held twice. PROPFIND and REPORT SHALL parse their multistatus into the representation the caller picks, vocabulary-agnostic (Multistatus parsing) or typed (Typed multistatus).

### Requirement: Property model
A property SHALL be identified by a namespace (URI plus preferred prefix) and a local name. Each RFC layer SHALL own the namespaces and property constants it speaks. The body generators SHALL emit XML from those values rather than from hard-coded templates, so no central namespace table exists.

### Requirement: DAV prefix
The DAV namespace SHALL be emitted with the D prefix, never as the default namespace. Strict servers reject bodies mixing a prefixed CalDAV or CardDAV root with default-namespace DAV children.

### Requirement: Property values
A property set pair SHALL carry a WebdavPropValue, either text that is XML-escaped on the way out, or a raw markup fragment emitted verbatim. Raw exists for properties whose value is child elements rather than text, which escaping would destroy.

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

### Requirement: Typed multistatus
The crate SHALL parse a multistatus into responses whose 2xx properties deserialize, merged across propstats, into a serde type the caller supplies. Children SHALL match by local name, prefixes ignored, the first of same-named properties winning. Text SHALL read trimmed, borrowed from the body where it needs no decoding; attributes SHALL read as `@name` keys; consecutive same-named children SHALL fill a sequence; an empty element SHALL read as unit. The envelope (href, response status, refused properties, sync token, depth cap, whole-document rule) SHALL keep the semantics of Multistatus parsing. A response whose properties do not fit the type SHALL fail the parse, naming its href, rather than drop out: a listing missing a member reads as its removal.

#### Scenario: A property the type cannot hold
- GIVEN a typed multistatus expecting a number in `getcontentlength`
- WHEN a response carries text there
- THEN the parse fails naming that response's href

### Requirement: A property update is verified, not assumed
A `PROPPATCH` SHALL return its parsed multistatus together with the properties the request asked to change, and the client SHALL fail unless every one of them came back accepted. A property the server refused fails the call, and so does one it never mentioned: RFC 4918 §9.2.1 wants a propstat per requested property, and a server that names none of them (iCloud, for a collection that does not exist) changed nothing while answering 207.

#### Scenario: A collection that does not exist
- GIVEN a PROPPATCH against a collection the server does not have
- WHEN the server answers 207 with an empty prop and a 200 status
- THEN the client fails, naming the properties the server ignored

### Requirement: An HTTP error is summarised, not dumped
A non-2xx status SHALL render as a summary of the body: its DAV responsedescription when it carries one, else an HTML title, else the body with markup stripped, whitespace collapsed and length capped. An empty body SHALL render as the status alone. The raw body SHALL remain available on the error value for consumers that inspect it.

### Requirement: A refused duplicate UID is named
A write answered with the CalDAV or CardDAV no-uid-conflict precondition, or with the CalDAV unique-scheduling-object-resource precondition (RFC 6638, the same refusal across every calendar of the user), SHALL surface as a dedicated error, distinct from any other send failure, carrying the status and the raw body like the others. The precondition element SHALL be what is matched, not the status, since the RFCs name the element and recommend the status.

The refusal is the server saying the collection already holds this UID, which is a state a consumer acts on: it names the resource to fix and the collection to fix it in. Left as an opaque conflict it is indistinguishable from a quota or a lock, and a consumer can only report a number.

#### Scenario: A duplicate UID is refused
- GIVEN a collection already holding a resource with a UID
- WHEN a client PUTs another resource carrying that UID
- AND the server answers with the no-uid-conflict precondition
- THEN the client receives the duplicate-uid error rather than an opaque status

### Requirement: Property children
A parsed property SHALL expose its direct child elements, each carrying its local name and its name attribute when it has one. Attribute-valued properties like supported-calendar-component-set are unreadable otherwise.

### Requirement: A parsed property keeps its whole text
The vocabulary-agnostic parser SHALL keep each property's whole text, references resolved and line ends normalised, on the parsed property; its text helper SHALL return it trimmed.

### Requirement: Resource identity
An item or card id SHALL be the last non-empty path segment of its href, percent-encoding kept as the server served it. A collection's id SHALL follow its layer's identity rule (Calendar identity, Address book identity). The crate SHALL never add nor strip a file extension anywhere in that derivation. A create, update or move SHALL return the id as the last segment of the URL it sent, percent-encoded, which is the form a listing returns, never the caller's string as given. The crate SHALL offer an encoder turning a name the caller mints into such a segment, every byte but the RFC 3986 unreserved ones escaped; an encoded name SHALL be sent as it stands.

#### Scenario: A name needing escapes
- GIVEN a create under a name holding a space and a non-ASCII letter
- WHEN the server answers without a Location header
- THEN the returned id is the percent-encoded segment the request addressed

### Requirement: An id stays inside what it names
The crate SHALL offer a check for collection ids and one for item and card ids, refusing with a dedicated error carrying the id. A collection id SHALL be refused when, its trailing slashes dropped and then the leading slash of an id starting with one, it is empty or holds an empty, `.` or `..` segment. An item or card id SHALL be refused when, its leading slashes dropped, it is empty or holds such a segment. Segments SHALL be split on `/` and `\` and compared percent-decoded, the forms a URL parser or a server resolves as dot segments. The path builders and the coroutines SHALL NOT check an id, checking it being their caller's part, and SHALL join it as A relative id is sent percent-encoded says.

#### Scenario: The parent of the home set
- GIVEN the collection id `..`, `%2e%2e` or `a/../..`
- WHEN the caller checks it
- THEN it is refused, where joined under the home set it would address the home set's parent

#### Scenario: A collection outside the home set
- GIVEN the collection id /caldav/v2/team%40g/events
- WHEN the caller checks it
- THEN it passes, and addresses /caldav/v2/team%40g/events/ as before

### Requirement: A relative id is sent percent-encoded
The collection path builder and the item and card path builders SHALL percent-encode each byte of a relative id that an RFC 3986 path segment cannot hold raw, any byte but the unreserved ones, the sub-delimiters, `:` and `@`, keeping `/` as the segment separator and an escape already there (`%` and two hex digits) as it stands. An id read from an href SHALL be joined byte-identical, and the id's percent-decoded form SHALL stay unchanged, so the id checks judge the path sent. A collection id starting with a slash SHALL be taken as the server gave it.

#### Scenario: An address book named with a space
- GIVEN the collection id `My Book`
- WHEN the client creates the address book
- THEN the MKCOL addresses `<home>/My%20Book/`, the path the id `My%20Book` read from a listing addresses too

#### Scenario: A backslash in an id
- GIVEN the collection id `a\b`
- WHEN the client deletes the address book
- THEN the DELETE addresses `<home>/a%5Cb/`, not the nested collection `<home>/a/b/`

### Requirement: Location header
When a server reports a created resource in a Location header, the id SHALL be that header's last path segment, query and fragment dropped, falling back to the last segment of the request URL when the header is absent or its segment is empty.

#### Scenario: Server names the resource itself
- GIVEN a PUT creating a resource under a caller-chosen name
- WHEN the server answers 201 with a Location pointing at a different name
- THEN the returned id is the server's name, which is what later reads address

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

### Requirement: Authentication
The crate SHALL support no authentication, HTTP Basic (RFC 7617) and HTTP Bearer (RFC 6750), reusing the io-http credential types. Coroutines SHALL never observe the credential directly, only the pre-formatted header value.
