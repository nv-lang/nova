<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 249 — пакет `nova-socks` + пример `examples/net/socks5_http_bridge/`

**Статус:** 📋 НАБРОСОК, **СТАРТ ЗАБЛОКИРОВАН** (реализация НЕ начата). Написан
интегратором 2026-08-04; ревизия док-сессии 2026-08-05 (пины линейности/half-close, форма
package-примера, дом-папка); ревизия интегратора 2026-08-06 — скетч §3.2 приведён к форме,
ПРОВЕРЕННОЙ СБОРКОЙ, добавлен пин Ф.0-г и лимит заголовков.
**§7 ЗАКРЫТ решениями владельца 2026-08-06** (отдельный пакет `nova-socks`; IPv6 → `Err`).
**Условие старта одно:** влитое компиляторное окно **p364**
(`[M-linear-capture-gate-blind-to-consume-let]`, 221.1 №364) — от него зависит Ф.0-г и
формулировка §3.2.
**Приоритет:** P3 (новый пример; не блокер релиза v0.1).
**Источник:** реестр [221.1](221.1-bug-sweep.md) №360, `[M-socks5-client-missing]`
(`docs/plans/backlog-followups.md`) — вопрос владельца о feasibility.

## 0. Мотив

Внешний, реальный сценарий (личный инструмент владельца, детали не публикуются):
провайдер выдаёт прокси по протоколу **SOCKS5 с логином/паролем**; системные настройки
Windows принимают только **HTTP-прокси** и не умеют передавать креды для SOCKS5.
Нужен локальный мост: браузер говорит с ним как с обычным HTTP-прокси, мост сам
переупаковывает трафик в SOCKS5 и подставляет авторизацию.

```
браузер ──HTTP──> 127.0.0.1:PORT ──SOCKS5+пароль──> прокси провайдера ──> интернет
```

**Языковая ценность примера** (независимо от личного сценария): демонстрирует реализацию
чужого бинарного протокола (SOCKS5, RFC 1928/1929) поверх сырого `std.net`, и
двусторонний байтовый relay через `spawn` + `into_split` — класс задач, которого среди
нынешних 12 примеров `nova-polaris` нет вовсе (все они — HTTP-уровень поверх готового
фреймворка).

## 1. Разведка (интегратор 2026-08-04 + док-сессия 2026-08-05)

- **SOCKS5-клиента нет нигде** в экосистеме — грепом подтверждено: `std`, `nova-http`,
  `nova-polaris`, `nova-tls` — ноль носителей.
- **Уже есть, переиспользуется без изменений:**
  - `std.net` (`TcpListener`/`TcpStream`/`Net`-эффект) — сырой TCP;
  - **`TcpStream consume @into_split() -> (TcpReadHalf, TcpWriteHalf)`**
    (`std/src/net/tcp.nv:320-384`) — половины по доке «могут работать в разных
    файберах»; двусторонний relay выразим в линейной модели БЕЗ доработки std:
    файбер A владеет `client.read + upstream.write`, файбер B — зеркальной парой;
  - `TcpStream mut @shutdown()` (`std/src/net/tcp.nv:246`) — есть; семантика
    close половинок — пин Ф.0 (см. §4);
  - `std.os`: `env(key) Os -> Option[str]`, `args() Os -> []str` — источник
    конфига моста (адрес прокси, креды, порт listen);
  - `Method.Connect` — готовый вариант `Method`-enum (`nova-http/src/method.nv:16`);
  - `ServerResponse.upgrade(fn(TcpStream) Net -> ())` (`nova-polaris/src/ws_upgrade.nv:
    110-115`) — ОБЩИЙ (не WS-специфичный) хук «захвата сокета после ответа»; структурно
    то, что нужно CONNECT-туннелю, НО...
- **Загвоздка, определяющая архитектуру:** `polaris.Router`/серверный парсер НЕ понимает
  CONNECT-цель (authority-form `host:port`, RFC 7230 §5.3.3 — без схемы и пути);
  `ServerRequest.@url()` парсит только обычный `Url`. Значит `upgrade()`-хук через Router
  подключить нельзя без доработки самого фреймворка (вне объёма примера) — **пример
  ОБЯЗАН идти поверх сырого `std.net`**, минуя `polaris` целиком.
