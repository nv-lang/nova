// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 116 — std/tls: TLS-слой поверх `TcpStream` (rustls-шим; HTTPS-энейблер)

> **Создан 2026-05-31** (историческое имя файла — «std-tls-effect»; сам эффект
>   ретрактирован актуализацией, см. ниже).
> **Статус:** 🟡 IN PROGRESS — **Ф.0 закрыта 2026-07-10** (актуализация, R-1
>   решён, FFI-дизайн, tls_shim-скелет собирается; см. Status в конце файла).
> **Ф.0-АКТУАЛИЗАЦИЯ 2026-07-10:** план 2026-05-31 писался под эффект `TcpNet`
>   (D201, Plan 91.12) — та модель ЗАМЕНЕНА. Все секции переписаны под:
>   - **Plan 183 (net-rework, D407):** единый эффект `Net`, byte-surface
>     (`read(buf mut []u8) -> Result[int, NetError]`), `TcpStream`/`TcpListener` —
>     `consume value`-записи с `priv handle *()`;
>   - **Plan 176 Ф.4:** `io.Read`/`io.Write` — structural protocols; `TcpStream`
>     конформит через проекцию `NetError @to_io_error` (Q3);
>   - **Plan 177 (Result-everywhere):** никаких `Fail[TlsError]` — всё
>     `Result[T, TlsError]`;
>   - **Plan 178 (std/http client):** HTTPS уже ЖДЁТ этот план — `real_http()`
>     возвращает `Err(HttpError{Tls})` на `https` ([M-178-https-needs-116],
>     std/http/transport/real.nv);
>   - **конвенции:** module-conventions §0/§4а (хендл = `C…Handle`-newtype в
>     extern-сигнатурах), compiler-conventions §5а (символы шима `tls_*`, без
>     vendor-префикса), D325 (fallible = обычное имя + `Result`).
>   Сводная таблица «было → стало» — §«Актуализация» ниже.
> **Приоритет:** P1 — **0.2 release feature** (post-0.1). Без TLS Nova не
>   годится для production backend (HTTPS, secure RPC, gRPC-over-TLS); std/http
>   клиент уже реализован и захардгейчен на этот план.
> **Оценка:** ~5-8 dev-day (shim + сборочная интеграция ~2.5 day; типы + pump +
>   API ~2 day; cert/SNI/ALPN + mTLS ~1.5 day; тесты/кросс-платформа + spec +
>   close ~1.5 day).
> **Зависимости (все ✅):**
>   - Plan 183 ✅ — единый `Net` effect, byte-surface, `TcpStream` value-тип.
>   - Plan 176 Ф.4 ✅ — `io.Read`/`io.Write` structural + `IoError`/`ErrorKind`.
>   - Plan 177 ✅ — Result-everywhere.
>   - Plan 178 ✅ — std/http client c https-гейтом (этот план его снимает).
>   - Plan 73 / 100.x ✅ — `consume` для type-safe close; consume-поле в
>     consume-записи (прецедент `Body`, std/http/body.nv).
>   - Plan 115/118.x ✅ — FFI: `*()`/`*u8`, `unsafe`, newtype-хендлы.
> **D-блоки:** **4 NEW**. Номера выделяются ПРИ ПРОМОУШЕНЕ от текущего
>   максимума (`grep -rh "^## D[0-9]*" spec/decisions/ | sort -V | tail -1`;
>   на 2026-07-10 максимум D414 → ориентир **D415-D418**). Числа D210-D213 из
>   ревизии-2026-05-31 НЕ занимать (нумерация давно ушла вперёд; правило —
>   memory `project-spec-dblock-numbering`).
> **Worktree:** `nova-116`, ветка `tls-116` (фактический; historical
>   `nova-p116` не создавался).
>
> **Recommended model:** **Opus + Thinking** — security-критичный дизайн
>   (cert-validation, handshake-пампинг, FFI-граница с Rust-крейтом).
>   Механические фазы (Ф.6-тесты по готовой карте) — sonnet допустим.
>
> **Workflow:** commit per phase; логи (project-creation/simplifications/
>   discussion-log); тесты РЯДОМ с модулем (`std/tls/*_test.nv`, memory
>   `feedback-module-tests-beside-module`); гейт корректности = targeted
>   pos+neg, не byte-baseline.

---

## Актуализация 2026-07-10 — «было → стало»

| Ревизия 2026-05-31 | Актуально (эта ревизия) |
|---|---|
| Новый эффект `Tls` (~10 опов) поверх эффекта `TcpNet`; `real_tls() TcpNet -> Effect[Tls]` | **Нового эффекта НЕТ.** std/tls — библиотечный слой (как std/encoding/compress): методы `TlsStream` несут `Net` (I/O идёт через существующий effect), rustls-сессия — FFI-компьют через module-private `extern "C"` |
| `perform TcpNet.read/write` в handler-теле | Обычный `.nv`-код: `Net.read(...)`/`stream.write_all(...)` (byte-surface D407) внутри методов `TlsStream` |
| `fn ... TcpNet Tls Blocking Fail[TlsError] -> T` | `fn ... Net -> Result[T, TlsError]` (Plan 177); `Blocking` не нужен — паркует libuv внутри `Net` |
| `TlsStream consume` opaque | `TlsStream consume { priv tcp TcpStream, priv session CTlsHandle }` + **structural `io.Read`/`io.Write`** (`@read`/`@write`/`@flush` → `Result[_, IoError]` через `TlsError @to_io_error`, образец `NetError` Q3) |
| FFI в `nova_rt/tls.{h,c}` (C-шим) | **Отдельный Rust staticlib-крейт `compiler-codegen/tls_shim/`** (rustls — Rust-крейт, C-обёртка не нужна); C-ABI символы `tls_*` (§5а), хендлы `CTlsHandle(*())`/`CTlsCfgHandle(*())` (§4а) |
| rustls FFI-вызовы `rustls_client_session_new` | `tls_client_new` и т.д. — `<модуль>_<имя>` БЕЗ vendor-префикса (§5а) |
| D210-D213 | Номера при промоушене (ориентир D415-D418) |
| `nova_tests/plan116/` fixtures | Тесты рядом с модулем: `std/tls/*_test.nv` (+ `neg/`) |
| Plan 117 (http client) — future | std/http УЖЕ есть (Plan 178): Ф.5 подключает https-ветку `real_http` и закрывает `[M-178-https-needs-116]` |

---

## Зачем

Nova имеет production-grade TCP/UDP/DNS (Plan 183: единый `Net`, zero-copy
byte-surface) и HTTP/1.1-клиент (Plan 178), но **не имеет TLS**. Это блокер для:

1. **HTTPS клиента** — std/http уже реализован, `https://` честно возвращает
   `Err(HttpError{kind: Tls})` до открытия сокета (`[M-178-https-needs-116]`).
2. **HTTPS / HTTP/2 server** (std/http/server ждёт того же слоя).
3. **gRPC / mTLS service mesh**.
4. **Secure WebSocket (`wss://`)**, SMTP/IMAP TLS, шифрованные бинарные протоколы.

| Язык | TLS |
|---|---|
| Rust | rustls / native-tls / openssl-rs |
| Go | crypto/tls (built-in) |
| Node / Python | OpenSSL FFI |
| Java / .NET | JSSE / SslStream |
| **Nova** | **отсутствует — Plan 116 закрывает** |

### Слоёная картина (актуальная)

```
Application / HttpClient (std/http, Plan 178)      ← Http effect (мокабелен)
  ↓ real_http()
std/tls  (этот план)                               ← БИБЛИОТЕЧНЫЙ слой, БЕЗ эффекта
  ↓ методы TlsStream несут `Net`; крипта — tls_shim (rustls) через extern "C"
std/net  (Plan 183)                                ← Net effect (мокабелен)
  ↓ real_net()
libuv                                              ← C FFI
```

---

## Ключевое решение Ф.0-1: нового эффекта НЕТ

module-conventions §0: эффект нужен, когда операция импурна и её **разумно
подменять в тестах**. Разбор импурности TLS:

