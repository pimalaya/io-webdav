---
cairn: log
change: a-listed-id-is-decoded
landed: 2026-11-15
---

# A listed id is decoded

An id read from a path segment is that segment percent-decoded when the decoding encodes back to it, so an id round-trips through create, list and address (line W7 of the CLI alignment round, from cardamum's W2).

**The defect.** Since 0.9.1 sends a relative id percent-encoded, an address book created as `My Book` is stored as `My%20Book`, and listings kept the href's last segment raw: cardamum `addressbook create "My Book"` printed an id its listings never showed. A card or item create returned the encoded segment it sent, the same split.

**The fix.** A crate-private `rfc4918::segment_id` decodes a segment when the decoding is UTF-8, passes `check_resource_id` and `encode_segments` gives the segment back byte for byte; any other segment stays as served. `collection_id` (its last-segment form), `id_from_location` (and through it the id a create, update or move returns), `WebdavResponseEntry::id` and `WebdavResponseOf::id` (now `Cow<str>`, and through them every item and card listing, enumeration, multiget and unreadable row) read through it. A path id (a collection outside the home set) stays as served, being sent as given.

**Kept raw, beyond the line.** The line asked to keep raw a decoded `/` or anything the checks refuse. The byte-for-byte rule also keeps raw an escape left in the decoding (`a%2541`), an escape of a byte the encoder writes raw (`a%40b`, `urn%3Auuid`, `card%2D1`) and lowercase hex (`caf%c3%a9`): decoding those would send a path other than the one served, relying on the server reading both alike. With the rule, every listed id is sent exactly as 0.9.1 sent it.

**Capabilities moved.** webdav-core: "A listed segment is decoded" added; "Resource identity" and "A relative id is sent percent-encoded" modified. caldav: "Calendar identity" and "Item identity" modified. carddav: "Card identity" modified.

**Verified.** `a_listed_segment_is_decoded_when_its_encoding_gives_it_back` (tests/rfc4918.rs), `a_listed_calendar_or_item_id_is_decoded_when_its_encoding_gives_it_back` (tests/rfc4791.rs), `a_listed_card_id_is_decoded_when_its_encoding_gives_it_back` (tests/rfc6352.rs), `an_id_round_trips_through_create_list_and_address` (tests/client.rs) and the three returned-id tests of 0.9.1, now expecting the decoded id, fail with `segment_id` returning the segment as it stands, and pass with it. Checks as for a-calendar-color-in-the-ical-namespace, all clean.

**Semver.** Breaking twice: `id()` returns `Cow<str>` (cargo-semver-checks 0.47.0 has no lint for it and passes), and the ids consumers store or compare change form for a segment holding a space, a non-ASCII letter or another byte the encoder escapes. An id read before still addresses its resource.

**Follow-ups, not done here.** carillon src/dav.rs `refresh_alarms` names a member by its raw href segment (`resource`) while the alarm book is keyed by listed ids and the multiget answers decoded ones: read the name with `id_from_location`, or a changed event's alarm is forgotten and a removed one's kept. neverest src/dav/client.rs keys items by its own `href_id` spelling of sync hrefs but takes ids from `fetch_bodies` (multiget and unreadable rows), `create` and `move_item`, and collection keys from `list_collections` and `collection_key`: re-encode each through `collection_path` or `join_path` before keying, or batched hydration misses decoded ids and a collection named with a space changes key. calendula src/caldav/backend.rs `update_calendar` compares a given id with listed ones as strings: an id read before (`My%20Cal`) no longer matches.
