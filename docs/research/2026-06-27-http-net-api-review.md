# Консолидированное ревью API: std/net + std/http (Plan 178)

## Вердикт по Q1–Q5

**Q1 — Самый удобный HTTP-API среди 7 пиров?** Да, на high-level client+server-поверхности. Это builder-эргономика reqwest + serve/mux axum/Go-1.22, но с тремя вещами, которых нет ни у одного пира: (1) **must-consume body превращает #1 footgun keep-alive/body-leak в COMPILE-ошибку**, (2) **non-consuming `error_for_status()`** (лучше reqwest, который consume'ит self и теряет body на error-пути), (3) **встроенный mock-триад** (`mock_http().on(...)`) — socket-free, compiler-checked, MANDATED. Плюс secure-by-default дефолты (timeout 30s, redirect limited(10), gzip/br/deflate on, ssrf_guard), которые бьют Go zero-Client (no timeout), JDK11 (no gzip, redirects NEVER), fetch (no timeout). **Отстаёт** в двух местах: типизированный `.json[T]`/`multipart` гейтнуты за неприземлёнными суб-планами (Ktor Codable, Swift Codable, fetch `res.json()` доступны СЕГОДНЯ); и скрипт-эргономика — `with Http = real_http()` многословнее `await fetch`/`http.Get`.

**Q2 — Самосогласованность и соответствие лучшим традициям?** В основном да, но есть реальные warts. http-слой внутренне когерентен (Result-everywhere, must-consume на всех ресурсах, byte-first). HIGH-нестыковка — **шов net↔http**: транспорт http (`Conn`, D327/Ф.0.5) ТРЕБУЕТ публичную `[]u8` byte-поверхность (`read_bytes/write_bytes`), которой std/net пока НЕ экспортирует. net — str-first, http — byte-first, поэтому «http строится на net» пока не 1:1.

**Q3 — Корректность структуры?** Да в целом. Валидированные value-newtype'ы с `parse->Result` (Method/StatusCode/HeaderName/HeaderValue/Mime/Url), consume-линейные ресурсы, чистый client/server-сплит, тонкий effect-seam. Структурные пятна: `error_for_status` non-consuming на фоне consume-сиблингов; стек `HttpResponse→Body→BodyReader` дублирует словарь консьюмеров; имя `Request` покрывает и клиент-built, и server-received; str-first read/write в net — неверный примитив для байтового транспорта.

**Q4 — Имена = смысл И современная практика?** В основном да: `bytes/text/json`, `get/post/...`, `bind/serve`, `accept/connect/recv_from/send_to`, `insert(replace)/append(add)` читаются верно. Реальные mismatch'и: (1) net `@read` возвращает `str` (все пиры — `[]u8/Data/[]byte/Buffer`); (2) `RequestBuilder @header` APPEND'ит, но OkHttp/Swift `.header()/setValue` REPLACE'ят — тихий footgun; (3) разнобой discharge-глаголов; (4) `#stable` `set_nodelay/set_keepalive/set_reuse_address` — тихие no-op заглушки.

**Q5 — Знаком ли алгоритм использования?** Да для happy-path: `send()?.error_for_status()?.json[T]()?` и `bind().serve(mux)` ложатся на reqwest/axum/Ktor/Go. Незнакомое: `with Effect = real_x()` (ни у кого нет ambient-capability церемонии), EOF-as-Err против `Ok(0)`/`io.EOF` (Rust/Go/Zig-циклы ломаются на zero-read), `supervised{spawn{}}` для raw DNS, `split()` для конкурентного R/W. Всё защитимо, но требует shortcut'ов + громких доков.

---

## Что сделано ХОРОШО (на уровне пиров или выше) — НЕ ТРОГАТЬ