- **Транспорт** — уже за эффектом `Net` (мокабелен: `mock_net()`).
- **Энтропия/время** (внутри rustls: ключи, проверка сроков сертификатов) —
  скрыты в крейте, осмысленному мокапу извне не поддаются (аналог hash-seed).
- **Сама криптография** — детерминированный компьют, как brotli-декодер
  (std/encoding/compress — прецедент FFI-компьюта без эффекта).

Что ДАЛ БЫ эффект `Tls`: ~10 vtable-опов с multi-word GC-пейлоадами — ровно
класс проблем, из-за которого Net переехал на byte-surface (erasure
Ok-пейлоадов в effect-vtable, D407 §2). Что дал бы мок такого эффекта:
ничего — «замоканный handshake» не тестирует TLS; для верхних слоёв мок-шов
уже есть (`Http` effect), для транспорта — `mock_net()`. Реальный TLS
тестируется парой real-client ↔ real-server на loopback (см. §Тесты).

**Итого:** std/tls = типы + методы (несут `Net`) + module-private
`extern "C"` к tls_shim. Отклонение от чек-листа §7.1 module-conventions
(«эффект-семейство») — мотивированное: новых импурных примитивов модуль не
вводит; конвенция §0 сама даёт критерий «когда эффект НЕ нужен».

## Ключевое решение Ф.0-2 (R-1): rustls 0.23, провайдер `ring`

**Подтверждаю дефолт плана — rustls 0.23** (pinned), с уточнением провайдера:

- **rustls** (pure Rust): memory-safe (устраняет целый класс OpenSSL-CVE),
  современные дефолты (TLS 1.3 preferred, 1.2 поддержан, 1.0/1.1 отвергнуты by
  design), идентичное поведение на всех платформах. **Новый аргумент против
  альтернатив, которого не было в ревизии-05-31:** Rust-toolchain и так
  обязателен для сборки Nova (компилятор — Rust) → rustls не добавляет НИ
  ОДНОГО нового toolchain-требования. OpenSSL добавил бы perl+nasm(+nmake)
  и платформозависимую линковку; native-tls — три разных бэкенда с разным
  поведением (и deprecated SecureTransport на macOS). Отвергнуты.
- **Крипто-провайдер: `ring`, НЕ дефолтный `aws-lc-rs`.** aws-lc-sys требует
  cmake + nasm на Windows — чужой toolchain в bootstrap-сборке; `ring`
  собирается имеющимся cc/clang/MSVC. Фичи крейта:
  `default-features = false, features = ["ring", "std", "tls12", "logging"]`.
- **Trust store: `webpki-roots`** (Mozilla CA bundle, вкомпилирован в шим) —
  единый на всех платформах (риск R-3 «works in browser, not in app» при corp
  CA закрыт режимом `CustomRoots`). OS-truststore — followup
  `[M-116-os-truststore]` (rustls-native-certs).
- **Изоляция зависимостей:** правило «никаких сторонних крейтов в компиляторе»
  (feedback_third_party_libs; nova-codegen = clap+anyhow) НЕ нарушается:
  tls_shim — **отдельный** крейт, НЕ зависимость nova-codegen; его артефакт —
  прекомпилированный staticlib, линкуемый в user-бинарь **условно по факту
  использования** (механизм brotli, D337). Cargo.lock закоммичен (pin всего
  дерева); полный `cargo vendor` исходников — followup `[M-116-cargo-vendor]`
  при необходимости офлайн-сборки.

---

## Дизайн

### Типы (std/tls)

```nova
// std/tls/stream.nv
/// Зашифрованный поток поверх TcpStream. MUST-CONSUME (D133); `consume` БЕЗ
/// `value` — держит consume-поле tcp (прецедент Body: value-копия оставила бы
/// consume-поле владельца неразряженным).
#stable(since = "0.2")
export type TlsStream consume {
    priv tcp     TcpStream      // underlying transport (владение перешло при handshake)
    priv session CTlsHandle     // rustls-сессия в tls_shim (§4а-newtype)
    priv rbuf    []u8           // ciphertext staging (вход из сокета)
}

// std/tls/config.nv — конфиги: ЧИСТЫЕ Nova-данные (никаких хендлов наружу);
// shim-конфиг строится эфемерно внутри connect/accept (см. FFI).
export type ClientConfig {
    server_name     str                 // SNI — ОБЯЗАТЕЛЕН (D-блок B)
    alpn_protocols  []str               // [] = без ALPN
    verification    VerificationMode
}
export type ServerConfig {
    cert_pem        []u8                // цепочка PEM (leaf + intermediates)
    key_pem         []u8                // приватный ключ PEM (RSA/ECDSA/Ed25519)
    alpn_protocols  []str
    client_cert     ClientCertMode      // mTLS
}
export type VerificationMode enum
    | SystemRoots                       // webpki-roots (Mozilla bundle) — default
    | CustomRoots([]u8)                 // свой CA-bundle (PEM bytes)
    | Pinned([][]u8)                    // SPKI SHA-256 pinning (32-byte hashes)
    | InsecureSkipVerify                // ТОЛЬКО тесты; см. D-блок B
export type ClientCertMode enum
    | NoClientAuth
    | Optional([]u8)                    // CA-bundle PEM
    | Required([]u8)                    // mTLS
export type TlsVersion enum | Tls12 | Tls13
```

Замечания против ревизии-05-31: `RootStore`/`CertChain`/`PrivateKey`/
`Certificate` opaque-типы УБРАНЫ — byte-first (§2 module-conventions): PEM/DER
ходят как `[]u8`, парсит их шим (rustls-pemfile). `timeout` из конфигов УБРАН —
дедлайны по конвенции 173 задаются supervised-scope на call-site, не полем
конфига (`[M-178-timeout-needs-173]` — та же линия). Билдеры:
`ClientConfig.new(server_name str)` (SystemRoots + ALPN `["http/1.1"]`),
`ServerConfig.new(cert_pem, key_pem)`.

### Ошибки — `TlsError` по образцу `NetError` (D407 §4 + Q3-проекция)

```nova
// std/tls/error.nv
export type TlsError enum
    | CertificateInvalid(str)       // валидация цепочки (детали текстом)
    | CertificateExpired
    | HostnameMismatch(str)         // ожидавшееся имя
    | UnsupportedProtocolVersion
    | HandshakeFailure(str)         // generic alert/протокольная ошибка
    | AlpnNoCommonProtocol
    | PeerMisbehaved(str)           // протокольное нарушение / возможный MITM
    | CloseNotify                   // чистый TLS-EOF (аналог NetError.Eof)
    | InvalidPem(str)               // parse cert/key PEM
    | Net(NetError)                 // ошибка underlying-транспорта
    | Internal(str)                 // rustls internal / shim

export fn TlsError @to_str() -> str                    // lowercase, как NetError
export fn TlsError @to_error_kind() -> ErrorKind       // best-effort (Q3):
    // Net(e) => e.to_error_kind(); CloseNotify => UnexpectedEof;
    // Certificate*/Hostname*/Alpn*/Handshake* => InvalidData; Internal => Other(0)
export fn TlsError @to_io_error(op str) -> IoError => IoError.new(@to_error_kind(), op)
// Классификация из шима: стабильный int-код + текст (см. FFI: tls_last_error*).
fn TlsError.from_shim(kind int, msg str) -> TlsError
```

Result-everywhere (177): все fallible-операции — `Result[T, TlsError]`;
`@read`/`@write`/`@flush` (io-conformance поверхность) — `Result[_, IoError]`
через `@to_io_error`, зеркально `TcpStream` (tcp.nv).

### Публичный API (std/tls/client.nv, server.nv, stream.nv)

