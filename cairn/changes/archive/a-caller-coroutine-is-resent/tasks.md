---
cairn: tasks
change: a-caller-coroutine-is-resent
---

# Tasks

- [x] src/client.rs: `WebdavClientStd::run` public, its doc; the `stream` field and the module header point at it.
- [x] Test: tests/client.rs `a_coroutine_the_caller_runs_is_resent_too` runs a `WebdavReport` and a `WebdavPropfind` of Depth 0 (`caldav report raw` and `account check`) over a closed stream: each is resent on a fresh connection and answered. Against the 0.10.0 sources it fails to compile, `run` being private.
- [x] Checks as for a-served-href-beside-the-id.
- [x] CHANGELOG; spec (client.md, see delta.md) folded; log cairn/log/2026-11-15-a-caller-coroutine-is-resent.md; status landed.
- [ ] An agent runs no git write command; the owning session commits and releases.