- **Must-consume линейные body** (`Body`/`HttpResponse`/`ResponseWriter`/`Request`/`Conn`): забыть слить/закрыть body — #1 footgun reqwest (Drop тихо роняет), Go (`defer resp.Body.Close()`), OkHttp, undici — здесь COMPILE-ошибка. Double-read = use-after-consume. Ни один пир не enforce'ит. **Headline-победа во всех 7 отчётах.**
- **`error_for_status()` non-consuming**: на `Err(Status)` Body жив, caller обязан слить. Строго лучше reqwest.
- **Валидированные value-newtype'ы**: `StatusCode.new` (отвергает вне 100..599), `HeaderValue.from_bytes` (отвергает CR/LF/NUL — anti-injection), `Method.parse`, `Url.parse`. Дисциплина http-crate, идиоматично.
- **mock-триад**: `real_http()/mock_http()` + server-вариант, программируемый `mock_http().on(method,path,|req|->MockResponse)`. Бьёт wiremock/httpmock, httptest/RoundTripper, nock/MockAgent, Ktor MockEngine/OkHttp MockWebServer, URLProtocol-stubbing.
- **Secure-by-default клиент**: timeout 30s, connect_timeout 10s, redirect `.limited(10)`, gzip/br/deflate on, pool 32/90s (Go DefaultTransport), `ssrf_guard(deny_private)`, `danger_accept_invalid_certs` (кричащее имя).
- **Verb-builder** `get/post/...header().json().send()` + `HttpResponse status/headers/bytes/text/json`: reqwest 1:1; эргономичнее JDK11 BodyHandlers / Go NewRequest / raw NWConnection.
- **Go-1.22 server**: `ServeMux` «METHOD /path/{param}»/«{name...}» (longest-wins, HEAD→GET, 404/405+Allow), `handler_fn`, onion `chain([...],h)` + `mw_*`, `serve_graceful(handler, CancelToken, drain)`. Превосходит stdlib Go/JDK/Swift/Zig.
- **`Http` инкапсулирует `Tls/TcpNet/DnsNet/Time`** (Q30): http↔https version-transparent. Sync-looking-over-fibers — async/await-читаемость без function-coloring (поглощает Ktor suspend, Swift async).
- **`Result[T, HttpError]` + OPEN `ErrorKind` + source-chaining**: лучше Go-строк, JDK IOException-сплита, Swift NSURLError-кодов. 4xx/5xx-as-valid = fetch `res.ok`/Ktor `isSuccess`.
- **First-class UDP + async DNS возвращает ВСЕ адреса** (`lookup -> []SocketAddr`): строгий суперсет над Zig std.net (нет UDP), как `getAddressList`.

---

## Проблемы по приоритету

### HIGH

**1. net byte-поверхность отсутствует / str-first примитив (layering + naming).** net `@read(max)->str`/`@write(str)` на байтовом транспорте подразумевает UTF-8/latin1-валидность, которой TCP/UDP не дают. `[]u8`-поверхность (`read_bytes/write_bytes/write_all_bytes`) есть лишь как внутренний C FFI + ОТЛОЖЕННЫЙ §3.10/D327/Ф.0.5, но `Conn` http её ТРЕБУЕТ. http byte-first (`Body.from_bytes`, `RequestBuilder @body []u8`), net str-first — слой не 1:1. _Единогласно все 7 пиров + self-consistency HIGH._ **Рекомендация:** приземлить `@read_bytes(max)->Result[[]u8,NetError]`, `@write_bytes([]u8)`, `@write_all_bytes([]u8)` на TcpStream/обе половины + байтовые `send_to/recv_from` на UdpSocket как ПЕРВИЧНУЮ поверхность ПЕРЕД/как фаза 1 http; str-варианты — fallible-обёртки `@read_str/@write_str` или удалить.

**2. Церемония `with`-скоупа для common-path.** Всё I/O за `with TcpNet=real_tcp_net(){}`/`with Http=real_http(){}`; даже чистый `SocketAddr.loopback(0)`/`@port()` несёт `AddrNet`. Ни у кого нет ambient-capability церемонии. _Все 7 пиров (high у JDK/Kotlin)._ **Рекомендация:** (1) umbrella `real_net()` (AddrNet+TcpNet+UdpNet+DnsNet сразу), `with Http = real_http()` как ЕДИНЫЙ http-вход; (2) сделать чистые `AddrNet`-аксессоры (`loopback/v4/@port/@ip/@to_str`) effect-free/ambient — они без I/O и parking; (3) default-installed real_http, чтобы `http.get(url)` работал в скриптах без видимого with-блока. В доках — рамка «seam = compiler-enforced test-DI».

