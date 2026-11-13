---
cairn: delta
change: the-whole-multistatus
---

# Delta

## ADDED Requirements

### Requirement: Self entry
An enumeration SHALL drop the row naming the collection itself, which a PROPFIND always answers with and some servers echo in the sync report, rather than list it as a member. The row SHALL be recognised by its path against the request path: an absolute URL reduced to its path, query and fragment dropped, the trailing slash ignored, both sides percent-decoded, a server being free to encode either differently.

#### Scenario: The collection spelled with an escape
- GIVEN an enumeration of a collection whose path holds an at sign
- WHEN the server spells its own row with that character percent-encoded
- THEN the row is dropped and only the members are listed

## MODIFIED Requirements

### Requirement: Multistatus parsing
The parser SHALL be vocabulary-agnostic, matching by local name and ignoring namespace prefixes. It SHALL read the envelope by position: a response under the root, an href, a status and a propstat under a response, a prop and a status under a propstat, the sync-token under the root, so a property value reusing one of those names stays a value. It SHALL keep properties from 2xx propstats only, recording the properties of non-2xx propstats as failures carrying their status, while still surfacing responses carrying no 2xx propstat as entries with empty props and their response-level status. It SHALL read the top-level sync-token. Predefined and numeric character references SHALL be resolved, unknown entity references kept verbatim, a DOCTYPE skipped with its internal subset (its entities left unexpanded). Element nesting deeper than 64 levels SHALL fail with a typed error, keeping the parse linear and its tree shallow enough for a 1 MiB stack.

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

### Requirement: Home set resolution
The per-protocol home set SHALL be discovered from the principal URL, calendar-home-set for CalDAV and addressbook-home-set for CardDAV. Each SHALL yield nothing rather than fail when the multistatus carries no matching href. The property holds any number of hrefs (RFC 4791 section 6.2.1, RFC 6352 section 7.1.1): each SHALL be read as its own element, and the first non-empty one SHALL be the home set returned.

#### Scenario: A principal with two home sets
- GIVEN a principal whose home set property names two hrefs
- WHEN the home set is discovered
- THEN the first href is returned, resolved against the base URL

### Requirement: Invalid token
A server rejecting the sync token with the valid-sync-token precondition (RFC 6578 section 3.2) SHALL surface as a dedicated error, distinct from any other send failure, so the consumer can fall back to a full enumeration. The precondition SHALL be what is matched, read as an element of the body, whatever status carries it: a refusal merely quoting its name SHALL NOT raise this error.

#### Scenario: Token expired server-side
- GIVEN a stored sync token the server no longer recognises
- WHEN the consumer runs the incremental report
- THEN it receives the invalid-sync-token error and re-enumerates instead of silently seeing an empty delta

#### Scenario: The precondition under another status
- GIVEN a server refusing an expired token with a 409 carrying the precondition
- WHEN the consumer runs the incremental report
- THEN it receives the invalid-sync-token error, as it would under a 403

### Requirement: Unsupported report
A server answering a REPORT with the RFC 3253 section 3.6 supported-report precondition SHALL surface as a dedicated error, distinct from any other send failure and from the invalid sync token, so the consumer can enumerate another way. The 405 and 501 statuses SHALL raise it too, both meaning the request was never going to run.

The precondition SHALL be what is matched, not the status carrying it: a server chooses which status wraps it, and one was found using 403, which is also the status of a genuine permission refusal. It SHALL be read as an element of the body: a refusal for lack of privileges SHALL NOT raise this error, nor one merely quoting the name, such as a body mentioning supported-report-set.

#### Scenario: A server without the extension
- GIVEN a collection whose supported-report-set omits sync-collection
- WHEN the consumer runs the incremental report against it
- THEN it receives the unsupported-report error rather than an opaque send failure

## REMOVED Requirements