- **Дом — ДВА артефакта (решение владельца 2026-08-06):**
  1. **Новый пакет `nova-socks`** (репа `d:/Sources/nv-lang/nova-socks`, зеркала ×3) —
     сам SOCKS5-клиент. Обоснование прецедентами: **никто не кладёт SOCKS5 внутрь
     HTTP-библиотеки** — Go `golang.org/x/net/proxy`, Rust `tokio-socks`/`socks`
     (reqwest подключает фичей `socks`), Python `PySocks` (`requests[socks]`),
     Node `socks` + `socks-proxy-agent` — все держат ОТДЕЛЬНО и связывают
     опционально; внутри платформы только Java/.NET (толстая платформа, не наш случай).
     Nova по устройству — тонкое ядро (`tls`/`compress` уже вынесены по этому же
     принципу, обоснование в манифесте `nova-http`), поэтому std отпадает.
     SOCKS5 проксирует ЛЮБОЙ TCP, не только HTTP — класть в `nova-http` значило бы
     тащить весь HTTP тому, кому нужен голый туннель.
  2. **Пример** — под-папка `examples/net/socks5_http_bridge/` (репа `nova`):
     `main.nv` + `README.md`/`README.ru.md`; зависит от пакета `nova-socks`.
     Соседние `echo_client.nv`/`echo_server.nv` остаются одиночными файлами —
     прецедент оформления README, не структуры. **НЕ** `nova-polaris/examples/`.

## 2. Объём V1

SOCKS5-клиент (CONNECT-команда, username/password auth, IPv4/domain-адреса — IPv6
опционально) + HTTP-мост (CONNECT-туннель — основной путь; обычный HTTP через прокси —
вторичный, как в мотивирующем сценарии). **НЕ входит:** SOCKS5-сервер (только клиент);
SOCKS4/4a; GSSAPI-аутентификация (RFC 1961, редкая в проде); UDP ASSOCIATE.

## 3. Дизайн

Тело — под **отдельный пакет** (решение владельца 2026-08-06, §7 в.1): поверхность
`export`-ная, как у `tls`/`compress`; пример — обычный внешний потребитель.

### 3.1 Пакет `nova-socks` — SOCKS5-клиент

**Скелет пакета — по образцу `nova-compress`** (самый близкий по размеру; сверить
пофайлово при создании): `nova.toml` (`[package] name = "socks"`, `[lib] src = "src"`
— module-путь БЕЗ `src`, D78), `src/socks5.nv` + пир `src/socks5_test.nv`,
`README.md`/`README.ru.md`, `LICENSE-MIT`/`LICENSE-APACHE`, `.gitignore`,
`scripts/githooks`. Нативных артефактов НЕТ (чистая Nova поверх `std.net`) — секция
`[ffi]` не нужна, это проще и tls, и compress.

**Зеркала ×3 и релиз-скоуп:** репа заводится на github+gitverse+sourcecraft (правило
трёх зеркал). **В скоуп тега v0.1.0 НЕ входит** — план P3, не блокер релиза; тег
пакета — отдельным решением после того, как появится первый стабильный потребитель
(`[M-178-client-policy-surface]`, прокси у `HttpClient`, — естественный кандидат).
Строку в план 221 про состав тегов НЕ трогать без отдельного слова владельца.

```nova
// Канон D406: без ведущего `|` (снятая форма тихо проскакивает — не переносить её в код)
type SocksError enum ConnectFailed { reason str }
    | AuthFailed
    | AuthRequired            // сервер требует auth, креды не даны
    | UnsupportedMethod
    | HostTooLong             // domain-адрес > 255 байт (протокольный лимит)
    | UnsupportedAddressType  // ATYP 0x04 (IPv6) — решение §7 п.2: честный Err, не разбор
    | GeneralFailure { code u8 }   // REP-код сервера, не наш (D30-conventions: код в поле)
    | Protocol { detail str }

export fn socks5_connect(
    proxy_host str, proxy_port int,
    user Option[str], pass Option[str],
    target_host str, target_port int
) Net -> Result[TcpStream, SocksError]
```