### MED

**3. Разнобой discharge-глаголов.** Четыре глагола под один концепт «release linear resource»: net+Conn+BodyReader → `@close()`; Body → `@discard()`; HttpResponse/Request → `@drain()`; ResponseWriter → `@finish()`. `Body.@discard` ≈ `HttpResponse.@drain` (near-synonyms на смежных слоях). Стек `HttpResponse→Body→BodyReader` дублирует `bytes/text/json/copy_to` со сдвигом discharge-имён на 3 уровнях. _Self-consistency (2× MED)._ **Рекомендация:** таксономия в conventions: `@drain` везде для release-без-материализации (свернуть `Body.@discard` в `@drain`); `@close` только для conn/reader-teardown; `@finish` ТОЛЬКО для ResponseWriter (пишет chunked-terminator); `@bytes/@text/@json` — единственные материализующие консьюмеры на всех слоях.

**4. `RequestBuilder @header` APPEND'ит (vs OkHttp/Swift REPLACE), нет set-варианта.** OkHttp `.header()` REPLACE / `.addHeader()` append; Swift `setValue` REPLACE / `addValue` append. «Set this header» → тихая дупликация. `HeaderMap` корректно различает `insert/append` — builder теряет. _Kotlin (med) + Swift (med)._ **Рекомендация:** добавить `RequestBuilder @set_header(name,value)` рядом с `@header`; либо `@header`=replace + `@add_header`=append.

**5. `error_for_status()` non-consuming ломает consume-паттерн.** Все терминальные методы HttpResponse (`@bytes/@text/@json/@body/@copy_to/@drain`) — consume; `@error_for_status()` — нет (forward self на 2xx/3xx; на Err Body жив). Имя читается как терминальная проверка. _Self-consistency (med); семантика верна и лучше reqwest._ **Рекомендация:** оставить семантику; сигнатура должна явно возвращать тот же `consume HttpResponse`, доки — «guard, forwards self, НЕ освобождает Body». Опц. `error_for_status_drained()`.

**6. EOF-as-Err vs `Ok(0)`/`io.EOF`.** `@read` → `Err(NetError.Eof)` на close; Rust `Ok(0)`, Go `(0, io.EOF)`, Zig `readNoEof`. Идиоматичные copy-циклы пиров ветвятся на zero-read. _Rust/Go (med)._ **Рекомендация:** оставить `Err(Eof)` (композится с `?`/`!!`), но громко задокументировать дивергенцию на `@read` + канонический цикл `match read { Ok(d)=>…, Err(Eof)=>break, Err(e)=>fail }`.

**7. `#stable` socket-опции — тихие no-op.** `set_nodelay/set_keepalive/set_reuse_address` `#stable(since="0.1")`, но options handling WIP (Ф.6) — фактически no-op. Go/Zig setsockopt работают. _Go/Zig (med)._ **Рекомендация:** реализовать setsockopt до `#stable`, либо снять `#stable`/сделать fail-or-warn.

**8. `connect/bind` только resolved SocketAddr; DNS требует `supervised{spawn{}}`.** Пиры резолвят прозрачно: tokio `ToSocketAddrs`, `net.Dial('tcp','host:port')`, `InetAddress.getByName`. _Rust/JDK (med)._ **Рекомендация:** string-overload'ы `TcpStream.connect('host:port')`/`TcpListener.bind('host:port')` с внутренним DNS (протокол `IntoSocketAddr`); convenience для one-shot lookup без ручного spawn, либо DnsNet в umbrella `real_net()`.

