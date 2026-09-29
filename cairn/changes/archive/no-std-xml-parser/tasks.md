---
cairn: tasks
change: no-std-xml-parser
---

- [x] Replace quick-xml with xmlparser in `has_element` and `parse_multistatus`.
- [x] Decode text and attribute values in-crate (line ends, attribute whitespace, references).
- [x] Stop the parse on a mismatched close, as quick-xml did.
- [x] Cover the new decoding, keep 100% coverage, run the Radicale and Stalwart suites.
- [x] Build for `thumbv7em-none-eabihf`.