Реализация — прямой перевод RFC 1928 (version/methods negotiation → auth
sub-negotiation RFC 1929, если сервер выбрал `0x02` → CONNECT-команда → разбор
bound-адреса ответа) в последовательные `read`/`write` на уже открытом
`TcpStream`. Без крипто — чистое byte-level message framing.

**Тестируемость закладывается в структуру:** сборка/разбор каждого
handshake-сообщения — **чистые encode/decode-функции над `[]u8`** (без `Net`),
тестируемые байтовыми фикстурами напрямую; сетевой слой (`socks5_connect`) —
тонкая обвязка «прочитай N байт → decode → encode → запиши». Реальный
SOCKS5-сервер для тестов не нужен. Заодно решается вопрос фиксированных длин:
один helper `read_exact(stream, n)` поверх `@read` (у `@read_bytes(max)`
семантика «до max», для протокольных полей нужен точный N).

### 3.2 Мост — `main.nv`

Конфиг из окружения/аргументов (`std.os`, эффект `Os`): `SOCKS5_PROXY`
(`host:port`), `SOCKS5_USER`/`SOCKS5_PASS` — через `env()`; порт listen —
первым аргументом `args()` (по умолчанию 8899).

```nova
fn main() Net Os Time -> () {
    ro cfg = load_config()                       // env + args, валидация на старте
    consume listener = TcpListener.bind(SocketAddr.loopback(cfg.listen_port))!!
    supervised {                                 // D50: spawn ТОЛЬКО внутри structured-scope
        loop {
            match listener.accept() {
                // ФОРМА ОБЯЗАТЕЛЬНА: биндинг приходит из match-АРМА и уходит в
                // файбер `spawn consume` (D415 §4). ВНЕШНИЙ `consume client = …` +
                // голый spawn: до №364-фикса падал кодогеном (симптом дыры гейта),
                // ПОСЛЕ №364 — честно красный E_LINEAR_CAPTURE_IN_FIBER на проверке.
                // Причина формы теперь — сам гейт, не codegen-маркер (пере-пин Ф.0-г).
                Ok(consume conn) => spawn consume conn { handle_client(conn, cfg) }
                Err(_)           => ()           // политика на Ф.2: лог и продолжить
            }
        }
    }
}

fn handle_client(consume client TcpStream, cfg Config) Net Time -> () {
    // ЛИМИТ ОБЯЗАТЕЛЕН: чтение до 

 без потолка — DoS-вектор (клиент шлёт
    // бесконечные заголовки, мост ест память). Превышение → 431 Request Header
    // Fields Too Large и закрыть, не молча обрывать.
    ro head = read_headers(client, MAX_HEADER_BYTES)?   // напр. 64 КБ
    match parse_request_line(head) {
        Connect(host, port) => match socks5_connect(cfg.proxy_host, cfg.proxy_port,
                                                    cfg.user, cfg.pass, host, port) {
            Ok(consume upstream) => {
                client.write_all("HTTP/1.1 200 Connection Established\r\n\r\n")!!
                pipe_bidirectional(client, upstream)
            }
            Err(_) => { ro _ = client.write_all("HTTP/1.1 502 Bad Gateway\r\n\r\n") }
        }
        Plain(host, port, rewritten) => { /* аналогично: connect → forward → pipe */ }
    }
}
```

**`pipe_bidirectional(consume a TcpStream, consume b TcpStream)`** — ядро
примера, владение раскладывается по `into_split`: `(ar, aw) = a.into_split()`,
`(br, bw) = b.into_split()`; файбер №1 качает `ar → bw`, файбер №2 — `br → aw`
(каждый — `spawn consume … { … }`, D415 §4).
Каждый файбер владеет своей парой половинок — линейность соблюдена без
разделяемого состояния. По EOF направление закрывает СВОЮ write-половину
(проброс FIN — пин Ф.0-а) и завершается; механизм завершения второго файбера —
пин Ф.0-б.

**Отличия от типового скрипт-прототипа этого класса задач (осознанные архитектурные
решения, не копирование один-в-один):**
- upstream-коннект **до** ответа клиенту, не после — 502 при провале естественный, без
  гонки «уже отправили 200, а сервер недоступен»;
- `pipe_bidirectional` — два `spawn` поверх `into_split`, не
  callback-ориентированный event-loop;
