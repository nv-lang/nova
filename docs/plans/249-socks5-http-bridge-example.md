<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 249 — пакет `nova-socks` + пример `examples/net/socks5_http_bridge/`

**Статус:** 🚧 ЧАСТИЧНО ИСПОЛНЕНО (2026-08-06, окно p249-socks-package).
**Ф.П/Ф.1/Ф.4 — ГОТОВО**: новая репа `nova-socks` (github+gitverse+sourcecraft
remotes подготовлены, пуш ждёт слова владельца), `src/socks5.nv` (SOCKS5-клиент,
RFC 1928 CONNECT + RFC 1929 auth) + `src/socks5_test.nv` (22 байтовых
фикстур-теста, зелёные), README EN+RU. Коммиты (репа `nova-socks`, ветка
`main`): `35f45c2` (Ф.П скелет), `48ca093` (Ф.1 клиент), `52749cf` (Ф.4 README).
**Ф.2 (мост)/Ф.3 (plain-HTTP) — ОСТАЮТСЯ ЗАБЛОКИРОВАНЫ**, вне объёма этого окна
(ждут `[M-73.1-destructure]`/`[M-consume-param-spawn-defer-active]`, см. ниже).
Написан интегратором 2026-08-04; ревизия док-сессии 2026-08-05 (пины
линейности/half-close, форма package-примера, дом-папка); ревизия интегратора
2026-08-06 — скетч §3.2 приведён к форме, ПРОВЕРЕННОЙ СБОРКОЙ, добавлен пин
Ф.0-г и лимит заголовков.
**§7 ЗАКРЫТ решениями владельца 2026-08-06** (отдельный пакет `nova-socks`; IPv6 → `Err`).
**Блокеров старта НЕТ** (уточнено 2026-08-06 по вопросу владельца — прежняя формулировка
«старт заблокирован» была перетянута):
- **Ф.П, Ф.1, Ф.4 — начинать можно немедленно**, внешних зависимостей нет: пакет/скелет,
  сам SOCKS5-клиент (последовательный код, ни `spawn`, ни захвата в файбер) и README.
- **Ф.2 (мост) — ЖЁСТКО ЗАБЛОКИРОВАН (пере-пин Ф.0-г исполнен 2026-08-06 ПОСЛЕ вливания
  №364).** Accept-часть (`match`-арм + `spawn consume conn` + `supervised`) — зелёная,
  проверена сборкой. **Но ядро `pipe_bidirectional` СЕГОДНЯ НЕ ВЫРАЗИМО:** каждому файберу
  нужны ОБЕ половинки `into_split()`, а `spawn consume` двигает РОВНО ОДНО значение —
  вторая считается захватом и (правильно) отвергается `E_LINEAR_CAPTURE_IN_FIBER`.
  Проверены и отвергнуты все обходы: `spawn consume a, b { … }` не парсится (один идент,
  `parser/mod.rs:10905`); `consume w = bw` внутри тела — всё ещё захват; вложенный
  `consume bw { … }` — `E_CONSUME_BLOCK_NOT_OWNED` + `E_CONSUME_BLOCK_MOVE_OUT` (блок даёт
  ро-вью, не владение); `@share()` — не та семантика (refcount-алиас вместо эксклюзивных
  половин). Единственная проходящая ЧЕКЕР форма — упаковать пару в один `consume`-тип
  (`type Leg consume { consume r TcpReadHalf; consume w TcpWriteHalf }`), и ровно она бьётся
  в открытый codegen-баг `[M-consume-param-spawn-defer-active]` (`_defer_N_M_active`
  undeclared) — репро сохранено: `docs/plans/repro/m_consume_param_spawn_defer_active_tcp.nv`.
  **ДВА ЯЗЫКОВЫХ БЛОКЕРА СНЯТЫ (проверено сборкой интегратора 2026-08-06 на пакете `7ed38407d`):** №378 (`consume (ar, aw) = a.into_split()`) и №379 (`spawn consume ar, bw { … }`) влиты; форма relay из §3.2 проходит `nova check` И `nova build --strict-effects` до бинаря — та самая, что до пакета падала кодогеном (`_defer_N_M_active`). Ф.2 теперь держит РОВНО ОДИН блокер — №390 ниже (плюс операционные ограничения №396/№398 из пакета: `cancel:` вокруг прямой блокирующей операции не прерывает, `with Fail` внутри `supervised` не ловит — учесть при написании моста).

  **ТРЕТИЙ блокер Ф.2, функциональный (№390, К1, найден окном p249 2026-08-06):** ВТОРОЙ `TcpStream.read()` после ЧАСТИЧНОГО чтения и close пира виснет навсегда, `supervised(timeout:)` его НЕ будит (репро `docs/plans/repro/p249_second_read_after_partial_hangs.nv`). Для МОСТА это не частный случай, а основная работа: `pipe_bidirectional` только и делает, что продолжает чтение после частичных данных — т.е. даже при закрытых №378/№379 мост будет вешать файберы на каждом закрытом пире. Ф.2 не сдавать, пока №390 открыт.

  **Причина уточнена 2026-08-06 (поправки владельца): дело не в «языку нечем выразить», а в
  ДВУХ отложенных пунктах, оба названы самой спекой:**
  1. **`[M-73.1-destructure]` (221.1 №378) — головной.** `consume (a, b) = …` и
     `consume {a, b} = …` не поддержаны (только простой идент), хотя для `ro`/`mut` обе формы
     живые. D180 (`05-memory.md:634`) отложил это «if запрос» — запрос теперь есть:
     `into_split()` отдаёт ПАРУ линейных значений, связать их owned нечем.
  2. **Мульти-var `spawn consume a, b { … }`** — пробел зеркала D188
     (`[M-consume-param-spawn-defer-active]`, там же разбор: сахар вложением НЕВОЗМОЖЕН,
     нужна своя move-семантика).
  **НУЖНЫ ОБА, п.1 недостаточен (поправка владельца 2026-08-06).** Вложенная форма
  `spawn consume ar { consume bw { … } }` не может работать ПРИНЦИПИАЛЬНО, а не из-за
  недоделки: `spawn consume X` переносит владение СИНХРОННО в момент spawn-statement'а
  (так и записано в разборе `[M-consume-param-spawn-defer-active]`), а вложенный
  re-consume исполнялся бы уже ВНУТРИ ребёнка — родитель к тому моменту мог выйти и
  запустить cleanup на `bw`; точка передачи владения оказывается неверной. Гейт это уже
  отражает: `bw` в теле spawn — захват (`E_LINEAR_CAPTURE_IN_FIBER`), сколько его ни
  оборачивай. Значит единственная корректная форма — **все move'ы В ТОЧКЕ spawn**, т.е.
  мульти-var `spawn consume ar, bw { pump(ar, bw) }`. Запасной путь без п.2 — Leg-упаковка
  (одно значение, один move), но она бьётся в открытый codegen-баг (репро
  `docs/plans/repro/m_consume_param_spawn_defer_active_tcp.nv`).
