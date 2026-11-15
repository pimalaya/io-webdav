---
cairn: log
change: a-served-href-beside-the-id
landed: 2026-11-15
---

# A served href beside the id

Every listed, enumerated, fetched or written item and card, and every listed calendar and address book, carries its href as served beside its decoded id (line W9 of the CLI alignment round, from W8).

**The gap.** Since 0.10 (a-listed-id-is-decoded) an entry carried its decoded id alone, so a client keying a store on the server's segment could not get it back: `My%20Cal` and a raw `My Cal`, which a non-conforming server serves, both read `My Cal`, and re-encoding the id gives `My%20Cal` for both. neverest's keys must stay what 0.9.1 gave, byte for byte, and it stayed on 0.9.1 for it. Two such calendars even collapsed into one element of the `BTreeSet` a listing returns.

**The fix.** `href: String` beside `id` on `CaldavItemRef`, `CaldavItemEntry`, `CarddavCardRef`, `CarddavCardEntry`, `CaldavCalendar` and `CarddavAddressbook`, the response's `<href>` as served, as `WebdavUnreadableEntry` already carried it. The item and card create, update and move outcomes carry the `Location` header as served when its last segment names the resource, else the path of the URL sent (the crate-private `sent_href`, which replaces `sent_id`), the `Destination` URL for a move; their id is read from it, the id 0.10 returned. The href's last segment, query and fragment dropped, is the 0.9.1 id byte for byte. The ids stay decoded. A collection given to a create ignores its href (`#[serde(default)]`).

**Capabilities moved.** webdav-core: "A served href beside the id" added. caldav: "Calendar properties" modified. carddav: "Address book properties" modified.

**Verified.** `a_listed_calendar_or_item_carries_its_href_as_served` and `a_written_item_carries_the_href_it_was_sent_to_or_located_at` (tests/rfc4791.rs), `a_listed_addressbook_or_card_carries_its_href_as_served` and `a_written_card_carries_the_href_it_was_sent_to_or_located_at` (tests/rfc6352.rs) run every listing, enumeration, multiget (with its unreadable rows) and write over a `%20` segment, a raw space, raw non-ASCII and a `%2F`, each exposing its exact served form; the relocation tests check an absolute `Location` kept whole. Built against the 0.10.0 sources (git archive v0.10.0), the four fail to compile, no `href` on any of the eleven types. fmt, tests with all and with no default features, doctests, clippy `--all-targets -D warnings` over all, default and no default features, rustdoc with broken and private links and missing docs denied, and `cargo deny check` are clean. No live suite ran.

**Semver.** Breaking: cargo-semver-checks 0.47.0 against v0.10.0, all features, fails `constructible_struct_adds_field` on the eleven `href` fields (cardamum builds `CarddavAddressbook` field by field) and warns `partial_ord_struct_fields_reordered` (the derived order now compares `href` right after `id`). The release is 0.11.0.

**Follow-ups, not done here.** neverest (src/dav/client.rs) moves to 0.11 keyed on hrefs: `fetch_bodies` keys rows and gone rows by the href's last segment (or by `href_id`, its sync key), `create` and `move_item` by `created.href` and `moved.href`, `list_collections` by the href's last segment unless the id starts with a slash, and `collection_key` keeps the raw segment instead of `collection_id`'s decoded one; `rename_collection` builds a `CaldavCalendarPatch` (0.10). cardamum src/carddav/backend.rs and src/carddav/mkcol.rs fill `href` (or `..Default::default()`) in their `CarddavAddressbook` literals. calendula and carillon compile unchanged.