```nova
/// Client handshake поверх УЖЕ подключённого TcpStream (владение забирается;
/// на ошибке handshake сокет закрывается внутри — TcpStream не «утекает»).
#stable(since = "0.2")
export fn TlsStream.connect(stream consume TcpStream, config ro ClientConfig)
    Net -> Result[TlsStream, TlsError]

/// Server handshake на принятом соединении.
#stable(since = "0.2")
export fn TlsStream.accept(stream consume TcpStream, config ro ServerConfig)
    Net -> Result[TlsStream, TlsError]

// io.Read / io.Write structural conformance (зеркально TcpStream, Q3):
export fn TlsStream mut @read(buf mut []u8)  Net -> Result[int, IoError]   // Ok(0) = clean TLS EOF
export fn TlsStream mut @write(data []u8)    Net -> Result[int, IoError]
export fn TlsStream mut @flush()             Net -> Result[(), IoError]

// Типизированная (TlsError) поверхность — как write_all/read_to_vec у TcpStream:
export fn TlsStream mut @write_all(data []u8) Net -> Result[(), TlsError]
export fn TlsStream mut @read_to_vec(max int) Net -> Result[[]u8, TlsError] // [] = EOF
export fn TlsStream mut @read_text(max int)   Net -> Result[str, TlsError]

// Инспекция (после handshake значения фиксированы — без эффекта в row не обойтись:
// значения читаются из shim-сессии; I/O нет, но unsafe-FFI есть → обычные методы):
export fn TlsStream @alpn_negotiated() -> Option[str]
export fn TlsStream @protocol_version() -> TlsVersion
export fn TlsStream @cipher_suite() -> str
export fn TlsStream @peer_cert_der(i int) -> Option[[]u8]   // DER bytes; парсинг — вне V1

/// Graceful close: best-effort close_notify + close underlying TCP (consume).
/// Возвращает () — как TcpStream.close (ошибки отправки alert'а при закрытии
/// глотаются; семантика — D-блок D).
#stable(since = "0.2")
export fn TlsStream consume @close() Net
```

Отличия от ревизии-05-31: `TlsStream.connect(addr, cfg)` (DNS+TCP+TLS one-shot)
ЗАМЕНЁН на `connect(stream consume TcpStream, cfg)` — byte-surface слоистость:
TCP-подключение делает вызывающий (или std/http), TLS-слой не дублирует
resolve/connect API. Ergonomic one-shot — followup `[M-116-connect-oneshot]`
если практика попросит.

### Handshake / I/O pump (весь в `.nv` — §4.4 nv-sourcing)

rustls — sans-I/O state machine; шим отдаёт четыре примитива трафика
(`read_tls`/`write_tls` = ciphertext, `read_plain`/`write_plain` = plaintext) и
три предиката (`is_handshaking`/`wants_read`/`wants_write`). Весь пампинг —
Nova-код поверх byte-surface `Net`:

```nova
// std/tls/pump.nv (module-private), эскиз
fn pump_handshake(tcp mut TcpStream, s CTlsHandle) Net -> Result[(), TlsError] {
    while unsafe { tls_is_handshaking(s) } != 0 {
        if unsafe { tls_wants_write(s) } != 0 { flush_tls_out(tcp, s)? continue }
        if unsafe { tls_wants_read(s) } != 0 {
            mut buf []u8 = []u8.new()
            buf.resize(16 * 1024, 0 as u8)
            ro n = match Net.read(tcp, buf) {
                Ok(0)  => return Err(TlsError.HandshakeFailure("peer closed during handshake"))
                Ok(n)  => n
                Err(e) => return Err(TlsError.Net(e))
            }
            feed_and_process(s, buf[0..n])?      // tls_read_tls + tls_process
        }
    }
    flush_tls_out(tcp, s)                        // финальный flight
}
// @read: пока tls_read_plain == 0 (нет расшифрованного) — Net.read → feed;
//        -1 от tls_read_plain = clean close_notify → Ok(0).
// @write: tls_write_plain → flush_tls_out (write_all ciphertext).
```

Инварианты: буферы `[]u8` живут на стеке фибры через park (лайфтайм — §4.6
module-conventions); `Ok(0)` из `Net.read` во время handshake = ошибка, после —
превращается в проверку close_notify (truncation-attack detection: обрыв БЕЗ
close_notify → `Err(PeerMisbehaved("truncated"))` на typed-поверхности,
`Ok(0)` не выдаётся — D-блок A).

### FFI-граница — крейт `compiler-codegen/tls_shim/`

**Форма:** отдельный Rust-крейт `crate-type = ["staticlib"]`, `#[no_mangle]
extern "C"`, `panic = "abort"` (паника через FFI-границу = UB). НЕ member
workspace nova-codegen (у того нет workspace; dep-tree компилятора не меняется).
Артефакт: `nova_tls_shim.lib` / `libnova_tls_shim.a` → кэш
`target/tls-cache/` (зеркало brotli-cache) + условная линковка по факту
использования (сканер call-site `tls_client_new(`/`tls_server_new(` в
сгенерированном C — механизм `c_file_uses_brotli`, test_runner.rs). Rust
staticlib тянет системные либы — точный список фиксируется в Ф.2 через
`RUSTFLAGS="--print native-static-libs"` (ожидаемо Windows: ws2_32, bcrypt,
advapi32, ntdll; Linux: pthread, dl, m — большинство уже в línk-line).

**Хендлы (§4а):** в `std/tls/ffi.nv` —

```nova
type CTlsHandle(*())      // rustls-сессия (Box<TlsSession> в шиме)
type CTlsCfgHandle(*())   // ЭФЕМЕРНЫЙ конфиг-билдер (живёт внутри одного connect/accept)
```

**Символы (§5а, `tls_*`; nova_int = intptr_t = isize в шиме):**

| Символ | Сигнатура (.nv) | Что |
|---|---|---|
| `tls_client_cfg_new` | `() -> CTlsCfgHandle` | билдер клиент-конфига |
| `tls_server_cfg_new` | `() -> CTlsCfgHandle` | билдер сервер-конфига |
| `tls_cfg_verify_system` | `(c CTlsCfgHandle) -> int` | webpki-roots (Mozilla) |
| `tls_cfg_verify_pem` | `(c, pem *u8, len int) -> int` | CustomRoots из PEM |
| `tls_cfg_verify_pinned` | `(c, hashes *u8, count int) -> int` | SPKI-pinning (count×32 байта) — Ф.4 |
| `tls_cfg_verify_insecure` | `(c) -> int` | InsecureSkipVerify (тесты) |
| `tls_cfg_alpn_add` | `(c, proto *u8, len int) -> int` | добавить ALPN-протокол (повторяемо) |
| `tls_cfg_cert_key_pem` | `(c, cert *u8, clen int, key *u8, klen int) -> int` | cert chain + key: сервер (обязательно) И клиент (mTLS-серт, Ф.5.2 — `ClientConfig` получит опциональные поля `client_cert_pem`/`client_key_pem`) |
| `tls_cfg_client_auth_pem` | `(c, roots *u8, len int, required bool) -> int` | mTLS-режим сервера |
| `tls_cfg_free` | `(c) -> ()` | освободить билдер (на error-пути; new/accept потребляют сами) |
| `tls_client_new` | `(c CTlsCfgHandle, sni *u8, len int, out_err *mut int) -> CTlsHandle` | сессия клиента (билдер потреблён); null → код в out_err |
| `tls_server_new` | `(c CTlsCfgHandle, out_err *mut int) -> CTlsHandle` | сессия сервера |
| `tls_is_handshaking` | `(h CTlsHandle) -> int` | 1/0 |
| `tls_wants_read` | `(h) -> int` | 1/0 |
| `tls_wants_write` | `(h) -> int` | 1/0 |
| `tls_read_tls` | `(h, p *u8, len int) -> int` | скормить ciphertext (n принятых, <0 err) |
| `tls_process` | `(h) -> int` | process_new_packets; 0 ok, <0 err |
| `tls_write_tls` | `(h, out *mut u8, cap int) -> int` | извлечь ciphertext (n, 0 = нечего) |
| `tls_read_plain` | `(h, out *mut u8, cap int) -> int` | plaintext: n>0; 0 = нет данных; -1 = clean close_notify; <-1 err |
| `tls_write_plain` | `(h, p *u8, len int) -> int` | n принятых, <0 err |
| `tls_send_close_notify` | `(h) -> ()` | поставить alert в исходящий буфер |
| `tls_alpn` | `(h, out *mut u8, cap int) -> int` | negotiated ALPN (len; 0 = нет) |
| `tls_version` | `(h) -> int` | 0x0303 / 0x0304 |
| `tls_cipher_suite` | `(h, out *mut u8, cap int) -> int` | имя suite (len) |
| `tls_peer_cert_der` | `(h, i int, out *mut u8, cap int) -> int` | DER серта i (0 = leaf); возврат = ПОЛНАЯ длина, копия min(cap,len); 0 = серта нет |
| `tls_last_error_kind` | `(h) -> int` | стабильный код класса ошибки (таблица в shim/lib.rs ↔ TlsError.from_shim) |
| `tls_last_error` | `(h, out *mut u8, cap int) -> int` | текст последней ошибки (len) |
| `tls_free` | `(h) -> ()` | освободить сессию (double-free-safe на null) |

