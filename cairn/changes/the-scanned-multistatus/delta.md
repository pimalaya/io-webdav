---
cairn: delta
change: the-scanned-multistatus
---

# Delta

## ADDED Requirements

### Requirement: Typed multistatus
The crate SHALL parse a multistatus into responses whose 2xx properties deserialize, merged across propstats, into a serde type the caller supplies. Children SHALL match by local name, prefixes ignored, the first of same-named properties winning. Text SHALL read trimmed, borrowed from the body where it needs no decoding; attributes SHALL read as `@name` keys; consecutive same-named children SHALL fill a sequence; an empty element SHALL read as unit. The envelope (href, response status, refused properties, sync token, depth cap, prefix of malformed input) SHALL keep the semantics of Multistatus parsing. A response whose properties do not fit the type SHALL fail the parse, naming its href, rather than drop out: a listing missing a member reads as its removal.

#### Scenario: A property the type cannot hold
- GIVEN a typed multistatus expecting a number in `getcontentlength`
- WHEN a response carries text there
- THEN the parse fails naming that response's href

## MODIFIED Requirements

### Requirement: Methods
The crate SHALL provide one coroutine per WebDAV method: PROPFIND, PROPPATCH, MKCOL, COPY, MOVE, DELETE, GET, PUT, OPTIONS and REPORT, plus the low-level send coroutine they all build on. PROPFIND and REPORT SHALL parse their multistatus into the representation the caller picks, vocabulary-agnostic (Multistatus parsing) or typed (Typed multistatus).

### Requirement: Multistatus parsing
The parser SHALL be vocabulary-agnostic, matching by local name and ignoring namespace prefixes. It SHALL keep properties from 2xx propstats only, recording the properties of non-2xx propstats as failures carrying their status, while still surfacing responses carrying no 2xx propstat as entries with empty props and their response-level status. It SHALL read the top-level sync-token. Predefined and numeric character references SHALL be resolved, unknown entity references kept verbatim, a DOCTYPE skipped with its internal subset (its entities left unexpanded), and malformed input SHALL yield whatever parsed before the error. Element nesting deeper than 64 levels SHALL fail with a typed error, keeping the parse linear and its tree shallow enough for a 1 MiB stack.
