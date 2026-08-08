---
cairn: log
change: guidelines-alignment
landed: 2026-08-08
---

# Pimalaya guidelines pass and Cairn adoption

Walked the whole atomic rule set and applied every gap it surfaced. The crate had last been aligned before the guidelines were restructured into per-rule ids and before Cairn superseded the docs/ folder.

The docs/ folder became cairn/. Its two staged plans, the CardDAV read-side sync plan and the CalDAV alignment plan, did not survive as plans: their current truth was folded into spec capabilities (coroutines, webdav-core, caldav, carddav, discovery, sync, client, packaging), their outcomes became dated log entries, and the CalDAV work became an archived change with its own proposal, tasks and delta. The pre-Cairn history was reconstructed from the git log and the changelog rather than invented, so it starts at the sync work rather than at the first commit. The AGENTS.md activation stanza landed at the repository root, and the CONTRIBUTING reading order now points at cairn/.

The one rule the crate failed wholesale was the naming canon: 123 of its 132 public items carried no domain prefix, and its coroutines were still verb-first. Every public type was renamed to read domain then target then verb. The domain is per protocol layer, following io-proxy, which prefixes its two protocol modules separately and keeps the crate name for the crate-spanning contract: Webdav for the RFC 4918 core, the discovery extension, the sync report and the coroutine and client contracts, Caldav and Carddav for their own layers. Module paths were left alone, so only the type names moved. Consts and free functions were left unprefixed, the canon governing types only for now.

Smaller conformance fixes: the manifest authors took the canonical holder string, both license files took it too and collapsed their year range to the repository's actual first-release year, SECURITY.md moved off a version line the crate had already left, the em dashes in rustdoc and inline comments were rewritten, and the one test-module super import was routed through crate. The crate header was refreshed, since it still described CardDAV as the only layer carrying batch multiget and ETag-only enumeration, and it now links the cairn folder as an inner resource.

A leak in the live suites was found while reviewing this pass and fixed with it. Those flows write to real production accounts, and their teardown was written as the last statements of each flow, so a failed assertion or a server hiccup skipped it and left a calendar, an address book, an event or a card behind for good, one per failed run. Teardown is now registered the moment the resource exists and runs however the flow exits, best-effort so a teardown that cannot reach the server reports the leftover by name instead of masking the failure the run was reporting. The same review found the card-only flow lagging its calendar twin, missing enumeration and multiget, so it gained both.

Spec updated: packaging (ADDED: coverage, live suites, live-suite teardown, byte orientation).