- ретраи на временную недоступность upstream (провайдеры нередко ротируют IP) —
  через `Time.sleep`, тем же паттерном, что остальные примеры используют для
  deterministic-тестируемости (`with Time = th.fixed_ms(...)` в тестах).

**Сознательно НЕ переносится:** список «шумных» доменов-трекеров, которые
скрипт-прототип не пишет в лог — это специфика конкретного internet-сценария, не
языковая демонстрация.

## 4. Фазы

- **Ф.0 — пины feasibility (ворота всего плана, пробой на компиляторе):**
  - **(а) half-close:** делает ли `TcpWriteHalf.consume @close()` реальный
    `shutdown(SHUT_WR)` (FIN уходит пиру, read-половина продолжает читать) — или
    просто роняет handle? **Уточнено 2026-08-06:** у половинок вообще НЕТ
    `@shutdown()` — только `consume @close()` (`tcp.nv:354/378`), т.е. это
    единственный кандидат и пин обязателен. Дока (`tcp.nv:376-378`: «closes the socket if the read
    half is gone») это не фиксирует; смотреть C-шим `net_tcp_shutdown`/close.
    Без проброса FIN туннели будут подвисать на протоколах, закрывающих
    соединение односторонне. Если shutdown-write нет — мини-доработка `std.net`
    (отдельный коммит, канал std, не обход в примере).
  - **(б) завершение пары файберов:** направление A получило EOF и закрылось —
    как завершить направление B, висящее в блокирующем `read`? Кандидаты:
    structured scope + cancel; либо естественное завершение по FIN из (а).
    Зафиксировать рабочий механизм пробой (два файбера, реальный loopback).
  - **(в) формы:** error-enum `SocksError` (D30-конвенция, канон D406 без
    ведущего `|`) компилируется; `read_exact`-helper поверх `mut @read`
    работает как ожидается.
  - **(г) пере-пин после №364 (гейт линейного захвата ужесточён):** весь
    скелет §3.2 прогнать пробой на компиляторе С влитым №364-фиксом —
    `consume listener = …` без аннотации теперь классифицируется по типу
    инициализатора; убедиться, что supervised-scope не считается захватом в
    файбер и пример остаётся зелёным, а внешний-биндинг-форма даёт именно
    `E_LINEAR_CAPTURE_IN_FIBER` (обновить комментарий §3.2, если текст
    диагностики другой). Судьба `[M-consume-param-spawn-defer-active]` — по
    отчёту окна p364.
- **Ф.П — создание пакета `nova-socks`** (отдельным коммитом, ДО кода): скелет по
  образцу `nova-compress` (§3.1), три зеркала, `nova.toml`, лицензии, README-пара,
  `.gitignore`, githooks. Приёмка: пустой пакет собирается (`nova check src`),
  зеркала сверены `ls-remote` (правило трёх зеркал — расходятся молча).
- **Ф.1** — `src/socks5.nv`: чистые encode/decode + обвязка `socks5_connect`;
  `src/socks5_test.nv` — байтовые фикстуры handshake-обменов, сверенные с текстом
  RFC 1928/1929 (не по памяти); фикстура на IPv6-ATYP → `Err(UnsupportedAddressType)`.
- **Ф.2** — мост (`examples/net/socks5_http_bridge/main.nv`): подключить `socks`
  в `examples/nova.toml` (git+semver по D420; локальная разработка — через
  `nova.override.toml`, НЕ `[replace]` в манифесте), конфиг из `Os`, accept-loop,
  парсинг первой строки (с `MAX_HEADER_BYTES`), CONNECT-путь, `pipe_bidirectional`.
- **Ф.3** — plain-HTTP-путь (переписывание absolute-URI → origin-form, срез
  `Proxy-*`-заголовков) — вторичный, можно отложить за Ф.2 отдельным коммитом.
- **Ф.4** — README (EN+RU, по конвенции examples) — что показывает, как
  запустить (переменные окружения из §3.2), честная пометка «нужен реальный
  SOCKS5-прокси для ручной проверки, не входит в CI-гейт».

## 5. Гейты

