---
cairn: delta
change: guidelines-alignment
---

## ADDED Requirements

### Requirement: Coverage
The offline suites SHALL resume every coroutine and client method against scripted HTTP responses and hold 100% line coverage, measured with cargo-tarpaulin on the LLVM engine. Coverage SHALL NOT be reached by twisting the code to suit the metric; unreachable code is deleted instead.

### Requirement: Live suites
Ignored integration suites SHALL exercise the full flow against real servers: Radicale and Stalwart from a local script, Fastmail, Google and iCloud from environment credentials. Each flow SHALL clean up what it created.

### Requirement: Byte orientation
The crate SHALL stay byte-oriented. iCalendar and vCard payloads SHALL be returned as raw bytes and accepted as raw bytes, the parse belonging to ical and vcard upstream. The crate SHALL NOT depend on either.

## MODIFIED Requirements

### Requirement: Source layout
The source tree SHALL be organised one module per RFC (rfc4918, rfc4791, rfc5397, rfc6352, rfc6578), mirroring how the WebDAV specifications are split. Code spanning the RFC modules SHALL live at the crate root: the coroutine contract and the optional client. Every public type SHALL carry its domain prefix, `Webdav` for the core and the protocol-neutral pieces, `Caldav` and `Carddav` for their layers, and SHALL read domain then target then verb.