Ошибки: shim классифицирует `rustls::Error` в стабильные int-коды
(`TLS_ERR_CERT_INVALID = 3`, …) + текст; `TlsError.from_shim(kind, msg)`
строит typed-вариант — зеркало `NetError.from_code`/`classify` (error.nv), но
классификация по коду, не по substring (коды наши, стабильность контролируем).

Конфиги: Nova-side чистые данные; на each connect/accept строится эфемерный
shim-билдер (создать → скормить режимы/ALPN/PEM → `tls_client_new` потребляет).
Пере-использование дорогого rustls-`ClientConfig` между коннектами (roots
парсятся каждый раз) — followup `[M-116-config-cache]`; для V1 — простота и
отсутствие долгоживущего конфиг-хендла в Nova-коде.

### Интеграция std/http (НОВАЯ фаза против ревизии-05-31)

`real_http()` (std/http/transport/real.nv) ветка `secure = true` → resolve +
`TcpStream.connect` + `TlsStream.connect(tcp, ClientConfig.new(host))` +
тот же write-request/read-to-EOF цикл поверх `TlsStream` (он `io.Read`/
`io.Write`, цикл обобщается или дублируется — решить в Ф.5 по месту).
Закрывает `[M-178-https-needs-116]`; тест real_test.nv «https → Err(Tls)»
заменяется на позитивный localhost-HTTPS smoke + негативный cert-тест.

---

## Грамматика

Library only — изменений языка нет. (`W_TLS_INSECURE_VERIFY` из ревизии-05-31
ретрактирован: компайлер-ворнинг на std-значение = хардкод std-семантики в
компиляторе против §3 compiler-conventions; страховка — имя
`InsecureSkipVerify` + док + D-блок B «testing only».)

---

## Фазы

### Ф.0 — GATE: актуализация + R-1 + FFI-дизайн + tls_shim-скелет ✅ 2026-07-10

- **Ф.0.1** ✅ Актуализация плана (этот файл переписан под 183/176.4/177/178 +
  конвенции §4а/§5а/D325).
- **Ф.0.2** ✅ R-1 решён: rustls 0.23 + `ring`-провайдер + webpki-roots
  (обоснование — §«Ключевое решение Ф.0-2»).
- **Ф.0.3** ✅ Решение «эффекта нет» (§«Ключевое решение Ф.0-1»).
- **Ф.0.4** ✅ FFI-граница спроектирована (таблица `tls_*` выше).
- **Ф.0.5** ✅ Вендоринг скелета: крейт `compiler-codegen/tls_shim/`
  (Cargo.toml pinned + Cargo.lock, src/lib.rs — компилируемый C-ABI слой,
  README-провенанс по образцу brotli/version.txt + LIBUV_UPDATE.md).
  `cargo build --release` зелёный → riесk R-2 (toolchain-путь) де-рискнут.
- **Ф.0.6** Draft D-блоков A-D — тексты в §«D-block changes»; номера при
  промоушене (Ф.7).

### Ф.1 — std/tls типы + ошибки (~½ dev-day)

- **Ф.1.1** `std/tls/error.nv` — `TlsError` + `@to_str` + `@to_error_kind` +
  `@to_io_error` + `from_shim` (+ тест-таблица классификации).
- **Ф.1.2** `std/tls/config.nv` — `ClientConfig`/`ServerConfig`/
  `VerificationMode`/`ClientCertMode`/`TlsVersion` + билдеры `new(...)`.
- **Ф.1.3** `std/tls/ffi.nv` — extern-декларации `tls_*` + `CTlsHandle`/
  `CTlsCfgHandle` (§4а).
- **Ф.1.4** `std/tls/stream.nv` — `TlsStream consume` запись.
- **Ф.1.5** Тесты деклараций pos+neg (`std/tls/*_test.nv`, `std/tls/neg/`).

### Ф.2 — Сборочная интеграция шима (~1 dev-day)

> **Safety hatch (сохранён):** если линковка Rust-staticlib в C-пайплайн
> упирается в платформенный конфликт (symbol clash, CRT mismatch /MT vs /MD,
> см. R-2) — extract в Plan 116.1 «tls_shim build integration», остальные фазы
> идут на Windows-only линковке. Decision point: конец Ф.2.

- **Ф.2.1** Сборка staticlib в `target/tls-cache/` (скрипт/док, зеркало
  brotli-cache; прекомпилят НЕ трекается в репо — ~8-15 MB, в отличие от
  brotli решение о трекинге прекомпилята отложено до замера размера).
- **Ф.2.2** test_runner.rs: `detect_tls()` + `c_file_uses_tls()` (call-site
  сканер по `tls_client_new(`/`tls_server_new(`) + условная линковка +
  `NOVA_DEBUG_TLS_LINK=1` диагностика — зеркало brotli (D337). Отсутствие
  либы → link-skip + деградация в `TlsError.Internal("tls shim not built")`
  на первом вызове (Q11-паттерн brotli: никогда не link-error).
- **Ф.2.3** native-static-libs: зафиксировать список системных либ на
  Windows (MSVC+clang) и Linux; добавить недостающие в link-line.
- **Ф.2.4** MSVC CRT: staticlib собирать с `-Ctarget-feature=+crt-static`
  соответствие /MT-пути (как bdwgc/brotli x64-windows-static) — проверить.

### Ф.3 — TlsStream: handshake pump + I/O методы (~1.5 dev-day)

- **Ф.3.1** `std/tls/pump.nv` — `pump_handshake`/`flush_tls_out`/
  `feed_and_process` (byte-surface, эскиз в §Дизайн).
- **Ф.3.2** `TlsStream.connect` (client) + `.accept` (server): билд
  эфемерного shim-конфига из Nova-конфига → сессия → pump; на ошибке —
  `tls_free` + `tcp.close()` (TcpStream не утекает — R-5).
- **Ф.3.3** Методы: `@read`/`@write`/`@flush` (IoError), `@write_all`/
  `@read_to_vec`/`@read_text` (TlsError), инспекция, `consume @close()`.
- **Ф.3.4** Smoke: self-signed пара client↔server через localhost real_net
  (fixture-PEM в `std/tls/testdata/`), encrypted round-trip.

### Ф.4 — Cert validation + SNI + ALPN (~1 dev-day)

- **Ф.4.1** SNI обязателен: `ClientConfig.new(server_name)` — единственный
  конструктор; пустое имя → `Err(TlsError.HandshakeFailure)` до сокета.
- **Ф.4.2** ALPN: списки в оба конца, `@alpn_negotiated()`,
  `AlpnNoCommonProtocol` (rustls: server отвергает handshake без пересечения).
- **Ф.4.3** Все 4 режима верификации; `Pinned` — кастомный
  `ServerCertVerifier` в шиме (SPKI SHA-256; hostname-check при pinning
  опционален — pinning заменяет цепочку, D-блок B).
- **Ф.4.4** Hostname verification (webpki, автоматом в rustls) →
  `HostnameMismatch`.
- **Ф.4.5** Тесты pos+neg на каждый режим (expired/self-signed/wrong-host
  fixture-PEM в testdata).

### Ф.5 — Server mTLS + std/http HTTPS (~1 dev-day)

- **Ф.5.1** `ServerConfig.new(cert_pem, key_pem)`; RSA/ECDSA/Ed25519 ключи
  (rustls-pemfile).
