---
cairn: tasks
change: duplicate-link-id-mints-an-item
---

# Tasks

- [x] An error variant for the refusal on `WebdavSendError`, beside `UnsupportedReport`, carrying the status and the body.
- [x] Classify it where the write returns, mirroring `unsupported_report` (src/rfc4918/report.rs): one body match, no status list, applied on the create and update paths of both flavours (src/rfc4791/item/create.rs and update.rs, src/rfc6352/card/create.rs and update.rs).
- [x] Match both namespaced spellings of the element, and match it as an element rather than as a substring of any body that happens to contain the words.
- [x] Tests: a canned `409` carrying each element surfaces the new error; a `409` carrying neither stays an ordinary status error; a `507` carrying the element still classifies (the element is the fact, not the status).
- [x] Document the RFC references on the variant (RFC 4791 §5.3.2, RFC 6352 §6.3.2) as the other variants document theirs.
- [x] `cargo test`, `cargo clippy --all-targets`, `cargo fmt`.
- [x] CHANGELOG `### Added`; a new variant on a public error enum is breaking, so the release is a minor bump under the crate's pre-1.0 rule.
- [x] Fold `delta.md` into `cairn/spec/caldav.md` and `cairn/spec/carddav.md` (the Preconditions requirement in each, with the CardDAV wording naming its own element); append the log entry; mark `landed`.
