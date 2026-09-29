---
cairn: log
change: no-std-xml-parser
landed: 2026-09-28
---

# no_std XML parser

io-webdav#1 reported that quick-xml pulls std into the `#![no_std]` core, breaking bare-metal builds. It has no no_std mode, so it was replaced by xmlparser 0.13 with default features off.

**Parsing is unchanged.** The multistatus parser keeps its stack machine and reads xmlparser tokens instead of quick-xml events. Text line ends normalise to LF and attribute whitespace to spaces (XML 1.0 §2.11, §3.3.3) before references resolve, as quick-xml did. A mismatched close still ends the parse, since the tokenizer does not pair tags. roxmltree was rejected because it parses all or nothing, which would lose the prefix of a truncated body.

Unreleased. Capabilities moved: packaging (MODIFIED: no_std core). Offline coverage grew by two tests (text and attribute normalisation, mismatched close) and stays at 100%. The Radicale and Stalwart suites pass, and the crate builds for `thumbv7em-none-eabihf`.
