// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 116 — std/tls: `Tls` effect on TcpNet (handshake + encrypted I/O, mockable)

> **Создан 2026-05-31.**
> **Статус:** 🆕 PLANNED.
> **Приоритет:** P1 — **0.2 release feature** (post-0.1). Без TLS Nova не
>   подходит для production backend (HTTPS, secure RPC, encrypted RPC,
>   gRPC-over-TLS, любой web service). После Plan 91.12 (std/net + TcpNet
>   effect) TLS — natural next layer в network stack.
> **Оценка:** ~5-8 dev-day (rustls FFI integration ~3 day; cert handling +
>   SNI/ALPN ~2 day; tests + cross-platform ~2 day; spec + close ~½ day).
> **Зависимости:**
>   - Plan 91.12 ✅ MUST closed — Plan 116 builds на `TcpNet` effect для
>     transport layer (wraps `perform TcpNet.connect`/`read`/`write`/`close`).
>   - Plan 73 / Plan 100.x ✅ closed — `consume` для type-safe close.
>   - Plan 114 ✅ closed (или ship'нут до 116) — Plan 116 пишется в post-114
>     syntax (`ro`/`mut`/`consume`, no `let`, `const` narrowed).
>   - **FFI binding к rustls** (новая dependency) — vendor через Cargo.toml
>     workspace (`rustls = "0.23"` или последняя stable). **Decision в Ф.0:**
>     rustls vs OpenSSL vs native-tls (system) — см. R-1 risk register.
>   - D201 (network stack layered effects architecture, Plan 91.12) —
>     foundation; Plan 116 — first concrete layer over `TcpNet`.
> **D-блоки:** **новые D210-D213** — см. §«D-block changes» ниже.
> **Worktree convention:** `nova-p116` (create через worktree hook).
>
> **Recommended model:**
>   - **Opus 4.7 + Thinking ON** — design-heavy (rustls FFI integration,
>     cert validation policy, ALPN negotiation, layered effect handler
>     wrapping TcpNet). Critical for production-grade security.
>   - **Sonnet 4.6 НЕ рекомендую** — TLS security-sensitive, ошибки в
>     handshake/cert-validation = security vulnerability. Opus required.
>
> **Workflow требования (для агента):** идентично Plan 114/91.12 —
>   commit per phase, update logs (project-creation/simplifications/
>   discussion-log), tests через release nova, status section в конце plan-
>   файла, safety hatches per phase preambles, без упрощений.

---

## Зачем

После Plan 91.12 (std/net hardening + 4 network effects: TcpNet/UdpNet/
UnixNet/DnsNet) Nova имеет **production-grade TCP/UDP/Unix layer**, но
**не имеет TLS**. Это блокер для:

1. **HTTPS клиента** (95%+ современного web использует HTTPS; HTTP/1.1
   plain — deprecated).
2. **HTTPS / HTTP/2 server** (browsers refuse plain HTTP for most APIs).
3. **gRPC / mTLS service mesh** (Kubernetes, service-to-service).
4. **Secure WebSocket** (`wss://`).
5. **Email TLS (SMTP/IMAP/POP3)**.
6. **Custom binary protocols с шифрованием** (financial, healthcare).

**Без TLS Plan 91.12 std/net = pre-2010 web only.** Plan 116 закрывает.

### Mainstream comparison — каждый язык имеет TLS

| Язык | TLS implementation |
|---|---|
| Rust | rustls (pure Rust) / native-tls (OS-native) / openssl-rs |
| Go | crypto/tls (built-in, pure Go) |
| Node | tls module (OpenSSL FFI) |
| Java | javax.net.ssl (JSSE built-in) |
| Python | ssl module (OpenSSL FFI) |
| .NET | System.Net.Security.SslStream |
| **Nova** | **отсутствует** (Plan 116 fills gap) |

### Layered effects pattern (Plan 91.12 D201 — applied)

```
Application                                           ← user code
  ↓
HttpClient (Plan 117) / HttpServer (Plan 122)        ← effect (future)
  ↓
Tls (Plan 116, this plan)                            ← effect (NEW)
  ↓
TcpNet (Plan 91.12)                                  ← effect (existing)
  ↓
libuv                                                 ← private external fn
```

**Tls wraps только `TcpNet`** (не UdpNet/UnixNet — TLS over UDP = DTLS,
отдельный standard, future plan; TLS over Unix sockets — exotic, не V1).
**Layered handler:** `real_tls()` requires `TcpNet` capability для
underlying transport.

---

## Дизайн

### `Tls` effect declaration

```nova
// std/tls/effect.nv

export effect Tls {
    // Client handshake — wraps existing TcpStream
    fn handshake_client(stream consume TcpStream, config ro ClientConfig) -> TlsStream
    
    // Server handshake — wraps accepted TcpStream
    fn handshake_server(stream consume TcpStream, config ro ServerConfig) -> TlsStream
    
    // Encrypted I/O — semantics identical to TcpNet ops but over encrypted channel
    fn read(stream mut TlsStream, max int) -> []u8
    fn write(stream mut TlsStream, data ro []u8) -> int
    fn flush(stream mut TlsStream) -> ()
    fn close(stream consume TlsStream)
    
    // Inspection
    fn peer_cert(stream ro TlsStream) -> Option[Certificate]
    fn alpn_negotiated(stream ro TlsStream) -> Option[str]
    fn cipher_suite(stream ro TlsStream) -> str
    fn protocol_version(stream ro TlsStream) -> TlsVersion
}
```

**~10 ops total** — narrow surface, focused на handshake + encrypted I/O.
Wraps TcpNet ops 1-to-1 для read/write/close pattern (но через encrypted
channel).

### Configuration types

```nova
// std/tls/config.nv

export type ClientConfig {
    root_store      ro RootStore           // trusted CA certificates
    server_name     ro str                  // SNI (e.g. "api.example.com")
    alpn_protocols  ro []str                // e.g. ["h2", "http/1.1"]
    verification    VerificationMode        // see below
    timeout         Option[Duration]        // handshake timeout
}

export type ServerConfig {
    cert_chain      ro CertChain            // server cert + intermediates
    private_key     ro PrivateKey
    alpn_protocols  ro []str                // protocols server supports
    client_cert     ClientCertMode          // None | Optional | Required
    timeout         Option[Duration]
}

export type VerificationMode
    | SystemRoots                           // OS truststore (default)
    | CustomRoots(ro RootStore)             // custom CA bundle
    | Pinned(ro []SubjectKeyHash)           // cert pinning (advanced)
    | InsecureSkipVerify                    // DANGEROUS — testing only

export type ClientCertMode
    | None                                  // no client auth
    | Optional(ro RootStore)                // request but don't require
    | Required(ro RootStore)                // mTLS

export type TlsVersion
    | TLS_1_2
    | TLS_1_3                               // default; preferred
```

### `TlsStream` type — consume semantics

```nova
// std/tls/stream.nv

export type TlsStream consume       // owned encrypted stream, consume @close
```

**Semantics:**
- `consume @close()` — graceful TLS shutdown (send close_notify) + underlying
  TcpStream close. Type-system enforce'ит «нельзя use after close».
- **Нет split** в V1 — TLS frames are stateful (sequence numbers); split
  на reader/writer halves требует separate session state lock. Future
  Plan 116.1 если использование показывает need.

### Public API surface

```nova
// std/tls/client.nv

// Client — connect + handshake one-shot convenience
export fn TlsStream.connect(
    addr SocketAddr,
    config ro ClientConfig
) TcpNet Tls Blocking Fail[TlsError] -> TlsStream {
    consume tcp = TcpStream.connect(addr)?      // uses TcpNet
    perform Tls.handshake_client(tcp, config)   // uses Tls (wraps tcp)
}

export fn TlsStream mut @read(max int) Tls Blocking Fail[TlsError] -> []u8 =>
    perform Tls.read(@, max)

export fn TlsStream mut @write(data ro []u8) Tls Blocking Fail[TlsError] -> int =>
    perform Tls.write(@, data)

export fn TlsStream mut @write_all(data ro []u8) Tls Blocking Fail[TlsError] -> () {
    mut written = 0
    while written < data.len() {
        ro n = @.write(data[written..])?
        written += n
    }
}

export fn TlsStream consume @close() Tls Fail[TlsError] -> () =>
    perform Tls.close(@)

export fn TlsStream @peer_cert() Tls -> Option[Certificate] =>
    perform Tls.peer_cert(@)

export fn TlsStream @alpn_negotiated() Tls -> Option[str] =>
    perform Tls.alpn_negotiated(@)

// std/tls/server.nv — server-side helpers
export fn TlsListener.accept(
    listener mut TcpListener,
    config ro ServerConfig
) TcpNet Tls Blocking Fail[TlsError] -> TlsStream {
    consume tcp = listener.accept()?
    perform Tls.handshake_server(tcp, config)
}
```

### Real (production) handler — rustls FFI

```nova
// std/tls/real.nv

export fn real_tls() TcpNet -> Effect[Tls] => effect Tls {
    handshake_client(tcp, cfg) => {
        ro session = rustls_client_session_new(cfg)?
        // Handshake loop: read/write через TcpNet до completion
        mut tls_stream = TlsStream.new(tcp, session)
        loop {
            if session.wants_write() {
                ro out = session.write_to_buffer()
                perform TcpNet.write(tcp, out)?     // perform на underlying TcpNet
            }
            if session.handshake_done() { break }
            if session.wants_read() {
                ro inp = perform TcpNet.read(tcp, 16*1024)?
                session.read_from_buffer(inp)?
            }
        }
        tls_stream
    }
    
    handshake_server(tcp, cfg) => {
        // Symmetrically — rustls server session
        ...
    }
    
    read(s, max) => {
        // Decrypt incoming frames; may multiple TcpNet.read до one TLS record
        loop {
            ro tcp_data = perform TcpNet.read(s.inner_tcp(), 16*1024)?
            s.session().read_from_buffer(tcp_data)?
            if ro plain = s.session().read_plaintext(max) { return plain }
            // else loop — need more bytes for full frame
        }
    }
    
    write(s, data) => {
        s.session().write_plaintext(data)?
        ro ciphertext = s.session().write_to_buffer()
        perform TcpNet.write(s.inner_tcp(), ciphertext)?
        data.len()
    }
    
    close(s) => {
        s.session().send_close_notify()
        ro out = s.session().write_to_buffer()
        perform TcpNet.write(s.inner_tcp(), out)?
        perform TcpNet.close(s.inner_tcp())?
        // s consumed — TlsStream + underlying TcpStream both released
    }
    
    peer_cert(s) => s.session().peer_certificates().first()
    alpn_negotiated(s) => s.session().alpn_protocol()
    cipher_suite(s) => s.session().cipher_suite_name()
    protocol_version(s) => s.session().version()
}

// Private FFI to rustls — Rust crate vendored
external fn rustls_client_session_new(cfg ro ClientConfig) Fail[TlsError] -> RustlsSession
external fn rustls_server_session_new(cfg ro ServerConfig) Fail[TlsError] -> RustlsSession
// ... ~15 FFI calls total
```

### Test mocking — granular

```nova
test "client handles cert error" {
    ro mock = effect Tls {
        handshake_client(tcp, cfg) => {
            tcp.close()?                                // cleanup TCP
            Fail.throw(TlsError.CertificateInvalid("self-signed"))
        }
        // Other ops — explicit reject
        read(_, _) => Fail.throw(TlsError.OperationNotPermittedInTest)
        write(_, _) => Fail.throw(TlsError.OperationNotPermittedInTest)
        // ...
    }
    with Tls = mock {
        // Need TcpNet тоже для underlying transport (mock or real):
        with TcpNet = mock_tcp_net() {
            ro result = TlsStream.connect(addr, client_config())
            assert(result.is_err())
            if Err(TlsError.CertificateInvalid(msg)) = result {
                assert(msg.contains("self-signed"))
            }
        }
    }
}
```

### `TlsError` enum

```nova
export type TlsError
    | CertificateInvalid(str)              // chain validation failed
    | CertificateExpired
    | CertificateNotYetValid
    | HostnameMismatch(str, str)           // (expected, actual)
    | ProtocolVersionMismatch(str)         // e.g. server only supports TLS 1.2 when 1.3 required
    | CipherSuiteRejected
    | HandshakeTimeout
    | HandshakeFailure(str)                // generic
    | DecryptError                         // possible MITM
    | ConnectionResetByPeer
    | AlpnNoCommonProtocol
    | InsufficientSecurity                 // peer cipher too weak
    | InternalError(str)                   // FFI rustls internal
    | TcpError(NetError)                   // underlying TcpNet failure
    | Closed
    | Cancelled                            // от supervised scope
    | OperationNotPermittedInTest          // mock helper
```

### Sample usage — HTTPS GET

```nova
fn fetch_https(host str, port u16, path str) TcpNet Tls DnsNet Blocking Fail[FetchError] -> str {
    // 1. DNS lookup
    ro addrs = Dns.lookup("${host}:${port}")?              // DnsNet
    
    // 2. TLS connect (TcpNet.connect + Tls.handshake_client one-shot)
    ro config = ClientConfig {
        root_store: SystemRoots,
        server_name: host,                                  // SNI
        alpn_protocols: ["http/1.1"],
        verification: VerificationMode.SystemRoots,
        timeout: Some(30.seconds)
    }
    consume tls = TlsStream.connect(addrs[0], config)?     // TcpNet + Tls
    
    // 3. Send HTTP request (manually serialized — until Plan 117)
    ro request = "GET ${path} HTTP/1.1\r\nHost: ${host}\r\nConnection: close\r\n\r\n"
    tls.write_all(request.as_bytes())?
    
    // 4. Read response
    mut response = Vec.new()
    loop {
        ro chunk = tls.read(8*1024)?
        if chunk.is_empty() { break }                       // EOF / close_notify
        response.extend(chunk)
    }
    
    // 5. Close (consume — type-checker enforce'ит)
    tls.close()?
    
    str.from_utf8(response.to_array())?
}
```

---

## Грамматика

Plan 116 — **library only**, без изменений языка. Используется существующая
grammar (effects, consume, perform, with-handler) из Plan 91.12 и earlier.

---

## Фазы

### Ф.0 — GATE: crypto backend decision + design freeze + D210-D213 drafts (~½ dev-day)

> **CRITICAL decision point:** rustls vs OpenSSL vs native-tls (system).
> Plan 116 commits на ONE backend для V1; alternative — followup plans.

- **Ф.0.1** Crypto backend trade-off audit:
  - **rustls** (Mozilla, pure Rust): memory-safe, modern (TLS 1.3 default,
    no 1.0/1.1), no OS dependencies, cross-platform identical, slower
    startup vs OpenSSL, smaller cipher suite list (intentional). **Recommend
    default.**
  - **OpenSSL**: industry-standard, fastest, supports legacy (TLS 1.0/1.1
    if needed), C library — security CVE'ы regular, OS-version-dependent
    linking, cross-platform pain (vendoring vs system).
  - **native-tls** (Schannel on Win / SecureTransport on macOS / OpenSSL
    on Linux): OS-managed roots, smallest binary, но inconsistent
    behavior across platforms; deprecated на macOS.
  - **Decision:** rustls — best memory-safety, modern defaults, cross-platform
    consistency. Vendored через Cargo (existing pattern в `nova_rt/` для
    other Rust crates).
- **Ф.0.2** Audit existing `nova_rt/Cargo.toml` для rustls compatibility
  (version pinning, feature flags).
- **Ф.0.3** Draft D210 (Tls effect contract), D211 (cert validation policy),
  D212 (ALPN negotiation), D213 (TLS session lifecycle + consume close).
- **Ф.0.4** Worktree `nova-p116` create + register.
- **Ф.0.5** Acceptance A1-A12 финализированы.

### Ф.1 — `Tls` effect declaration + types (~½ dev-day)

- **Ф.1.1** Создать `std/tls/effect.nv` с `effect Tls { … }` (~10 ops).
- **Ф.1.2** Создать `std/tls/config.nv`:
  - `ClientConfig` / `ServerConfig` records
  - `VerificationMode` / `ClientCertMode` / `TlsVersion` sum-types
  - `RootStore` / `CertChain` / `PrivateKey` / `Certificate` / `SubjectKeyHash`
    opaque types (backed by rustls)
- **Ф.1.3** Создать `std/tls/error.nv` с `TlsError` enum (~15 variants).
- **Ф.1.4** Создать `std/tls/stream.nv` с `TlsStream consume` type-decl.
- **Ф.1.5** Tests T1 series (parser/type-checker accept declarations).

### Ф.2 — Real handler + rustls FFI integration (~2 dev-day)

> **Safety hatch:** если rustls FFI integration оказывается значительно
> сложнее expected (callback-based async patterns, memory management
> corner cases, build issues на одной из platforms) — extract в Plan
> 116.1 «rustls FFI foundation» + Plan 116.2 «Tls effect on top». Decision
> point: конец Ф.2.3 (handshake smoke на localhost cert).

- **Ф.2.1** `nova_rt/Cargo.toml` add `rustls = "0.23"`, `rustls-pemfile`,
  `webpki-roots` (для system roots).
- **Ф.2.2** C-ABI FFI shims в `nova_rt/tls.h`/`.c` (~15 functions):
  - Session lifecycle (`rustls_client_session_new`, `_server_session_new`,
    `_free`)
  - Handshake state machine (`wants_read`, `wants_write`, `handshake_done`)
  - I/O buffer transfer (`read_from_buffer`, `write_to_buffer`,
    `read_plaintext`, `write_plaintext`)
  - Inspection (`peer_certificates`, `alpn_protocol`, `cipher_suite_name`,
    `version`)
  - Shutdown (`send_close_notify`)
- **Ф.2.3** Создать `std/tls/real.nv` с `real_tls() -> Effect[Tls]`
  handler — wrapping rustls через FFI; uses `perform TcpNet.*` для
  underlying transport. Smoke test: localhost handshake с self-signed
  cert.
- **Ф.2.4** Cross-platform validation: Windows (MSVC + clang), Linux
  (clang + gcc), macOS (clang). rustls compiles на всех; FFI shim
  должен скомпилироваться identically.

### Ф.3 — Public API surface (~½ dev-day)

- **Ф.3.1** Создать `std/tls/client.nv`:
  - `TlsStream.connect(addr, config) TcpNet Tls Blocking Fail[TlsError] -> TlsStream`
  - Methods: `@read`, `@write`, `@write_all`, `@close` (consume),
    `@peer_cert`, `@alpn_negotiated`, `@cipher_suite`, `@protocol_version`
- **Ф.3.2** Создать `std/tls/server.nv`:
  - `TlsListener.accept(listener, config) TcpNet Tls Blocking Fail[TlsError] -> TlsStream`
- **Ф.3.3** Convenience builders:
  - `ClientConfig.default(server_name str) -> ClientConfig` — SystemRoots
    + ALPN ["h2", "http/1.1"] + 30s timeout
  - `ServerConfig.from_pem(cert_pem str, key_pem str) -> ServerConfig`
- **Ф.3.4** Tests T3 series.

### Ф.4 — Cert validation + SNI + ALPN (~1 dev-day)

- **Ф.4.1** **SNI enforcement** (mandatory для modern HTTPS): ClientConfig
  обязательно carry's `server_name`; rustls fails handshake без SNI на
  multi-tenant servers (Cloudflare, AWS).
- **Ф.4.2** **ALPN negotiation**: client posts list, server picks one;
  `TlsStream.alpn_negotiated()` returns negotiated (or None).
- **Ф.4.3** **Cert validation modes**:
  - `SystemRoots` (default) — webpki + OS truststore
  - `CustomRoots(RootStore)` — explicit CA bundle
  - `Pinned([SubjectKeyHash])` — cert pinning (compare hash, not chain)
  - `InsecureSkipVerify` — testing only; emits compile warning
    `W_TLS_INSECURE_VERIFY`
- **Ф.4.4** **Hostname verification**: rustls webpki — verifies server cert
  matches SNI hostname; mismatch → `TlsError.HostnameMismatch`.
- **Ф.4.5** Tests T4 series (positive + negative для каждого mode).

### Ф.5 — Server-side: mTLS + cert + key loading (~½ dev-day)

- **Ф.5.1** `ServerConfig.from_pem(cert_pem, key_pem)` — parse PEM-encoded
  cert chain + private key (RSA, ECDSA, Ed25519 supported).
- **Ф.5.2** Client cert modes: `None` / `Optional(roots)` / `Required(roots)`
  для mTLS service mesh.
- **Ф.5.3** TLS server smoke test (`examples/tls/echo_server.nv` +
  `echo_client.nv`).
- **Ф.5.4** Tests T5 series.

### Ф.6 — Tests + cross-platform (~1 dev-day)

- **Ф.6.1** Fixtures `nova_tests/plan116/`:
  - T1-T5 series mapped (10+ positive + 8+ negative)
  - Cross-platform smoke: localhost mTLS handshake Windows/Linux/macOS
  - Cert validation negative tests (expired, self-signed, hostname
    mismatch, wrong issuer)
  - ALPN negotiation tests (`h2` + `http/1.1` priorities)
  - Cancel-safety: in-flight handshake aborted on supervised cancel
- **Ф.6.2** Property tests:
  - `prop_handshake_roundtrip` — client+server pair completes, opaque
    bytes match
  - `prop_close_notify_graceful` — proper close_notify exchange
- **Ф.6.3** Full `nova test` ≥ baseline (post-Plan 91.12 baseline).
- **Ф.6.4** Cross-platform CI: Windows + Linux + macOS × clang + MSVC.

### Ф.7 — Spec D-blocks + docs + close (~½ dev-day)

- **Ф.7.1** Promote D210-D213 drafts → active в `spec/decisions/08-runtime.md`
  (D210 — Tls effect contract; D211 — cert validation policy; D212 — ALPN;
  D213 — TLS session lifecycle).
- **Ф.7.2** Cross-ref D201 (Plan 91.12 layered architecture): теперь
  «Plan 116 первый concrete layer над TcpNet — pattern для Plan 117/122».
- **Ф.7.3** `nova doc` regen: std/tls API doc page.
- **Ф.7.4** `examples/tls/`: client + server pair (HTTPS GET + echo server
  with self-signed cert).
- **Ф.7.5** `docs/project-creation.txt` — sprint section.
- **Ф.7.6** `docs/simplifications.md` — close `[M-116-*]` markers.
- **Ф.7.7** `nova-private/discussion-log.md` — design decisions, лессоны
  (crypto backend choice, FFI patterns).
- **Ф.7.8** Memory `project-plan116-status.md`.
- **Ф.7.9** Update closure summary в этом файле.
- **Ф.7.10** Final merge в `main`.

---

## D-block changes

### D210 (NEW) — `Tls` effect contract

**Локация:** `spec/decisions/08-runtime.md` (рядом с D202 Net effects family).

**Что.** Точный contract effect Tls — ~10 ops + production handler invariants
+ layered dependency на TcpNet.

**Operations:** см. §«Дизайн» Plan 116.

**Production handler invariants** (`real_tls()`):
- **Requires TcpNet** capability в caller scope (`fn real_tls() TcpNet ->
  Effect[Tls]`); layered architecture per D201.
- Thread-safe (multiple fibers через separate handshake sessions).
- Cancel-aware: supervised-cancel пропагирует через rustls into TcpNet close.
- Memory-safe: rustls (Mozilla, pure Rust) eliminates entire class of TLS
  CVE'ов common в OpenSSL.

**Mock requirements:** аналогично TcpNet — tests реализуют только нужные
ops, остальные throw `TlsError.OperationNotPermittedInTest`.

**Cross-ref:** D201 (layered architecture), D202 (TcpNet contract), D50
(Blocking).

### D211 (NEW) — TLS certificate validation policy

**Локация:** `spec/decisions/08-runtime.md`.

**Что.** 4-mode taxonomy для cert chain validation:

| Mode | Use case | Behavior |
|---|---|---|
| `SystemRoots` (default) | normal HTTPS | webpki + OS truststore (uses `webpki-roots` Rust crate) |
| `CustomRoots(RootStore)` | private CA / corp PKI | explicit CA bundle parsed from PEM |
| `Pinned([SubjectKeyHash])` | cert pinning (e.g. mobile apps) | compare SHA-256 of SubjectPublicKeyInfo, ignore chain |
| `InsecureSkipVerify` | **testing only** | accept any cert; compiler emits `W_TLS_INSECURE_VERIFY` warning |

**Hostname verification:** mandatory для SystemRoots + CustomRoots. Pinned
mode — hostname verification optional (pinning replaces hostname checks).

**Default policy:** `SystemRoots` + hostname verification + TLS 1.3
preferred (TLS 1.2 acceptable; TLS 1.0/1.1 rejected at handshake — rustls
default).

### D212 (NEW) — ALPN protocol negotiation

**Локация:** `spec/decisions/08-runtime.md`.

**Что.** ALPN (Application-Layer Protocol Negotiation, RFC 7301) — mandatory
для HTTP/2 (must negotiate "h2"). Client posts ordered list of preferred
protocols; server picks one (or rejects if no overlap).

**Contract:**
- `ClientConfig.alpn_protocols: []str` — empty list = no ALPN
- `ServerConfig.alpn_protocols: []str` — list of supported protocols
- `TlsStream.alpn_negotiated() -> Option[str]` — None если ALPN не использовался
- Error `TlsError.AlpnNoCommonProtocol` если client + server lists не
  intersect (handshake fails)

**Default:** client `["h2", "http/1.1"]` (modern HTTPS prefers HTTP/2);
server explicit per service.

### D213 (NEW) — TLS session lifecycle + `consume close`

**Локация:** `spec/decisions/05-memory.md` (consume foundation).

**Что.** `TlsStream consume @close() -> Result[(), TlsError]` — graceful
shutdown via close_notify alert + underlying TcpStream close. Type-system
enforce'ит «нельзя use after close».

**Что НЕ supported в V1:**
- `split` на reader/writer halves — TLS frames stateful, split requires
  separate session-lock infrastructure. Followup `[M-116-tls-split]`.
- Session resumption (0-RTT / tickets) — perf optimization, security
  considerations. Followup `[M-116-session-resumption]`.
- Renegotiation — TLS 1.3 deprecates; рустls не поддерживает. **Out of
  scope permanently.**

**Cross-ref:** D131/D180 (consume foundation), D202 (TcpNet contract —
underlying), D210 (Tls effect).

---

## Tests

### T1 — Effect + types declaration

- **T1.1** `effect Tls { … }` parses; type-checker accepts.
- **T1.2** `ClientConfig` / `ServerConfig` / `VerificationMode` / etc parse.
- **T1.3** `TlsStream consume` type-decl parses; consume semantics enforce'тся.
- **T1.4** `TlsError` enum parses.

### T2 — Real handler smoke

- **T2.1** `real_tls()` compiles; returns `Effect[Tls]`.
- **T2.2** Client+server localhost handshake с self-signed cert — succeeds.
- **T2.3** `tls.write(b"GET /\r\n\r\n")` → `tls.read(1024)` — encrypted
  round-trip works.

### T3 — Public API

- **T3.1** `TlsStream.connect(addr, ClientConfig.default("localhost"))` —
  end-to-end один call.
- **T3.2** `TlsListener.accept(tcp_listener, ServerConfig.from_pem(...))`
  — server-side.
- **T3.3** `consume @close()` — close_notify exchange happens; underlying
  TCP closed.
- **NEG-T3.4** Use TlsStream after `close()` → compile error (consume).

### T4 — Cert validation + SNI + ALPN

- **T4.1** Valid cert + correct SNI + matching ALPN — handshake succeeds.
- **NEG-T4.2** Expired cert → `TlsError.CertificateExpired`.
- **NEG-T4.3** Self-signed cert + `SystemRoots` → `TlsError.CertificateInvalid`.
- **T4.4** Self-signed cert + `CustomRoots([self_signed_ca])` — succeeds.
- **NEG-T4.5** Cert with `example.com` CN + SNI `wrong.com` →
  `TlsError.HostnameMismatch`.
- **T4.6** ALPN negotiation: client `["h2", "http/1.1"]`, server `["http/1.1"]`
  → `alpn_negotiated() == Some("http/1.1")`.
- **NEG-T4.7** ALPN no overlap → `TlsError.AlpnNoCommonProtocol`.
- **T4.8** Cert pinning: `Pinned([known_hash])` accepts matching cert,
  rejects non-matching.

### T5 — mTLS server

- **T5.1** Server `ClientCertMode.Required(client_ca)` + client provides cert
  → handshake OK.
- **NEG-T5.2** Required + client no cert → `TlsError.CertificateInvalid`.
- **T5.3** `ClientCertMode.Optional` accepts both with-cert and no-cert.

### T6 — Cancel + cross-platform

- **T6.1** In-flight handshake aborted при supervised-scope cancel.
- **T6.2** Cross-platform Windows + Linux + macOS — все T1-T5 pass.

### T7 — Layered effect

- **T7.1** `with TcpNet = real_tcp_net() { with Tls = real_tls() { … } }` —
  layered handler stack works.
- **T7.2** Mock TcpNet + real Tls — integration test без real network.
- **T7.3** Mock Tls + real TcpNet — test TLS-level retries без real cert
  infrastructure.

### Regression

- **R1** Full `nova test` ≥ post-Plan 91.12 baseline.
- **R2** Cross-platform CI.
- **R3** `examples/tls/echo_*` — works на всех platforms.

---

## Acceptance criteria

| # | Критерий | Verification |
|---|---|---|
| A1 | `effect Tls` declared (~10 ops) с layered dependency на TcpNet | T1.1 + spec D210 |
| A2 | `real_tls()` handler implemented через rustls FFI; localhost handshake works | T2 series |
| A3 | Public API (`TlsStream.connect/accept`, methods) carry `TcpNet Tls Blocking Fail[TlsError]` | T3 series + grep |
| A4 | `consume @close()` enforce'ит type-safe close (compile error на use-after) | NEG-T3.4 |
| A5 | SNI mandatory; SystemRoots + CustomRoots + Pinned + InsecureSkipVerify modes implemented | T4 series |
| A6 | ALPN negotiation works (`h2`/`http/1.1` prioritization) | T4.6 + T4.7 |
| A7 | Hostname verification (mandatory кроме Pinned) | NEG-T4.5 |
| A8 | mTLS server (`ClientCertMode.Required/Optional/None`) | T5 series |
| A9 | TlsError typed end-to-end; rustls error mapping в spec D210 | grep + spec |
| A10 | Cancel-aware: in-flight handshake aborted on supervised cancel | T6.1 |
| A11 | Cross-platform PASS (Windows + Linux + macOS × clang + MSVC) | R2 |
| A12 | D210-D213 promoted в active в spec | spec diff |
| A13 | `examples/tls/echo_client.nv` + `echo_server.nv` — works | R3 |

---

## Risk register

| # | Риск | Митигация |
|---|---|---|
| **R-1** | **rustls vs OpenSSL vs native-tls decision** — wrong backend = production pain | Ф.0.1 audit с trade-offs; default **rustls** (memory-safe, modern, consistent cross-platform); если practice shows pain — `[M-116-openssl-backend]` followup для alternative backend (parallel handler implementation) |
| R-2 | rustls FFI integration сложнее expected (callback async, memory ownership) | **Safety hatch Ф.2 preamble:** extract в Plan 116.1 (rustls FFI foundation) + Plan 116.2 (Tls effect). Decision point: конец Ф.2.3 |
| R-3 | Cross-platform cert store differences (Windows Schannel vs Linux ca-certificates vs macOS Keychain) | `webpki-roots` Rust crate vendors Mozilla CA bundle — same trust store на всех platforms (vs OS-specific). Может вызвать «works in browser, not in app» если user has corp CA — Mitigation: `CustomRoots` mode |
| R-4 | TLS 1.0/1.1 needed для legacy servers | rustls rejects 1.0/1.1 by design (security). Legacy support = OpenSSL backend (`[M-116-openssl-backend]`) — explicit opt-in для legacy compatibility |
| R-5 | Memory leak в TLS session (rustls Session не Drop'нут properly) | Plan 100.4 cleanup-on-failure ловит forgotten consume TlsStream. Plus FFI shim explicitly free'ит rustls session on `close`. Test `assertNoLeakedTlsSessions()` |
| R-6 | Handshake timeout edge cases (slow server, network jitter) | `ClientConfig.timeout: Option[Duration]` — explicit per-connection. Default 30s. Cancel-safe — supervised abort works |
| R-7 | mTLS cert validation complexity (client cert chains, OCSP, CRL) | V1 — basic chain validation only (rustls default). OCSP / CRL — `[M-116-ocsp-crl]` followup |
| R-8 | rustls version updates breaking FFI | Pin `rustls = "0.23"` exact; update в отдельных followups (`[M-116-rustls-upgrade]`) с testing |

---

## Out of scope (explicitly deferred)

| Маркер | Что | Куда |
|---|---|---|
| `[M-116-tls-split]` | `consume @split() -> (TlsReader, TlsWriter)` для concurrent r/w | V1 stateful TLS frames — split requires session-lock. Followup |
| `[M-116-session-resumption]` | TLS 1.3 session tickets / 0-RTT | Perf optimization; 0-RTT security considerations (replay attacks) |
| `[M-116-ocsp-crl]` | OCSP stapling + CRL checking для cert revocation | Beyond rustls default; needs custom validation logic |
| `[M-116-openssl-backend]` | Alternative `real_tls_openssl()` handler для legacy TLS 1.0/1.1 | Optional backend; parallel implementation |
| `[M-116-native-tls-backend]` | OS-native backend (Schannel/SecureTransport) | Smaller binary; per-platform behavior inconsistency |
| `[M-116-dtls]` | DTLS (TLS over UDP) — для QUIC / WebRTC | Separate standard; future plan if needed |
| `[M-116-tls-over-unix]` | TLS over Unix domain sockets | Exotic use case; deferred |
| `[M-116-quic]` | QUIC protocol (HTTP/3 transport) — separate from TLS but related | Future plan, post-HTTP/2 maturation |
| `[M-116-tls-pre-shared-key]` | PSK ciphers (no PKI) | Niche use case (IoT) |
| `[M-116-tls-pq-crypto]` | Post-quantum cipher suites | Standards still evolving (2026) |
| `[M-116-extracted-to-116.1]` | **Conditional** — если safety hatch fires в Ф.2, extract rustls FFI в Plan 116.1 sub-plan | Trigger при срабатывании R-2 |

---

## Rollback strategy

1. **Revert PR** на main — atomic.
2. Worktree `nova-p116` preserved для diagnosis.
3. Per-phase rollback (Ф.1-Ф.7 = отдельные commits).
4. **rustls dependency removal** — `nova_rt/Cargo.toml` reverted; FFI shims
   удаляются. **Не trivial** если другие plans уже depend на rustls (unlikely
   pre-0.2).
5. Cross-platform CI smoke за ~1 hour (rustls compilation на 3 platforms).

---

## Cross-references

### Связь с уже-закрытыми / planned планами

- **Plan 91.12** (std/net + TcpNet effect) — **hard dependency**. Plan 116
  builds на TcpNet для underlying TCP transport. Layered architecture
  per D201.
- **Plan 73** (consume), **Plan 100.x** (consume static analysis) — для
  `TlsStream consume @close()`.
- **Plan 114** (keyword refresh) — Plan 116 пишется в post-114 syntax
  (`ro`/`mut`/`consume`).
- **Plan 117** (std/http/client — future) — будет building на Plan 116 Tls
  effect (HTTPS support). Layered: HttpClient → Tls → TcpNet.
- **Plan 122** (std/http/server — future) — same pattern.
- **Plan 110** (scoped resources — future) — orthogonal. `consume`
  semantics из Plan 110 будут naturally apply к TlsStream.

### Spec D-blocks

- **D50** ([06-concurrency.md#d50](../../spec/decisions/06-concurrency.md#d50))
  — Blocking effect (cross-ref).
- **D61** ([04-effects.md#d61](../../spec/decisions/04-effects.md#d61))
  — effects + handlers foundation.
- **D131** ([05-memory.md#d131](../../spec/decisions/05-memory.md#d131))
  — consume foundation.
- **D180** ([05-memory.md#d180](../../spec/decisions/05-memory.md#d180))
  — consume binding syntax (Plan 73.1).
- **D201** ([08-runtime.md](../../spec/decisions/08-runtime.md))
  — network stack layered effects architecture (Plan 91.12).
- **D202** ([08-runtime.md](../../spec/decisions/08-runtime.md))
  — TcpNet contract (Plan 91.12) — Plan 116 builds on.
- **D210** (NEW, [08-runtime.md](../../spec/decisions/08-runtime.md))
  — Tls effect contract.
- **D211** (NEW, [08-runtime.md](../../spec/decisions/08-runtime.md))
  — TLS certificate validation policy.
- **D212** (NEW, [08-runtime.md](../../spec/decisions/08-runtime.md))
  — ALPN protocol negotiation.
- **D213** (NEW, [05-memory.md](../../spec/decisions/05-memory.md))
  — TLS session lifecycle + consume close.

---

## Status — closure summary

> Заполняется агентом по завершении Plan 116. Поля:
> - Что сделано (per phase)
> - Что extracted в Plan 116.1+ (если safety hatch fire'нул на rustls FFI)
> - Crypto backend choice rationale (rustls vs alternatives)
> - Final `nova test` results + cross-platform PASS
> - rustls version pinned + Cargo.toml diff
> - Ссылки на коммиты
> - Memory `project-plan116-status.md` создан
> - `docs/project-creation.txt` sprint section updated
> - `docs/simplifications.md` updated с закрытыми `[M-116-*]`
> - `nova-private/discussion-log.md` updated
> - Plan 91.12 cross-refs sync (Plan 115 → Plan 116 mention adjustments)
