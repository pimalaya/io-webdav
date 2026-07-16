# Contributing guide

Thank you for investing your time in contributing to I/O WebDAV.

Whether you are a human or an AI agent, read these in order before touching the code:

1. the [Pimalaya README](https://github.com/pimalaya) for what the project is and how its repositories stack;
2. the [Pimalaya CONTRIBUTING](https://github.com/pimalaya/.github/blob/master/CONTRIBUTING.md) guide, which chains to the shared architecture and guidelines;
3. the inline header documentation, starting with src/lib.rs: it is the architecture document of this crate;
4. the docs/ folder for the development history and living plans.

Everything below documents only what differs from the Pimalaya standards.

## Coverage

The offline test suites (tests/rfc4918, rfc4791, rfc5397, rfc6352, rfc6578, client) resume every coroutine and client method against scripted HTTP responses and reach 100% line coverage. Coverage is measured with cargo-tarpaulin (LLVM engine), locally and in CI.

## Live provider tests

Next to the offline suites, ignored integration tests exercise the full coroutine flow against real CalDAV / CardDAV servers. All are gated behind --ignored.

Two run against a local server bootstrapped by a script:

```sh
./tests/radicale.sh
cargo test --test radicale -- --ignored
```

```sh
./tests/stalwart.sh
cargo test --test stalwart -- --ignored
```

The Radicale script runs the server in a container with a single htpasswd user over plain HTTP on port 5232; the Stalwart script provisions one domain and one user, serving DAV on port 8080.

Three run over HTTPS against a real account, each reading its credentials from the environment:

```sh
FASTMAIL_EMAIL="user@fastmail.com" FASTMAIL_APP_PASSWORD="<app-password>" \
  cargo test --test fastmail -- --ignored
```

```sh
GOOGLE_ACCESS_TOKEN="<oauth-access-token>" \
  cargo test --test google -- --ignored
```

```sh
ICLOUD_EMAIL="user@icloud.com" ICLOUD_APP_PASSWORD="<app-password>" \
  ICLOUD_CALENDAR_ID="<calendar-id>" ICLOUD_ADDRESSBOOK_ID="<addressbook-id>" \
  cargo test --test icloud -- --ignored
```

Each flow creates its own collections and cleans everything up on success.