- `[M-178-client-policy-surface]` (прокси у `HttpClient`) — **НЕ блокер**: это будущий
  потребитель пакета, который ЭТОТ план и делает возможным.
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
- **Ф.П — ✅ ИСПОЛНЕНО (2026-08-06, окно p249-socks-package, коммит `35f45c2`).**
  Скелет по образцу `nova-compress` (§3.1): `nova.toml`, лицензии MIT/Apache-2.0,
  `.gitignore`, `scripts/githooks/pre-commit`. Три remote'а (github/gitverse/
  sourcecraft) добавлены в репу; **пуш НЕ выполнен** (ждёт слова владельца —
  `ls-remote`-сверка возможна только после пуша). `docs/guide/PUBLISHED.list` НЕ
  заведён (нет публикуемых guide-страниц — страж вакуумно зелёный, репа на сайт
  до тега не выводится).
- **Ф.1 — ✅ ИСПОЛНЕНО (коммит `48ca093`).** `src/socks5.nv`: чистые encode/decode
  + `socks5_connect`; `src/socks5_test.nv` — 22 байтовых фикстуры (все шаги
  хендшейка + негативы: 0xFF→UnsupportedMethod, auth-провал→AuthFailed,
  ATYP 0x04→UnsupportedAddressType, domain>255→HostTooLong, REP≠0→
  GeneralFailure), сверены с текстом RFC 1928/1929. `nova check`/`nova test src`
  зелёные (22/22), `nova lint` — 2 находки (обе намеренно оставлены,
  документированы в коде: `?`/`.map_err()?`/`.ok_or()?` на
  `Result[SocketAddr, SocksError]` роняет компилятор внутренней ошибкой
  P67-LEGACY — компиляторные файлы вне объёма этого окна).
- **Ф.2** — мост (`examples/net/socks5_http_bridge/main.nv`): подключить `socks`
  в `examples/nova.toml` (git+semver по D420; локальная разработка — через
  `nova.override.toml`, НЕ `[replace]` в манифесте), конфиг из `Os`, accept-loop,
  парсинг первой строки (с `MAX_HEADER_BYTES`), CONNECT-путь, `pipe_bidirectional`.
  **НЕ начата** (вне объёма окна p249-socks-package; ждёт
  `[M-73.1-destructure]`/`[M-consume-param-spawn-defer-active]`, см. статус выше).
- **Ф.3** — plain-HTTP-путь (переписывание absolute-URI → origin-form, срез
  `Proxy-*`-заголовков) — вторичный, можно отложить за Ф.2 отдельным коммитом.
  **НЕ начата** (ждёт Ф.2).
- **Ф.4 — ✅ ИСПОЛНЕНО (коммит `52749cf`).** README EN+RU: что пакет делает,
  почему отдельно от nova-http, объём V1, пример `socks5_connect`, честная
  пометка «ручной end-to-end smoke — вне CI, нужен реальный внешний
  SOCKS5-прокси».

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