- **Ф.5.2** `ClientCertMode` NoClientAuth/Optional/Required (mTLS).
- **Ф.5.3** **std/http https-ветка:** real.nv → TLS-путь; закрыть
  `[M-178-https-needs-116]`; обновить real_test.nv.
- **Ф.5.4** `examples/tls/echo_server.nv` + `echo_client.nv`.

### Ф.6 — Тесты + кросс-платформа (~1 dev-day)

- **Ф.6.1** Тесты РЯДОМ с модулем (`std/tls/*_test.nv` + `neg/`): T-серии
  ниже; deterministic без внешней сети (loopback real_net; mock_net для
  handshake НЕ пригоден — он stub, не пайп; это осознанно: реальный TLS
  тестируем реальным TLS, транспорт — loopback).
- **Ф.6.2** Cancel-safety: handshake в supervised-scope, abort посреди
  пампа → сокет закрыт, сессия освобождена (R-5/R-6).
- **Ф.6.3** Кросс-платформа: Windows (MSVC + clang) обязательна этой волной;
  Linux/macOS — по доступности CI (як brotli: отсутствие либы = деградация,
  не поломка).
- **Ф.6.4** Прогон detect172 pos+neg → без регрессий; targeted-гейт.

### Ф.7 — Spec D-блоки + docs + close (~½ dev-day)

- **Ф.7.1** Промоушен D-блоков A-D (номера = next-free на момент промоушена;
  A/B/C → `spec/decisions/08-runtime.md`, D → `05-memory.md`).
- **Ф.7.2** Cross-ref D407 (net byte-surface): std/tls — первый слой поверх;
  D337 (conditional link): второй потребитель механизма.
- **Ф.7.3** docs-guide `docs/tls-internals.md` (модель: sans-I/O pump,
  таблица Nova ↔ Go crypto/tls ↔ Rust rustls) + `nova doc` regen.
- **Ф.7.4** Логи: project-creation.txt, simplifications.md (маркеры),
  discussion-log (nova-private), memory `project-plan116-status.md`.
- **Ф.7.5** Closure summary здесь + merge в main.

---

## D-block changes (номера — при промоушене; ориентир D415-D418)

### D-блок A (NEW, 08-runtime.md) — std/tls слой: контракт и модель

- std/tls — библиотечный слой БЕЗ собственного эффекта (мотивировка = §0
  module-conventions; транспортная импурность за `Net`, крипта = FFI-компьют).
- `TlsStream consume { priv tcp TcpStream, priv session CTlsHandle }`;
  методы несут `Net`; io-conformance поверхность (`@read`/`@write`/`@flush`)
  — `Result[_, IoError]`, typed-поверхность — `Result[_, TlsError]`.
- Handshake/трафик — sans-I/O пампинг в `.nv` поверх byte-surface `Net`
  (16 KiB staging, инварианты Ok(0)/close_notify/truncation — §Дизайн).
- Shim-граница: символы `tls_*` (§5а), хендлы-newtype (§4а), стабильные
  int-коды ошибок ↔ `TlsError.from_shim`.
- Cross-ref: D407 (Net byte-surface), D337 (conditional link), D133 (must-
  consume), Q3/D322 (ErrorKind-проекция).

### D-блок B (NEW, 08-runtime.md) — политика валидации сертификатов

| Режим | Use case | Поведение |
|---|---|---|
| `SystemRoots` (default) | обычный HTTPS | webpki-roots (Mozilla bundle, вкомпилирован) |
| `CustomRoots([]u8 PEM)` | private CA / corp PKI | явный bundle |
| `Pinned([][]u8)` | cert pinning | SHA-256 SubjectPublicKeyInfo; цепочка игнорируется; hostname-check опционален |
| `InsecureSkipVerify` | ТОЛЬКО тесты | принять любой серт; без компайлер-ворнинга (ретракт W_TLS_INSECURE_VERIFY — язык std-семантику не хардкодит) |

Hostname verification обязательна для SystemRoots/CustomRoots. Дефолт:
TLS 1.3 preferred / 1.2 accepted / 1.0-1.1 rejected (rustls by design).
OS-truststore — `[M-116-os-truststore]`.

### D-блок C (NEW, 08-runtime.md) — ALPN

RFC 7301. `ClientConfig.alpn_protocols` (упорядочен), `[]` = без ALPN;
`@alpn_negotiated() -> Option[str]`; отсутствие пересечения → handshake-fail
`AlpnNoCommonProtocol`. Дефолт клиента из `ClientConfig.new` = `["http/1.1"]`
(h2 добавится планом HTTP/2).

### D-блок D (NEW, 05-memory.md) — жизненный цикл TLS-сессии + consume close

