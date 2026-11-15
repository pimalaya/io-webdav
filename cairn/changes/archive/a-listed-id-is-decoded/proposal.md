---
cairn: change
id: a-listed-id-is-decoded
status: landed
created: 2026-11-15
---

# A listed id is decoded

## Why

Line W7 of the CLI alignment round (.github/cairn/changes/cli-alignment-2026-11-15/tasks.md), from cardamum's W2: since an id is sent percent-encoded (an-id-is-sent-percent-encoded), a collection created as `My Book` is stored as `My%20Book`, and a listing, which kept the href's last segment raw, returned `My%20Book`. cardamum `addressbook create "My Book"` printed an id its listings never showed. Both address the same collection; only the spelling differed. Items and cards had the same split, a create returning the encoded segment it sent.

## What

- A crate-private `rfc4918::segment_id`: a segment is percent-decoded when the decoding is UTF-8, passes `check_resource_id`, and `encode_segments` gives the very segment back; otherwise it stays as served.
- Every id derived from a segment goes through it: `collection_id` (the last-segment form; a path id stays as served), `id_from_location` and so the id a create, update or move returns, `WebdavResponseEntry::id` and `WebdavResponseOf::id` (now `Cow<str>`) and so every item and card listing, enumeration, multiget and unreadable row.
- The docs that called a listed id verbatim say how it is read.

## Decisions

- **Decode only what encodes back byte for byte.** An id read from a listing is sent back through `encode_segments`. Requiring the encoding to give the server's segment back makes the request path exactly the one 0.9.1 sent for that listing, on every server, and makes the decoding and the encoding inverse on every id it decodes. A segment that does not qualify stays as served and still addresses its resource:
  - a decoded `/` (`a%2Fb`) would address two segments;
  - an escape left in the decoding (`a%2541`) would be kept as an escape and address `aA`;
  - an escape of a byte the encoder writes raw (`a%40b`, `urn%3Auuid`, `card%2D1`) or in lowercase hex (`caf%c3%a9`) does not come back as written; decoding it would rely on the server treating both spellings alike;
  - a decoding the checks refuse (`%2E%2E`, `a%5C..`) or that is no UTF-8 (`%FF`).

  The line asked to keep raw a decoded `/` or anything the checks refuse; the three middle cases are kept raw for the same reason, an id that would not be sent as the segment it came from.
- **A path id stays as served.** A collection outside the home set is named by its path, which `collection_path` sends as given, never encoded: decoding it would change the request.
- **Created ids read the same way.** A create returning the encoded segment while the listing returns the decoded one would split them again, the defect itself.
- **`id()` returns `Cow<str>`.** It now decodes, which a borrowed `&str` cannot hold.

## Scope / non-goals

- No version bump: the release is the owner's. The change alters ids consumers may store or compare (see the log entry), so it belongs in a release no consumer picks up by `cargo update` alone.
- The sync-collection report returns hrefs, not ids; a consumer deriving an id from one reads it with `id_from_location`.
