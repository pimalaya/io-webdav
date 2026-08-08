---
cairn: change
id: guidelines-alignment
status: landed
created: 2026-08-08
---

# Apply the Pimalaya guidelines and adopt Cairn

## Why

The crate was last aligned on 2026-07-16, before the guidelines were restructured into atomic rules and before Cairn superseded the docs/ folder. A conformance pass found gaps across every group: a docs/ folder where cairn/ belongs, no activation stanza, a SECURITY.md pinned to a version line the crate left behind, a copyright holder and year that match neither the canonical string nor the repository's own history, em dashes in rustdoc, and a crate header describing a CalDAV layer that has since caught up with CardDAV.

## What

Walk the whole rule set, apply every fix, and record the result as a per-rule table.

- Migrate docs/ to cairn/, converting the two plans into spec capabilities, a change record and log entries.
- Add the AGENTS.md activation stanza.
- Fix the manifest authors, both license files, SECURITY.md, and the CONTRIBUTING reading order.
- Purge em dashes from rustdoc and comments, and route the one super import through crate.
- Refresh the crate header so it describes the layer as it now stands.