- `TlsStream consume @close() Net -> ()`: best-effort close_notify → close
  underlying TCP → `tls_free`. Возврат `()` — зеркало `TcpStream.close`
  (ошибка отправки alert'а при закрытии неактуальна вызывающему).
- Use-after-close = compile error (D133); double-close невозможен.
- На error-путях handshake ресурсы не утекают: `tls_free` + `tcp.close()`
  внутри `connect`/`accept`.
- НЕ в V1: split (`[M-116-tls-split]` — TLS-фреймы stateful), session
  resumption/0-RTT (`[M-116-session-resumption]`), renegotiation (TLS 1.3
  deprecates — out of scope permanently).
- Cross-ref: D131/D180 (consume), D407 §6 (split-модель net), Body-прецедент
  (consume-поле в consume-записи).

---

## Тесты (std/tls/*_test.nv + std/tls/neg/; fixture-PEM в std/tls/testdata/)

- **T1 декларации:** типы/enum/конфиги парсятся и чек-аются; NEG: use-after-
  close TlsStream — compile error (D133).
- **T2 handshake smoke:** self-signed пара client↔server на loopback
  (real_net): рукопожатие, encrypted round-trip (`write_all` → `read_to_vec`),
  `@close` с обеих сторон.
- **T3 io-conformance:** `TlsStream` проходит generic `io.copy`/`read_to_end`
  (structural `Read`/`Write`); `Ok(0)` = clean close_notify.
- **T4 cert/SNI/ALPN:** позитив на каждый режим; NEG: expired /
  self-signed-vs-SystemRoots / hostname-mismatch / ALPN-no-overlap /
  pinned-wrong-hash; truncation (обрыв без close_notify) →
  `PeerMisbehaved` на typed-поверхности.
- **T5 mTLS:** Required + серт → OK; NEG: Required без серта; Optional оба
  варианта.
- **T6 cancel + errors:** supervised-cancel посреди handshake; Net-ошибка
  внутри пампа → `TlsError.Net(...)`.
- **T7 http:** localhost HTTPS GET через `HttpClient` (закрытие
  [M-178-https-needs-116]); NEG: https к серверу с bad cert → `HttpError`.
- **Regression:** detect172 pos+neg; полный `nova test` в конце — baseline.

---

## Acceptance criteria

| # | Критерий | Verification |
|---|---|---|
| A1 | tls_shim собирается (rustls 0.23 + ring, pinned Cargo.lock), C-ABI `tls_*` | Ф.0 ✅ / cargo build |
| A2 | Условная линковка по факту использования (Q11-деградация без либы) | Ф.2 + NOVA_DEBUG_TLS_LINK |
| A3 | `TlsStream.connect/accept` + handshake pump; localhost smoke | T2 |
| A4 | `TlsStream` structural `io.Read`/`io.Write` (IoError-проекция) | T3 |
| A5 | `consume @close()` — close_notify + TCP close; use-after → compile error | T1-NEG + T2 |
| A6 | SNI обязателен; 4 режима верификации; hostname verification | T4 |
| A7 | ALPN negotiation + `AlpnNoCommonProtocol` | T4 |
| A8 | mTLS (NoClientAuth/Optional/Required) | T5 |
| A9 | `TlsError` typed end-to-end + `from_shim`-классификация + Q3-проекция | T4/T6 + grep |
| A10 | Cancel-safe handshake (supervised) | T6 |
| A11 | std/http https работает; `[M-178-https-needs-116]` закрыт | T7 |
| A12 | D-блоки A-D промоучены (номера next-free) | spec diff |
| A13 | `examples/tls/echo_*` работают | Ф.5.4 |
| A14 | Windows MSVC+clang PASS; Linux/macOS — по CI-доступности | Ф.6.3 |

---

## Risk register (актуализирован)

| # | Риск | Митигация |
|---|---|---|
| ~~R-1~~ | ~~выбор бэкенда~~ | ✅ РЕШЁН Ф.0: rustls 0.23 + ring + webpki-roots (§Ключевое решение Ф.0-2); альтернативные бэкенды — `[M-116-openssl-backend]`/`[M-116-native-tls-backend]` followups |
| R-2 | Линковка Rust-staticlib в C-пайплайн (CRT /MT, symbol clash, системные либы) | Ф.0-скелет уже собирается; Ф.2 safety hatch → Plan 116.1; native-static-libs фиксируется явно; crt-static зеркалит x64-windows-static |
| R-3 | webpki-roots ≠ OS truststore (corp CA) | `CustomRoots`; `[M-116-os-truststore]` followup |
| R-4 | Legacy TLS 1.0/1.1 | rustls отвергает by design; legacy = `[M-116-openssl-backend]` opt-in |
| R-5 | Утечка сессии/сокета на error-путях | consume + явные `tls_free`/`tcp.close()` в connect/accept; cancel-тест T6 |
| R-6 | Handshake-зависание (медленный peer) | дедлайн — supervised-scope на call-site (конвенция 173), не поле конфига |
| R-7 | OCSP/CRL revocation | вне V1 — `[M-116-ocsp-crl]` |
| R-8 | Обновления rustls ломают шим | pin exact в Cargo.lock; апгрейды — `[M-116-rustls-upgrade]` с прогоном T-серий |
| R-9 | Supply-chain (crates.io) | Cargo.lock закоммичен; полный `cargo vendor` — `[M-116-cargo-vendor]` при офлайн-требовании |
| R-10 | Размер бинаря (+rustls+ring+roots) | условная линковка (A2): не-TLS программы не платят ничего; замер в Ф.2.1 |

---

## Out of scope (deferred markers)

| Маркер | Что |
|---|---|
| `[M-116-tls-split]` | split на read/write-половины (stateful TLS-фреймы → session lock) |
| `[M-116-session-resumption]` | session tickets / 0-RTT |
| `[M-116-ocsp-crl]` | OCSP stapling / CRL |
| `[M-116-os-truststore]` | rustls-native-certs (Schannel/Keychain/ca-certificates) |
| `[M-116-openssl-backend]` / `[M-116-native-tls-backend]` | альтернативные бэкенды |
| `[M-116-config-cache]` | переиспользование построенного rustls-конфига между коннектами |
| `[M-116-connect-oneshot]` | ergonomic `resolve+connect+handshake` one-shot |
| `[M-116-peer-cert-parse]` | парсинг X.509 (subject/issuer/validity) из DER — V1 отдаёт сырые DER-байты |
| `[M-116-cargo-vendor]` | вендоринг исходников dep-tree для офлайн-сборки |
| `[M-116-dtls]` / `[M-116-quic]` / `[M-116-tls-pre-shared-key]` / `[M-116-tls-pq-crypto]` | DTLS / QUIC / PSK / post-quantum |

---

## Rollback strategy

1. Revert PR — atomic; worktree `nova-116` сохраняется для диагностики.
2. Per-phase rollback (Ф.1-Ф.7 = отдельные коммиты).
3. Удаление tls_shim = удаление каталога + отвязка условной линковки
   (test_runner.rs) — не задевает dep-tree nova-codegen (шим изолирован).
4. std/http https-ветка возвращается к `Err(Tls)`-гейту одним revert'ом Ф.5.3.

---

## Cross-references

- **Plan 183** (net-rework, D407) — фундамент: `Net` byte-surface, `TcpStream`.
- **Plan 176 Ф.4** (io protocols + Q3) — `io.Read`/`io.Write`, ErrorKind-проекция.
- **Plan 177** — Result-everywhere.
- **Plan 178** (std/http client) — потребитель: `[M-178-https-needs-116]`.
- **Plan 179** (encoding/compress) — прецедент FFI-компьюта без эффекта +
  условной линковки (D337) + §4а-хендла (CBrotliHandle).
- **Plan 73 / 100.x** — consume; **Body** (std/http/body.nv) — consume-поле
  в consume-записи.
- **Spec:** D407 (net byte-surface), D337 (conditional link), D133 (must-
  consume), D322/D323 (io protocols + IoError), D282 (extern "C" literal),
  D325 (fallible naming), D99 (#cfg).

---

## Status — closure summary

> Заполняется по фазам.

### Ф.0 — АКТУАЛИЗАЦИЯ + R-1 + FFI-дизайн + скелет ✅ 2026-07-10 (opus)

- План переписан целиком под Plan 183/176.4/177/178 + конвенции §4а/§5а
  (таблица «было → стало» — §Актуализация). Ключевые решения: **эффекта
  нет** (библиотечный слой, мотивировка §Ф.0-1), **rustls 0.23 + ring +
  webpki-roots** (§Ф.0-2), D-номера — next-free при промоушене (D210-D213
  ретрактированы).
- FFI-граница: ~27 символов `tls_*` (таблица в §Дизайн), хендлы
  `CTlsHandle(*())`/`CTlsCfgHandle(*())`, стабильные int-коды ошибок.
- Вендоринг скелета: `compiler-codegen/tls_shim/` (staticlib, panic=abort,
  Cargo.lock pinned) — компилируемая реализация C-ABI поверхности поверх
  rustls (client/server сессии, конфиг-билдеры SystemRoots/CustomRoots/
  Insecure + ALPN + server cert/key + mTLS, трафик-примитивы, инспекция,
  error-классификация). `Pinned`-verifier — заглушка `TLS_ERR_UNSUPPORTED`
  до Ф.4 (реализация кастомного ServerCertVerifier — там же).
- Сборка/верификация (2026-07-10, Windows x64, Rust 1.95):
  - `cargo build --release` зелёный первой попыткой: rustls 0.23.41 + ring
    0.17.14 (имеющийся cc/clang/MSVC, без cmake/nasm) → артефакт
    `target/release/nova_tls_shim.lib` (~6.1 MB до финального dead-strip).
  - `cargo test --release` — **5/5 PASS** (санити C-ABI: ClientHello строится
    и wants_write, bad-PEM → TLS_ERR_INVALID_PEM, bad-SNI → TLS_ERR_INVALID_SNI,
    Pinned → TLS_ERR_UNSUPPORTED, сервер без cert/key → TLS_ERR_BADARG).
  - **R-2 (CRT) де-рискнут:** дефолтная сборка требует `/defaultlib:msvcrt`
    (/MD — конфликт с /MT-пайплайном x64-windows-static); с
    `-C target-feature=+crt-static` → `/defaultlib:libcmt` (/MT). Флаг
    закреплён в `tls_shim/.cargo/config.toml`.
  - native-static-libs (Windows, для link-line Ф.2.3): `bcrypt.lib
    advapi32.lib kernel32.lib ntdll.lib userenv.lib ws2_32.lib dbghelp.lib`
    (ws2_32/advapi32 уже в net-линковке; bcrypt/ntdll/userenv/dbghelp —
    добавить при условной линковке).
- Следующая фаза: Ф.1 (std/tls типы + ошибки + ffi.nv).

### Ф.1 — std/tls типы + ошибки ✅ 2026-07-10

- После слияния main: **D415 занят Plan 173.3 (#share)** — ориентир промоушена
  сместился на D416+ (правило «next-free по grep» без изменений).
- Создано: `std/tls/error.nv` (TlsError 12 вариантов + `@to_str` +
  `from_shim` по стабильным кодам + Q3-проекция `@to_error_kind`/
  `@to_io_error`; добавлен вариант `InvalidServerName(str)` для
  TLS_ERR_INVALID_SNI — не был в эскизе), `std/tls/config.nv`
  (VerificationMode/ClientCertMode/TlsVersion + `from_wire`;
  ClientConfig/ServerConfig + `new` + with_*-модификаторы), `std/tls/ffi.nv`
  (29 extern `tls_*` + `CTlsHandle(*())`/`CTlsCfgHandle(*())` — codegen
  подтверждён: `typedef void* Nova_CTlsHandle`, ABI не меняется).
- **Ф.1.4 (stream.nv) ПЕРЕНЕСЁН в Ф.3** — обнаружено чекером: consume-тип без
  consume-метода в модуле ill-formed (`D133-empty-consume`), а честный
  `@close` требует pump + линковку шима (Ф.2/Ф.3). Заодно подтверждена
  форма поля: `priv consume tcp TcpStream` (D133-field-marker).
- Тесты (все PASS через test-build, toolchain clang): `error_test.nv` (9
  тестов: to_str / from_shim все коды / проекция / Net-делегация / to_io_error),
  `config_test.nv` (5 тестов: дефолты, with_* = новое значение, mTLS-режимы,
  from_wire), `neg/config_ro_frozen_neg.nv` (E_READONLY_FIELD — ro-конфиг
  заморожен). Ф.1 умышленно БЕЗ вызовов tls_* (link-free до Ф.2).
- Наступили на pre-existing ICE `[M-176-xmod-payload-variant-ctor]`
  (`Type.Variant.method()` без ro-биндинга) — обход по net-прецеденту
  (ro-биндинг), в тест-файлах ссылка на маркер.

### Ф.2 — Сборочная интеграция шима ✅ 2026-07-10

- **Условная линковка (механизм brotli/D337), test_runner.rs:** `TlsConfig` +
  `detect_tls` (порядок: `target/tls-cache/` → cargo-артефакт крейта →
  **auto-build через cargo** по образцу `detect_or_build_libuv` — свежий клон
  не деградирует; провал сборки → честный stub-путь) + `c_file_uses_tls`
  (маркер Ф.2: call-site/decl `tls_client_cfg_new(`/`tls_server_cfg_new(`;
  NB Ф.5: после импорта std/tls из std/http перейти на скан манглед-обёрток —
  урок brotli, отмечено в коде) + ветки линковки в clang (gcc-стиль
  `-lbcrypt -lntdll`) / MSVC (`bcrypt.lib ntdll.lib`) / gcc-Unix +
  `NOVA_DEBUG_TLS_LINK=1` диагностика.
- **Q11-заглушка `nova_rt/tls_stub.c`** (29 символов, TLS_ERR_UNSUPPORTED/-11,
  текст «как собрать»): компилируется вместо либы; взаимоисключающие ветки —
  clash невозможен.
- **Найден и закрыт реальный дефект механизма:** вызов `tls_*` в
  сгенерированном C был implicit declaration (D82 — декларации не эмитятся) →
  возврат int (32 бита) → **трункация хендла-указателя → SEGV**
  (`tls_cfg_verify_system+0xB`, пойман NOVA_DIAG_SEGV-локалайзером; изолированный
  C-тест с тем же clang работал — рознились именно прототипы). Fix по
  net/brotli-образцу: **`nova_rt/tls_shim.h`** (чистые прототипы, включён
  безусловно из nova_rt.h; tls_stub.c включает его же — сверка сигнатур).
- **Верификация в три стороны** (NOVA_DEBUG_TLS_LINK=1):
  1) TLS-CU + либа → `LINK …nova_tls_shim.lib` → `shim_link_test.nv` **PASS**
     (реальный rustls сквозь Nova: bad-SNI → -12, ClientHello в write_tls,
     кривой PEM → -10 → `from_shim` → `InvalidPem`);
  2) не-TLS CU (`std/data/semver_range_test`) → `no tls` + PASS;
  3) либа скрыта → `STUB tls_stub.c` → стаб-проба PASS (cfg_new=0,
     err=-11 → `Internal("unsupported…")`) — link-error нет.
