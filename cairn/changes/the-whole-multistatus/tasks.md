---
cairn: tasks
change: the-whole-multistatus
---

# Tasks

- [x] Regression tests written first and seen failing on the unfixed code, one per defect
- [x] `XmlScanner::is_done`, `WebdavMultistatusParseError::UnexpectedRoot` and `Incomplete`, the whole-document rule in `parse_multistatus` and in the typed envelope
- [x] `Tag::of` by parent in `parse_multistatus`
- [x] `WebdavHrefs`, the two home set lookups on `parse_multistatus_as`
- [x] `has_element` for `valid-sync-token` (any status) and `supported-report`
- [x] `same_path` and one `href_path` for the self-entry comparison
- [x] Invert the tests pinning the prefix of a malformed body
- [x] cargo fmt, offline suite, clippy with `-D warnings`, missing docs, rustdoc links, `--no-default-features` build
- [x] Fold the delta into spec/webdav-core.md, spec/sync.md and spec/discovery.md, log, CHANGELOG
