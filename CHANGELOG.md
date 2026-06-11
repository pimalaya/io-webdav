# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.1] - Unreleased

### Added

- Initial release: WebDAV (RFC 4918), CalDAV (RFC 4791) and CardDAV (RFC 6352) coroutines + std client.
- Exposed `WebdavClientStd::stream` (and the `WebdavStream` trait) so higher-level crates can pump their own `WebdavCoroutine`s against the connected stream while reusing the client's discovery cache (mirrors io-jmap's public stream).

[0.0.1]: https://github.com/pimalaya/io-webdav/releases/tag/v0.0.1