- **Гейт: conformance --positive --compile-error = 82/0** (эталон волны).
- Pre-existing красные, ДОКАЗАННО не мои (baseline без tls_shim.h-инклуда —
  идентичный фейл): `std/io/d322_lines_test` RUN-FAIL (seek `requires n >= 0`);
  `std/net` folder-CU НЕ КОМПИЛИРУЕТСЯ — `E_CONCURRENT_MUT_CAPTURE` в
  `tcp_test.nv:89` (свежий чекер 173.3/D415 vs незамигрированный spawn-тест
  net) — эскалировано интегратору волны 173.3, вне периметра 116.

### Ф.3 — TlsStream + pump + connect/accept ✅ 2026-07-10 (дедлок и вскрытые дефекты закрыты — см. дополнение ниже)

- **Написано и КОМПИЛИРУЕТСЯ/ЛИНКУЕТСЯ:** `std/tls/stream.nv` (TlsStream
  consume-запись; sans-I/O пампинг свободными fn `flush_out`/`fill_from_tcp`/
  `pump_handshake`/`read_step`/`write_step` над byte-surface `Net`; io.Read/
  io.Write методы + typed `@write_all`/`@read_to_vec`/`@read_text`; инспекция
  `@alpn_negotiated`/`@protocol_version`/`@cipher_suite`/`@peer_cert_der`;
  `consume @close`), `std/tls/client.nv` (`TlsStream.connect`), `std/tls/
  server.nv` (`TlsStream.accept`). Тесты типов/ошибок/конфигов (Ф.1) + shim-link
  (Ф.2) PASS.
