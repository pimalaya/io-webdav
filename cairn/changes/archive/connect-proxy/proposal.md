---
cairn: change
id: connect-proxy
status: landed
created: 2026-10-01
---

# Proxy on connect

## Why

`connect` took the TLS settings as a bare argument and left the proxy at pimalaya-stream's default, so a caller wanting a configured proxy had to open the stream itself and use `new`. io-jmap, io-msgraph and io-imap all take one through their connect options.

## What

`connect(url, auth, WebdavClientStdConnectOptions { tls, proxy })`, the io-jmap shape. The default keeps the old behaviour: TLS backend default, proxy resolved from the environment.
