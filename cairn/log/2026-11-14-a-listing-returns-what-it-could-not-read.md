---
cairn: log
change: a-listing-returns-what-it-could-not-read
landed: 2026-11-14
---

# A listing returns what it could not read

A card or item listing and a card or item multiget return every row they could not read beside their entries, where they dropped it, so cardamum and calendula can name each card or item a listing missed.

**The unreadable entry.** `rfc4918::typed::WebdavUnreadableEntry { id, href, status, failures }` is a response naming a resource that carried no data: a response-level 403 or 404, a refused `address-data` or `calendar-data` propstat (its status in `failures`), or data missing or empty under a 2xx. `card_from_response` and `item_from_response` answer `Some(Ok(entry))`, `Some(Err(unreadable))` or `None`; `None` stays for the self-entry, the id-less href and, new, a 507 row, which names the collection and only sets `truncated` even when the server drops the trailing slash. The plan's `WebdavUnreadRow` and `unread` landed as `WebdavUnreadableEntry` and `unreadable`, io-maildir's names for the same change, since `unread` reads as the unseen flag in a PIM family.

**The outputs.** `CarddavCardListOk` gains `unreadable`. `CaldavItemList` returns `CaldavItemListOk { items, truncated, unreadable }`, `truncated` read from a 507 row as the card listing reads it; it returned a bare `BTreeSet<CaldavItemEntry>`. `CarddavCardMultiget` and `CaldavItemMultiget` return `CarddavCardMultigetOk { cards, unreadable }` and `CaldavItemMultigetOk { items, unreadable }`, where they returned a bare `Vec`, so a multiget naming a missing card or item says so. `WebdavClientStd::list_items`, `multiget_items` and `multiget_cards` return the new outputs. All breaking. The sync-collection listing and the enumeration spine are unchanged.

**Capabilities moved.** carddav and caldav: the Listing requirement returns the unreadable rows (caldav's also reports truncation), and the new requirement "A listing returns the rows it could not read" covers listings and multigets.

**Verified.** `list_cards_returns_the_rows_it_could_not_read` and `list_items_returns_the_rows_it_could_not_read` (an entry, a self-entry, a 403 row, a 404 data propstat, a 507 row), `multiget_cards_returns_a_missing_card` and `multiget_items_returns_a_missing_item` failed with the projections answering `None` for a data-less row, as before; `list_cards_skips_a_truncation_row_naming_the_collection_bare` failed without the 507 skip. The existing list and multiget tests now see their data-less fixture row as unreadable. Offline suite, clippy with `-D warnings` over all features, fmt and rustdoc clean; clippy `--no-default-features` is clean on the library, its `--all-targets` run failing on the test targets at 185ab63 already (they import `io_webdav::client` ungated). The live suites were compiled and not run. neverest, which calls both multigets, pins 185ab63 and adapts when it re-pins.
