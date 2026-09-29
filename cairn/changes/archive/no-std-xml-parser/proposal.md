---
cairn: change
id: no-std-xml-parser
status: landed
created: 2026-09-28
---

# quick-xml pulls std into the no_std core

io-webdav#1: the crate is `#![no_std]`, yet quick-xml has no no_std mode, so a bare-metal build fails.

**Swap to xmlparser.** It builds without std and streams tokens, so a truncated multistatus still yields its prefix. roxmltree was rejected: it parses all or nothing. Text and attribute decoding, which quick-xml did, moves into the crate with the same XML 1.0 normalisation.