**9. Нет stream-адаптеров.** Нет `read_to_end/read_exact`, buffered line reader, accept-stream; TcpStream не impl `io.Read/io.Write` — каждый протокол вручную пишет read-цикл (`Body.copy_to` уже целит `impl io.Write`). _Rust/Go/Zig (med)._ **Рекомендация:** добавить `read_to_end(max)/read_exact(n)`, buffered line reader, accept-iterator на TcpListener; impl `io.Read/io.Write` для TcpStream/половин.

**10. Типизированный `.json[T]`/`multipart` гейтнуты.** В surface есть, но за неприземлёнными serde (Q20)/Plan 176. Ktor `body<T>`, Swift Codable, fetch `res.json()` — СЕГОДНЯ. _Kotlin/Swift/JS (med)._ **Рекомендация:** sequencing — приоритизировать landing `json[T]` (RequestBuilder/HttpResponse/Request/Response) + `Multipart`; до того сделать динамический `.json()->JsonValue` first-class one-call.

### LOW

**11. Free one-shot узки + bytes-only POST.** Только `http.get/post(url,[]u8)/head`; `post` без content-type (риск octet-stream), нет put/delete/patch/JSON one-shot. _JS/Kotlin/Swift (low)._ **Рекомендация:** добавить `http.put/delete/patch` или единый `http.fetch(method,url,opts)`; `http.post_json[T]` или optional content-type.

**12. `split()` для конкурентного R/W; нет `close_read/close_write`.** Go шарит `*net.TCPConn` без split + есть `CloseRead/CloseWrite`. _Go (med)/Swift/JDK (low)._ **Рекомендация:** задокументировать (consume-ownership → split, как safe-альтернатива шарингу); добавить `close_read/close_write`.

**13. Имя `Request` покрывает client-built и server-received.** Клиент-built `Request` (opaque) и server `Request` (богатые `@path/@param/...`) — одно имя, разные роли. _Self-consistency (low)._ **Рекомендация:** если один тип — задокументировать, что rich-аксессоры server-context-only; если разные — переименовать клиентский в `OutgoingRequest`/`BuiltRequest`.

**14. `HttpServer.bind()` не байндит сокет; `NetError.InvalidPort` нет в doc-comment.** `bind()` возвращает config — реальный listen в `serve()` (AddressInUse всплывает в `serve()`). error.nv doc-comment перечисляет 13 вариантов, в enum'е 14 (`InvalidPort` пропущен). _Go (low) + self-consistency (low)._ **Рекомендация:** переименовать в `config(addr)`/`new(addr)` ИЛИ задокументировать «AddressInUse в serve()»; добавить `InvalidPort` в doc-comment.

---

## Top-изменения (по убыванию impact)

1. **Приземлить публичную `[]u8` byte-поверхность в net** (`@read_bytes/@write_bytes/@write_all_bytes` + байтовый UDP) как ПЕРВИЧНУЮ ДО/как фаза 1 http; str → fallible-обёртки. Чинит HIGH layering-gap (D327/Ф.0.5) + единогласный «sockets are bytes».
2. **Свернуть `with`-церемонию**: umbrella `real_net()`, effect-free `AddrNet`-аксессоры, default-installed real_http для `http.get(url)`. Закрывает крупнейшую дивергенцию от всех пиров.
3. **Одна discharge-таксономия**: `@drain` для release-без-материализации на всех слоях (свернуть `Body.@discard`), `@close` только conn/reader, `@finish` только ResponseWriter.
4. **`RequestBuilder @set_header` (replace)** рядом с `@header` (append) — match OkHttp/Swift.
5. **Приземлить `json[T]` + `Multipart`**; динамический `.json()->JsonValue` first-class. Главное место, где http ОТСТАЁТ.
6. **String-addr overload'ы** `connect('host:port')`/`bind('host:port')` + one-shot lookup без `supervised{spawn{}}`.
7. **`error_for_status()`**: сигнатура явно `consume HttpResponse`, доки «не освобождает Body»; опц. `error_for_status_drained()`.
8. **Доделать или снять `#stable`** с no-op socket-опций.
9. **Stream-адаптеры** (`read_to_end/read_exact`, buffered lines, accept-stream) + `io.Read/io.Write`; громко задокументировать EOF-as-Err с каноническим циклом.