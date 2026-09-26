---
cairn: tasks
change: carddav-query-filter
---

# Tasks

- [x] src/rfc6352/filter.rs: `CarddavFilter`, `CarddavFilterTest`, `CarddavPropFilter`, `CarddavPropCond`, `CarddavParamFilter`, `CarddavParamCond`, `CarddavTextMatch`, `CarddavMatchType`, and their XML.
- [x] src/rfc4918.rs: `escape_attr`; an element matcher shared by `DuplicateUid` and `UnsupportedFilter`.
- [x] src/rfc6352/addressbook.rs: `addressbook_query_body` takes the filter and the limit.
- [x] src/rfc6352/card/list.rs: `CarddavCardListOptions`, `CarddavCardListOk` with `truncated`, the unsupported-filter classification.
- [x] src/rfc4918/send.rs: `WebdavSendError::UnsupportedFilter`.
- [x] src/client.rs: `list_cards` takes the options and returns `CarddavCardListOk`.
- [x] Tests: filter XML for every condition and escaping, the limit, truncation, both preconditions and a bare 403.
- [x] `cargo test`, `cargo clippy --all-targets`, `cargo fmt`, tarpaulin at 100%.
- [x] CHANGELOG under [Unreleased].
- [x] Fold the delta into cairn/spec/carddav.md, write the log entry, mark `landed`.
