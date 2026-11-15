---
cairn: log
change: an-id-is-sent-percent-encoded
landed: 2026-11-15
---

# An id is sent percent-encoded

A relative collection, item or card id has each byte a path segment cannot hold raw percent-encoded before it is joined, an escape already there kept, so a listed id is still sent byte-identical (line W4 of the CLI alignment round).

**The defect.** `collection_path` and both `join_path` joined a relative id as given. The url crate escaped a space, `?`, `#` and non-ASCII bytes when the request URL was built, so `My Book` already left as `My%20Book`, but it turned `\` into `/` (the address book `a\b` addressed `a/b/`), left `[`, `]`, `|`, `^` and a `%` starting no escape raw, and a multiget's hrefs, written into the XML body, never went through it (`My Card.vcf` was sent as a raw href).

**The fix.** A crate-private `rfc4918::encode_segments` escapes every byte outside RFC 3986 `pchar` as `%XX`, keeping `/` and an existing escape; `collection_path` runs it on a relative id, the item and card `join_path` on the resource id. An id starting with a slash stays the server's path, as given. The escape writer is shared with `encode_id`, whose output passes through unchanged. The encoding adds only escapes decoding to the byte they replace, so W1's checks, which compare segments percent-decoded, judge an id and its encoded path alike.

**Capabilities moved.** webdav-core: "A relative id is sent percent-encoded" added; "An id stays inside what it names" modified (the builders encode rather than join as given).

**Verified.** `an_id_holding_reserved_bytes_is_sent_percent_encoded` (tests/client.rs: `My Book`, `a\b`, `50%off`, `a|b^[c].ics` and a multiget href encoded, a listed id and an `encode_id` name sent byte-identical), `a_relative_collection_id_is_percent_encoded` (tests/rfc4918.rs) and both `join_path` tests fail with the encoding neutralised and pass with it; `an_encoded_id_is_judged_as_the_id_it_came_from` holds both ways, and W1's refusal tests pass unchanged. cargo-semver-checks 0.47.0 (nixpkgs rustc 1.95.0) against the v0.9.0 tree, all features, as a patch release: 222 checks pass, no semver update required. fmt, tests with all and with no default features, clippy `--all-targets -D warnings` over all, default and no default features, rustdoc with broken and private links denied, and `cargo deny check` are clean. No live suite ran.
