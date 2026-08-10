<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# `http_proxy_chain` — a local HTTP-to-SOCKS5 bridge

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

## File layout

One module, equal-standing files (plan 249 §9) — all declare `module
flagship.http_proxy_chain`:

| file | contents |
|---|---|
| `config.nv` | `Config`, `load_config` — listen port + upstream selection (§8 below) |
| `http.nv` | request-head reading and parsing, `Proxy-*` stripping, origin-form rewrite |
| `relay.nv` | `pipe_bidirectional`/`pump` — the bidirectional byte relay |
| `upstream.nv` | the `Upstream` effect, `UpstreamSource`, `all_proxy` URL parsing |
| `upstream_socks5.nv` | the SOCKS5 handler for the `Upstream` effect |
| `main.nv` | accept loop, per-connection dispatch, HTTP responses |

`Upstream` is an **effect**, not a value — see `upstream.nv`'s doc comment
for the full reasoning (ambient capability, protocol-switch-is-handler-
switch, testability). Adding a second protocol (e.g. SOCKS4) costs: a new
variant in `UpstreamSource`, a new arm in `UpstreamSource @to_handler()`, a
new handler-factory file (`upstream_socks4.nv`, mirroring
`upstream_socks5.nv`), and extending the scheme check in `str
@to_upstream_source()` — no EXISTING handler changes.

## Two request paths

- **CONNECT (Ф.2, primary path)** — a browser configured with
  `127.0.0.1:PORT` as its **HTTPS** proxy sends `CONNECT host:port
  HTTP/1.1`. The bridge tunnels to `host:port` via the `Upstream` effect
  (`Upstream.dial`), answers `200 Connection Established`, then relays raw
  bytes bidirectionally (`pipe_bidirectional`) — this is how HTTPS traffic
  crosses the bridge; the bridge never sees the TLS payload.
- **Plain HTTP-over-proxy (Ф.3, secondary path)** — a browser configured
  with `127.0.0.1:PORT` as its plain **HTTP** proxy sends an ordinary
  method with an absolute-URI target instead of CONNECT, e.g.
  `GET http://example.com/path HTTP/1.1` (RFC 7230 §5.3.2). The bridge:
  1. extracts `host`/`port` from the absolute-URI (default port 80 for
     `http://`; only the `http://` scheme is understood — a client
     proxying HTTPS is expected to use CONNECT instead);
  2. tunnels to that `host:port` via `Upstream.dial`, exactly like the
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

## Configuration (`Os` effect) — canon, plan 249 §8

Configuration follows the ordinary shape for this class of program (local
forward proxy): the listen port is a flag-or-env with a hardcoded fallback;
the upstream is a single `all_proxy`-style URL in the environment (the same
convention curl/git/docker already use); the password/credentials are
environment (or a file) ONLY, never argv — argv is visible to any user via
`ps`, and lands in shell history/supervisor logs.

**Priority, most specific wins:**

| what | chain |
|---|---|
| listen port | `argv[1]` → `env("LISTEN_PORT")` → `8899` |
| upstream | `SOCKS5_PROXY` (explicit) → `ALL_PROXY`/`all_proxy` URL → fatal error |

| Var / arg | Required | Meaning |
|---|---|---|
| `SOCKS5_PROXY` | see below | `host:port` of the upstream SOCKS5 proxy — explicit override, takes priority over `ALL_PROXY` |
| `SOCKS5_USER` | no | RFC 1929 username (paired with `SOCKS5_PASS`), only meaningful with `SOCKS5_PROXY` |
| `SOCKS5_PASS` | no | RFC 1929 password, only meaningful with `SOCKS5_PROXY` |
| `ALL_PROXY` / `all_proxy` | see below | `all_proxy`-compatible URL: `socks5://[user:pass@]host:port` or `socks5h://…` (both accepted, dispatch identically — see `upstream.nv`) |
| `argv[1]` | no | local listen port, overrides `LISTEN_PORT` |
| `LISTEN_PORT` | no | local listen port, overrides the `8899` default |

One of `SOCKS5_PROXY` or `ALL_PROXY`/`all_proxy` is required — the bridge
panics at startup with neither set (config validation happens once, at
startup, not as a silently-broken bridge that fails every connection
later).

```sh
SOCKS5_PROXY=proxy.example.com:1080 SOCKS5_USER=me SOCKS5_PASS=secret \
  nova build examples/flagship/http_proxy_chain/main.nv -o bridge && ./bridge 8899
# Point a browser's HTTP *and* HTTPS proxy settings at 127.0.0.1:8899.

# Equivalent, via a single all_proxy URL (what's already set for curl/git/docker):
ALL_PROXY=socks5://me:secret@proxy.example.com:1080 \
  nova build examples/flagship/http_proxy_chain/main.nv -o bridge && LISTEN_PORT=8899 ./bridge
```

**Listen address is hardcoded to the loopback** (`SocketAddr.loopback`) —
deliberately not configurable. A forward proxy reachable from anywhere but
the loopback, with no destination-port allowlist and no client
authentication, becomes an open relay within a day and starts carrying
someone else's traffic; that is the "sensible default" real proxies get
burned by. If this bridge is ever changed to listen on a non-loopback
address, a destination-port allowlist stops being optional and becomes
mandatory alongside it.

**Deliberately out of scope** (plan 249 §8, and why): a destination-port
allowlist and client access control (both moot while the listen address is
loopback-only), log levels, an idle timeout on the established tunnel, and
a config file. Four knobs total is under the threshold where a config file
earns its keep — `argv` + environment is the right mechanism for a program
this size (compare `gost`/`microsocks`, flag-only, vs. `cntlm`/`tinyproxy`/
`privoxy`/`3proxy`, dozens of settings, config-file-based).

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
> the bridge could not move a single byte (see plan 249's status header for
> the history). Treat this smoke test as the acceptance gate, not the build.

Credentials live in a git-ignored `.env` next to this README; copy the
committed template and fill it in:

```sh
cp examples/flagship/http_proxy_chain/.env.example \
   examples/flagship/http_proxy_chain/.env
$EDITOR examples/flagship/http_proxy_chain/.env

set -a && . examples/flagship/http_proxy_chain/.env && set +a
nova build examples/flagship/http_proxy_chain/main.nv --strict-effects -o bridge
./bridge
```

`./bridge` (no argv) picks the port up from `LISTEN_PORT` in `.env` —
`argv[1]` would override it if passed instead (§8 canon: argv beats env
beats default). To confirm the env chain is actually wired (not just
coincidentally matching the `8899` default), run once with `LISTEN_PORT=9001
./bridge` and verify the startup log line and a `curl -x
http://127.0.0.1:9001 ...` both agree.

To exercise the `ALL_PROXY` form specifically (as opposed to
`SOCKS5_PROXY`/`_USER`/`_PASS`), comment out `SOCKS5_PROXY` in `.env` and
uncomment the `ALL_PROXY=socks5://user:pass@host:port` line instead — the
startup log line should show the same `host:port` either way (with
credentials redacted).

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
