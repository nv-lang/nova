<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Nova TLS Example

> **Plan 193** (nova-tls extraction) **+ Plan 195** (native modules: C, not
> Rust). This pair is the worked example for *consuming* an external native
> module — see [nova-tls's own
> README](https://github.com/nv-lang/nova-tls#readme) for the *authoring*
> side (how the package itself is laid out).

## Files

| File | What |
|---|---|
| `echo_server.nv` | Accepts one TCP connection, TLS-handshakes as server, echoes one message |
| `echo_client.nv` | Connects, TLS-handshakes as client, sends a message, prints the echo |
| `certs/` | Self-signed `localhost` cert/key pair — **test-only fixture**, copied from `nova-tls`'s own `src/testdata/` (keys are public by design, see `nova-tls/src/testdata/README.md`); regenerate with the `openssl` recipe documented there before reusing this pattern for anything real |

Both files mirror `examples/net/echo_server.nv` / `echo_client.nv` 1:1 —
same `TcpListener`/`TcpStream`/`spawn`/`supervised` shape — with exactly one
extra step inserted: `TlsStream.accept(tcp, server_config)` /
`TlsStream.connect(tcp, client_config)` between the raw TCP handshake and
the read/write. Everything below the TLS handshake (`write_all`,
`read_to_vec`, `close`) is the same `io.Read`/`io.Write`-shaped surface
`TcpStream` already has — `TlsStream` is a drop-in encrypted replacement,
not a parallel API to relearn.

## Running it

```sh
nova build examples/tls/echo_server.nv -o echo_server && ./echo_server &
nova build examples/tls/echo_client.nv -o echo_client && ./echo_client
```

`nova check examples/tls/echo_server.nv examples/tls/echo_client.nv`
type-checks both files with no native library required — useful to verify
the code itself is sound. `nova build`/`nova run`, unlike `nova test`, has
no detect-and-degrade: it always tries to actually link, so it needs a real
mbedTLS (`mbedtls`/`mbedx509`/`mbedcrypto`) reachable exactly as described
below — without one, expect a linker error (`undefined symbol: tls_*`), not
a silent skip. `nova test std/http/transport` (a real `tls` consumer
already in the tree) is the easiest way to check your setup first — SKIP
means "no mbedTLS yet", not "broken".

## How the dependency is wired

`tls.*` is **not** part of `std` — it lives in a separate repository,
[`nova-tls`](https://github.com/nv-lang/nova-tls), a sibling checkout next
to this one (`../../nova-tls` relative to this repo's root). It is declared
as an ordinary external `path` dependency in `examples/nova.toml`:

```toml
[dependencies]
tls = { path = "../../nova-tls" }
```

This is the *exact same* mechanism `std/nova.toml` itself uses to pull in
TLS support for `std.http.transport` — an external package is not a
special case, it's declared and imported like any other dependency:

```nova
import tls.{TlsStream, ClientConfig, ServerConfig, VerificationMode}
```

`nova-tls` ships zero Rust/cargo: a `.nv` facade over a thin C shim
(`native/tls_c_shim.c`) linked against a prebuilt mbedTLS static library,
wired entirely through the standard `[ffi]` build pipeline (see
`docs/guide/ffi-cookbook.md` and `examples/ffi/README.md`) — no compiler-internal
special case. If mbedTLS isn't installed on your machine, `nova test`
degrades this package's tests to a clean `SKIP` (`[ffi] lib not found in
lib_dirs`) instead of a hard link failure — see `nova-tls/nova.toml`'s
`[ffi] lib_dirs` comment.

## See also

- `nova-tls` repository — package layout, module-path notes (root peers,
  D78 rev-4 — bare `tls`, no more `tls.tls` statter), standalone build
  instructions
- `docs/plans/193-nova-tls-repo.md` — the extraction plan (std/tls →
  nova-tls) this example validates end-to-end
- `docs/plans/195-native-modules-c-not-rust.md` — the general native-module
  pattern nova-tls is the reference instance of
- `examples/ffi/README.md` — the underlying `[ffi]` manifest pipeline
  (`c_shims`/`include_dirs`/`libs`/`lib_dirs`) nova-tls builds on
- `examples/net/echo_server.nv` / `echo_client.nv` — the plain-TCP version
  this example adds TLS on top of
