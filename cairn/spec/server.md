---
cairn: spec
capability: server
status: current
---

# Server side

The optional server layer, gated behind the server feature: parsers for the request bodies and headers a WebDAV, CalDAV and CardDAV server receives, and writers for the bodies it answers with. No session and no HTTP: the embedding server owns the exchange and hands bytes and header values in. no_std with alloc.

### Requirement: Namespace-aware request bodies
A request body SHALL be read with every prefix resolved to its namespace URI, names matched by namespace and local name, never by prefix. A body that is not UTF-8, not one whole XML document, nested deeper than 64 elements, using an undeclared prefix, or naming an element or a namespace prefix with blank, control or markup characters SHALL fail. An empty or blank body SHALL read as the RFC default of its method: allprop for PROPFIND, no properties for MKCOL and MKCALENDAR, a refresh for LOCK; it SHALL fail for PROPPATCH and REPORT.

### Requirement: Raw elements stand alone
An element kept as raw XML (a set property value, a LOCK owner, a calendar-data or address-data request) SHALL be the element as sent, carrying a declaration for every namespace prefix it inherited and does not redeclare, so it parses the same once moved into another document.

#### Scenario: A dead property moved into a multistatus
- GIVEN a PROPPATCH setting a property of a namespace declared on the root
- WHEN its raw element is written as a property of a multistatus
- THEN the client multistatus parser reads it with the same name and text

### Requirement: Method bodies
The crate SHALL parse PROPFIND (prop, propname, allprop with include), PROPPATCH (set and remove instructions in document order), extended MKCOL (RFC 5689), MKCALENDAR (RFC 4791), LOCK (lockscope, write locktype, owner) and REPORT bodies: calendar-multiget, calendar-query (nested comp-filter, time-range, prop-filter, param-filter, text-match, is-not-defined, timezone), free-busy-query, addressbook-multiget, addressbook-query (filter test, prop-filter, param-filter, text-match with match-type, collation and negation, limit) and sync-collection (token, level, limit, properties). An addressbook-query filter SHALL read into the filter type the client sends. Any other report SHALL come back by name.

### Requirement: Property names
A property name SHALL be typed for the DAV, CalDAV, CardDAV, CalendarServer and Apple iCal properties a CalDAV and CardDAV server serves, and kept as namespace plus local name otherwise.

### Requirement: Multistatus writer
The crate SHALL write a multistatus of responses, each with its href and either a response-level status or its properties grouped by status into propstats, an optional error and description, plus the multistatus description and sync token. Property values SHALL cover text, hrefs, raw fragments, whole raw elements, resource types, component sets, supported calendar and address data, privileges, supported reports, lock discovery and supported locks. A carriage return in text SHALL be written as a character reference, so an iCalendar or vCard keeps its CRLF.

### Requirement: Error bodies
The crate SHALL write the DAV:error body of every precondition of RFC 4918, RFC 5689, RFC 3253, RFC 4791, RFC 6352 and RFC 6578 a CalDAV and CardDAV server reports, with the hrefs lock-token-submitted, no-conflicting-lock and no-uid-conflict carry.

### Requirement: Headers
The crate SHALL parse Depth, Destination (an absolute URL or path, percent-decoded, its origin kept apart), Overwrite, If (tagged or untagged lists of lock tokens and entity tags, Not), If-Match and If-None-Match (`*` or entity tags), Timeout (Second-n and Infinite, in order) and Lock-Token, answering nothing for a malformed value.

### Requirement: Client oracle
Every request body the client builds SHALL parse back to what the client meant, and every body the writers produce SHALL read back through the client parsers and coroutines to what the server meant.
