---
cairn: change
id: an-id-is-sent-percent-encoded
status: landed
created: 2026-11-15
---

# An id is sent percent-encoded

## Why

Line W4 of the CLI alignment round (.github/cairn/changes/cli-alignment-2026-11-15/tasks.md), from calendula's K5: `rfc4918::collection_path` and the item and card `join_path` join a relative id as given, so a byte a path segment cannot hold raw reaches the request (cardamum `addressbook create "My Book"` on CardDAV).

Checked against the url crate before the change: a space, `?`, `#`, `"`, `<`, `>`, a backtick, braces and non-ASCII bytes were escaped when the request URL was built, so `My Book` already left as `My%20Book` on the request line. The rest was not:

- `\` became `/` (the url crate takes it as a separator under http and https), so the address book `a\b` addressed the collection `a/b/`;
- `[`, `]`, `|` and `^` left raw, outside RFC 3986's path grammar;
- a `%` not starting an escape (`50%off`) left raw, which a server decodes as it likes;
- the hrefs of a multiget are written into the XML body, never through the url crate, so `My Card.vcf` was sent as a raw href.

## What

- A crate-private `rfc4918::encode_segments`: every byte of a relative path outside RFC 3986 `pchar` (unreserved, sub-delimiters, `:` and `@`) is escaped as `%XX`, uppercase; `/` stays the segment separator and an escape already there (`%` and two hex digits) stays as it is.
- `collection_path` encodes a relative collection id, the item and card `join_path` the resource id. An id starting with a slash stays the server's path, as given.
- `encode_id` shares the escape writer; its output (unreserved bytes and escapes only) passes through unchanged.

## Decisions

- **An existing escape is kept.** An id read from a listing is an href segment, already encoded: it must be sent byte-identical, or `My%20Book` would become `My%2520Book`. A `%` followed by two hex digits is therefore an escape, even in a name a caller typed; a caller who means a literal `%41` mints the name through `encode_id`.
- **The checks judge the path sent.** The encoding keeps every escape and adds only ones that decode to the byte they replace, and W1's checks compare segments percent-decoded, so an id and its encoded path pass or fail alike (`a\..` still fails; `a%5C..`, its encoded form, too).
- **`pchar` stays raw.** RFC 3986 lets a segment hold the sub-delimiters, `:` and `@`, and servers list ids holding them (`a%40b` or `a@b`); escaping them would rename what a listing returned.
- **Absolute ids untouched.** A collection outside the home set is named by the path its href gave, encoded as the server served it.

## Scope / non-goals

- No version bump: the release (0.9.1) is the owner's.
- cardamum's raw report and carillon's `under` build DAV paths themselves (see W2); they encode nothing yet.
