---
cairn: change
id: the-whole-multistatus
status: landed
created: 2026-11-13
---

# The whole multistatus

## Why

A sync consumer reads a member missing from a full listing as a deletion. The family audit of 2026-11-13 found five places where this crate could hand such a consumer a wrong answer without an error.

- DAV-01. Both multistatus parsers returned `Ok` for any body: an HTML page answered with a 2xx gave zero rows, a document cut mid-way gave the responses closed before the cut, an empty body gave zero rows. Neither checked that the root was `multistatus`, that it closed, or that the scan reached the end of the body. The spec said so on purpose ("malformed input SHALL yield whatever parsed before the error"), against its own neighbour ("a listing missing a member reads as its removal").
- DAV-03. `parse_multistatus` recognised `response`, `propstat` and `prop` by name at any depth, where `parse_multistatus_as` reads them by position. A property value nesting those names (the answer to an expand-property REPORT, RFC 3253 section 3.8) replaced the response being read: the caller got the nested rows and lost the ones it asked about, and the two parsers disagreed on one body.
- DAV-05. `calendar-home-set` and `addressbook-home-set` are `(DAV:href*)` (RFC 4791 section 6.2.1, RFC 6352 section 7.1.1). The lookups read the property's flattened text, so two hrefs came back as one URL made of both and the indentation between them, naming neither home set.
- DAV-08. Two preconditions were searched for as substrings. `valid-sync-token` was matched under status 403 only, so a server refusing an expired token with 409 surfaced as an opaque status and the consumer never re-enumerated; `supported-report` matched any refusal whose body mentioned `supported-report-set`, turning it into an unimplemented report.
- DAV-15. An enumeration drops the row naming the collection itself by comparing paths byte for byte. A server spelling its own collection `/dav/user%40example.org/work/` against a request for `/dav/user@example.org/work/` had the collection listed as a member.

## What

- Both parsers fail unless the body is one whole `multistatus` document: the first element is `multistatus`, it closes, every end tag matches, no element follows it and the scan reaches the end of the body. `WebdavMultistatusParseError` gains `UnexpectedRoot`, carrying the root found, and `Incomplete`, carrying the count of responses read. A closed `multistatus` holding no response still parses as an empty listing. Text outside the root is ignored: a byte order mark or a notice a server prints around the document hides no response.
- `parse_multistatus` reads the envelope by position, like the typed parser: an element is a `response` under the root only, a `propstat`, `href` or `status` under a response only, a `prop` or `status` under a propstat only.
- The home set lookups parse a typed multistatus and take the first non-empty `href` child. Returning every home set would change the return type and the client's discovery cache, which is a separate decision.
- `valid-sync-token` and `supported-report` are matched with `has_element`, as the other preconditions already are, whatever status carries them.
- The self-entry comparison goes through the percent-decoding `collection_id` already uses, and the two private `href_path` helpers become one.

Status 207 is not required: a server answering 200 with a whole multistatus keeps working, and the root check alone closes the three bodies.

The shared envelope the audit suggests for DAV-03 (driving `parse_multistatus` from the typed parser's envelope) was not taken. The typed envelope drops the indentation before a property's content and keeps each element's text apart from its children's, so `WebdavPropItem::text`, documented as the concatenated descendant text, would change for every caller. Reading by position inside the existing loop fixes the divergence and leaves that text as it was.
