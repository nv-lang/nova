<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 182 — std/http: message-model + URL + HTTP/1.1 (client+server) + HTTPS + HTTP/2

> **Top-level umbrella** (как Plan 180 = io+fs+os): полный HTTP-стек Nova в ОДНОМ плане — message-model, URL, HTTP/1.1 client+server, HTTPS, HTTP/2 (client+server).
> Создан 2026-06-26. Статус **proposed**.
> Маркер **[M-182-std-http]**. Запуск: «выполни план 182».
> Эталон: **Go `net/http`** (архитектура: синхронный API над фибрами; client/server-симметрия; `Handler`/`ServeMux`; io.Reader-тела; context-style cancel) + **reqwest/requests** (builder-эргономика клиента) + **WHATWG / http-crate** (data-model: Method/StatusCode/HeaderMap/Body). Nova **чинит Go-footguns типами**: must-consume `Body` (compile-time force-close), deadline/timeout-by-default (Plan 173), Result-everywhere typed-errors (D325).
> **D-блоки (NEW): D327** (`Http`/`HttpServer` effect-контракт + net byte-surface) · **D328** (message-model) · **D329** (`Body` must-consume + stream + bomb-cap) · **D330** (redirect/cookie/auth-strip/retry/proxy/pool-eviction) · **D331** (server contract) · **D332** (HTTP-over-TLS + HTTP/2).
> **🔴 HARD-GATES:** Ф.4 (HTTPS) и Ф.5 (HTTP/2) жёстко гейтятся на **Plan 116** (`std/tls`, сейчас `PLANNED`, backend rustls; даёт `Tls`-effect / `TlsStream consume` / `ClientConfig` / `ServerConfig` / SNI / **ALPN** — D210-D213). **Auto-decompress** (gzip/deflate/br) жёстко гейтится на **NEW sub-plan `std/encoding/compress`** (RFC 1950/1951/1952 + brotli; ДОЛЖЕН быть создан отдельно — НЕ в этом плане; owner 2026-06-26). **Typed `.json[T]()`** жёстко гейтится на **NEW serde-sub-plan** (auto-derive `Serialize`/`Deserialize`; owner 2026-06-26); **dynamic `.json()->JsonValue`** приземляется СЕЙЧАС над существующим `std/encoding/json`. Plan 116 толкается параллельно Ф.0-Ф.3; plaintext HTTP/1.1 (Ф.1-Ф.3) + `identity`/`chunked` transfer + dynamic-`JsonValue` — **самодостаточный deliverable**, приземляется независимо.
> **Координация:** Plan 116 (TLS), Plan 173 (structured concurrency/errors), Plan 179 (Time), Plan 181 (D325), std/net-семейство (**ПОЛНАЯ byte-surface migration `str`→`[]u8`** — owner-approved governance amendment 2026-06-26, §9), Plan 103.3/103.4 (Mutex/Semaphore), **`std/encoding/json`** (dynamic JSON, существует), **NEW `std/encoding/compress`** (decompress gate), **NEW serde-sub-plan** (typed json gate).
> **Сквозной критерий (§8.0):** **ОБЯЗАТЕЛЬНО: без упрощений, как для прода** — ни одного «решим потом» на критическом пути; каждая behavior-change несёт pos+neg-тесты + аргумент звучности, на КАЖДОЙ приземлённой фазе.

---

## 1. Зачем

В Nova **есть `std/net`** (TcpNet/UdpNet/DnsNet/AddrNet, 91.12/83.12 — production-grade TCP/UDP, park/wake over libuv) и **планируется `std/tls`** (Plan 116, rustls, SNI/ALPN), но **нет `std/http`** — ни клиента, ни сервера. Это держит Nova на уровне «сырой сокет»: чтобы дёрнуть REST API или поднять backend-эндпоинт, пользователь сам сериализует `GET … HTTP/1.1\r\nHost:…\r\n\r\n` руками поверх `TcpStream.write` (ровно как `fetch_https` в [116:338-369](116-std-tls-effect.md) — заглушка «manually serialized — until Plan 117»). Любой современный язык несёт HTTP в std/первой-партии (Go `net/http`, Rust reqwest/hyper, Node `fetch`/undici, Kotlin Ktor/OkHttp, Java `java.net.http`, Zig `std.http`, Swift `URLSession`); без него Nova **не годится для своей же P0-ниши** — backend-сервисов, API-клиентов и тулинга (Plan 18: web/backend для 0.2).

**Что это разблокирует:** (1) **backend-сервисы** — HTTP/1.1 + HTTP/2 сервер с роутингом, middleware, graceful-shutdown, поверх structured-concurrency (Plan 173): сервер = supervised-scope, per-conn = spawned fiber, что **бесплатно наследует** sync-looking-over-fibers модель net (ровно Go blocking-over-goroutines, [net/effect.nv:10-15](../../std/net/effect.nv)); (2) **API-клиенты** — pooled `HttpClient` с reqwest-builder-эргономикой (`client.get(url).header(k,v).json(body).send()`), keep-alive, авто-decompress, redirects, cookie jar, auth; (3) **тулинг** — `nova` package-registry клиент (Plan 03.x dangling — нужен HTTP для remote-fetch), webhook-интеграции, health-checks, MCP/RPC-over-HTTP. HTTPS и HTTP/2 — поздние фазы (HARD-GATE Plan 116, §4); plaintext HTTP/1.1 (клиент+сервер) приземляется первым и самодостаточен. **Этот план консолидирует** dangling forward-refs Plan 116 (`Plan 117 = HttpClient`, `Plan 122 = HttpServer`, [116:74-79/753-755](116-std-tls-effect.md)) → **Plan 182** (под-планы 182.1 server / 182.2 h2, если поздние фазы разрастутся).

## 1a. Где Nova ЛУЧШЕ peers (differentiators — в доку)

- **🏆 must-consume `HttpResponse.body` (Plan 80/D133): `consume @close()`/`consume @bytes()`/`consume @text()` — ЕДИНСТВЕННЫЙ способ разрядить тело; незакрытый body = compile-error.** Это превращает **самый известный Go-footgun** (`resp.Body` не закрыт → leak соединения из пула, exhaustion под нагрузкой; `defer resp.Body.Close()` глотает ошибку чтения) в **compile-time-гарантию**. reqwest/hyper полагаются на `Drop` (молча роняет недочитанное тело, ломает keep-alive-reuse), Node `fetch` требует ручного `.body.cancel()` (никто не зовёт), Java `HttpResponse` буферизует всё в память. **Никакой peer не может это enforce'ить статически.** Самый крупный differentiator — прямой аналог must-consume `File` из Plan 180 ([180:29-32](180-io-fs-os.md)).
- **🏆 deadline/timeout-by-default (Plan 173): запрос ограничен deadline supervised-scope ПО УМОЛЧАНИЮ; timeout → `throw Timeout`, cancel пропагирует в net-park.** Чинит **главный Go-footgun** — `http.DefaultClient` БЕЗ таймаута (зависший сервер вешает горутину навечно; классический prod-инцидент; нужно вручную городить `context.WithTimeout`). У Nova таймаут — свойство scope, не опциональный городок; cancel cancel-safe доходит до libuv-park ([net/effect.nv:10-15](../../std/net/effect.nv)). reqwest имеет таймаут опционально, Node `fetch` — только через `AbortController` (ручной), java.net.http — `.timeout()` опционально.
- **🏆 h2-stream = fiber (Nova differentiator): каждый HTTP/2 stream = отдельный fiber под supervised-scope.** Мультиплексирование укладывается на M:N-планировщик **естественно** — flow-control/backpressure = park/wake, отмена stream = cancel фибры, без ручной callback-машины (hyper h2 — futures+poll, Go — горутина-на-stream но без structured-cancel). Прямой выигрыш от 83.x M:N.
- **🏆 SSRF/smuggling/injection-устойчивость by-construction:** строгий URL/host-валидатор (reject control/NUL/whitespace в host, canonical IP-literal, bracket-IPv6, reject decimal/octal/hex-IP-obfuscation), opt-in **SSRF-guard** (`deny_private_ranges` — блок link-local/loopback/`169.254.169.254`-metadata), reject двойной `Content-Length`/`Transfer-Encoding` (CL.TE/TE.CL-smuggling), reject CR/LF/NUL в header-name/value (response-splitting), cross-origin auth/cookie-strip. **Это упущенная пирами differentiator-ось** (никто не делает SSRF-guard в std из коробки) — Nova берёт её.
- **Result-everywhere typed `HttpError` (D325, [181:6-9](181-fallible-result-everywhere.md)):** единый структурный `HttpError{kind, url, source}` с OPEN `ErrorKind` (Connect/Dns/Tls/Timeout/Protocol/TooManyRedirects/InvalidUrl/Closed/Canceled/BodyTooLarge/Status/Io) и source-chaining `NetError`/`TlsError`/`ParseUrlError`; wildcard-arm forced. Бьёт Go stringly-typed (`err.Error()`-substring-match, опечатка компилится), Node reject-`any`, Java checked-exception-шум. **4xx/5xx = ВАЛИДНЫЙ `Response`, НЕ ошибка**; reqwest-style opt-in `.error_for_status()` конвертирует — корректнее Java (бросает на не-2xx некоторыми клиентами) и понятнее Go.
- **structured-concurrency сервер (Plan 173): per-conn fiber под supervised-scope, bounded `Semaphore`, graceful-drain через deadline.** Падение/паника хэндлера изолированы scope'ом (MultiError-аггрегация), `defer` всегда отрабатывает, shutdown = cancel scope + bounded drain. Go `net/http` сам спавнит горутину-на-conn **без** родительского scope (утечка горутин при панике в кастомном коде — реальный класс багов); Nova даёт structured-shutdown из коробки.
- **effect-injectable transport (`Http`/`HttpServer` seam + `mock_http()`):** `with Http = mock_http() { … }` → детерминированный тест клиента/сервера **без сокетов и без сети** (in-memory request→response). Триада-конвенция (effect + `real_http()` + `mock_http()`) + мокабельность закрывают Q9. Go нужен `httptest`/`RoundTripper`-DI, reqwest — `wiremock`/trait-mock, Node — `nock`-monkeypatch; все слабее статически-проверяемого effect-seam.
- **byte-first body done RIGHT:** `Body` = `[]u8` | streaming reader; `str` только через **fallible** `.text() -> Result[str, HttpError]` (UTF-8-валидация по charset, не Node-`U+FFFD`-порча). `.json()` тоже `Result`. Бьёт Node (silent lossy decode) и Go (`io.ReadAll`→`[]byte`, decode на пользователе).
- **version-transparent API:** один `HttpClient`/`Handler` работает поверх HTTP/1.1, HTTPS и HTTP/2 — ALPN авто-договаривается (Plan 116), `Version` (Http10/Http11/Http2) лишь экспонируется. Пользователь не переписывает код под h2 (в отличие от hyper, где h1/h2 — разные типы до высокого уровня).

## 2. Эталон (cross-lang http)

Сравнение фича-за-фичей. **Колонки:** Nova-target | Rust reqwest/hyper | Go net/http | TS fetch/undici | Kotlin Ktor/OkHttp | Java java.net.http | Zig std.http | Swift URLSession. **🏆** = Nova **строго лучше** лучшего peer'а по строке; **=** = на уровне лучшего.

| Фича | **Nova-target** | Rust reqwest/hyper | Go net/http | TS fetch/undici | Kotlin Ktor/OkHttp | Java java.net.http | Zig std.http | Swift URLSession |
|---|---|---|---|---|---|---|---|---|
| client builder | **🏆 `client.get(url).header().query().json().send()->Result`** (reqwest-эрг + typed err) | reqwest builder (эталон эрг) | `http.NewRequest`+manual | `fetch(url,{...})` вербозно | Ktor DSL / OkHttp builder | `HttpRequest.newBuilder()` | manual `Client.open` | `URLRequest` + delegate |
| **body-close-safety** | **🏆 must-consume `body`: незакрытый = COMPILE-ERROR** | `Drop` (молча роняет, ломает reuse) | `defer Body.Close()` (**leak-footgun**, err игнор) | ручной `.cancel()` (никто не зовёт) | `use{}`/`.close()` (runtime) | авто-буфер в память | manual `deinit` | авто (managed) |
| **timeout-default** | **🏆 deadline-by-default (173 scope), cancel→net-park** | opt `.timeout()` | **DefaultClient = NO timeout** (footgun) | `AbortController` (ручной) | opt config | opt `.timeout()` | manual | opt `.timeoutInterval` |
| conn pool / keep-alive | **🏆 pooled + structured-lifetime + cancel-safe + retry-idempotent** | pool (hyper) = | transport pool (mature) = | undici pool / fetch=no-pool | OkHttp pool (эталон) = | HTTP/2 conn pool = | **отсутствует** (no pool) | URLSession pool = |
| redirects | **= policy (limit + `TooManyRedirects` typed + cross-origin auth-strip)** | reqwest policy = | follows (configurable) | follows (no count-cap) | OkHttp follows = | `.followRedirects()` | manual | follows = |
| decompression | **= auto gzip/deflate/br (opt-out) + bomb-cap** | reqwest auto = | gzip auto (transport) | auto (browser) / undici | auto (OkHttp) = | manual | **отсутствует** | auto = |
| cookies jar | **= typed `CookieJar` (per-client, RFC 6265bis: Secure/__Host-/SameSite/PSL)** | reqwest jar = | `http.CookieJar` = | browser jar / undici=no | OkHttp `CookieJar` = | `CookieManager` = | **отсутствует** | `HTTPCookieStorage` = |
| multipart | **= `multipart/form-data` builder** | reqwest multipart = | `mime/multipart` (manual-ish) | `FormData` = | Ktor/OkHttp = | manual | **отсутствует** | manual |
| streaming bodies | **🏆 must-consume streaming `Body` (reader, backpressure=park) + trailers** | `Stream`/`Body` (Drop-roняет) | `io.Reader` Body (leak-risk) | `ReadableStream` (manual cancel) | `ByteReadChannel` (Ktor) = | reactive `Flow.Subscriber` (verbose) | reader (manual) | `bytes`/delegate = |
| HTTP/2 | **= h2 (gate 116 ALPN), stream=fiber** 🏆*архитектура* | hyper h2 (futures) = | h2 built-in (transparent) = | undici h2 / fetch=auto | OkHttp h2 = | h2 built-in = | **отсутствует** (h1 only) | h2 auto = |
| **server** | **🏆 supervised-scope, per-conn fiber, h1+h2** | hyper (manual wiring) / axum | `net/http` Server (эталон простоты) | undici / node:http | Ktor server = | `com.sun.net.httpserver` (basic) / Netty | **basic** (experimental) | **отсутствует** (client-only) |
| routing | **= `ServeMux` (path-params + method, Go-1.22 precedence)** | axum/actix (3rd-party) | `ServeMux` (1.22 path-params) = | express/router (3rd-party) | Ktor routing DSL = | manual / 3rd-party | manual | n/a |
| middleware | **= interceptor chain (client+server, onion)** | tower (3rd-party) | `http.Handler`-wrap | express middleware | Ktor plugins / OkHttp interceptors = | manual | manual | URLProtocol/delegate |
| TLS / mTLS | **= over Plan 116 (rustls; SNI/ALPN/mTLS)** gate | rustls/native-tls = | crypto/tls (built-in) = | OpenSSL (node) | Conscrypt/JSSE = | JSSE = | **есть** (std.crypto.tls client) | SecureTransport = |
| proxy | **= `Proxy`{http/https/socks5} + CONNECT-tunnel + NO_PROXY env** | reqwest proxy = | `ProxyFromEnvironment` = | undici `ProxyAgent` | OkHttp proxy = | `ProxySelector` = | **отсутствует** | `connectionProxyDictionary` |
| **cancellation** | **🏆 structured cancel → net-park (cancel-safe, MultiError)** | `tokio` cancel (drop-future, not structured) | `context.Context` (manual plumbing) | `AbortController` (manual) | coroutine cancel (structured) = | reactive cancel (verbose) | manual | `URLSessionTask.cancel()` |
| **graceful-shutdown** | **🏆 scope-cancel + bounded deadline-drain (built-in)** | manual (axum `with_graceful_shutdown`) | `Server.Shutdown(ctx)` (manual ctx) = | manual | Ktor `stop(grace)` = | manual | n/a |
| **SSRF/smuggling defense** | **🏆 strict host-validate + opt-in SSRF-guard + CL/TE reject + CRLF reject** | partial (host-reject) | строг (CL/TE) | partial | partial | partial | minimal | partial |
| error model | **🏆 typed `HttpError`+OPEN kind+source-chain; 4xx=Response** | typed `reqwest::Error` (source-chain) = | stringly `err.Error()` (опечатка компилится) | reject `any` | typed sealed (Ktor) = | checked `IOException`-шум | error-union (typed) = | typed `URLError` = |
| mock/test transport | **🏆 `with Http=mock_http()` — статически-проверяемый effect-seam** | `RoundTripper`-trait / wiremock | `httptest`/`RoundTripper`-DI | `nock`-monkeypatch | OkHttp `MockWebServer` | `HttpClient`-mock (3rd-party) | manual | `URLProtocol`-stub |

