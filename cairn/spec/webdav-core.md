---
cairn: spec
capability: webdav-core
status: current
---

# WebDAV core

The RFC 4918 layer: the HTTP methods WebDAV adds, the property model every higher layer speaks, the multistatus parser, and the authentication modes. CalDAV and CardDAV are built entirely on top of it.

### Requirement: Methods
The crate SHALL provide one coroutine per WebDAV method: PROPFIND, PROPPATCH, MKCOL, COPY, MOVE, DELETE, GET, PUT, OPTIONS and REPORT, plus the low-level send coroutine they all build on.

### Requirement: Property model
A property SHALL be identified by a namespace (URI plus preferred prefix) and a local name. Each RFC layer SHALL own the namespaces and property constants it speaks. The body generators SHALL emit XML from those values rather than from hard-coded templates, so no central namespace table exists.

### Requirement: DAV prefix
The DAV namespace SHALL be emitted with the D prefix, never as the default namespace. Strict servers reject bodies mixing a prefixed CalDAV or CardDAV root with default-namespace DAV children.

### Requirement: Property values
A property set pair SHALL carry a WebdavPropValue, either text that is XML-escaped on the way out, or a raw markup fragment emitted verbatim. Raw exists for properties whose value is child elements rather than text, which escaping would destroy.

### Requirement: Multistatus parsing
The parser SHALL be vocabulary-agnostic, matching by local name and ignoring namespace prefixes. It SHALL keep properties from 2xx propstats only, recording the properties of non-2xx propstats as failures carrying their status, while still surfacing responses carrying no 2xx propstat as entries with empty props and their response-level status. It SHALL read the top-level sync-token. Predefined and numeric character references SHALL be resolved, unknown entity references kept verbatim, and malformed input SHALL yield whatever parsed before the error.

### Requirement: A property update is verified, not assumed
A `PROPPATCH` SHALL return its parsed multistatus together with the properties the request asked to change, and the client SHALL fail unless every one of them came back accepted. A property the server refused fails the call, and so does one it never mentioned: RFC 4918 §9.2.1 wants a propstat per requested property, and a server that names none of them (iCloud, for a collection that does not exist) changed nothing while answering 207.

#### Scenario: A collection that does not exist
- GIVEN a PROPPATCH against a collection the server does not have
- WHEN the server answers 207 with an empty prop and a 200 status
- THEN the client fails, naming the properties the server ignored

### Requirement: An HTTP error is summarised, not dumped
A non-2xx status SHALL render as a summary of the body: its DAV responsedescription when it carries one, else an HTML title, else the body with markup stripped, whitespace collapsed and length capped. An empty body SHALL render as the status alone. The raw body SHALL remain available on the error value for consumers that inspect it.

### Requirement: A refused duplicate UID is named
A write answered with the CalDAV or CardDAV no-uid-conflict precondition SHALL surface as a dedicated error, distinct from any other send failure, carrying the status and the raw body like the others. The precondition element SHALL be what is matched, not the status, since the RFCs name the element and recommend the status.

The refusal is the server saying the collection already holds this UID, which is a state a consumer acts on: it names the resource to fix and the collection to fix it in. Left as an opaque conflict it is indistinguishable from a quota or a lock, and a consumer can only report a number.

#### Scenario: A duplicate UID is refused
- GIVEN a collection already holding a resource with a UID
- WHEN a client PUTs another resource carrying that UID
- AND the server answers with the no-uid-conflict precondition
- THEN the client receives the duplicate-uid error rather than an opaque status

### Requirement: Property children
A parsed property SHALL expose its direct child elements, each carrying its local name and its name attribute when it has one. Attribute-valued properties like supported-calendar-component-set are unreadable otherwise.

### Requirement: Resource identity
A resource id SHALL be the last non-empty path segment of its href. The crate SHALL never add nor strip a file extension anywhere in that derivation.

### Requirement: Location header
When a server reports a created resource in a Location header, the id SHALL be that header's last path segment, query and fragment dropped, falling back to the caller-supplied name when the header is absent or its segment is empty.

#### Scenario: Server names the resource itself
- GIVEN a PUT creating a resource under a caller-chosen name
- WHEN the server answers 201 with a Location pointing at a different name
- THEN the returned id is the server's name, which is what later reads address

### Requirement: Authentication
The crate SHALL support no authentication, HTTP Basic (RFC 7617) and HTTP Bearer (RFC 6750), reusing the io-http credential types. Coroutines SHALL never observe the credential directly, only the pre-formatted header value.
