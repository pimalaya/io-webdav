---
cairn: log
change: first-release
landed: 2026-07-16
---

# Guidelines alignment and first release

Brought the crate to the Pimalaya standard and cut v0.1.0.

The types catch-all was retired: each subdomain's shared types and vocabulary moved into its own sibling module next to its folder, and every single-coroutine result type moved into that coroutine's file. The generic DAV vocabulary that both protocols speak, the sync token and the CalendarServer ctag among it, landed in the RFC 4918 module rather than being hoisted out of a calendar utils module.

Capabilities recorded for the first time: coroutines, discovery, client, packaging.
