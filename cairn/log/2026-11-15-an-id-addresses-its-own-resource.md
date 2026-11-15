---
cairn: log
change: an-id-addresses-its-own-resource
landed: 2026-11-15
---

# An id addresses its own resource

A collection, item or card id that would address another resource is refused before any request, so `cardamum addressbook delete -k ..` can no longer send a DELETE to the home set's parent. Found by reading (line W1 of the CLI alignment round), never tried on a server.

**The defect.** `collection_path` and both `join_path` joined an id as given, and the url crate resolves dot segments itself when the request URL is built: `/dav/books/../`, `/dav/books/%2e%2e/` and `/dav/books/a\..\../` all leave as `/dav/`. The id `/` addressed the server root, `""` the home set (`<home>//`), and an empty item id or `.` the calendar or address book holding it.

**The checks.** `rfc4918::check_collection_id` and `rfc4918::check_resource_id` return `Result<(), rfc4918::WebdavIdError>`, `WebdavIdError(pub String)` carrying the id. A collection id is checked with its trailing slashes dropped (tolerated, as `collection_path` always trimmed them) and then the leading slash of an absolute id; an item or card id with its leading slashes dropped (tolerated, as `join_path` trimmed them), so a trailing slash on it is refused. What remains is refused when empty or holding an empty, `.` or `..` segment, split on `/` and `\` and compared percent-decoded.

**The client.** Every `WebdavClientStd` method taking an id checks it once the home set is known, before any byte is written: the collection ids of every calendar, address book, item and card method, the own id of a calendar or address book to create or update, the move target, each item or card id and each id of a multiget. A refused id fails with `WebdavClientStdError::Io` of kind `InvalidInput`, the `WebdavIdError` boxed inside and downcastable. No error enum of the crate is `#[non_exhaustive]`, so a new variant would have broken exhaustive matches downstream; the existing `Io` variant keeps this a patch release, and a dedicated variant waits for the next breaking one. The coroutines and the path builders keep their signatures and output, joining ids as given; their docs and the crate header point at the checks.

**Capabilities moved.** webdav-core: "An id stays inside what it names" added. client: "A refused id sends nothing" added.

**Verified.** `a_collection_id_climbing_out_of_the_home_set_sends_nothing` (19 methods, 12 ids) and `a_resource_id_climbing_out_of_its_collection_sends_nothing` (11 methods, 9 ids) fail with the check neutralised and pass with it, the recording stream left empty; `a_clean_id_addresses_the_path_it_did_before` checks a relative, a trailing-slash and an absolute collection id and a leading-slash item id against the request lines they produced before. tests/rfc4918.rs covers each refused form and the clean ones. cargo-semver-checks 0.47.0 (nixpkgs rustc 1.95.0) against the v0.9.0 tree, all features, as a patch release: 222 checks pass, no semver update required. fmt, tests with all and with no default features, clippy `--all-targets -D warnings` over all, default and no default features, rustdoc with broken links denied, and `cargo deny check` are clean. No live suite ran.
