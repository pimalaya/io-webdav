---
cairn: tasks
change: the-scanned-multistatus
---

# Tasks

- [x] Harness: scanner (scalar and memchr), untyped rebuild, typed serde parser, fixtures, consumer mappers
- [x] Differential check against `parse_multistatus` and the crate's mappers on fixtures, edge cases and the depth cap
- [x] Timings on an idle core, SSE2 and AVX2 builds
- [x] Decide the API: generic `WebdavPropfind<M>` and `WebdavReport<M>` over a sealed parse trait (user, 2026-11-04); one bad response fails the parse
- [x] Move the scanner (src/rfc4918/xml.rs) and typed parser (src/rfc4918/typed.rs) into src/, drop xmlparser, port the mappers
- [x] Differential fuzzer against the pre-change crate (harness `fuzz`), 2.4M bodies, no mismatch, no panic
- [x] Unit tests for the scanner, the decoders and the typed reading rules
- [x] cargo fmt, clippy, rustdoc links, offline suite
- [x] Fold the delta into spec/webdav-core.md, log, CHANGELOG
