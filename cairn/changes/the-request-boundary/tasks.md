---
cairn: tasks
change: the-request-boundary
---

# Tasks

- [x] Regression tests written first and seen failing on the unfixed code, one or more per defect
- [x] `WebdavHrefs` reading `href` children, then bare text when there is none
- [x] `unquoted` behind `read_etag` and `unquoted_etag`, `entity_tag` checking both ends
- [x] `sent_id` in the item and card create and update and in the item move, public `encode_id`
- [x] Trailing slash kept by the client's `calendar_path` and `addressbook_path`
- [x] `WebdavSendError::UnexpectedRedirect` carrying `url`, `keep_alive` and `same_origin`
- [x] `WebdavMemberFailure`, `WebdavSendError::MemberFailures`, `check_members` in `WebdavDelete`, `WebdavCopy`, `WebdavMove` and `CaldavItemMove`
- [x] `invalid_header` in `WebdavSendRaw` and `WebdavFollowRedirects`, `InvalidHeader` on both error types
- [x] `resolve_reference` refusing NUL and signed numbers
- [x] cargo fmt, offline suite, clippy with `-D warnings`, missing docs, rustdoc links, `--no-default-features` build
- [x] Fold the delta into spec/webdav-core.md, spec/client.md and spec/discovery.md, log, CHANGELOG
- [ ] Live suites against the five providers, the trailing slash now going out on every collection request (owner)
