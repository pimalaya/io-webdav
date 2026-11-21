---
cairn: delta
change: an-enumeration-returns-what-it-could-not-read
---

# Delta

## ADDED Requirements

### Requirement: An enumeration returns the member rows it could not read
(sync) Both enumerations, the `sync-collection` REPORT and its `PROPFIND` fallback, SHALL return every member row answered with a response-level status other than 2xx, 404 and 507 (a 403, a 5xx) as an unreadable entry, in the order met, carrying the member's id and href, the status and the refused properties. Such a row SHALL be neither a change nor a removal. The self-entry SHALL stay skipped whatever its status, and a row naming no member id SHALL be neither. A 2xx row whose `getetag` the server refused SHALL stay a change with no ETag: the member exists, only its version is unknown.

#### Scenario: A member the server refuses to show
- GIVEN an initial sync whose answer holds one member, a row for `bob.vcf` with `<status>HTTP/1.1 403 Forbidden</status>` and a row for `carol.vcf` answered 503
- WHEN the collection is enumerated
- THEN the one member is changed, and `bob.vcf` and `carol.vcf` are unreadable with their status, so a consumer knows the listing is not the whole collection

## MODIFIED Requirements

### Requirement: Delta shape
(sync) The report SHALL return the changed members (href plus ETag), the hrefs of vanished members, the member rows it could not read, the next sync token, and whether the server truncated the result set.

## REMOVED Requirements

None.
