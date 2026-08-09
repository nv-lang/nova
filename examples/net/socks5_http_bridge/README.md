<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# `socks5_http_bridge` — a local HTTP-to-SOCKS5 bridge

`main.nv` here is Plan 249's HTTP-to-SOCKS5 bridge
(`docs/plans/249-socks5-http-bridge-example.md`): some SOCKS5 proxies
require a username/password (RFC 1929) that Windows' system HTTP-proxy
setting cannot carry. This bridge listens as a plain HTTP proxy on
`127.0.0.1` and re-packages traffic into an authenticated SOCKS5 tunnel to
the real proxy:

```
browser --HTTP--> 127.0.0.1:PORT --SOCKS5+auth--> upstream proxy --> internet
```

It is deliberately built straight over `std.net` (raw `TcpListener`/
`TcpStream`), NOT `nova-polaris`: polaris' server parser only understands
ordinary request URLs, not CONNECT's authority-form target (`host:port`, no
scheme/path, RFC 7230 §5.3.3).

## Two request paths

- **CONNECT (Ф.2, primary path)** — a browser configured with
  `127.0.0.1:PORT` as its **HTTPS** proxy sends `CONNECT host:port
  HTTP/1.1`. The bridge tunnels to `host:port` via `socks5_connect`,
  answers `200 Connection Established`, then relays raw bytes
  bidirectionally (`pipe_bidirectional`) — this is how HTTPS traffic
  crosses the bridge; the bridge never sees the TLS payload.
- **Plain HTTP-over-proxy (Ф.3, secondary path)** — a browser configured
  with `127.0.0.1:PORT` as its plain **HTTP** proxy sends an ordinary
  method with an absolute-URI target instead of CONNECT, e.g.
  `GET http://example.com/path HTTP/1.1` (RFC 7230 §5.3.2). The bridge:
  1. extracts `host`/`port` from the absolute-URI (default port 80 for
     `http://`; only the `http://` scheme is understood — a client
     proxying HTTPS is expected to use CONNECT instead);
  2. tunnels to that `host:port` via `socks5_connect`, exactly like the
     CONNECT path;
  3. **rewrites the request line to origin-form** (`GET /path HTTP/1.1`)
     before forwarding — RFC 7230 §5.3.1 is what an origin server is
     guaranteed to accept, and not every real-world server tolerates
     receiving the absolute-form line a proxy client sends;
  4. **drops every `Proxy-*` header** (`Proxy-Connection`,
     `Proxy-Authorization`, ...; matched case-insensitively) before
     forwarding — those are addressed to this bridge, not the origin
     server;
  5. forwards the rewritten request head to upstream, then relays the
     rest bidirectionally with the same `pipe_bidirectional` the CONNECT
     path uses (the origin server's response arrives through the tunnel
     like any other byte — the bridge does not synthesize a response of
     its own for this path).

  A request that is neither a CONNECT authority-form target nor a
  recognized `http://` absolute-URI gets an honest `400 Bad Request`
  (previously used `501 Not Implemented` as a "not yet supported" stub
  before Ф.3 — since Ф.3 shipped, an unrecognized target is really just
  malformed, not merely unimplemented).

  **Deliberately NOT in scope** (plan 249 §2): keep-alive connection
  pooling, HTTP/2, response caching, request/response body rewriting.

## Configuration (`Os` effect)

| Var / arg | Required | Meaning |
|---|---|---|
| `SOCKS5_PROXY` | yes | `host:port` of the upstream SOCKS5 proxy |
| `SOCKS5_USER` | no | RFC 1929 username (paired with `SOCKS5_PASS`) |
| `SOCKS5_PASS` | no | RFC 1929 password |
| `argv[1]` | no | local listen port (default `8899`) |

```sh
SOCKS5_PROXY=proxy.example.com:1080 SOCKS5_USER=me SOCKS5_PASS=secret \
  nova build examples/net/socks5_http_bridge/main.nv -o bridge && ./bridge 8899
# Point a browser's HTTP *and* HTTPS proxy settings at 127.0.0.1:8899.
```

## Known V1 limitations

- No idle/read timeout on an established tunnel — a connection that never
  closes on its own lives until a peer closes it or the process exits
  (plan 249 §6 risk register: accepted for an example, a production bridge
  would want idle timeouts too).
- IPv6 literal authorities (`[::1]:port`) are not supported by either path
  (same V1 decision as the `nova-socks` client itself — plan 249 §7 п.2).
- Header block is capped at 64 KiB (`MAX_HEADER_BYTES`); exceeding it is a
  typed `431 Request Header Fields Too Large`, not a silent hang.

## Manual smoke test (NOT run in CI)

A full round trip needs a real external SOCKS5 proxy, so this is not part
of the automated gate — `nova build --strict-effects` (compiles the whole
file) and `nova lint` are the CI-checked gates; the following is a manual,
by-hand check.

> **The build gates prove the example COMPILES, not that it WORKS.** That
> distinction is not pedantic here: on 2026-08-09 both gates were green while
> the bridge could not move a single byte (see "Current status" below). Treat
> this smoke test as the acceptance gate, not the build.

Credentials live in a git-ignored `.env` next to this README; copy the
committed template and fill it in:

```sh
cp examples/net/socks5_http_bridge/.env.example \
   examples/net/socks5_http_bridge/.env
$EDITOR examples/net/socks5_http_bridge/.env

set -a && . examples/net/socks5_http_bridge/.env && set +a
nova build examples/net/socks5_http_bridge/main.nv --strict-effects -o bridge
./bridge "${LISTEN_PORT:-8899}"
```

**Run this control FIRST, before blaming the bridge** — it talks to the proxy
directly, with no Nova code in the path:

```sh
curl --socks5-hostname "$SOCKS5_PROXY" \
     --proxy-user "$SOCKS5_USER:$SOCKS5_PASS" https://api.ipify.org
```

If that fails, the proxy or the credentials are at fault. If it prints an
external IP and the bridge still does not work, the bridge is at fault — and
the difference between those two outcomes is exactly what makes this control
worth the extra command.

**CONNECT path:** point a browser's HTTPS proxy setting at
`127.0.0.1:PORT` and browse an HTTPS site — the bridge should log nothing
unusual, and the site should load normally through the configured SOCKS5
proxy.

**Plain path:** point a browser's HTTP (not HTTPS) proxy setting at
`127.0.0.1:PORT` and browse a plain HTTP site, or send a raw request by
hand:

```
GET http://example.com/ HTTP/1.1
Host: example.com
Proxy-Connection: keep-alive

```

Two things are directly verifiable without a real upstream SOCKS5 server
at all — start the bridge with `SOCKS5_PROXY` pointed at any TCP listener
that speaks enough SOCKS5 to observe the handshake bytes (a minimal script
is enough): the bridge's SOCKS5 CONNECT request correctly encodes the
target `host`/`port` parsed out of the absolute-URI, and the request head
it forwards is confirmed rewritten to origin-form (`GET / HTTP/1.1`, no
`Proxy-Connection`) with `Host`/other ordinary headers preserved verbatim.
