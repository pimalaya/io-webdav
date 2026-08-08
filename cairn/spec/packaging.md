---
cairn: spec
capability: packaging
status: current
---

# Packaging

io-webdav is an I/O-free library following the Pimalaya crate conventions: a no_std core with an optional std-blocking client, dual-licensed MIT OR Apache-2.0.

### Requirement: no_std core
The crate SHALL be no_std unconditionally, pulling in alloc for its owned buffers. std SHALL be reachable only through the client feature.

### Requirement: Feature layering
The crate SHALL expose a client feature gating the std-blocking client, and one feature per TLS provider (rustls-ring by default, rustls-aws, native-tls), each implying client and selecting the matching pimalaya-stream provider. A vendored feature SHALL forward weakly to pimalaya-stream. This follows the golden rule that a feature is justified only when it changes the crate set.

### Requirement: Source layout
The source tree SHALL be organised one module per RFC (rfc4918, rfc4791, rfc5397, rfc6352, rfc6578), mirroring how the WebDAV specifications are split. Code spanning the RFC modules SHALL live at the crate root: the coroutine contract and the optional client. Every public type SHALL carry its domain prefix, Webdav for the core and the protocol-neutral pieces, Caldav and Carddav for their layers, and SHALL read domain then target then verb.

### Requirement: Byte orientation
The crate SHALL stay byte-oriented. iCalendar and vCard payloads SHALL be returned as raw bytes and accepted as raw bytes, the parse belonging to ical and vcard upstream. The crate SHALL NOT depend on either.

### Requirement: Coverage
The offline suites SHALL resume every coroutine and client method against scripted HTTP responses and hold 100% line coverage, measured with cargo-tarpaulin on the LLVM engine. Coverage SHALL NOT be reached by twisting the code to suit the metric; unreachable code is deleted instead.

### Requirement: Live suites
Ignored integration suites SHALL exercise the full flow against real servers: Radicale and Stalwart from a local script, Fastmail, Google and iCloud from environment credentials. Each flow SHALL clean up what it created.

### Requirement: Live-suite teardown
A live suite SHALL remove everything it creates on every exit path, the failing one included, since it writes to real production accounts. Teardown SHALL be registered as soon as the resource exists rather than written as the last steps of the flow, which a panic skips. Teardown SHALL be best-effort: it reports what it could not remove and never replaces the failure the run was reporting.
