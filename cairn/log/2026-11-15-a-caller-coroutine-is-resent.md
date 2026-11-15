---
cairn: log
change: a-caller-coroutine-is-resent
landed: 2026-11-15
---

# A caller's coroutine is resent

`WebdavClientStd::run` is public: a coroutine the caller builds runs over the client stream with the resend every client method gets when the server closed the connection after the previous request (line K27 of the CLI alignment round, from W8).

**The gap.** calendula sends `caldav report raw` (src/caldav/report/raw.rs) and the PROPFIND of `account check` on a pinned home (src/account/check.rs) by writing and reading `WebdavClientStd::stream` itself; cardamum does the same for `carddav report raw` (src/carddav/report/raw.rs) and `propfind_home` (src/carddav/client.rs), neverest in its own `run` (src/dav/client.rs). A server closing the connection after a response (`Connection: close`, an idle keep-alive) failed those requests, where the same request through a client method is resent once on a fresh connection.

**The fix.** The private pump every method goes through, `run`, is public and documented for that use; the `stream` field and the module header point at it. The line suggested a raw request method answering a status and a body; the callers already build a `WebdavReport` or a `WebdavPropfind` and read its parsed multistatus and typed `WebdavSendError`, so the pump takes them unchanged, with one method and no type added. `run_redirect` stays private.

**Capabilities moved.** client: "Light client" modified.

**Verified.** `a_coroutine_the_caller_runs_is_resent_too` (tests/client.rs) runs a `WebdavReport` and a Depth 0 `WebdavPropfind` through `run` over closed streams: each is resent on a fresh connection and answered, two reconnects. Built against the 0.10.0 sources it fails to compile, `run` being private. Checks as for a-served-href-beside-the-id.

**Semver.** A public method added, minor on its own; it ships with a-served-href-beside-the-id in 0.11.0.

**Follow-ups, not done here.** calendula: src/caldav/report/raw.rs drops `run_report` for `client.run(coroutine)?`; src/account/check.rs `check_caldav` replaces its loop with `client.run(propfind)?`. cardamum: src/carddav/report/raw.rs drops `run_report` the same way; src/carddav/client.rs `propfind_home` returns `client.run(coroutine).map(drop)`. neverest: src/dav/client.rs drops its free `run` for `dav.run(coroutine)` in `propfind_href`. carillon's `pump` and `pump_redirect` (src/dav.rs) check a shutdown flag between timed-out reads, which `run` does not, and stay.
