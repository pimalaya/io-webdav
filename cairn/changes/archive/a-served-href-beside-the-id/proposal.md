---
cairn: change
id: a-served-href-beside-the-id
status: landed
created: 2026-11-15
---

# A served href beside the id

## Why

Line W9 of the CLI alignment round (.github/cairn/changes/cli-alignment-2026-11-15/tasks.md), from W8 (neverest). Since 0.10 (a-listed-id-is-decoded) a listed, enumerated or multiget entry carries its decoded id alone. A client keying a store on the segment the server served cannot get it back: a decoded `My%20Cal` and a raw `My Cal` a non-conforming server served both read `My Cal`, so re-encoding the id is not exact. neverest's keys must stay byte for byte what 0.9.1 gave, and it stays on 0.9.1 until it can. Only `WebdavUnreadableEntry` carried the href.

## What

- `href: String` beside `id` on `CaldavItemRef`, `CaldavItemEntry`, `CarddavCardRef` and `CarddavCardEntry` (listings, enumerations, multigets) and on `CaldavCalendar` and `CarddavAddressbook` (collection listings): the response's `<href>` as served.
- `href` on `CaldavItemCreateOk`, `CaldavItemUpdateOk`, `CaldavItemMoveOk`, `CarddavCardCreateOk` and `CarddavCardUpdateOk`: the `Location` header as served when its last segment names the resource, else the path of the URL sent (the `Destination` URL for a move). The returned id is read from that href, the id 0.10 returned.
- `WebdavUnreadableEntry::href` keeps its shape; its doc, like every new field's, says when to use which: `id` to address the resource and show it, `href` to key a store keeping the server's spelling.

## Decisions

- **The href, not its segment.** `WebdavUnreadableEntry`, `WebdavMemberFailure` and the sync-collection delta (`WebdavSyncChange::href`, `vanished`) already hand out hrefs: one shape everywhere lets a consumer key a sync href and a multiget row through the same function. The href's last segment, query and fragment dropped, is the 0.9.1 id byte for byte.
- **The id stays decoded**, as 0.10 made it.
- **A move's href is its `Destination`**, where the item now lives; the id, the same segment on both sides, is read from it.
- **A collection to create ignores its href**: create addresses `id`. The field is `#[serde(default)]`, so a collection serialized before deserializes.
- **No `#[non_exhaustive]`.** It would forbid the `..Default::default()` literals consumers build collections with.

## Scope / non-goals

- No version bump. A field added to a struct callers build literally breaks them (cardamum builds `CarddavAddressbook` field by field), so the release is a minor one, 0.11.0, the owner's call.
- No consumer changes here.
