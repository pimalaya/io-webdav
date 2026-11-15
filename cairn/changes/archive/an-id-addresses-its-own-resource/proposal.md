---
cairn: change
id: an-id-addresses-its-own-resource
status: landed
created: 2026-11-15
---

# An id addresses its own resource

## Why

Line W1 of the CLI alignment round (.github/cairn/changes/cli-alignment-2026-11-15/tasks.md): remote data at risk, found by reading and never tried on a server.

`rfc4918::collection_path(home, id)` joins a collection id under the home set as given. The id `/` gives `/`, the server root; `..` gives `<home>/../`, which the url crate itself resolves to the home set's parent before the request line is written (`Url::set_path` and `Url::join` resolve dot segments, `%2e%2e`, `%2E.` and `.%2e` included, and take `\` as a separator under http and https); `""` gives `<home>//`; `a/../..` and `./x` climb too. The item and card `join_path` are the same: an empty item id or `.` addresses the calendar itself and `..` the home set, so `delete_item(calendar, "..")` sends a DELETE to the home set.

cardamum, calendula and neverest hand the client ids a user typed (`cardamum addressbook delete -k <id>`, calendula's calendar delete and update, the carddav and caldav protocol commands), so a DELETE or a PROPPATCH may reach the server root, the principal or the home set.

## What

- `rfc4918::check_collection_id(id)` and `rfc4918::check_resource_id(id)`, returning `Result<(), rfc4918::WebdavIdError>`, `WebdavIdError(pub String)` carrying the id.
- A collection id is refused when, its trailing slashes dropped and then the leading slash of an absolute id, it is empty or holds an empty, `.` or `..` segment. An item or card id is refused when, its leading slashes dropped, it is empty or holds such a segment. Segments are split on `/` and `\` and compared percent-decoded.
- Every `WebdavClientStd` method taking a collection, item or card id checks it after the home set and before building any request: the collection methods and the item and card methods (`calendar_path` and `addressbook_path`), the calendar or address book's own id in create and update, the move target, each id of a multiget. A refused id fails with `WebdavClientStdError::Io` of kind `InvalidInput` carrying the `WebdavIdError`.
- `collection_path`, both `join_path` and every coroutine constructor keep their signatures and their output; their docs point at the checks.

## Decisions

- **Non-breaking, so 0.9.1.** No error enum of the crate is `#[non_exhaustive]`, so a new variant of `WebdavSendError` or `WebdavClientStdError` breaks every exhaustive match downstream, and every coroutine constructor returns `Self`. The refusal therefore lives in new public functions plus a guard in the client methods, returning the existing `Io` variant: `io::ErrorKind::InvalidInput` is std's kind for an argument refused, and the boxed `WebdavIdError` downcasts. A dedicated client variant waits for the next breaking release.
- **The coroutines stay unchecked.** A coroutine pumped by hand joins its ids as given, and its caller checks them with the two functions; the client, which every app's destructive path goes through, checks them itself.
- **Trailing slashes stay tolerated on a collection id**, as `collection_path` always trimmed them: `contacts/` and `contacts//` address `<home>/contacts/`. A trailing slash on an item or card id is refused: it would name a collection, and a listing never returns one.
- **Leading slashes stay tolerated on an item or card id**, as `join_path` always trimmed them.
- **Encoded and backslash forms count**: the url crate resolves `%2e%2e` and `a\..` before the request leaves, and a server decoding `%2F` or `%5C` may resolve `a%2F..`, so segments are compared percent-decoded and split on both separators.
- **Items and cards too**: the same defect holds for them, worse for an empty id (the DELETE of an item reaches its whole calendar).

## Scope / non-goals

- No version bump: the release (0.9.1) is the owner's.
- The apps move to 0.9.1 in their own lines (cardamum, calendula, carillon, neverest). Two build a path from an id themselves and should call the checks: cardamum's report raw command (src/carddav/report/raw.rs) and carillon's `under` (src/dav.rs).
