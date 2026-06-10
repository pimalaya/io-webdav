# I/O WebDAV [![Documentation](https://img.shields.io/docsrs/io-webdav?style=flat&logo=docs.rs&logoColor=white)](https://docs.rs/io-webdav/latest/io_webdav) [![Matrix](https://img.shields.io/badge/chat-%23pimalaya-blue?style=flat&logo=matrix&logoColor=white)](https://matrix.to/#/#pimalaya:matrix.org) [![Mastodon](https://img.shields.io/badge/news-%40pimalaya-blue?style=flat&logo=mastodon&logoColor=white)](https://fosstodon.org/@pimalaya)

WebDAV (RFC 4918), CalDAV (RFC 4791) and CardDAV (RFC 6352) client library, written in Rust.

The crate ships I/O-free coroutines plus a standard, blocking [`WebdavClientStd`] gated behind the `client` feature. CalDAV is gated behind `rfc4791`, CardDAV behind `rfc6352`. TLS is provided by [pimalaya-stream](https://github.com/pimalaya/stream) via the `rustls-ring`, `rustls-aws` or `native-tls` features.

Visit [pimalaya.org](https://pimalaya.org) for more details about the Pimalaya stack.

## License

This project is licensed under either of:

- [MIT license](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.
