---
cairn: log
change: multistatus-depth-cap
landed: 2026-10-11
---

# The multistatus parser is bounded

An audit found `parse_multistatus` (src/rfc4918.rs) cloning each closed element's subtree into its parent and appending its text to every ancestor: quadratic, and about 35 KB of nested elements overflowed a 1 MiB stack through the recursive clone of `WebdavPropChild`, killing an Android process from any CalDAV or CardDAV server. Children are now moved, structural elements (multistatus, response, propstat, prop) no longer accumulate descendant text, and nesting past `MULTISTATUS_MAX_DEPTH` (64) fails with `WebdavMultistatusParseError::TooDeep`, surfaced as `WebdavSendError::Multistatus` and `WebdavFollowRedirectsError::Multistatus`. `parse_multistatus` returns a `Result`, a breaking change.

Verified: tests/rfc4918.rs parses 6000 and 200000 nested levels to the typed error on a 1 MiB thread, keeps the full tree at the limit, and parses 20000 responses with nested markup; offline suite green, clippy clean on all features.

Capabilities moved: [webdav-core](../spec/webdav-core.md) MODIFIED "Multistatus parsing".