**Взять:** reqwest **builder-эргономику** (`.get().header().query().json().send()`) + `.error_for_status()` opt-in + idempotent-retry; Go **`ServeMux` path-params** (1.22) + `Server.Shutdown(ctx)` + `ProxyFromEnvironment` + server-простоту; hyper/h2 **stream-multiplexing** модель (но через fiber-на-stream, не futures); OkHttp **interceptor-chain** + connection-pool-зрелость; WHATWG/http-crate **data-model**; Zig **allocation-transparency**-дух (нет скрытого глобального клиента для prod-кода — §3.0 Q15). **Избегать:** Go silent-`Body`-leak + `DefaultClient`-no-timeout + stringly-errors; reqwest/hyper `Drop`-роняет-body; Node ручной-`AbortController` + silent-lossy-decode + `cancel()`-который-никто-не-зовёт; Java checked-exception-шум + eager-буфер-в-память; Zig **отсутствие pool/cookies/decompression/proxy** (не prod). **Доказательство ≥ best-peer построчно:** строки **body-close-safety / timeout-default / streaming / cancellation / graceful-shutdown / error-model / mock-transport / server / SSRF-defense / pool-retry** помечены 🏆; остальные — **=** (паритет с лучшим peer'ом).

---

## 3. Архитектура

**Принцип (net-precedent).** HTTP — это **value-types + Nova-логика** поверх байт-транспорта. Разбор/сборка HTTP/1.1 и h2 (framing/HPACK) — `.nv`, не C. Сетевой плумбинг (park/wake, libuv) уже даёт net-семейство; HTTP добавляет только **тонкий `Http`-seam** (клиент) и `HttpServer`-seam (сервер) для мокабельности и `with`-инъекции. Всё байт-first (`[]u8`), всё fallible → `Result[T, HttpError]` (R1/D325), все ресурсы (`Body`, соединение) — **must-consume** (D133).

### Layering diagram

```
┌──────────────────────────────────────────────────────────────────────────┐
│ App      client.get(url).header(...).send()  │  mux.handle("GET /p", h)    │   ← user value-API
├──────────────────────────────────────────────┼─────────────────────────────┤
│ HttpClient (pool/redirect/cookies/decompress/ │ HttpServer / ServeMux       │   ← Ф.2/Ф.3 (.nv логика)
│   retry/proxy)  · request/response · Body     │   Handler · middleware       │
├──────────────────────────────────────────────┴─────────────────────────────┤
│ HTTP/1.1 wire codec (.nv)        │      HTTP/2 framing + HPACK (.nv, Ф.5)    │   ← парсинг = Nova, не C
├──────────────────────────────────────────────────────────────────────────┤
│ Http effect (real_http / mock_http)  ·  HttpServer effect                   │   ← seam: triad, мокабелен (Q9)
├──────────────────────────────────────────────────────────────────────────┤
│  Tls (Plan 116, GATE: https/h2 ALPN)  │  TcpNet  │  DnsNet  │  Time (173)   │   ← транспорт
├──────────────────────────────────────────────────────────────────────────┤
│                              libuv (park/wake)                              │
└──────────────────────────────────────────────────────────────────────────┘
```

**Version-transparent:** один `HttpClient`/`Handler` обслуживает h1/h2/https; ALPN авто-выбирает версию (gate 116); `Version` лишь *наблюдаема*, не выбирается пользователем на уровне API. h2-специфика (`:method`/`:path`/`:scheme`/`:authority` псевдо-headers, запрет hop-by-hop) **нормализуется framing-слоем в обычный `HeaderMap`** на входе/выходе.

### 3.1. `Method` — enum + extension-вариант

```nova
module std.http

/// HTTP-метод. Стандартные — варианты-константы; нестандартные (RFC 7231 §4.1
/// extension-method, напр. "PROPFIND") — Other(str).
#stable(since = "0.1")
export type Method
    | Get | Head | Post | Put | Delete | Connect | Options | Trace | Patch
    | Other(str)                            // токен метода (uppercase, RFC 7230 tchar)

export fn Method.parse(s str) -> Result[Method, HttpError]      // валидирует tchar; "" / non-token → Protocol
export fn Method @as_str(self) -> str                            // "GET" / "PATCH" / Other → as-is
export fn Method @is_safe(self) -> bool                          // GET/HEAD/OPTIONS/TRACE (RFC 7231 §4.2.1)
export fn Method @is_idempotent(self) -> bool                    // safe + PUT/DELETE
export fn Method @allows_body(self) -> bool                      // GET/HEAD/DELETE без тела по умолчанию
```

`Other(str)` хранит уже-провалидированный токен (нельзя сконструировать минуя `parse` — поле обёрнуто фабрикой). Сравнение case-sensitive (метод регистрозависим, RFC 7230 §3.1.1). Бьёт Go (`string`, опечатка компилится). `@is_idempotent` — load-bearing для retry-policy (Q16).

### 3.2. `StatusCode` — u16 newtype + классы + reason

```nova
/// HTTP status code (100..599). value-newtype (D215) над u16.
#stable(since = "0.1")
export type StatusCode value { priv code u16 }

export fn StatusCode.new(code u16) -> Result[StatusCode, HttpError]   // вне 100..599 → Protocol
export fn StatusCode @as_u16(self) -> u16
export fn StatusCode @class(self) -> StatusClass                       // 1xx..5xx
export fn StatusCode @is_informational(self) -> bool                   // 100..199
export fn StatusCode @is_success(self)       -> bool                   // 200..299
export fn StatusCode @is_redirect(self)      -> bool                   // 300..399
export fn StatusCode @is_client_error(self)  -> bool                   // 400..499
export fn StatusCode @is_server_error(self)  -> bool                   // 500..599
export fn StatusCode @reason(self) -> str    // канон. reason-phrase ("Not Found"); неизвестный → ""

export type StatusClass | Informational | Success | Redirection | ClientError | ServerError

// именованные zero-arg фабрики (читаемость; НЕ top-level let — value-конструкторы, §3.0 Q17):
export fn StatusCode.ok()             -> StatusCode   // 200
export fn StatusCode.not_found()      -> StatusCode   // 404
export fn StatusCode.internal_error() -> StatusCode   // 500   ... (полный RFC-набор)
```

**Ключевое решение:** `4xx`/`5xx` — **валидный `HttpResponse`, НЕ ошибка** (reqwest/fetch). Конверсия — opt-in `response.error_for_status() -> Result[HttpResponse, HttpError]`: `Err(HttpError{kind: Status(code)})` для не-2xx, иначе `Ok(self)`. `@reason()` — для логов; на проводе reason-phrase игнорируется при разборе (RFC 7230 §3.1.2).

### 3.3. `Version`

```nova
export type Version | Http10 | Http11 | Http2     // Http3 — followup §11
export fn Version @as_str(self) -> str            // "HTTP/1.0" | "HTTP/1.1" | "HTTP/2"
export fn Version @supports_keepalive(self) -> bool   // h10: только при Connection: keep-alive
```

Версия **наблюдается** на `Request`/`Response`, но выбирается транспортом: h1 vs h2 — результат ALPN (gate 116) или явного `http2_prior_knowledge()` (h2c). HTTP/0.9 не поддерживается (намеренно — устарел, source of smuggling).

### 3.4. `HeaderName` / `HeaderValue` / `HeaderMap`

Заголовки — **case-insensitive по имени, ordered, multi-value** (RFC 7230 §3.2). Имя нормализуется (lowercase) для сравнения, сохраняет порядок вставки при сериализации. **§3.0 Q18 (str↔[]u8 контракт):** имя — ASCII-only валидированный tchar; значение хранится `[]u8` (octet-корректно, obs-text), `str`-API — **latin1 fast-path**, `@to_str()` **fallible на non-ASCII**.

```nova
/// Валидированное имя заголовка (RFC 7230 tchar, ASCII-only). Сравнение case-insensitive.
export type HeaderName value { priv lower str }      // канон. lowercase-форма
export fn HeaderName.parse(s str) -> Result[HeaderName, HttpError]   // не-tchar / non-ASCII → Protocol
export fn HeaderName @as_str(self) -> str

/// Значение заголовка. Байты (visible ASCII + obs-text); запрет CR/LF/NUL.
export type HeaderValue value { priv bytes []u8 }
export fn HeaderValue.from_bytes(b []u8) -> Result[HeaderValue, HttpError]   // CR/LF/NUL → Protocol (anti-injection)
export fn HeaderValue.from_str(s str)  -> Result[HeaderValue, HttpError]     // latin1 fast-path
export fn HeaderValue @as_bytes(self) -> []u8
export fn HeaderValue @to_str(self)   -> Result[str, HttpError]              // non-ASCII/obs-text → Protocol (fallible)

/// Упорядоченная мультимапа заголовков, case-insensitive по имени.
#stable(since = "0.1")
export type HeaderMap value { priv entries []HeaderEntry }   // insertion-order сохранён
priv type HeaderEntry value { ro name HeaderName, ro value HeaderValue }

export fn HeaderMap.new() -> HeaderMap
export fn HeaderMap @get(self, name str)      -> Option[HeaderValue]   // ПЕРВОЕ значение (case-insens)
export fn HeaderMap @get_all(self, name str)  -> []HeaderValue         // ВСЕ значения, в порядке
export fn HeaderMap @contains(self, name str) -> bool
export fn HeaderMap mut @insert(self, name str, value str) -> Result[(), HttpError]  // ЗАМЕНЯЕТ все прежние
export fn HeaderMap mut @append(self, name str, value str) -> Result[(), HttpError]  // ДОБАВЛЯЕТ ещё одно
export fn HeaderMap mut @remove(self, name str) -> []HeaderValue        // удаляет все, возвращает удалённые
export fn HeaderMap @len(self) -> int
export fn HeaderMap @iter(self) -> HeaderIter                            // (HeaderName, HeaderValue) в порядке

// Типизированные хелперы (парсят/валидируют общие заголовки):
export fn HeaderMap @content_type(self)      -> Option[ContentType]      // парсит Content-Type → Mime+params
export fn HeaderMap @content_length(self)    -> Result[Option[int], HttpError]   // дублирующиеся/конфликтные → Protocol
export fn HeaderMap @transfer_encoding(self) -> []str                    // ["chunked"] и т.п.
export fn HeaderMap @trailer(self)           -> []str                    // объявленные trailer-имена (RFC 7230 §4.4)
export fn HeaderMap @host(self)              -> Option[str]
export fn HeaderMap mut @set_content_type(self, ct ContentType) -> ()
```

**Безопасность (differentiator).** `insert`/`append`/`HeaderValue.from_*` **отвергают CR/LF/NUL** в значениях и не-tchar/non-ASCII в именах → **request/response-splitting невозможен by construction**. `content_length` явно ловит **конфликт `Transfer-Encoding: chunked` + `Content-Length`** (request-smuggling, RFC 7230 §3.3.3) на уровне типа-хелпера.

### 3.5. `Body` — MUST-CONSUME (фикс Go body-leak)

Самый крупный differentiator модуля. `Body` — **линейный must-consume** ресурс (D133): единственный способ его «разрядить» — потребляющий метод. Незакрытое тело = **compile-error**, не утечка соединения в рантайме.

```nova
/// Тело запроса/ответа: либо буфер в памяти, либо потоковый ридер чанков.
/// MUST-CONSUME (D133): закрывается ТОЛЬКО потребляющим методом.
#stable(since = "0.1")
export type Body consume value { priv repr BodyRepr }
priv type BodyRepr
    | InMemory([]u8)                          // тело целиком в памяти
    | Stream(BodyReader)                       // ленивый источник чанков (h1 chunked / h2 DATA)

/// Источник потокового тела: yield-ит []u8-чанки до EOF. Чистая Nova-логика над
/// транспортом (chunked/CL/h2-DATA-декодер композится над TcpReadHalf/TlsStream/h2-stream).
/// Сам must-consume.
export type BodyReader consume value { priv repr ReaderRepr }   // НЕ C-handle — Nova-декодер над byte-source (§3.0 Q19)

// ── Конструкторы (Request-side) ──
export fn Body.empty()              -> Body
export fn Body.from_bytes(b []u8)   -> Body                         // InMemory (replayable на 307/308)
export fn Body.from_str(s str)      -> Body                         // UTF-8 байты
export fn Body.from_reader(r consume BodyReader) -> Body            // стрим (consume reader; non-replayable)

// ── Потребляющие методы (Response-side; разряжают must-consume) ──
export fn Body consume @bytes(self) Http -> Result[[]u8, HttpError]   // дочитать всё → []u8 (с max-guard)
export fn Body consume @text(self)  Http -> Result[str, HttpError]    // bytes + charset-decode (см. ниже)
export fn Body consume @json[T](self) Http -> Result[T, HttpError]    // bytes + decode (§ encoding/json — gate Q20)
export fn Body consume @discard(self) Http -> Result[(), HttpError]   // дренаж+release БЕЗ материализации
export fn Body consume @into_reader(self) -> BodyReader               // взять стрим напрямую (ручной pull)
export fn Body consume @copy_to(self, w mut impl io.Write) Http -> Result[int, HttpError]  // stream→writer (download-to-file)
export fn Body consume @trailers(self) Http -> Result[HeaderMap, HttpError]   // chunked/h2 trailer-блок ПОСЛЕ дренажа

// ── Стримовое чтение (на BodyReader; для прокси/больших тел) ──
export fn BodyReader mut @next_chunk(self) Http -> Result[Option[[]u8], HttpError]   // None = EOF
export fn BodyReader consume @close(self) Http -> Result[(), HttpError]               // явный release

// ── Max-size guard (защита от DoS / OOM) ──
export fn Body @with_limit(self, max_bytes int) -> Body   // bytes()/text()/json() → BodyTooLarge при превышении
```

- **Force-close:** компилятор требует потребить `Body` (D133) → соединение всегда возвращается в пул либо корректно закрывается. `consume @discard()` — дешёвый «мне не нужно тело».
- **Стрим = `[]u8`-чанки**, каждый через `Http`-seam (park над транспортом, cancel-safe в supervised-scope 173). `bytes/text/json/copy_to` — convenience поверх дренажа.
- **`@text()` charset (закрыто §3.0 Q21, НЕ followup):** charset из `Content-Type`; **UTF-8 и ISO-8859-1/latin1 декодируются всегда** (latin1 = тривиальный byte→codepoint, корректно для legacy-тел); прочие charset'ы (Shift_JIS/GBK/…) → `HttpError{Protocol("unsupported charset")}` с явным советом взять `.bytes()` и декодировать вручную. Это **scoped-out с rationale**, не «решим потом»; round-trip latin1-тело покрыт pos-тестом.
- **Max-guard обязателен** на `bytes/text/json`: без `with_limit` — дефолтный клиентский лимит **2 MiB** (config); превышение = `BodyTooLarge`. Серверный request-body лимит — отдельный (HttpServer config, default `None`/handler-set). Stream-методы (`next_chunk`/`copy_to`) — unbounded (caller сам решает).
- **Trailers (RFC 7230 §4.4):** `@trailers()` после полного дренажа отдаёт trailing-header-блок (gRPC-Web `grpc-status` едет в trailers; h2 — HEADERS-frame с END_STREAM). Объявленные имена — `HeaderMap.@trailer()`.

### 3.6. `Mime` / `ContentType`

```nova
export type Mime value { priv type_ str, priv subtype str }   // RFC 6838, нормализован lowercase
export fn Mime.parse(s str) -> Result[Mime, HttpError]
export fn Mime @essence(self) -> str           // "text/html"
export fn Mime @type_(self) -> str             // "text"
export fn Mime @subtype(self) -> str           // "html"
export fn Mime.text_plain()        -> Mime
export fn Mime.application_json()   -> Mime
export fn Mime.application_octet()  -> Mime    // ... частые фабрики

export type ContentType value { ro mime Mime, priv params []Param }
priv type Param value { ro key str, ro value str }
export fn ContentType.parse(s str) -> Result[ContentType, HttpError]   // "text/html; charset=utf-8"
export fn ContentType @charset(self)  -> Option[str]
export fn ContentType @boundary(self) -> Option[str]                    // для multipart
export fn ContentType @to_header(self) -> str
```

### 3.7. `Cookie` / `SetCookie`

```nova
/// Cookie из заголовка запроса (Cookie: k=v; k2=v2 — RFC 6265 §5.4).
export type Cookie value { ro name str, ro value str }
export fn Cookie.parse_header(s str) -> Result[[]Cookie, HttpError]
export fn Cookie @to_header_pair(self) -> str                          // "name=value"

/// Set-Cookie из ответа (полные атрибуты, RFC 6265 §4.1).
export type SetCookie value {
    ro name      str
    ro value     str
    ro domain    Option[str]
    ro path      Option[str]
    ro expires   Option[Timestamp]      // 179 Timestamp (не str-дата)
    ro max_age   Option[int]            // секунды
    ro secure    bool
    ro http_only bool
    ro same_site SameSite               // Strict | Lax | None
}
export type SameSite | Strict | Lax | None
export fn SetCookie.parse(s str)   -> Result[SetCookie, HttpError]
export fn SetCookie @serialize(self) -> str
export fn SetCookie @to_cookie(self) -> Cookie
```

`expires` — типизированный `Timestamp` (Plan 179), не строка-дата (бьёт Go/Node). **Send-side инварианты (RFC 6265bis, закрыто §3.0 Q10 / D330):** `Secure`-cookie **НЕ** отправляется по plaintext `http://`; `__Secure-`/`__Host-`-префиксы enforce'ятся (Host: без Domain + Path=/ + Secure); `SameSite=None` требует `Secure`; domain-match через public-suffix-list (минимум — reject `domain`=TLD, supercookie-защита). `CookieJar` (хранилище клиента) — §3.client.

### 3.8. `Url` — промоут из `_experimental`

`Url` промоутится `std/_experimental/encoding/url.nv` → **`std/http/url.nv`** (stable, `module std.http`):

```nova
export type Url value {
    ro scheme str, ro user Option[str], ro password Option[str],
    ro host Option[str], ro port Option[int], ro path str,
    ro query Option[str], ro fragment Option[str]
}
export fn Url.parse(s str) -> Result[Url, HttpError]    // R2-rename: from(Fail) → parse(Result), D325
export fn Url @to_str(self) -> str
export fn Url.encode_query(s str) -> str
export fn Url.decode_query(s str) -> Result[str, HttpError]
export fn Url @origin(self) -> (str, str, int)          // (scheme, host, port) — для cross-origin auth-strip / pool-key
export fn Url @is_private_target(self) -> bool          // loopback/link-local/RFC1918/metadata — SSRF-guard hook
```

**Задачи промоута (Ф.0.5) — все ОБЯЗАТЕЛЬНЫ к закрытию (§8 acceptance, не «доводка»):**
1. **Чинить bootstrap-баг `decode_query`:** tuple-destructure type-inference (`let (a,b)=...` инферится как `nova_int`, ломая `b.starts_with(...)` strict-bool, [url.nv:343-352]). **Re-check на текущем main** (мог быть починен Plan 06/136.1); если воспроизводится — чинить инференс (предпочтительно) либо переписать без проблемного destructure. **HARD-prereq фазы** (файл idle пока баг жив).
2. **R2/D325 нейминг:** `Url.from(s) Fail[ParseUrlError]` → `Url.parse(s) -> Result[Url, ...]`; `into()` → `@to_str()`. `ParseUrlError` маппится в `HttpError{kind: InvalidUrl}` (source-chaining).
3. **Прод-корректность host/encode (SSRF + multi-byte):** (a) **IPv6-host в скобках** (`[::1]`); (b) **строгий host-валидатор** — reject control/NUL/whitespace, canonical IP-literal (reject decimal/octal/hex-обфускацию `0x7f.1`/`017700000001`), reject userinfo-confusion-атаки на cross-origin; (c) **фикс `encode_query` multi-byte UTF-8** — текущий код ([url.nv:320-324]) для >127 эмитит ОДИН байт (corrupts multi-byte) → percent-encode **каждого UTF-8-байта**. Эти три — **§8 Ф.0.5 acceptance**, иначе «stable» Url молча неверен.

### 3.9. `HttpError` — единый структурный + OPEN ErrorKind

```nova
/// Единственная структурная ошибка домена http (R5/D325). Source-chaining
/// NetError / TlsError(116) / ParseUrlError / Utf8Error.
#stable(since = "0.1")
export type HttpError value { ro kind ErrorKind, ro url Option[Url], ro source Option[ErrSource] }
export type ErrorKind
    | Connect | Dns | Tls | Timeout                  // транспорт
    | Protocol(str)                                  // парсинг/framing/HPACK/smuggling/charset-деталь
    | InvalidUrl | InvalidHeader
    | Status(StatusCode)                             // от error_for_status (opt-in, Q4)
    | TooManyRedirects(int) | BodyTooLarge
    | Closed | Canceled                              // 173 cancel → Canceled
    | Blocked(str)                                   // SSRF-guard deny (private-range target)
    | Other(str)                                     // OPEN → wildcard-arm обязателен
priv type ErrSource | Net(NetError) | Url(ParseUrlError) | Utf8(Utf8Error)   // + Tls(TlsError) когда 116 готов
export fn HttpError @to_str(self) -> str
```

`Timeout` (deadline/timeout 173) и `Canceled` (cancel propagation 173) — first-class. `Status` — **только** через явный `error_for_status()`. `Io(NetError)` НЕ дублируется в `kind` — транспортная ошибка несётся как `kind: Connect/Closed/Timeout` + `source: Net(NetError)` (унификация с другими секциями, §3.0 Q2).

### 3.10. ⚠ Требуемая правка net: `[]u8` read/write surface

Сейчас `TcpStream.@read(max) -> Result[str, NetError]` / `@write(data str)` ([tcp.nv:146/155]), `TcpReadHalf.@read`/`TcpWriteHalf.@write`/`@write_all` — все **`str`-payload** ([tcp.nv:236/277/283]) — нарушает byte-first. **Необходима координированная правка net** (Q5, объём как Plan 180 Q6). **ПОЛНАЯ byte-surface-дельта** (закреплена в D327; критик-gap «under-scoped»):

```nova
export fn TcpStream    mut @read_bytes(self, max int) TcpNet -> Result[[]u8, NetError]
export fn TcpStream    mut @write_bytes(self, data []u8) TcpNet -> Result[int, NetError]
export fn TcpStream    mut @write_all_bytes(self, data []u8) TcpNet -> Result[(), NetError]
export fn TcpReadHalf  mut @read_bytes(self, max int) TcpNet -> Result[[]u8, NetError]
export fn TcpWriteHalf mut @write_bytes(self, data []u8) TcpNet -> Result[int, NetError]
export fn TcpWriteHalf mut @write_all_bytes(self, data []u8) TcpNet -> Result[(), NetError]
```

C-backing уже байтовый (`tcp_stream_read_bytes`/`tcp_stream_write` берут длину) — правка на Nova-сигнатурах + erasure. **Plan 116 `Tls` уже `[]u8`** (116:104-106), так что HTTP-кодек над Tls единообразен; только TcpNet отстаёт. **`str`-варианты сохраняются**; полная миграция net `str`→`[]u8` — **отдельный byte-baseline-guarded коммит ПОСЛЕ HTTP** (не блокирует 182), per-file loop для callers. Без byte-surface HTTP-парсер гонит всё через `str` (lossy/паника на бинарных телах) — **неприемлемо (§8.0)**.

### 3.11. `mock_http` для тестов (обязателен, convention triad)

`Http`-seam имеет `real_http()` (над TcpNet+Tls+DnsNet+Time) и **`mock_http()`** — in-memory: запрос → запрограммированный `HttpResponse` без сокета. Делает client-тесты детерминированными. Симметрично `mock_http_server()`. **Единая mock-форма для client/server** (§3.0 Q22): билдер `.on(method, path, |req| -> MockResponse)`. Mock-handler-тест **MANDATORY** (test-conventions, effect-модуль).

### 3.client. Client (Go `net/http` arch + reqwest builder)

**Принцип.** Клиент — синхронный API над фибрами (req паркует фибру через `Http`-seam → `TcpNet`/`Tls`/`DnsNet`). Архитектура = Go `Client`/`Transport`/`RoundTripper`; ergonomics = reqwest; типы фиксят Go-footguns (must-consume `Body`, deadline-by-default, typed `HttpError`). HTTP/1.1-сериализация, парсинг, chunked, redirect-loop, decompress, retry, CONNECT-tunnel — **всё Nova-logic в `real_http()`-слое** над байтовым транспортом.

#### HttpClient + builder-конфиг

```nova
// std/http/client.nv
export type HttpClient value { priv inner *HttpClientInner }   // shared-pooled rc; CLONE дёшев (reqwest::Client)
export type HttpClientBuilder value { /* priv поля */ }

export fn HttpClient.builder() -> HttpClientBuilder
export fn HttpClient.new() -> HttpClient => HttpClient.builder().build().unwrap()  // прод-дефолты

export fn HttpClientBuilder @timeout(d Duration) -> HttpClientBuilder              // per-request wall-clock (173); дефолт 30s
export fn HttpClientBuilder @connect_timeout(d Duration) -> HttpClientBuilder      // TCP+TLS connect; дефолт 10s
export fn HttpClientBuilder @deadline(at Instant) -> HttpClientBuilder             // абсолютный (173) — приоритетнее timeout
export fn HttpClientBuilder @default_header(name str, value str) -> HttpClientBuilder
export fn HttpClientBuilder @default_headers(h HeaderMap) -> HttpClientBuilder
export fn HttpClientBuilder @redirect(policy RedirectPolicy) -> HttpClientBuilder  // дефолт .limited(10)
export fn HttpClientBuilder @cookie_store(on bool) -> HttpClientBuilder            // дефолт false (opt-in jar, Q10)
export fn HttpClientBuilder @cookie_jar(jar CookieJar) -> HttpClientBuilder        // явный shared jar
export fn HttpClientBuilder @proxy(p Proxy) -> HttpClientBuilder                   // http/https/socks5 + CONNECT (Q23)
export fn HttpClientBuilder @no_proxy() -> HttpClientBuilder                       // игнор env HTTP(S)_PROXY/NO_PROXY
export fn HttpClientBuilder @tls_config(cfg ClientConfig) -> HttpClientBuilder     // Plan 116 (SNI/ALPN/roots) — gate Ф.4
export fn HttpClientBuilder @danger_accept_invalid_certs(on bool) -> HttpClientBuilder  // → InsecureSkipVerify; имя кричит
export fn HttpClientBuilder @ssrf_guard(deny_private bool) -> HttpClientBuilder    // блок loopback/link-local/RFC1918/metadata (Q24)
export fn HttpClientBuilder @gzip(on bool)   -> HttpClientBuilder                  // авто-decompress; дефолт true
export fn HttpClientBuilder @brotli(on bool) -> HttpClientBuilder                  // дефолт true
export fn HttpClientBuilder @deflate(on bool) -> HttpClientBuilder                 // дефолт true
export fn HttpClientBuilder @no_decompress() -> HttpClientBuilder                  // сырой Content-Encoding наружу
export fn HttpClientBuilder @max_decompressed(n int) -> HttpClientBuilder          // bomb-cap; дефолт 100 MiB (Q12)
export fn HttpClientBuilder @pool_max_idle_per_host(n int) -> HttpClientBuilder    // дефолт 32
export fn HttpClientBuilder @pool_idle_timeout(d Duration) -> HttpClientBuilder    // дефолт 90s (Go DefaultTransport)
export fn HttpClientBuilder @http1_only() -> HttpClientBuilder                     // запретить ALPN h2
export fn HttpClientBuilder @http2_prior_knowledge() -> HttpClientBuilder          // h2c без ALPN (gate h2)
export fn HttpClientBuilder @user_agent(ua str) -> HttpClientBuilder               // дефолт "nova-http/0.x"
export fn HttpClientBuilder consume @build() -> Result[HttpClient, HttpError]      // fallible: tls roots / proxy-конфиг

export type RedirectPolicy | None | Limited(int) | Custom(fn(Url, []Url) -> RedirectAction)
export type RedirectAction | Follow | Stop | Error
```

**Differentiator vs Go.** Go `&http.Client{}` = **нет таймаута** (висящий запрос держит conn вечно). `HttpClient.new()` несёт **30s `timeout` + 10s `connect_timeout`** (173); снять — явный `@timeout(Duration.MAX)`. `send()` обёрнут в `supervised` с `deadline:` → cancel пропагирует в park `TcpNet`/`Tls`.

#### Proxy (Q23 — РЕАЛЬНАЯ Ф.2-задача, не followup)

```nova
export type Proxy value { ro scheme ProxyScheme, ro url Url, ro auth Option[(str, str)] }
export type ProxyScheme | Http | Https | Socks5
export fn Proxy.from_env() -> Result[Option[Proxy], HttpError]   // HTTP_PROXY/HTTPS_PROXY/NO_PROXY precedence (Go-семантика)
export fn Proxy.http(url str)   -> Result[Proxy, HttpError]
export fn Proxy.socks5(url str) -> Result[Proxy, HttpError]
export fn Proxy @bypass(self, target Url) -> bool                 // NO_PROXY-match (suffix/CIDR)
```

`@proxy()` — **полноценный механизм Ф.2** (критик «contradiction»-gap): для `https://`-через-proxy — **HTTP `CONNECT`-tunnel** (`CONNECT host:port` → 200 → TLS поверх туннеля), для `http://`-через-http-proxy — absolute-form request-target, для SOCKS5 — handshake. `Proxy-Authorization: Basic` при `auth`. CONNECT-tunnel приземляется в Ф.2 (plaintext-proxy) + Ф.4 (TLS-over-CONNECT). **Не followup.**

#### RequestBuilder (reqwest)

```nova
// VERB-методы: url — str ИЛИ Url (IntoUrl). Парс-ошибка url — ЛЕНИВАЯ (всплывает на send() как InvalidUrl),
// чтобы цепочка builder'а оставалась non-fallible (reqwest-эргономика; закрыто §3.0 Q25 как осознанное исключение R1).
export fn HttpClient @get(url IntoUrl) -> RequestBuilder
export fn HttpClient @post(url IntoUrl) -> RequestBuilder
export fn HttpClient @put(url IntoUrl) -> RequestBuilder
export fn HttpClient @delete(url IntoUrl) -> RequestBuilder
export fn HttpClient @head(url IntoUrl) -> RequestBuilder
export fn HttpClient @patch(url IntoUrl) -> RequestBuilder
export fn HttpClient @request(method Method, url IntoUrl) -> RequestBuilder        // произвольный/extension-метод
export fn HttpClient @execute(req consume Request) Http -> Result[HttpResponse, HttpError]  // повторная отправка built-req

export type RequestBuilder value { /* priv: client-ref, parts, body, per-req overrides, lazy url-err */ }

export fn RequestBuilder @header(name str, value str) -> RequestBuilder            // append (multi-value)
export fn RequestBuilder @headers(h HeaderMap) -> RequestBuilder                   // merge
export fn RequestBuilder @query(params []( str, str )) -> RequestBuilder           // percent-encode + append к url.query
export fn RequestBuilder @body(bytes []u8) -> RequestBuilder                       // BYTE-FIRST; replayable (CL)
export fn RequestBuilder @text(s str) -> RequestBuilder                            // CT text/plain; charset=utf-8
export fn RequestBuilder @json[T](value T) -> RequestBuilder                       // (де)сериализация (gate Q20) + CT json
export fn RequestBuilder @form(fields []( str, str )) -> RequestBuilder            // application/x-www-form-urlencoded
export fn RequestBuilder @multipart(form Multipart) -> RequestBuilder              // multipart/form-data (File-part gate Plan 180)
export fn RequestBuilder @body_stream(reader consume BodyReader) -> RequestBuilder // chunked TE; non-replayable
export fn RequestBuilder @bearer_auth(token str) -> RequestBuilder                 // Authorization: Bearer …
export fn RequestBuilder @basic_auth(user str, pass Option[str]) -> RequestBuilder // base64(user:pass)
export fn RequestBuilder @timeout(d Duration) -> RequestBuilder                    // per-request override
export fn RequestBuilder @deadline(at Instant) -> RequestBuilder
export fn RequestBuilder @version(v Version) -> RequestBuilder                     // форсировать Http11/Http2 (иначе ALPN)
export fn RequestBuilder consume @send() Http -> Result[HttpResponse, HttpError]   // RoundTrip; consume builder (one-shot)
export fn RequestBuilder @build() -> Result[Request, HttpError]                    // материализовать без отправки

export type IntoUrl protocol { @into_url(self) -> Result[Url, HttpError] }         // impl для str и Url
```

#### HttpResponse — MUST-CONSUME body

```nova
// std/http/response.nv
export type HttpResponse consume value { priv inner *RespInner, priv body Body }  // CONSUME: незакрытое тело = compile-error

export fn HttpResponse @status() -> StatusCode                                    // 2xx/3xx/4xx/5xx — ВСЕ валидны (НЕ error)
export fn HttpResponse @headers() -> HeaderMap                                    // case-insensitive, ordered, multi-value
export fn HttpResponse @version() -> Version                                      // Http11 | Http2 (по ALPN)
export fn HttpResponse @header(name str) -> Option[str]                           // первое значение
export fn HttpResponse @content_length() -> Option[int]
export fn HttpResponse @final_url() -> Url                                        // после редиректов
export fn HttpResponse @cookies() -> []SetCookie                                  // Set-Cookie распарсенные

// Терминальные body-consumers (разряжают consume; авто-дренаж+release conn в пул на keep-alive):
export fn HttpResponse consume @bytes() Http -> Result[[]u8, HttpError]           // полное тело (декомпрессия применена)
export fn HttpResponse consume @text() Http -> Result[str, HttpError]             // bytes → charset-decode (UTF-8/latin1)
export fn HttpResponse consume @json[T]() Http -> Result[T, HttpError]            // bytes → десериализация (gate Q20)
export fn HttpResponse consume @body() -> Body                                    // забрать стрим-тело (must-consume) вручную
export fn HttpResponse consume @copy_to(w mut impl io.Write) Http -> Result[int, HttpError]  // download-to-file (Java/Swift-паритет)
export fn HttpResponse consume @drain() Http -> Result[(), HttpError]             // прочитать-и-выкинуть → conn в пул
// error_for_status — reqwest opt-in. РАБОТАЕТ ДО взятия Body: при Err Body НЕ потреблён,
// caller ОБЯЗАН разрядить (must-consume сохраняется). Возвращает self при 2xx/3xx (§3.0 Q4).
export fn HttpResponse consume @error_for_status() -> Result[HttpResponse, HttpError]
```

**🏆 Differentiator.** Забыл `.bytes()`/`.text()`/`.json()`/`.drain()` = **compile-error** (D133), double-read = use-after-consume compile-error (D131). Конвенс-методы дренируют тело и **возвращают conn в пул**. `error_for_status` — non-consuming: на `Err(Status)` Body всё ещё жив и обязан быть разряжен caller'ом.

#### Свободные convenience-функции

```nova
// std/http/lib.nv — глобальный shared-клиент (lazy Once). СКРИПТЫ/one-shot ТОЛЬКО.
// §3.0 Q15: prod-код использует явный HttpClient (no hidden global state — критика requests.Session);
// http.get() — осознанное удобство для скриптов, документировано как «не для prod».
export fn http.get(url IntoUrl) Http -> Result[HttpResponse, HttpError]
export fn http.post(url IntoUrl, body []u8) Http -> Result[HttpResponse, HttpError]
export fn http.head(url IntoUrl) Http -> Result[HttpResponse, HttpError]
```

#### Транспорт-семантика (всё внутри `real_http()`, Nova-logic над байтами)

- **Connection pool + keep-alive.** Per-`(scheme, host, port, alpn)` пул idle-conn (`pool_max_idle_per_host=32`, `pool_idle_timeout=90s`). **Pool-ключ включает ALPN-результат** (h1-conn НЕ переиспользуется как h2, §3.0 Q26). Body-дренаж/`@drain()` возвращает conn в пул (если keep-alive и тело полностью прочитано); иначе закрывается.
- **Pool-eviction-on-error (D330, критик-gap).** Соединение, ошибшееся mid-response (read/write/parse error) — **EVICT'ится, не возвращается** в пул. Только happy-path `drain→return` возвращает.
- **Idempotent-retry (D330, Q16 — критик-gap).** При сбое **reused-from-pool** conn (stale: сервер закрыл idle) ДО получения первого байта ответа — **авто-retry на свежем conn, max 1, ТОЛЬКО для idempotent-методов** (`Method.@is_idempotent`). **Свежие conn и не-idempotent (POST) — НИКОГДА не реплеятся.** pos-тест (reused-dead-conn retry) + neg-тест (POST не реплеится).
- **Redirect following.** `RedirectPolicy.Limited(10)` дефолт. **303 → GET-ify** (метод→GET, тело сброшено); **301/302 на не-GET/HEAD → GET-ify** (browser-совместимость); **307/308 сохраняют метод+тело** (тело replayable: in-memory; `@body_stream` non-replayable → `HttpError.Protocol("non-replayable body on 307/308")`). **Cross-origin hop → strip `Authorization`/`Cookie`** (anti-leak security-инвариант, Q9). `TooManyRedirects` при превышении.
- **Auto-decompress (Q12, кодек-gate — критик/simplification-gap).** **Дефолтное поведение Ф.2 = identity + chunked** (всегда работает). **gzip/deflate/br — 🔴 HARD-GATE на NEW под-план `std/encoding/compress`** (НЕ существует; owner 2026-06-26 — отдельный под-план RFC 1950/1951/1952 + brotli, создаётся ВНЕ 182). Если кодеки не приземлены к Ф.2-close — `@gzip/@brotli/@deflate` остаются объявлены, но **acceptance §8.4 для decompress помечен gated**; identity-путь — самодостаточный landed-deliverable. Bomb-cap `max_decompressed=100 MiB` → `BodyTooLarge`. Кодеки — Nova-logic над C-zlib FFI (nv-sourcing).
- **Chunked transfer + trailers.** Ответ: `Transfer-Encoding: chunked` декодируется стримово; trailing-header-блок → `Body.@trailers()`. Запрос: `@body_stream` без длины → chunked; `@body([]u8)`/`@json`/`@form` → `Content-Length`.
- **Expect: 100-continue.** Для request-body выше порога (1 KiB) или явного стрима — `Expect: 100-continue`, ждать `100 Continue` (таймаут 1s); на `417`/таймаут — лить тело сразу (Go-совместимо).
- **Cookie jar (RFC 6265bis, Q10).** Opt-in (`@cookie_store(true)`). Парс/хранение per-(domain,path), отправка matching `Cookie:` (send-инварианты §3.7: Secure→https-only, PSL-match, `__Host-`/`__Secure-`). `CookieJar` — shared value, внутренний `Mutex` (Plan 103.3) для конкурентной записи из фибр.
- **Cancel/deadline.** `send()` под `deadline:`/`timeout:` (173); срабатывание → `Err(HttpError.Timeout/Canceled)`, cancel доходит до park'нутого `TcpNet.read`/`Tls`-handshake. Соединение при отмене закрывается (не в пул).
- **SSRF-guard (Q24).** При `@ssrf_guard(true)`: после `DnsNet.lookup` — резолвленный адрес проверяется против private-range deny-list (loopback/link-local/RFC1918/`169.254.169.254`-metadata) **до** connect → `Err(HttpError.Blocked)`. Default OFF (не ломает internal-service-вызовы), но доступен из коробки — differentiator.
- **HTTPS/h2 (gate Plan 116).** version-transparent; ALPN (`["h2","http/1.1"]`) авто-выбирает; `https://` → `Tls`-handshake (SNI=host). h2: каждый стрим = фибра (детально §3.https-h2).

#### End-to-end примеры

```nova
// (1) GET с дедлайном, JSON, must-consume body разряжается через .json().
fn fetch_user(id int) Http -> Result[User, HttpError] {
    ro client = HttpClient.builder()
        .timeout(Duration.seconds(5))
        .default_header("Accept", "application/json")
        .build()?
    ro resp = client
        .get("https://api.example.com/users/${id}")
        .query([("fields", "name,email")])
        .bearer_auth(env_token())
        .send()?                       // паркует фибру; редиректы/decompress/keep-alive/retry — прозрачно
        .error_for_status()?           // 4xx/5xx → Err(Status); при Err Body НЕ потреблён, .json() ниже не достигается → ?-bubble
    resp.json[User]()                  // consume: дренит тело, возвращает conn в пул
}

fn main() {
    with Http = real_http() {
        supervised deadline: Instant.now() + Duration.seconds(10) {
            match fetch_user(42) {
                Ok(u)  => println("user: ${u.name}")
                Err(e) => println("http error: ${e.to_str()}")
            }
        }
    }
}

// (2) тест клиента через mock_http() — детерминированно, без сети.
test "upload posts json and reads 201" {
    with Http = mock_http()
        .on("POST", "/v1/reports", |req| {
            assert(req.header("Content-Type") == Some("application/json"))
            MockResponse.new(StatusCode.created()).header("Location", "/v1/reports/7")
        }) {
        ro client = HttpClient.new()
        consume resp = client.post("https://ingest.example.com/v1/reports")
            .json(sample_report()).send().unwrap()
        assert(resp.header("Location") == Some("/v1/reports/7"))
        resp.drain().unwrap()          // consume-разрядка
    }
}
```

### 3.server. Server (Go `net/http`: `Handler`/`ServeMux` поверх фиберов)

**Принцип (net-precedent).** Сервер — синхронный API над фиберами: `serve` = `supervised`-scope (Plan 173.1, value-expr), каждое соединение = `spawn`-фибра, каждый park — libuv. Это Go (`Server.Serve` → `go c.serve()`), но Nova даёт на уровне типов: **must-consume `Body`**, **дедлайн-по-умолчанию**, **graceful drain как примитив**, **типизированный `HttpError`-source-chain**. Symmetry: те же `Request`/`Response`/`HeaderMap`/`Body`/`StatusCode`/`Url`. HTTP/1.1-парсинг — **Nova-логика (`.nv`)**; seam `HttpServer` тонкий.

#### Layering

```
serve loop (supervised scope, Plan 173.1)
  └─ accept-фибра ── Semaphore(max_conns, Plan 103.4) ── spawn per-conn фибра
       └─ conn driver: keep-alive loop над TcpStream/TlsStream ([]u8 transport)
            ├─ HTTP/1.1 wire parse (.nv): request-line + headers + body framing (CL/chunked)
            ├─ build Request (streaming Body = must-consume reader над conn)
            ├─ Handler chain: Middleware∘…∘ServeMux.route → Handler.handle(req) -> Response
            └─ serialize Response → wire (auto CL vs chunked) → flush
```

`HttpServer`-seam = `real_http_server()` (над `TcpNet`+`Tls`+`Time`) / `mock_http_server()` (in-memory loopback, тест хендлеров без сокета).

#### `HttpServer` — bind / serve

```nova
export type HttpServer value {
    ro addr           SocketAddr
    ro max_conns      int            // bounded concurrency (Semaphore); default 1024
    ro read_timeout   Duration       // header+body read deadline; default 30.sec
    ro header_timeout Duration       // SOLO header-read deadline (slowloris); default 10.sec
    ro idle_timeout   Duration       // keep-alive простой; default 60.sec
    ro write_timeout  Duration       // response-write deadline; default 30.sec
    ro max_header_bytes int          // суммарный header-блок; default 64*1024
    ro max_body_bytes Option[int]    // дефолтный лимит тела (None = handler-set)
    ro tls            Option[ServerConfig]   // None = plaintext h1; Some = HTTPS (gate 116)
}

export fn HttpServer.bind(addr SocketAddr) -> HttpServer
    => { addr, max_conns: 1024, read_timeout: 30.sec, header_timeout: 10.sec,
         idle_timeout: 60.sec, write_timeout: 30.sec, max_header_bytes: 64*1024,
         max_body_bytes: None, tls: None }

export fn HttpServer @max_conns(n int)        -> HttpServer
export fn HttpServer @read_timeout(d Duration) -> HttpServer
export fn HttpServer @header_timeout(d Duration) -> HttpServer
export fn HttpServer @idle_timeout(d Duration) -> HttpServer
export fn HttpServer @write_timeout(d Duration) -> HttpServer
export fn HttpServer @max_header_bytes(n int) -> HttpServer
export fn HttpServer @max_body_bytes(n int)   -> HttpServer
export fn HttpServer @tls(cfg ServerConfig)   -> HttpServer        // gate Plan 116

/// serve = supervised-scope-выражение: паркует вызывающую фибру, accept → spawn per-conn.
/// Возврат — после graceful drain (scope cancel) ИЛИ throw на критич. ошибке listener'а.
export fn HttpServer @serve[H Handler](handler H) Http -> Result[(), HttpError]
export fn HttpServer @serve_mux(mux ServeMux) Http -> Result[(), HttpError] => @serve(mux)
```

Хендлер на корне: `with Http = real_http() { server.serve(mux)!! }`. Тесты — `with Http = mock_http_server()`.

#### `Handler`-протокол (Go `http.Handler`-симметрия, typed Response)

```nova
export type Handler protocol {
    @handle(req consume Request) Http -> Response       // Body запроса — streaming must-consume; consume в @handle
}
export type StreamHandler protocol {
    @serve_stream(req consume Request, w consume ResponseWriter) Http -> Result[(), HttpError]
}
/// Адаптер замыкания → Handler (как Go http.HandlerFunc). §3.0 Q27 подтверждает `-> impl Handler`.
export fn handler_fn(f fn(consume Request) Http -> Response) -> impl Handler
```

`@handle` возвращает **`Response`, не `Result`**: 4xx/5xx — валидный `Response`; throw из хендлера (`req.json()?`) → драйвер ловит, логирует, шлёт `500` (recover-middleware). `req consume Request` — Body must-consume, забытое тело = compile-error (чинит Go `r.Body`-leak + сломанный keep-alive).

#### `ServeMux` — маршрутизация (Go 1.22-style)

```nova
export type ServeMux value { priv routes []Route, priv not_found Option[Handler], priv mna Option[Handler], priv tslash bool }
export fn ServeMux.new() -> ServeMux => { routes: [], not_found: None, mna: None, tslash: true }

/// pattern = "METHOD /path/{param}" | "/path". Сегменты: литералы, "{name}" (один сегмент),
/// "{name...}" (greedy tail, только последним). Конфликт паттернов → throw на регистрации.
export fn ServeMux mut @handle[H Handler](pattern str, h H) -> Result[(), HttpError]
export fn ServeMux mut @get[H Handler](path str, h H)    -> Result[(), HttpError]
export fn ServeMux mut @post[H Handler](path str, h H)   -> Result[(), HttpError]
export fn ServeMux mut @put[H Handler](path str, h H)    -> Result[(), HttpError]
export fn ServeMux mut @delete[H Handler](path str, h H) -> Result[(), HttpError]
export fn ServeMux mut @patch[H Handler](path str, h H)  -> Result[(), HttpError]
export fn ServeMux mut @head[H Handler](path str, h H)   -> Result[(), HttpError]
export fn ServeMux mut @not_found[H Handler](h H) -> ServeMux
export fn ServeMux mut @method_not_allowed[H Handler](h H) -> ServeMux   // ставит Allow-заголовок
export fn ServeMux mut @redirect_trailing_slash(on bool) -> ServeMux     // default true
export fn ServeMux @handle(req consume Request) Http -> Response         // impl Handler (sub-mux nesting)
```

**Dispatch-семантика (закрыто §3.0 Q28):**
- **404** — путь не совпал → `not_found` или дефолт.
- **405** — путь совпал, метод нет → `405` + **`Allow:`** со списком (точнее Go).
- **Trailing-slash** — strict по умолчанию; зарегистрированный `/x/` → **301** `/x`→`/x/`; `redirect_trailing_slash(false)` отключает (→ 404).
- **Path-нормализация ДО матчинга** — percent-decode, схлоп `//`, **reject `..`-traversal → 400**, reject control/NUL. `req.param(name)` — decoded.
- **longest-pattern-wins** precedence (Go 1.22). **`HEAD`** авто-роутится на `GET`-хендлер (тело отбрасывается на сериализации), если явный `HEAD` не зарегистрирован.

#### `Request`

```nova
export type Request consume value {
    ro method    Method
    ro url       Url             // origin-form target → reconstructed absolute
    ro version   Version
    ro headers   HeaderMap
    ro peer_addr SocketAddr      // remote (X-Forwarded — НЕ доверяем по умолчанию, §11 opt-in)
    ro local_addr SocketAddr
    priv body    Body
}
export fn Request @method() -> Method
export fn Request @path() -> str
export fn Request @query(key str) -> Option[str]
export fn Request @query_all(key str) -> []str
export fn Request @param(name str) -> Option[str]      // {name} из ServeMux (decoded)
export fn Request @header(name str) -> Option[str]
export fn Request @headers_all(name str) -> []str
export fn Request @content_length() -> Option[int]
export fn Request @content_type() -> Option[ContentType]
export fn Request @cookie(name str) -> Option[Cookie]
export fn Request @peer_cert() -> Option[Certificate]  // mTLS (gate 116) — для service-mesh authz

// Body-консьюмеры (каждый consume'ит весь Request):
export fn Request consume @body_bytes(max int) Http -> Result[[]u8, HttpError]   // > max → BodyTooLarge
export fn Request consume @text(max int)       Http -> Result[str, HttpError]
export fn Request consume @json[T](max int)    Http -> Result[T, HttpError]      // gate Q20
export fn Request consume @form(max int)       Http -> Result[HeaderMap, HttpError]
export fn Request consume @into_body() -> Body
export fn Request consume @drain() Http -> Result[(), HttpError]                 // для ранних 4xx без чтения
```

**Лимиты enforced на драйвере И consume:** header-блок > `max_header_bytes` → `431` (не доходит до хендлера); `body_bytes(max)` → `BodyTooLarge`.

#### `Response` / `ResponseWriter`

```nova
export type Response value { ro status StatusCode, ro headers HeaderMap, priv body Body }
export fn Response.new(status int) -> Response
export fn Response.text(status int, s str) -> Response          // text/plain; charset=utf-8
export fn Response.html(status int, s str) -> Response
export fn Response.json[T](status int, v T) -> Result[Response, HttpError]   // gate Q20
export fn Response.bytes(status int, ct ContentType, data []u8) -> Response
export fn Response.redirect(status int, location str) -> Response
export fn Response.empty(status int) -> Response                // 204/304
export fn Response @header(name str, value str) -> Response
export fn Response @cookie(c SetCookie) -> Response
export fn Response @content_type(ct ContentType) -> Response

/// Потоковая запись (StreamHandler). Линейный (consume): write_head один раз, write*/flush, finish().
/// Незакрытый writer = compile-error (force-close → корректный chunked-terminator).
export type ResponseWriter consume value { priv conn ConnHandle, priv state WriterState }
export fn ResponseWriter mut @write_head(status int, headers HeaderMap) Http -> Result[(), HttpError]
export fn ResponseWriter mut @write(data []u8) Http -> Result[int, HttpError]
export fn ResponseWriter mut @write_str(s str) Http -> Result[int, HttpError]
export fn ResponseWriter mut @flush() Http -> Result[(), HttpError]
export fn ResponseWriter mut @write_trailers(t HeaderMap) Http -> Result[(), HttpError]   // RFC 7230 §4.4 / h2
export fn ResponseWriter consume @finish() Http -> Result[(), HttpError]   // chunked-terminator — единств. разрядка
```

**Content-Length vs chunked (закрыто §3.0).** in-memory `Body` → точный **`Content-Length`**; `ResponseWriter` без CL → **chunked** (h1) / DATA (h2). `HEAD`/`204`/`304` → тело не пишется. Конфликт user-`Content-Length`+chunked → драйвер игнорирует user-CL (chunked wins). **Smuggling defense:** дублированные/конфликтующие `Content-Length` или `CL`+`TE` во входящем → `400` (RFC 9112 §6.1).

#### Middleware — onion-композиция

```nova
export type Middleware protocol { @wrap[H Handler](next H) -> impl Handler }
export fn chain[H Handler](mws []Middleware, handler H) -> impl Handler   // chain([a,b,c],h)=a(b(c(h)))
export fn mw_logging(log fn(Request, StatusCode, Duration)) -> impl Middleware
export fn mw_recover() -> impl Middleware            // throw → 500
export fn mw_request_timeout(d Duration) -> impl Middleware   // per-request supervised(timeout:) → 503/504
export fn mw_compress() -> impl Middleware           // gzip/br по Accept-Encoding (gate codec)
export fn mw_basic_auth(check fn(str, str) -> bool) -> impl Middleware   // 401 + WWW-Authenticate
export fn mw_max_body(max int) -> impl Middleware
```

**Порядок (закрыто §3.0):** снаружи-внутрь `recover` → `logging` → `request_timeout` → `compress` → `auth` → router. `defer`-cleanup в каждом слое **всегда добегает** (173) даже при throw/cancel.

#### Structured concurrency: serve-loop, bounded conns, graceful shutdown

```nova
export fn HttpServer @serve[H Handler](handler H) Http -> Result[(), HttpError] {
    mut listener = TcpListener.bind(@addr)?                 // или TLS-listener при @tls (gate 116)
    ro sem = Semaphore.new(@max_conns)                     // Plan 103.4 — bounded concurrency
    supervised(cancel: server_cancel_token()) {            // 173.1 value-scope
        loop {
            ro stream = listener.accept()?                 // park; cancel прерывает accept чисто
            sem.acquire()                                  // backpressure (cancel-aware, §3.0 Q29)
            spawn {
                defer sem.release()                         // всегда добегает (173)
                serve_conn(stream, handler, @, sem)
            }
        }
    }                                                       // ← scope join: graceful drain после cancel
    Ok(())
}
```

Per-conn driver: header-timeout (slowloris) → отдельный дедлайн на чтение заголовков (`431` при превышении лимита, `408` при timeout); per-request полный дедлайн (`read_timeout`); keep-alive с `idle_timeout`; `conn.close()` consume-гарантирован. **100-continue:** при `Expect: 100-continue` — `100 Continue` ПЕРЕД чтением Body хендлером, либо `417`/финальный статус если хендлер ответил не читая.

**Graceful shutdown (Plan 173 deadline-drain):**

```nova
export fn HttpServer @serve_graceful[H Handler](handler H, shutdown CancelToken, drain Duration)
    Http -> Result[(), HttpError] {
    // 1. shutdown.cancel() → stop-accept (listener.close), новые conn refused.
    // 2. in-flight фибры доделывают в пределах `drain` (supervised join с deadline).
    // 3. по истечении — cancel в park'нутые conn-фибры; defer-cleanup (sem.release/conn.close) ГАРАНТ. добегает.
    supervised(deadline: Monotonic.now() + drain, cancel: shutdown) {
        @serve(handler)!!
    }
    Ok(())
}
```

Это **примитив**, не ручная пляска `WaitGroup`+`context` (Go) / `ChannelGroup` (hyper).

#### End-to-end (router + middleware + graceful shutdown)

```nova
import std.http.{ HttpServer, ServeMux, Request, Response, handler_fn,
                  chain, mw_recover, mw_logging, mw_request_timeout }
import std.net.SocketAddr
import std.concurrency.CancelToken

fn build_routes() -> ServeMux {
    mut mux = ServeMux.new()
    mux.get("/health", handler_fn(|req| Response.text(200, "ok")))!!
    mux.get("/users/{id}", handler_fn(|req| {
        Response.text(200, "user ${req.param("id").unwrap_or("?")}")
    }))!!
    mux.post("/echo", handler_fn(|req| {
        match req.text(1*1024*1024) {       // лимит 1 MiB → BodyTooLarge при превышении
            Ok(s)  => Response.text(200, s)
            Err(e) => Response.text(400, "bad body: ${e.to_str()}")
        }
    }))!!
    mux.not_found(handler_fn(|req| Response.text(404, "nope")))
    mux
}

fn main() Http {
    ro app = chain([ mw_recover(),
                     mw_logging(|req, st, dur| println("${req.method().as_str()} ${req.path()} → ${st.as_u16()} (${dur})")),
                     mw_request_timeout(15.sec) ], build_routes())
    ro shutdown = CancelToken.new()
    supervised {
        spawn { on_sigint(); shutdown.cancel() }
        spawn {
            ro srv = HttpServer.bind(SocketAddr.loopback(8080)).max_conns(2048).header_timeout(10.sec).idle_timeout(60.sec)
            srv.serve_graceful(app, shutdown, 30.sec)!!
        }
    }
}
// прод: with Http = real_http() { main() }   |   тест: with Http = mock_http_server() { … }
```

### 3.https-h2. HTTPS + HTTP/2 (LATER — gated phases, prod-grade)

> Отдельные фазы ПОСЛЕ plaintext HTTP/1.1 (Ф.1-Ф.3). Каждая — **🔴 HARD-GATE Plan 116**. «Без упрощений» (§8.0) на КАЖДОЙ. APIs version-transparent.

#### HTTPS (Ф.4 — gate Plan 116)

**Принцип: HTTP-логика не знает про TLS.** HTTP/1.1-парсер/сериализатор работают над byte-транспортом; HTTPS = подставить `TlsStream` под тот же seam. Один внутренний транспорт-тип:

```nova
type Conn consume | Plain(TcpStream) | Secure(TlsStream)   // both must-consume → Conn must-consume
fn Conn mut   @read_bytes(buf mut []u8) TcpNet Tls -> Result[int, HttpError]
fn Conn mut   @write_bytes(data []u8)   TcpNet Tls -> Result[int, HttpError]
fn Conn consume @close()                TcpNet Tls -> Result[(), HttpError]    // close_notify (Secure) + TCP close
```

**Effect-инкапсуляция (§3.0 Q30 — критик-gap «effect-сигнатура»):** `Tls`/`TcpNet`/`DnsNet` НЕ протекают в user-сигнатуры — **`real_http()` требует их в своём `with`-scope**, user-код видит только эффект `Http`. Иначе http↔https меняли бы тип fn (ломая version-transparency). User: `fn fetch() Http -> Result[…]`; `real_http()` объявлен как требующий `TcpNet Tls DnsNet Time`.

**Клиент (HTTPS):** `https`-схема → после `DnsNet.lookup`+`TcpStream.connect` → `Tls.handshake_client(tcp, cfg)` (consume tcp → `TlsStream`); **SNI** = host из `Url`; **ALPN** = `["h2","http/1.1"]` (или `["http/1.1"]` при `http1_only`); `Tls.alpn_negotiated()` → `"h2"` ⇒ h2-ветка, иначе h1-over-TLS. **Cert-verify** проксируется в `ClientConfig.verification`: `SystemRoots` (дефолт)/`CustomRoots`/`Pinned`/`InsecureSkipVerify` (`danger_accept_invalid_certs`, warning из 116 пробрасывается). Hostname-verify — 116 mandatory.

**Сервер (HTTPS):** тот же `Handler`/`ServeMux`, отличие — `@tls(ServerConfig)`/`bind_tls`: per-conn accept → `Tls.handshake_server` → `alpn_negotiated()` → h2/h1. cert/key ← `ServerConfig.from_pem` (in-memory ИЛИ file-path — file gate Plan 180 fs); mTLS ← `ServerConfig.client_cert`: `None`/`Optional`/`Required`; peer-cert → `Request.@peer_cert()`.

**🔴 HARD-GATE — Plan 116 ДОЛЖЕН дать:** `TlsStream consume`+`@close()` (D213); `ClientConfig`(SNI/ALPN/verification/roots/timeout)+`ServerConfig`(cert/key/ALPN/client_cert); `Tls`-effect ops (`handshake_client/server`, byte-first `read/write/close`, `peer_cert`, **`alpn_negotiated()`**, `cipher_suite`, `protocol_version`); SNI+ALPN (D212); cert-verify+root-store (D211); D210-D213 в spec.

**Error-mapping:** `TlsError` → `HttpError{kind: Tls, source: ErrSource.Tls(TlsError)}` (`CertificateInvalid`/`HostnameMismatch`/`HandshakeTimeout`/`AlpnNoCommonProtocol` сохраняются как source).

#### HTTP/2 (Ф.5 — двойной гейт: Plan 116 ALPN-`h2` + после Ф.4)

h2 framing/HPACK = **Nova-логика (.nv) над byte-транспортом** внутри `real_http*` (никакого C, кроме TLS-FFI 116). `Method`/`StatusCode`/`HeaderMap`/`Body`/`Url` переиспользуются 1:1 — h2 это **другой wire-формат той же модели**.

**🏆 Nova-differentiator: КАЖДЫЙ h2-stream = фибра.** Одно TLS/TCP-conn = supervised-scope; per-stream = spawned фибра; demux-фибра читает кадры и роутит по `stream_id`; flow-control/backpressure = park/wake; cancel conn → cancel всех stream-фибр через scope. **HPACK encode/decode сериализуется на conn-фибре** (dynamic-table строго sequential per direction — §3.0 Q31, иначе table-corruption); per-stream фибры только обрабатывают данные.

| h2-механизм | RFC | Реализация |
|---|---|---|
| Connection preface | 9113 | client `PRI*…` + SETTINGS; server SETTINGS; валидация |
| Framing | 9113 | `type Frame` (DATA/HEADERS/SETTINGS/WINDOW_UPDATE/RST_STREAM/PING/GOAWAY/CONTINUATION); reader/writer-фибры; `MAX_FRAME_SIZE` enforce (default 16 KiB) |
| HPACK | 7541 | `std/http/h2/hpack.nv`; static+dynamic+Huffman; **dynamic-table bounded** (default 4 KiB, anti-bomb); decode → `HeaderMap` |
| Multiplexing | 9113 | **each stream = фибра**; `MAX_CONCURRENT_STREAMS` enforce (Semaphore, default 256) |
| Flow control | 9113 | conn+per-stream window (default 64 KiB); producer **парк**ится при исчерпании, `WINDOW_UPDATE` → wake; честный backpressure |
| SETTINGS | 9113 | exchange+ACK; `MAX_CONCURRENT_STREAMS`/`INITIAL_WINDOW_SIZE`/`MAX_FRAME_SIZE`/`MAX_HEADER_LIST_SIZE`(default 16 KiB)/`HEADER_TABLE_SIZE`/`ENABLE_PUSH` |
| PING | 9113 | авто-ACK; client keepalive opt |
| GOAWAY | 9113 | graceful drain (173 supervised+deadline): новые reject, in-flight ≤ last_stream_id дорабатывают |
| RST_STREAM | 9113 | cancel stream-фибры (не рвёт conn) |
| Anti-DoS | 9113 | **rapid-reset CVE-2023-44487** (счётчик RST/sec → GOAWAY), **CONTINUATION-flood mitigation**, header-list-size cap. Все дефолты **конфигурируемы** в HttpServerConfig/HttpClientConfig |
| Server push | 9113 | **OFF** (`SETTINGS_ENABLE_PUSH=0`; не инициируем); opt-in — НЕ реализуем V1 (deprecated в браузерах, §11) |
| Priority | 9113 §5.3 | **SKIP** (deprecated); PRIORITY parse+drop |

**h2c prior-knowledge (opt):** `http2_prior_knowledge()` — h2 cleartext без ALPN (preface сразу). HTTP/1.1-`Upgrade: h2c` — **НЕ реализуем** (deprecated RFC 9113 §3.2). Дефолт — `Auto` (ALPN).

**Версионная прозрачность (контракт):** один `HttpClient`/`Handler`/`Request`/`Response`. Разница h1↔h2 целиком в `real_http*`. `Version` наблюдаема, поведение хендлера от неё не зависит. h2-псевдо-headers нормализуются framing-слоем в `HeaderMap`.

**Под-планы при росте:** server → **182.1**, h2 → **182.2**. HTTP/3/QUIC — вне scope (§11).

---

## 3.0. Закрытые решения (Q1–Q31 — РЕШЕНЫ)

| # | Вопрос | РЕШЕНИЕ | Обоснование |
|---|---|---|---|
| Q1 | `Http`-seam vs pure-code | **`Http` effect = высокоуровневый client-seam (request→response) + `real_http()` (over `TcpNet`+`Tls`+`DnsNet`+`Time`) + `mock_http()`.** h1/h2 framing/parsing = **Nova-логика (.nv)** ВНУТРИ `real_http`-слоя над byte-транспортом. Server — `HttpServer`-seam. Seam тонкий (~5 ops); 95% HTTP = value-типы+логика. | Триада (как `TcpNet`) → мокабельность без сети — недостижимо в Go (`RoundTripper`-DI вручную)/reqwest/fetch. Парсинг в .nv = nv-sourcing-максимум. Закрывает **Q9** (core .nv vs C-транспорт) |
| Q2 | `HttpError` taxonomy | **ОДИН `HttpError{kind, url Option[Url], source}`; `ErrorKind` OPEN** (wildcard обязателен). Транспортная ошибка = `kind: Connect/Closed/Timeout` + `source: Net(NetError)` (НЕ дублируется как `kind.Io`). `ErrSource = Net\|Url\|Utf8(\|Tls когда 116)`. | Rust `reqwest::Error{kind,url,source}`. Source-chain через typed `NetError`/`TlsError`/`ParseUrlError`. OPEN = forward-compat h2/h3 |
| Q3 | must-consume `Body` | **Response `Body` must-consume (D133): `consume @close/bytes/text/json/discard/copy_to/into_reader`.** Незакрытый = **compile-error**. | Чинит главный Go-footgun `resp.Body`-leak на compile-time; бьёт reqwest(`Drop`)/fetch/OkHttp(runtime) |
| Q4 | 4xx/5xx как value | **4xx/5xx — ВАЛИДНЫЙ `HttpResponse`.** `send()` падает только на transport/protocol. `error_for_status()` opt-in → `Err(Status)`; **non-consuming** (на Err Body жив, caller обязан разрядить). | reqwest-модель. 404 — успешный обмен. error_for_status ДО взятия Body |
| Q5 | byte-first + net amendment | **Body/headers byte-first (`[]u8`); `str` через fallible.** net amendment (**owner 2026-06-26 — ПОЛНАЯ миграция `str`→`[]u8`, НЕ minimal-additive**): весь byte-surface std/net мигрирует на `[]u8` — `TcpNet` read/write/write_all, accept()-стримы, **split-half** (read_half_read/write_half_write/write_half_write_all), `UdpNet` send_to/recv_from (если используется HTTP). Тонкий `str`-convenience может лежать сверху. Sequencing: byte-surface (additive `[]u8`) = **prereq Ф.0.5** (HTTP строится на нём, `str` транзитом); **полный демоут `str` + миграция всех net-callers** = **committed deliverable 182** (owner-approved, НЕ optional/droppable), guarded byte-baseline-commit ПОСЛЕ доказанного HTTP byte-path — старт HTTP не блокирует (§3.10/§4 Ф.0.5/§6/§9, D327). | HTTP-тело — байты; `str`(UTF-8) lossy на gzip/binary. Go/Rust/undici byte-first. Governance-amendment одобрен владельцем (тот же owner std/net) |
| Q6 | URL промоут | **`_experimental/encoding/url.nv` → `std/http/url.nv` (`module std.http`).** Фикс `decode_query`-bug + `encode_query` multi-byte + IPv6-bracket + host-валидатор (§3.8). `Url.parse`→Result (D325). | URL — часть message-model; one-folder-module. Rust `url`/Go `net/url` рядом с http |
| Q7 | version-transparency | **Один `HttpClient`/`Handler` для h1/h2/h3.** `Version` exposed, НЕ ветвит API; ALPN авто. | reqwest/Go/Java. Версия — деталь транспорта. Бьёт Node (раздельные `http`/`http2`) |
| Q8 | h2-stream = fiber | **Каждый h2-stream = фибра.** Conn-state (HPACK/flow-window) под Mutex/atomics (103.3); HPACK-encode сериализован на conn-фибре (Q31). | Go (goroutine-per-stream) + Nova structured-concurrency first-class. Бьёт hyper(poll)/Node(callback) |
| Q9 | redirect default | **Follow по умолчанию (limit=10 → `TooManyRedirects`).** `RedirectPolicy{Follow(max)\|None\|Custom}`. **Cross-origin → STRIP `Authorization`/`Cookie`.** 303→GET; 307/308 preserve. | reqwest+Go (оба auth-strip; CVE-класс). Soundness: auth-strip — security-инвариант |
| Q10 | cookie default | **Jar OPT-IN per-client (default OFF).** RFC 6265bis: domain/path/Secure/HttpOnly/SameSite/PSL/`__Host-`/`__Secure-`; **Secure→https-only send, SameSite=None требует Secure**. | reqwest(default off)+Go. No-surprise-state. Send-side инварианты — security |
| Q11 | auth | **`.basic_auth(u,p)`/`.bearer(t)` + cross-origin strip (Q9).** | reqwest/requests. Typed > ручной header |
| Q12 | auto-decompress | **gzip/deflate/br ON по умолчанию (opt-out), bomb-cap 100 MiB → `BodyTooLarge`** — но **🔴 HARD-GATE на NEW sub-plan `std/encoding/compress`** (RFC 1950 zlib/1951 deflate/1952 gzip + brotli; owner 2026-06-26, точно как HTTPS гейтится на 116). **`identity`+`chunked` transfer приземляются СЕЙЧАС без него**; decompress — НЕ в plaintext-core landed-acceptance (§8), приземляется когда compress-sub-plan приземлится. | reqwest+undici. Cap — zip-bomb DoS. compress-sub-plan ДОЛЖЕН быть создан отдельно (НЕ в 182) |
| Q13 | graceful shutdown | **supervised scope (173); per-conn фибра, bounded Semaphore (103.4); drain под `deadline:`.** | Go `Shutdown(ctx)` + Nova structured = drain+bound бесплатно |
| Q14 | smuggling/injection | **СТРОГО: reject CL-vs-TE conflict (TE-приоритет); reject CR/LF/NUL в header; reject non-token method/name; `Host` обязателен (h1).** | CVE «request smuggling»/«header injection». §8.0 — НЕ опционально |
| Q15 | hidden global client | **`http.get()` — global lazy `Once`, ТОЛЬКО скрипты/one-shot; prod → явный `HttpClient`** (документировано). | Zig-дух allocation/state-transparency; критика `requests.Session`-неявности учтена — но удобство для скриптов оставлено явно-фенсед |
| Q16 | retry/idempotency | **Auto-retry ТОЛЬКО reused-from-pool conn, до 1 байта ответа, ТОЛЬКО idempotent (`@is_idempotent`), max 1. Fresh conn и POST — НИКОГДА.** Pool-eviction-on-error. | Go/OkHttp/reqwest silently-retry idempotent on stale conn. POST-replay = footgun, запрещён |
| Q17 | named constants | **zero-arg фабрики (`StatusCode.ok()`/`Mime.text_plain()`), НЕ top-level `let`** (top-level value-let не подтверждён в std). | Верифицировано: код использует фабрики, не export-let-value-константы |
| Q18 | header str↔[]u8 | **NAME ASCII-only tchar; VALUE `[]u8`; `str`-API latin1 fast-path; `@to_str()` fallible на non-ASCII.** | reqwest `HeaderValue` (bytes, to_str fallible). Эргономика+корректность |
| Q19 | BodyReader backing | **Чисто-Nova-декодер (chunked/CL/h2-DATA композится над byte-source TcpReadHalf/TlsStream/h2-stream), НЕ C-handle.** | nv-sourcing-максимум; декодинг — логика, не транспорт |
| Q20 | json (де)сериализация | **`@json[T]` GATED на `std/encoding/json` + reflective/derive (НЕ существует).** До него — ручной `to_json/from_json`-протокол ИЛИ под-план. Из §8-acceptance landed-фаз `json()` ИСКЛЮЧ�ён пока gate открыт. | Нет reflective serde в Nova; honest-gate, не «available» |
| Q21 | charset decode | **UTF-8 + ISO-8859-1/latin1 декодируются ВСЕГДА; прочие charset → `Protocol("unsupported charset")` + совет `.bytes()`.** Scoped-out с rationale, НЕ followup. | latin1-тело должно читаться корректно; экзотика — явный scope-out + pos-тест latin1 |
| Q22 | mock форма | **Единая `mock_http()`/`mock_http_server()`: билдер `.on(method, path, \|req\|→MockResponse)`** для client+server. | Унификация триады между секциями |
| Q23 | proxy | **`Proxy{http/https/socks5, no_proxy, auth}` + CONNECT-tunnel (HTTPS-via-proxy) = РЕАЛЬНАЯ Ф.2/Ф.4-задача**, НЕ followup. `from_env` (HTTP(S)_PROXY/NO_PROXY precedence). | Go `ProxyFromEnvironment`/reqwest/undici. Builder-метод без deferred-механизма = §8.0-violation → закрыт |
| Q24 | SSRF-guard | **opt-in `@ssrf_guard(deny_private)` — после DNS-resolve блок loopback/link-local/RFC1918/metadata → `Blocked`. Default OFF.** + host-валидатор (Q6). | Аудит-флаг SSRF; differentiator-ось (никто не даёт в std из коробки) |
| Q25 | lazy url-err builder | **VERB-методы non-fallible; url-парс-ошибка ленивая → `send()` как `InvalidUrl`** (осознанное исключение R1 ради reqwest-эргономики). | reqwest так делает; цепочка builder'а должна быть chainable |
| Q26 | pool-key + h2 | **Pool-ключ = `(scheme, host, port, alpn)`** — h1-conn НЕ переиспользуется как h2. | Иначе h1/h2 conn-mix → protocol-error |
| Q27 | impl Trait / protocol-bound | **`-> impl Handler`/`impl Middleware` (existential) + `[H Handler]` protocol-bound поддержаны** (handler_fn/chain/mw_*). Если spec не даёт — fallback concrete newtype `HandlerFn`. | Верифицировать против spec в Ф.0; есть fallback |
| Q28 | mux dispatch | **404/405+`Allow`; trailing-slash strict+301-on-`/x/` (opt-out); path-normalize ДО матча (reject `..`→400); longest-pattern-wins; HEAD→GET.** | Go 1.22 precedence + RFC 9110. Явное закрытие footgun-prone выбора |
| Q29 | Semaphore cancel | **`Semaphore.acquire()` cancel-aware (под supervised(cancel:)) — не висит при graceful shutdown.** | Иначе backpressure-слот задержит drain. Сверить с 103.4 |
| Q30 | Tls effect-leak | **`Http`-seam ИНКАПСУЛИРУЕТ `Tls`/`TcpNet`/`DnsNet`/`Time`; user видит ТОЛЬКО `Http`.** `real_http()` объявлен требующим их в scope. | Version-transparency: иначе http↔https меняют тип fn |
| Q31 | HPACK concurrency | **HPACK encode/decode СЕРИАЛИЗОВАН на conn-фибре** (dynamic-table sequential per direction); per-stream фибры только данные. | Иначе table-corruption между фибрами |

---

## 4. Фазы

**Dep-chain:** Ф.0 → **Ф.0.5** → Ф.1 → Ф.2 → Ф.3 → **[HARD-GATE Plan 116]** → Ф.4 → **[HARD-GATE Plan 116 ALPN]** → Ф.5 → Ф.6. **«сейчас»:** Ф.0–Ф.3, Ф.6(h1-часть). **«позже/gated»:** Ф.4 (HTTPS), Ф.5 (h2). Коммит после каждой фазы (§10). Server при росте → 182.1; h2 → 182.2.

- **Ф.0 — GATE (без кода). «сейчас».** Закрыть §3.0 (Q1–Q31, готово); написать **D327–D332 spec-first** (§5); подтвердить расписание Plan 116 (гейт Ф.4/Ф.5; параллельно); **решить net `[]u8`-амендмент** (Q5, owner-sign-off под conventions-governance); зарезервировать D-номера (D327 старт); **reconcile Plan 116 forward-refs** (117/122 → 182, §6/§9); **verify** на main: `str.from_bytes` (есть — jwt.nv:75; убрать из HARD-PREREQ, оставить confirm-step), `decode_query`-bug-repro, `-> impl Trait`/protocol-bound (Q27 fallback), top-level value-let (Q17). **GATE.** DEP: Plan 181, 173, 116-schedule.
- **Ф.0.5 — PREREQ: URL-промоут + net byte-surface. «сейчас».** (1) Промоут url.nv → `std/http/url.nv` (Q6); **фикс `decode_query`-bug**; `Url.parse`/Result (D325); **ОБЯЗАТЕЛЬНО (§8): фикс `encode_query` multi-byte + IPv6-bracket + строгий host/SSRF-валидатор** (§3.8 — не «доводка»). (2) net byte-surface (ПОЛНЫЙ список §3.10): `read_bytes`/`write_bytes`/`write_all_bytes` на Stream+ReadHalf+WriteHalf; `real_tcp_net`+`mock_tcp_net`+C-shim; `str`-варианты сохранить. **HARD-BLOCKER для Ф.1.** pos: `Url.parse` round-trip; `decode_query` round-trip (был idle); `encode_query` multi-byte UTF-8 round-trip; `read_bytes` байт-в-байт incl. не-UTF-8. neg: malformed-port→`InvalidPort`; truncated-`%X`→`InvalidPercentEncoding`; octal/hex-IP-obfuscation→reject. DEP: Ф.0, net.
- **Ф.1 — message-model. «сейчас».** `Method`(enum+`Other`), `StatusCode`(newtype+классы+`reason`+фабрики), `HeaderMap`(case-insens/ordered/multi-value; reject CRLF/NUL Q14; str↔[]u8 Q18; trailer), `Version`, `Url`(Ф.0.5), `Body`(**must-consume** Q3; in-mem`[]u8`\|stream`BodyReader`; bytes/text/json/discard/copy_to/into_reader/trailers consume; charset Q21; bomb-cap), `Mime`/`ContentType`, `Cookie`/`SetCookie`(RFC 6265bis send-инварианты), `HttpError`/`ErrorKind`(Q2). Чистые value+логика. spec: D328, D329. pos: HeaderMap case-insens/multi/ordered; StatusCode класс/reason; Method round-trip; Body.bytes consume; latin1 text-decode. neg: **body не consume→`EXPECT_COMPILE_ERROR`**(Q3); double-consume; CRLF в header→reject; non-token method→reject. DEP: Ф.0.5.
- **Ф.2 — HTTP/1.1 client. «сейчас».** `Http`+`real_http()`(TcpNet+DnsNet+Time, h1-парсер .nv)+`mock_http()` (Q1). `HttpClient`(pooled; builder: deadline/timeout (173), RedirectPolicy (Q9), default-headers, cookie_store (Q10), **Proxy+CONNECT-tunnel (Q23)**, decompress (Q12), **ssrf_guard (Q24)**, **retry (Q16)**). reqwest-builder; convenience `http.get` (Q15). pool+keep-alive+**eviction-on-error**+**idempotent-retry**; chunked TE+**trailers**; auto-decompress **🔴 HARD-GATE на NEW под-план `std/encoding/compress`** (gzip/deflate/br + bomb-cap) — identity/chunked самодостаточны; auth (Q11); `error_for_status` (Q4); строгий парсинг (Q14). Request body: `[]u8`\|stream\|form\|multipart\|json(**gate Q20**). **Decompress gate:** NEW под-план `std/encoding/compress` (.nv над C-zlib/brotli FFI; owner 2026-06-26 — отдельный, НЕ в 182) — пока не приземлён, decompress-acceptance §8.4 gated. spec: D327, D330. pos: GET/POST mock; chunked decode+trailers; redirect-follow; pool-reuse; **reused-dead-conn retry**; 404=Ok (Q4); `error_for_status`; CONNECT-tunnel (plaintext-proxy). neg: redirect-loop→`TooManyRedirects`; timeout→`Timeout`; **auth НЕ утекает cross-origin** (Q9); **POST не реплеится** (Q16); **errored-conn НЕ reused**; malformed status-line→`Protocol`; CL+TE→`Protocol` (Q14); bomb→`BodyTooLarge`; **SSRF private-target→`Blocked`** (Q24). DEP: Ф.1, 173, net byte-surface.
- **Ф.3 — HTTP/1.1 server. «сейчас».** `HttpServer.bind/serve`; `Handler`(+streaming `ResponseWriter`+trailers); `ServeMux`(Go-1.22 patterns/params/methods/405+Allow/trailing-slash-301/path-normalize Q28); middleware-chain (onion); graceful shutdown (Q13); per-conn фибра bounded Semaphore (Q13/Q29); body-streaming; keep-alive; `100-continue`; строгий request-парсинг+`Host`-mandatory+CL/TE-reject (Q14); slowloris (header_timeout). spec: D331. pos: echo через `mock_http_server`; routing+params; middleware-order; keep-alive; graceful-drain. neg: malformed request-line→400; `..`-traversal→400; header-injection reject; CL+TE→400; slow-loris bounded; shutdown-deadline→force-close. DEP: Ф.1, 173, 103.4.
- **Ф.4 — HTTPS client+server. «позже/gated». 🔴 HARD-GATE Plan 116.** `Conn`-seam (Plain\|Secure) под `real_http*`; client SNI+cert-verify+`ClientConfig`; server cert/key+`ServerConfig`+optional mTLS; CONNECT-tunnel-over-TLS. Version-transparent (Q7/Q30: Tls инкапсулирован). ALPN `["http/1.1"]` (h2 — Ф.5). spec: D332 (HTTP-over-TLS+ALPN→Version). pos: HTTPS GET (mock-Tls); SNI; server-cert round-trip (self-signed CustomRoots); mTLS Required; peer_cert. neg: cert-invalid→`Tls`; hostname-mismatch; HTTPS-on-plain-port→`Connect`. DEP: **Plan 116 Ф.1-Ф.5 closed**, Ф.2/Ф.3.
- **Ф.5 — HTTP/2 (client+server). «позже/gated». 🔴 HARD-GATE Plan 116 ALPN (D212).** h2 framing+HPACK (.nv)+**stream=fiber** (Q8/Q31)+flow-control+SETTINGS/GOAWAY/PING+anti-DoS (rapid-reset/CONTINUATION-flood/caps)+push-OFF+PRIORITY-drop. Version-transparent. opt-in h2c. spec: D332 §h2. pos: multiplexing (N stream=N фибр); HPACK round-trip (Huffman+dynamic-evict); flow-control backpressure (park/wake); GOAWAY graceful; SETTINGS exchange; RST_STREAM cancel-one-stream. neg: HPACK-bomb→`Protocol` (не OOM); flow-violation→`Protocol`; rapid-reset→GOAWAY; push-opt-out соблюдается. DEP: **Plan 116 ALPN**, Ф.4.
- **Ф.6 — тесты+docs+polish. «сейчас» (h1); h2-тесты после Ф.5.** §7 pos+neg полный; D327–D332 финал; `docs/http.md`(модель+cross-lang+§1a+https/h2)+`idioms/http-client.md`+`http-server.md`; **`Body.copy_to`/download-to-file** (Java/Swift-паритет, fs-gate Plan 180); slow real-socket (`*_slow.nv`). DEP: all (h2-блок gated Ф.5).

---

## 5. Spec / D / Q / docs

**D-номера:** D325=Result-everywhere (181), D326=ref-param-mode (эта сессия) → HTTP старт **D327**. ⚠ verify D316–D324 (179/180) уже в `spec/decisions/` к присвоению; иначе зафиксировать gap (как Plan 181 §«D316–D324 зарезервированы»). Если 180 не приземлится первым — гейтить присвоение или взять D327+ безусловно с нотой gap (решение в Ф.0).

- **NEW D327** — `Http`/`HttpServer` effect-контракт (`spec/decisions/04-effects.md`, рядом с D281/D295 net, D210 Tls): client-seam ops (`send`/`connect_pool`); server-seam; `real_http()` requires `TcpNet`+`DnsNet`+`Time` (+`Tls` для https, **инкапсулировано** Q30); h1/h2-framing = .nv (НЕ effect-op); mock-контракт; **net byte-surface-амендмент** (полный список §3.10, Q5).
- **NEW D328** — message-model: `Method`(token+`Other`), `StatusCode`(u16+классы+reason), `HeaderMap`(case-insens/ordered/multi; CRLF/NUL-reject Q14; str↔[]u8 Q18; trailer), `Version`, `Cookie`/`SetCookie`(RFC 6265bis send-инварианты), `Mime`/`ContentType`; request-target нормализация; **строгий host/URL-валидатор+IPv6-bracket+multi-byte-encode** (Q6, SSRF).
- **NEW D329** — `Body` must-consume (D133/D131/D180): Response-Body линейный, `consume @close/bytes/text/json/discard/copy_to/into_reader/trailers`; незакрытый = compile-error; **чисто-Nova `BodyReader`** (Q19); charset UTF-8/latin1 (Q21); decompression-bomb-cap (Q12); chunked-trailers.
- **NEW D330** — redirect+cookie+auth+retry+proxy+pool: 10-default, `RedirectPolicy`, 303→GET/307-308 preserve, **cross-origin auth/cookie-strip** (Q9); cookie-jar opt-in RFC 6265bis (Q10); auth (Q11); **idempotent-retry+POST-never-replay+pool-eviction-on-error** (Q16); **Proxy+CONNECT-tunnel** (Q23); **SSRF-guard** (Q24); pool-key+alpn (Q26).
- **NEW D331** — server: `Handler`/`ResponseWriter`(+trailers)/`ServeMux`(Q28)/middleware(onion); graceful shutdown (Q13); bounded Semaphore (Q29); `100-continue`; строгий request-парсинг (`Host`-mandatory, CL/TE, `..`-reject Q14/Q28).
- **NEW D332** — HTTP-over-TLS + HTTP/2: ALPN→`Version` (Q7); Tls-инкапсуляция (Q30); h2 frame/HPACK/**stream=fiber** (Q8)/HPACK-serialized-on-conn (Q31)/flow-control/SETTINGS/GOAWAY/anti-DoS(rapid-reset/CONTINUATION)/push-OFF/PRIORITY-drop; version-transparency-инвариант.
- **Q9 closure** — RESOLVED §3.0/Q1: HTTP-логика core .nv; byte-транспорт+zlib/brotli C-routed.
- **docs/* (новые):** `docs/http.md`, `docs/idioms/http-client.md`, `docs/idioms/http-server.md`.

---

## 6. Миграция

Аддитивно (`std/http/*`). **Промоут URL** (Q6): url.nv → `std/http/url.nv` (`module encoding.url`→`module std.http`); **фикс `decode_query`-bug** (re-check на main: tuple-destructure infer → `nova_int`, [url.nv:343-352]; чинить инфер предпочтительно, либо переписать без destructure); **фикс `encode_query` multi-byte** ([url.nv:320-324] эмитит ОДИН байт для >127 — percent-encode каждого UTF-8-байта) + **IPv6-bracket + host/SSRF-валидатор** (§8 acceptance, не «доводка»). `Url.from`+`Fail`→`Url.parse`→`Result` (D325). После фикса — `_experimental/encoding/url.nv` удалить (Grep `encoding.url` — нет импортёров) либо re-export-shim deprecation отдельным коммитом.

**net byte-surface** (Q5, Ф.0.5): добавить ПОЛНЫЙ список (§3.10) в [effect.nv]; `real_tcp_net`+`mock_tcp_net`+C-shim (`tcp_stream_read_bytes` уже есть, обернуть в `[]u8`-возврат). `str`-варианты сохранить транзитом; **полная миграция net `str`→`[]u8`** (owner-approved 2026-06-26, committed — НЕ optional) — byte-baseline-guarded коммит ПОСЛЕ доказанного HTTP byte-path (как Plan 180 Q6), старт HTTP не блокирует; mass compile-errors → per-file loop.

**Reconcile Plan 116 forward-refs:** [116:74-76]/[116:753-755] «Plan 117=HttpClient / Plan 122=HttpServer» → **консолидировать в 182** (HttpClient=Ф.2, HttpServer=Ф.3). Обновить layered-диаграмму 116 + §«Cross-references». Нота §9.

**`str.from_bytes` — НЕ HARD-PREREQ** (критик/accuracy): валидирующий Result-returning `str.from_bytes` уже в активном использовании ([_experimental/crypto/jwt.nv:75/109]); `Body.text()` опирается на него напрямую. В Ф.0 — confirm-step (verify сигнатуру на main), не блокирующий unknown.

**`_experimental` cleanup:** после промоута — удалить url.nv; пересобрать `nova-cli` после `.nv` (`include_str!`); верификация против чистого бинаря.

---

## 7. Тесты (pos + neg; `nova_tests/http182/`, neg `neg/`)

Раскладка как net/180: pos = folder-module (`module nova_tests.http182`); neg = `neg/` subdir (`module neg.<name>` + `EXPECT_*`, один маркер/файл); классификация по маркеру. **mock-handler-тест MANDATORY** (`mock_http`/`mock_http_server`). slow/real-socket = `*_slow.nv` (skipped by default).

**pos / контрактные:**
- **message-model:** HeaderMap case-insens+multi+ordered; StatusCode класс/reason; Method round-trip incl. `Other`; Cookie/SetCookie RFC 6265bis; Mime/ContentType; **latin1 text-decode** (Q21).
- **must-consume Body (pos):** `bytes()`/`text()`/`json()`(gate) разряжают; `into_reader()` дренирует; `copy_to(writer)`; chunked-**trailers**.
- **URL (Ф.0.5):** `Url.parse` round-trip (userinfo/port/query/fragment); **`decode_query` round-trip** (bug-fix-regression-guard); **`encode_query` multi-byte UTF-8** (regression-guard); IPv6-bracket.
- **client (`mock_http`):** GET/POST; query-builder; default-headers merge; chunked decode+trailers; redirect-follow (301/302/303→GET / 307-308 preserve); pool-reuse+keep-alive; **reused-dead-conn idempotent-retry** (Q16); `404=Ok` (Q4); `error_for_status`; **CONNECT-tunnel** (plaintext-proxy Q23).
- **server (`mock_http_server`):** echo; ServeMux params+method+405-Allow; trailing-slash-301; middleware-order; keep-alive; graceful-shutdown (in-flight завершается, новые reject); `100-continue`; streaming.
- **byte-roundtrip:** body `[]u8` write→read **байт-в-байт** incl. не-UTF-8.
- **decompress (gated codec):** gzip/deflate/br round-trip (если codec-subtask landed).
- **HTTPS (Ф.4, mock-Tls):** SNI; cert round-trip; mTLS; version-transparent.
- **h2 (Ф.5):** multiplexing (N stream=N фибр); HPACK round-trip (Huffman+evict); flow-control backpressure; GOAWAY graceful; SETTINGS; RST_STREAM cancel-one.

**neg (`EXPECT_COMPILE_ERROR`):**
- **body не consume** (главный, Q3); double-consume; use-after-consume; `ErrorKind`-match без wildcard.

**neg (`EXPECT_RUNTIME_PANIC`/`EXPECT_STDERR`-substring):**
- malformed request/status-line→`Protocol`/400; gigantic header→`431`/`HeaderTooLarge`; chunked edge (bad size/missing CRLF/premature EOF)→`Protocol`.
- redirect-loop→`TooManyRedirects`; timeout→`Timeout`.
- graceful-shutdown под нагрузкой.
- **smuggling:** двойной CL vs TE (CL.TE/TE.CL)→`Protocol` (Q14).
- **header-injection:** CR/LF/NUL→reject (Q14).
- **decompression-bomb:** > cap→`BodyTooLarge` (Q12).
- **auth-leak:** `Authorization`/`Cookie` НЕ cross-origin (Q9).
- **POST НЕ реплеится** на reused-conn-fail; **errored-conn НЕ reused** (Q16).
- **SSRF:** private-target → `Blocked` (Q24); octal/hex-IP-obfuscation reject; userinfo-confusion.
- **cookie:** Secure-cookie НЕ отправляется по `http://` (Q10).
- `..`-path-traversal→400 (Q28).
- HTTPS-on-plain-port→`Connect`; cert-invalid→`Tls` (Ф.4); HPACK-bomb/flow-violation→`Protocol`; rapid-reset→GOAWAY (Ф.5).

**slow/integration (`*_slow.nv`, opt-in):** real-socket loopback client↔server; large-body streaming copy; real gzip large-file; h2 stress (100 stream'ов).

---

## 8. Критерии приёмки

0. **🔴 ОБЯЗАТЕЛЬНО: «без упрощений, как для прода».** Ни одного «решим потом» на критическом пути (парсинг, security: smuggling/injection/auth-strip/bomb-cap/SSRF/cookie-send-инварианты, must-consume, timeout-by-default, retry-idempotency, proxy-CONNECT) — на КАЖДОЙ приземлённой фазе (Ф.0–Ф.6). Каждая behavior-change — **pos+neg + аргумент звучности**; «звучность по построению»/корпус НЕ заменяют edge-тесты. 0 regressions vs **чистый бинарь** (kill-switch на ТОМ ЖЕ бинаре); полный регресс зелёный (батчами <10мин). **Явно gated, НЕ «решим потом»:** `json()` (Q20, нет serde), gzip/br-decompress (Q12, нет codec) — исключены из landed-acceptance пока их зависимость открыта; identity/chunked-путь самодостаточен. **Явно scoped-out с rationale (НЕ violation):** non-UTF-8/latin1 charset (Q21), HTTP/3/WebSocket/server-push/h2-PRIORITY/h2c-Upgrade/zstd (§11).
1. **message-model (Ф.1):** `Method`/`StatusCode`/`HeaderMap`/`Version`/`Url`/`Body`(must-consume)/`Cookie`/`Mime`/`HttpError`(OPEN `ErrorKind`); CRLF/NUL-reject; latin1-decode. **must-consume:** непрочитанный/незакрытый/double-consume body → compile-error.
2. **URL (Ф.0.5):** промоут; **`decode_query`-bug fixed** + **`encode_query` multi-byte fixed** + **IPv6-bracket + host/SSRF-валидатор** (round-trip+neg зелёные); `Url.parse`→Result.
3. **client (Ф.2):** `Http`-seam+`real_http`/`mock_http`; reqwest-builder+convenience; pool+keep-alive+**eviction**+**idempotent-retry** (POST-never); chunked+trailers; redirect+**cross-origin auth-strip**; auth; **Proxy+CONNECT**; **SSRF-guard**; deadline/timeout (173) by-default; `error_for_status`; smuggling/injection-reject; bomb-cap. (decompress gated codec; json gated serde.) `mock_http` детерм.
4. **server (Ф.3):** `HttpServer`/`Handler`/`ServeMux`(Q28)/middleware; per-conn bounded (Semaphore); **graceful shutdown**; keep-alive; `100-continue`; `Host`-mandatory+строгий парсинг+`..`-reject. `mock_http_server` детерм.
5. **HTTPS (Ф.4) — 🔴 HARD-GATE 116:** SNI+cert-verify, server cert/key+optional mTLS+peer_cert; **version-transparent** (Tls инкапсулирован); honest-gate (116 не готов → Ф.4 не стартует, явно).
6. **h2 (Ф.5) — 🔴 HARD-GATE 116 ALPN:** framing+HPACK+**stream=fiber**+flow-control+SETTINGS/GOAWAY+anti-DoS(rapid-reset/CONTINUATION)+push-OFF; version-transparent; HPACK/flow neg.
7. **byte-first:** body/headers `[]u8`; `str` через fallible; net byte-surface (полный §3.10) приземлён.
8. **spec:** D327–D332 в `spec/decisions/`; Q9 закрыт; docs; §1a; Plan 116 forward-refs reconciled → 182.
9. **net `str`→`[]u8` ПОЛНАЯ миграция** (owner-approved, committed/в-scope) — byte-baseline-guarded коммит ПОСЛЕ доказанного HTTP byte-path; старт HTTP не блокирует. Большие/h2-stress тесты вне дефолт-сэмпла (`*_slow.nv`).

---

## 9. Конвенции + координация

**Конвенции (refs):** module-conventions (folder=один модуль `std.http`; effect-ТРИАДА effect+`real_http`+`mock_http`; scalars=value-record D215; resources=must-consume D133 — `Body`; **byte-first** `[]u8`, `str` via fallible; FFI в `ffi.nv` extern "C" path→CStr / data→(*u8,len); логика парсинга/кодеков в .nv не C — nv-sourcing). **D325** (R1 fallible→`Result[T,HttpError]`; R2 bare-имя; R3 `try_` только для from-сиблинга; R4 `Option`=genuine absence; R5 `Fail[E]` запрещён для своих, ок forwarding). **consume** D131/D133/D180. **test-conventions** (EXPECT_*; pos folder-module / neg `neg/`; mock-тест mandatory). conventions-governance: изменения только по согласованию.

**Координировать:**
- **Plan 116 (std/tls)** — 🔴 HARD-GATE Ф.4/Ф.5; `Tls`/`TlsStream consume`/`ClientConfig`/`ServerConfig`/SNI/**ALPN** (D210-D213). Параллельно Ф.0-Ф.3. **Reconcile forward-refs** (117/122 → 182, §6).
- **Plan 173** — supervised scope (server), per-conn spawn+bounded Semaphore, `deadline:`/`timeout:` (client+h2-stream+drain), `defer` always, MultiError, cancel→net-park; **Q29 acquire cancel-aware** verify; **Q25 else_timeout/match** на пойманном Timeout verify.
- **Plan 179 (Time)** — `Timestamp`/`Duration`/`Instant` (deadline/cookie-expiry/idle); verify конструкторы (`Duration.seconds`/`Instant.now`).
- **Plan 181 (D325)** — Result-everywhere (conformant by-construction).
- **net-семейство** — byte-surface-амендмент; triad+layered (как 116 над net).
- **Plan 103.3/103.4** — Mutex (h2 conn-state/cookie-jar) / Semaphore (server bound + h2 MAX_CONCURRENT_STREAMS).
- **Plan 180 (fs/io)** — `Body.copy_to`/download-to-file + multipart-File-part + cert-from-file гейтят на fs; `impl io.Write`/`io.Read` для `Conn` — координация.

**⚠ Конвенции/std, МОГУТ потребовать амендмента (owner-sign-off, conventions-governance):**
- **net `str`→`[]u8` surface (Q5)** — `TcpNet.read/write` сейчас `str` ([effect.nv:52-54], half-варианты [tcp.nv:236/277/283]). Необходимость: HTTP-тело — байты; `str` ломается на gzip/binary (нарушает byte-first + §8.0). **Амендмент ОДОБРЕН владельцем 2026-06-26 (ПОЛНАЯ миграция).** Sequencing: byte-surface additive (Ф.0.5, `str` транзитом) → полная миграция `str`→`[]u8` (committed, guarded commit после HTTP byte-path). Governance: тот же owner std/net.
- **`std/encoding/json` + reflective serde (Q20)** — НЕ существует; `json()` gated. Решить: под-план или ручной `to_json/from_json`-протокол MVP.
- **NEW под-план `std/encoding/compress` (Q12, owner 2026-06-26)** — gzip/deflate/brotli НЕ существуют; decompress 🔴 HARD-GATE на этот отдельный под-план (создать ВНЕ 182).
- **`str.from_bytes`** — УЖЕ есть (jwt.nv:75), НЕ HARD-PREREQ — только confirm-step Ф.0.

После большой задачи — обновить `project-creation.txt` + `nova-private/discussion-log.md` + `simplifications.md`.

---

## 10. Фоновые агенты

- **НЕ `git stash`** (worktree делят `.git` → repo-global коллизия/потеря); baseline — **temp-worktree / commit+reset**. Постоянный worktree `nova-p182` (naming `nova-pNN`) первой командой, самозарегистрироваться; cwd сбрасывается в main → **префикс абсолютным путём в каждой команде**; ссылки на файлы worktree — полный абсолютный путь.
- **Rate-limit-устойчивость (фазы resumable/идемпотентны):** коммит после каждой фазы, без amend; малые батчи; `agent()`-null-tolerant — фильтровать выживших, ре-ран упавших; `git add` только конкретные файлы (никогда `-A`/`.`); `git diff --cached --stat` перед commit; без `Co-Authored-By`. Подтверждение перед background-`Agent`.
- **Тесты:** `nova test` — не гейт корректности (byte-baseline), гейт = targeted pos+neg + soundness; полный `nova test` >10мин-cap → батчи <10мин; mass compile-errors (net `[]u8`) → per-file loop. **net/http-тесты с cwd=worktree** (libuv `repo_root=current_dir`); env `NOVA_GC_LIB_DIR`/`INCLUDE_DIR`→main, libuv-submodule из main + удалить `libuv/.git`. **Пересобрать `nova-cli` после правок `.nv`** (`include_str!`). Не выдумывать синтаксис — `spec/decisions/`+`examples/`.

---

## 11. Followup

`[M-182-std-http]`. **Под-планы при росте:** **182.1** (server), **182.2** (h2). Deferred (с rationale, НЕ на критпути h1+https+h2-core):
- **WebSocket upgrade** (`ws://`/`wss://`, RFC 6455 — h1-Upgrade + frame-protocol; java.net.http-паритет-gap).
- **HTTP/3 / QUIC** (gate DTLS/QUIC-транспорт; `[M-116-quic]`/`[M-116-dtls]`; нужен UDP+TLS1.3-в-транспорте).
- **gRPC** (h2 + protobuf + service-codegen) — поверх Ф.5.
- **forward/reverse proxy** (полный CONNECT-tunneling-сервер, header-forwarding; **trusted X-Forwarded-For opt-in** — `peer_addr` за прокси).
- **response caching + conditional** (`If-None-Match`/`If-Modified-Since`/304, RFC 9111).
- **server-push opt-in** (мёртв в браузерах — только если возникнет нужда; V1 = `ENABLE_PUSH=0`).
- **Brotli server-side encode** (client-decode есть; server-encode followup).
- **connection coalescing** (h2: один conn для нескольких origin при cert-match).
- **zstd decompress**, **TLS session resumption** (зависит от 116 followup), **h2c-via-Upgrade** (НЕ реализуем — deprecated; только prior-knowledge), **multipart streaming upload** (large file, fs-gate), **proxy-auth digest**, **cookie-jar persistence** (disk), **non-UTF-8 charset decode** (Shift_JIS/GBK — Q21), **reflective JSON serde** (если станет prereq для `json()`).

Имена/детали — финал при реализации (после Ф.0).