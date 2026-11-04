---
cairn: log
change: the-scanned-multistatus
landed: 2026-11-04
---

# The scanned multistatus

A proof of concept measured what a scanner built for WebDAV bodies would gain over xmlparser plus the crate's text decoding, and what a serde layer would cost; it was then wired in. src/rfc4918/xml.rs scans with memchr, decodes text in one pass and leaves clean text borrowed; `parse_multistatus` runs on it with unchanged output, and xmlparser is gone.

**Typed properties.** src/rfc4918/typed.rs adds `parse_multistatus_as`, deserializing the 2xx properties of each response into a serde type of the caller. `WebdavPropfind` and `WebdavReport` became generic over what they parse into, `WebdavMultistatus` by default or `WebdavMultistatusOf<P>`, through a sealed trait; call sites that never name the result type now annotate the coroutine (the tests did). A response whose properties do not fit fails the parse with the new `WebdavMultistatusParseError::InvalidProps`, since a listing silently missing a member reads as its removal. The CalDAV and CardDAV item and collection mappers and sync-collection read typed props now.

**Verified.** The harness in the change folder links the pre-change crate (39bb548): outputs agree on generated multiget, sync and PROPFIND bodies and on edge cases, and a mutation fuzzer ran 2.4M bodies, 522k of them tokenized by xmlparser, with no mismatch and no panic, after two fixes it found (a DOCTYPE internal subset is skipped rather than ending the scan; a repeated attribute keeps its last value). The parse runs 2.7 to 6.2 times faster and the mappers end to end 3 to 6 times (wall clock, perf being absent). Offline suite, doctests and clippy green; typed.rs and xml.rs gained unit tests, the uncovered lines left being serde plumbing nothing reaches. Live suites not run.

Capabilities moved: [webdav-core](../spec/webdav-core.md) ADDED "Typed multistatus", MODIFIED "Methods" and "Multistatus parsing".
