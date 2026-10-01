---
cairn: log
change: connect-proxy
landed: 2026-10-01
---

# Proxy on connect

`WebdavClientStd::connect` now takes `WebdavClientStdConnectOptions { tls, proxy }` after the credential, the shape io-jmap uses, and threads the proxy into both the plain TCP and the TLS stream. The default is what the old signature did, so a caller passing `Default::default()` connects as before. Requested by Cardamum, which had to open the stream itself to honour its `proxy` option.

Breaking for every caller of `connect`. Verified with the full test suite and clippy over all features, none, and `client` alone.

The [client](../spec/client.md) capability moved: the full client requirement now names the proxy.