**Пакет `nova-socks`:** `nova check src` + `nova test src` (тесты пиром,
`socks5_test.nv` — мок-байты + loopback для Ф.0-проб, внешней сети ноль) +
`nova lint` (ритуал приёмки .nv-волн) + пуш на три зеркала со сверкой `ls-remote`.
**Пример:** тесты примера гоняются `nova test` по его папке. Вся
папка компилируется `--strict-effects` (конвенция examples). **Ручной smoke —
вне CI** (нужен реальный внешний SOCKS5-прокси, недоступен в автоматическом
прогоне) — задокументировать это ограничение явно в README примера, не заявлять
«протестировано end-to-end». Если Ф.0-а потребует доработку `std.net` — на неё
дополнительно обычный std-гейт (`nova test std`).

## 6. Риски

| Риск | Митигация |
|---|---|
| Half-close не пробрасывает FIN (Ф.0-а) — туннели подвисают | пин до старта волны; при отсутствии — мини-доработка std.net отдельным коммитом, не обход в примере |
| SOCKS5 handshake — точная байтовая раскладка, легко ошибиться в порядке полей | чистые encode/decode + юнит-тест на КАЖДЫЙ шаг протокола отдельно, фикстуры сверены с RFC 1928/1929 текстом, не с памятью |
| Безлимитное чтение заголовков — DoS по памяти | `MAX_HEADER_BYTES` в `read_headers` (§3.2), превышение → 431 + закрытие; фикстура на превышение лимита обязательна в Ф.2 |
| Файбер-утечка: зависшие соединения без таймаутов чтения живут вечно | для примера допустимо (V1 без таймаутов), но честно назвать в README как известное ограничение; проверка руками в smoke |
| `Router`/`polaris` эволюционирует и научится CONNECT — пример устареет архитектурно | не блокирует V1; ревизия при появлении authority-form в polaris — отдельный follow-up |
| Ручной smoke невозможен в CI | честно задокументировано (§5), не выдаётся за протестированное |

## 7. Решения владельца (закрыто 2026-08-06)

1. **Дом — ✅ РЕШЕНО: отдельный пакет `nova-socks`** (владелец 2026-08-06, «ДА» на
   вариант (а)). Рассмотрены и отклонены: `std.socks` (против устройства «тонкое
   ядро» — прецедент манифеста `nova-http`), внутрь `nova-http` (против ВСЕХ
   индустриальных прецедентов — Go/Rust/Python/Node держат SOCKS5 отдельно от
   HTTP-библиотеки; SOCKS5 проксирует любой TCP), внутрь примера (потерялся бы
   реальный потребитель — прокси у `HttpClient`). Инфраструктурная цена (репа +
   3 зеркала + CI) принята владельцем осознанно, ДО тега.
2. **IPv6 — ✅ РЕШЕНО: НЕ реализовывать в V1** (владелец 2026-08-06, «ДА»).
   Address type `0x04` → `Err(UnsupportedAddressType)`; домен + IPv4 покрывают
   мотивирующий сценарий. Вариант enum'а `SocksError` (§3.1) дополнить этим
   членом; фикстура на честный `Err` обязательна (не молчаливый неверный разбор).

## Связи

Пакет-образец `nova-compress` (скелет; `nova-tls` — тот же extraction-shape) ·
`[M-178-client-policy-surface]` (nova-http: прокси у `HttpClient` — будущий второй
потребитель) · прецеденты индустрии: Go `x/net/proxy`, Rust `tokio-socks`/reqwest
`socks`-feature, Python `PySocks`/`requests[socks]`, Node `socks`+`socks-proxy-agent` ·
реестр [221.1](221.1-bug-sweep.md) №360 / `[M-socks5-client-missing]`
(`backlog-followups.md`) · `std/src/net/tcp.nv` (`into_split`/половины —
несущая конструкция relay; `@shutdown`) · `std/src/os/os.nv` (`env`/`args` —
конфиг моста) · `nova-http/src/method.nv` (`Method.Connect`) ·
`nova-polaris/src/ws_upgrade.nv` (`ServerResponse.upgrade` — прецедент хука, не
переиспользуется напрямую из-за загвоздки §1) · `examples/net/` (категория-дом;
`echo_client.nv`/`echo_server.nv` — прецедент оформления README) · RFC 1928
(SOCKS5) · RFC 1929 (username/password auth) · RFC 7230 §5.3.3 (authority-form
request-target).
