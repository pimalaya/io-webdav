---
cairn: tasks
change: an-id-is-sent-percent-encoded
---

# Tasks

- [x] src/rfc4918.rs: `encode_segments`, `is_pchar`, `push_escape` (shared with `encode_id`); `collection_path` encodes a relative id.
- [x] src/rfc4791/item.rs and src/rfc6352/card.rs: `join_path` encodes the resource id; the docs of both create modules and the src/lib.rs header say so.
- [x] Tests: tests/rfc4918.rs `a_relative_collection_id_is_percent_encoded` and `an_encoded_id_is_judged_as_the_id_it_came_from`; tests/rfc4791.rs and tests/rfc6352.rs `*_join_path_percent_encodes_what_a_segment_cannot_hold`; tests/client.rs `an_id_holding_reserved_bytes_is_sent_percent_encoded` (request lines and a multiget href; a listed id and an `encode_id` name sent byte-identical). All but the judging test fail with the encoding neutralised; W1's refusal tests pass unchanged.
- [x] cargo-semver-checks 0.47.0 against v0.9.0 as a patch release: no finding.
- [x] fmt, `cargo test --all-features`, `cargo test --no-default-features`, clippy `--all-targets -D warnings` over all, default and no default features, rustdoc with broken and private links denied, `cargo deny check`.
- [x] CHANGELOG; spec (webdav-core.md, see delta.md) folded; log cairn/log/2026-11-15-an-id-is-sent-percent-encoded.md; status landed.
- [ ] An agent runs no git write command; the owning session commits and releases 0.9.1.