- **2 codegen-обхода, задокументированы маркерами:**
  - `[M-116-result-over-ptr-newtype-mono]` (NEW): `Result[<newtype-над-*()>, E]`
    мис-моноится в `NovaRes_nova_int_nova_str` (CC-FAIL passing TlsError* as
    nova_str). Обход: **хендлы = newtype над `int`** (`CTlsHandle(int)`,
    brotli-прецедент), НЕ над `*()`; C-заголовок tls_shim.h/tls_stub.c → intptr_t
    (ABI-идентично на x64). Repro в backlog.
  - `[M-178-consume-field-ctor-from-var]` (существующий): move consume-переменной
    в consume-поле/Ok-пейлоад не распознаётся — обход pass-through-вызовом
    (`tcp_move`) + wrap только в `Ok(...)` после успешного handshake (pump берёт
    tcp view-borrow'ом `mut`, TlsStream строится ОДНИМ вызовом).
- **RUNTIME-ДЕДЛОК (открыт, локализован — НЕ в крипте):**
  - **Шим+rustls ДОКАЗАНО корректны:** новый Rust-тест
    `full_in_memory_handshake_completes` (tls_shim/src/lib.rs) гоняет ПОЛНЫЙ
    TLS 1.3 handshake client↔server через C-ABI, вручную перекладывая
    ciphertext: **is_handshaking обнуляется на ОБЕИХ сторонах за 2 шага**,
    app-data "ping" расшифровывается. 6/6 shim cargo test PASS.
  - **Дедлок — в Nova-транспорте (socket pump), не в TLS.** Smoke на loopback
    (real_net, 2 фибры) виснет: обе фибры паркуются в `read_to_vec` (Net.read),
    watchdog-dump `pending_remote=2 pstate=1`. Диагностика доказала: pump НЕ
    зацикливается (iteration-panic >40 НЕ срабатывает) → блокировка в
    ЕДИНСТВЕННОМ read, который не будится (genuine mutual-read park). Байты
    handshake РЕАЛЬНО текут по проводу (CH 251 → flight ~720 → CCS+Fin 80 →
    ticket 184), но соединение не финализируется на уровне сокет-обмена.
  - **Не воспроизвелось cert-режимом** (InsecureSkipVerify виснет так же) и не
    зависит от echo (handshake+close-only виснет). Гипотеза: edge-case net.c
    park/wake при плотном alternating read/write из одной фибры на одном сокете
    (stress_test — иной паттерн, не ловит), ЛИБО тонкость Net.read-семантики.
    Требует socket-уровневого трейса. `[M-116-handshake-socket-deadlock]`.
  - **Инструментальная заметка:** exe test-build удаляется на TIMEOUT →
    трейс-грабы ненадёжны; стоит добавить в test-runner опцию сохранения exe /
    inherit-stdout для отладки виснущих тестов (отдельный tooling-followup).

#### Ф.3 — закрытие дедлока + вскрытых дефектов ✅ 2026-07-10 (race-инвестигация, sonnet)

- **Root cause дедлока `[M-116-handshake-socket-deadlock]` — НЕ pump и НЕ
  lost-wake latch, а loop-affinity гонка issue-стороны uv-опов** — ровно
  «остаточный класс», предсказанный записью `[M-183-net2-loop-affinity-cross-thread-op]`:
  волокно уводится work-stealing'ом на другой worker МЕЖДУ парковками
  (write→park→read на одном сокете — паттерн TLS-pump, каждый park = окно
  миграции), следующий uv-оп выдаётся на хендле, пришпиленном к loop'у СТАРОГО
  worker'а, с ЧУЖОГО потока → конкурентная мутация не-thread-safe uv_loop →
  completion mis-queued/потерян → обе стороны навечно в `Net.read`-park
  (наблюдённый watchdog-дамп pending_remote=2 pstate=1). Частота ~1/300
  (доказано серией: 1 HANG на 300 одиночных прогонов чистого exe).
- **Фикс A (rt, 5ca0ace10):** `nova_loop_defer_call` — generic-обобщение
  `nova_loop_defer_close` (Plan 83.10.2): per-loop `NovaDeferredCallQueue`
  (main + каждый worker; init/drain/destroy рядом с close_queue), маршалит
  `fn(arg)` на owning-loop-поток через mutex-очередь + `uv_async_send`.
  В net.c ВСЕ issue-точки на ранее созданных хендлах (tcp read/write/accept/
  shutdown; udp send/recv) ветвятся: same-thread (обычный случай) = прямой
  вызов, байт-в-байт прежнее поведение; cross-thread = `_deferred`-обёртка,
  ТОЛЬКО она публикует completion-latch + wake. **Урок (пойман split_test):**
  unconditional latch+wake на same-thread пути = reentrant self-wake ДО
  собственной парковки волокна → гонка с gopark/goready park-state (~50%
  зависаний accept-пути) — задокументировано в net.c. Accepted-stream
  наследует `lst->loop` (не `nova_current_loop()`).
- **Фикс B (codegen, a89597277) — вскрыт снятием дедлока:** heap-promoted
  примитивный локал (Plan 118 Ф.1, эскейп `&x` аргументом вызова) читался
  БЕЗ разыменования — `mut err int = 0; c_fn(&err); use(err)` передавал
  heap-АДРЕС вместо значения (`from_shim` получал ~2.8e12 вместо -10 → NEG
  bad-PEM классифицировался Internal вместо InvalidPem; тест-тела эскейп-
  анализ не проходят — прямые FFI-пробы врали «работает»). Фикс: Ident-ветка
  emit_expr дерефит promoted-локал (`(*name)`, зеркало var_boxed; assign-
  таргеты тем же путём), var_types хранит базовый тип.
- **Регресс:** `std/net/pingpong_test.nv` — alternating write→read на одном
  сокете (64-цикловый строгий ping-pong + две конкурентные пары).
- **Ложный след (закрыт как не-дефект):** «split_test виснет ~50% на baseline»
  — это slow-DNS (NXDOMAIN-тест dns_test до ~17с) при 8с-таймауте моей
  стресс-обвязки; с 60с — 35/35 стабильно, и на baseline, и с фиксами.
- **Гейты волны:** handshake_test **19/19 PASS** (smoke + NEG hostname-
  mismatch + NEG SNI/bad-PEM) — против TIMEOUT до фикса; стресс smoke
  0 зависаний на 720+ прогонах (8×90) против ~1/300; std/net folder-CU
  (37 тестов, вкл. новый pingpong) ×3 PASS; conformance **90/0** (на слитом
  main d1b9b2bc8); err173-корпус 5/5 PASS (δ0).
- Остались Ф.3.4-хвосты следующей волне: encrypted round-trip уже покрыт
  smoke; cancel-safety (Ф.6.2) и cert-режимы (Ф.4) — по плану.

### Ф.4 + Ф.4.3 — cert-режимы + Pinned-verifier ✅ 2026-07-10

- **Ф.4.3 (шим):** снята заглушка `TLS_ERR_UNSUPPORTED` — `PinnedVerify`
  (compiler-codegen/tls_shim): ручной DER-walk `SubjectPublicKeyInfo` из
  leaf-серта (`der_tlv`/`spki_der` — без новых крейтов) + SHA-256 (`ring`,
  уже транзитивно в lock → 0 новых пакетов); подпись рукопожатия ВСЁ РАВНО
  проверяется (`verify_tls12/13_signature`), hostname заменён пиннингом
  (D-блок B). Shim cargo **8/8** (pinned correct/wrong-pin/wrong-sni +
  cross-check SPKI-пина против openssl).
- **Ф.4 (Nova, `std/tls/cert_modes_test.nv`) — 6/6 loopback PASS:** Pinned
  верный-пин→OK / неверный→reject / wrong-SNI-но-верный-пин→OK (пиннинг
  заменяет hostname); SystemRoots vs self-signed → CertificateInvalid;
  ALPN пересечение (h2/http1.1→http/1.1) + no-overlap→fail. SPKI-пин
  фикстуры (`x"92f7…34"`) cross-checked с shim-алгоритмом.

### Ф.5 — server mTLS + client-cert ✅ 2026-07-10

- `ClientConfig` получил `client_cert_pem`/`client_key_pem` + `@with_client_cert`
  (пусто = без mTLS); `client.nv build_client_cfg` шлёт серт когда задан (та же
  `tls_cfg_cert_key_pem`, что серверная). Server-side `ClientCertMode`
  (NoClientAuth/Optional/Required) был готов с Ф.3 (server.nv).
- Fixtures: `client_ca_cert.pem` (CA:TRUE) + `client_cert.pem`/`client_key.pem`
  (leaf, EKU=clientAuth, подписан CA). `std/tls/mtls_test.nv` **4/4 PASS**:
  Required+серт→OK, Required-без-серта→server отвергает (в TLS 1.3 клиент
  завершает сторону оптимистично → отказ ловит сервер), Optional с/без серта.

### Ф.5.3 — https-разгейт std/http ✅ 2026-07-10 (код; runtime-verify через прокси)

- `real_http()` `secure=true` → `https_send_over_net` (TLS поверх `TcpStream`:
  SNI=host, SystemRoots, ALPN http/1.1) вместо `Err(Tls)`. Закрывает
  `[M-178-https-needs-116]`.
- **Верификация:** паттерн (resolve→connect→TLS→write→read-loop→close) прогнан
  в compress-free CU (proxy-тест в std.tls, mock non-TLS peer → детерминир.
  Err) — **PASS**; сам TLS-слой — std/tls loopback-тесты Ф.3/Ф.4/Ф.5.
- **⚠ БЛОКЕР (PRE-EXISTING, не Plan-116):** весь http-CU (`client_test`/
  `real_test`) НЕ КОМПИЛИТСЯ — `[M-compress-checksum-structvariant-ctor-xmodule]`
  (std/encoding/compress/error.nv:121 struct-payload variant-ctor `Checksum`
  → E_UNKNOWN_TYPE в multi-ErrorKind CU). Доказано pre-existing: `client_test`
  (без TLS) падает идентично; compress соло — PASS. Эскалировано (codegen-зона).
- **Followups:** `[M-116-https-client-custom-roots]` (HttpClient TLS-config хук
  для self-signed loopback HTTPS), `[M-178-errsource-tls]` (типизированный source).
- **Гейт волны:** conformance --positive --compile-error **91/0**; все std/tls
  тесты (handshake 19/19 + cert_modes 6 + mtls 4 + error/config/shim-link + neg)
  PASS; shim cargo 8/8.
- **Осталось:** Ф.5.4 examples/tls/echo, Ф.6 cancel-safety/кросс-платформа,
  Ф.7 spec D-блоки A-D + docs-guide + close.
