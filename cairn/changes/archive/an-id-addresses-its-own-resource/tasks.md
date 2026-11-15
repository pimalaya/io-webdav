---
cairn: tasks
change: an-id-addresses-its-own-resource
---

# Tasks

- [x] `WebdavIdError`, `check_collection_id`, `check_resource_id` beside `collection_path` (src/rfc4918.rs); `collection_path` and both `join_path` docs point at them.
- [x] src/client.rs: every method taking a collection, item or card id checks it before any request, refusing as `Io` of kind `InvalidInput`; the `Io` variant's doc says so.
- [x] Tests: tests/rfc4918.rs (each refused form, clean relative and absolute ids keeping their `collection_path`, item and card ids); tests/client.rs (every collection, item and card method writes no byte for a refused id; a clean relative, trailing-slash and absolute id address the paths they did before). Both client tests fail with the check neutralised.
- [x] cargo-semver-checks 0.47.0 against v0.9.0 as a patch release: no finding.
- [x] fmt, `cargo test --all-features`, `cargo test --no-default-features`, clippy `--all-targets -D warnings` over all, default and no default features, rustdoc with broken links denied, `cargo deny check`.
- [x] CHANGELOG; spec (webdav-core.md and client.md, see delta.md) folded; log cairn/log/2026-11-15-an-id-addresses-its-own-resource.md; status landed.
- [ ] An agent runs no git write command; the owning session commits and releases 0.9.1.
