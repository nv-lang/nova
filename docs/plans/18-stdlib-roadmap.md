// SPDX-License-Identifier: MIT OR Apache-2.0
# План 18: Stdlib roadmap для Nova

> ⚠️ **СТАТУС: DRAFT, не финализирован.** Не использовать как авторитет
> для реализации. Q1–Q8 открыты (см. ниже) — требуют решения перед
> утверждением плана. Финализация = снятие этого баннера + переход
> статуса в "active" в [README.md](README.md).

**Дата создания:** 2026-05-09 (rev 3, переосмысление под spec/decisions).
**Цель:** определить, что из stdlib Rust/Go нужно Nova под backend/CLI нишу, расставить приоритеты P0/P1/P2 и зафиксировать дизайн-решения **поверх** built-in эффектов и concurrency-примитивов спеки. Не план реализации — план направления.

---

## Context

Nova = backend/CLI/system-альтернатива Go и Rust + AI-first. Эффекты в типах, structured concurrency, один код на interpreter/JIT/AOT через C-codegen pipeline (nova → emit `.c` → cl.exe → exe). Spec уже даёт built-in эффекты (`Fail[E]`, `Io`, `Fs`, `Net`, `Db`, `Time`, `Random`, `Log`, `Trace`, `Ask[T]`, `Detach`, `Blocking`) и concurrency-примитивы (`Channel` D79, `supervised`, `spawn`, `parallel for`, `cancel_scope`, `realtime nogc`). Plan 18 описывает stdlib **поверх** этого фундамента.

---

## P0 — must-have для v0.1–v0.2

Все эффекты уже в spec — stdlib *расширяет* их, добавляя операции (см. Q1 ниже про механизм расширения).

| Модуль | Эффект | Что добавляет |
|---|---|---|
| **`std.fs`** | расширяет `Fs` | `open`, `stat`, `list`, `mkdir`, `remove`, `walk`, `copy`, `rename`, `temp_file/dir` + opaque `File` (через `external`, обёртка libuv `uv_fs_*`) |
| **`std.io`** | использует `Io` | structural protocols `Reader { read(buf []byte) Io Fail[IoError] -> int }`, `Writer { write(...) ... }`, `BufReader { use _ Reader, read_line() ... }`. Helpers: `copy[R Reader, W Writer]`, `Scanner`. stdin/stdout/stderr — через handler |
| **`std.net`** | расширяет `Net` | `listen_tcp/dial_tcp/udp_*`, `dns_resolve` + opaque `TcpListener`/`TcpStream`/`UdpSocket` (libuv `uv_tcp_t`/`uv_udp_t`). Не-blocking IO через fiber-yield под капотом — Async invisible (D62) |
| **`std.http`** | использует `Net` | HTTP/1.1 client (`HttpClient.get/post/request`) + server (`HttpServer.serve(handler)`). TLS — отложен в P1 |
| **`std.os`** | новый эффект `Os` | args/env/exit/hostname/spawn_process/signal. Отдельно от `Io` — capability более узкая (см. Q2) |
| **`std.sync`** | без эффекта (compile-time + runtime) | **Channel built-in** (D79) — главный паттерн (owner-actor). `Atomic[T]`, `Mutex[T]`, `RwLock[T]`, `WaitGroup`, `Once[T]`, `Semaphore` — opaque external, M:N-correct API сразу |
| **`std.time`** | расширяет `Time` | `instant()`, `realtime()` + типы `DateTime/Date/TimeOfDay`, `Ticker`, `Timer`, `parse/format` (RFC3339) |
| **`std.fmt`** | без эффекта | helper-функции (`print[T Into[str]]`, `println`, `eprint`, `eprintln`, `fmt(template str, ...args)`). **Без `Display`/`Debug` protocols** — конверсия в str через `str.from(v)` (D73). String interpolation `"${expr}"` уже работает (Plan 17 ✅) |
| **`std.flag`** | без эффекта | CLI args parser: short/long flags, subcommands, env-fallback, help-генерация |
| **`std.log`** | handler-фабрики для built-in `Log` | `console_handler()`, `json_handler(out Writer)`, `filtered(level, inner)`, `with_fields(fields, inner)` |
| **`std.sort`** | generic fns | `sort[T Ord]`, `sort_by`, `binary_search`, `min/max`. `Ord` — структурный protocol с `lt(other Self) -> bool` |

**Удалено из P0:**
- `std.strconv` — заменено `str.from(v)` / `T.try_from(s)?` (D73 + D77)
- `Display`/`Debug` protocols в `std.fmt` — заменены `str.from(v)` (D70 REPLACED → D73)

---

## P1 — для v0.3–v0.4 (production-ready)

`std.tls` (mbedTLS), `std.crypto.aead/aes/rsa/ecdsa/secure_random`, `std.compress` (gzip/zstd), `std.encoding.{yaml,xml,binary}`, `std.archive` (tar/zip), `std.mime`, `std.template`, `std.context` (если cancel_scope не покроет), `std.db` driver pool. **macOS support** (Win+Linux в P0).

---

## P2 — после v0.6+ package manager (сторонние пакеты)

HTTP/2/3, WebSocket, gRPC, image/audio, full IANA tz, ssh/ftp/smtp, advanced templating. **AI-стек** (ndarray/tensor, embeddings, tokenizers, LLM bindings) — через сторонние пакеты, не stdlib.

---

## Зафиксированные дизайн-решения

1. **HTTP в одном модуле** `std.http` (client+server), как Go `net/http`.
2. **`std.sync` API сразу M:N-correct.** Bootstrap-impl упрощённая (single-thread без CAS), API стабилен при переходе на M:N.
3. **Форматирование через `str.from(v)` (D73) + string interpolation `"${expr}"`** (Plan 17 ✅). `Display`/`Debug`/`ToStr` НЕ существуют — D70 REPLACED → D73.
4. **TLS — bundled mbedTLS** (чистый C, статически линкуется в runtime через cl.exe). rustls отвергнут (Rust в build chain). OpenSSL отвергнут (системная зависимость).
5. **C-слой для IO/Net — libuv** как dependency (epoll/IOCP/kqueue под капотом). Не патчим — обёртки только в `nova_rt/`.
6. **Платформы:** Windows + Linux first. macOS в P1.
7. **stdlib: Nova + C гибрид** — алгоритмика на Nova, syscall-обёртки на C.
8. **AI вне stdlib** — через сторонние пакеты после v0.6+ package manager.
9. **Concurrency-главный паттерн = Channel + spawn (owner-actor)**, не Mutex. Mutex/Atomic/RwLock — для случаев когда actor избыточен.
10. **Protocol body — без `@`**, методы записываются как `name(args) Effects -> Ret` (D53). `Self` — late-bound тип.
11. **Receiver методов через `@`** — `fn TcpStream @read(buf []byte) Net Fail[NetError] -> int` (D35).
12. **Per-fiber handler isolation (D80)** — `spawn` наследует snapshot handler-стека. Logger/db/time-mock конфигурируются через `with X = h { body }` без глобальных синглтонов.

---

## Что **не копируем** из Rust/Go

- `std::iter` / Go range loops — это синтаксис языка
- `std::error` / `errors` — у nova `Fail[E]` + `Result`/`Option`
- `context.Context` — structured concurrency и `cancel_scope` покрывают
- `sync.Pool`, `sync.Map` — премат, зависит от GC
- `async`/`await` — у nova нет цвета функций (D62 ambient)
- `slice` package в Go — методы на встроенном `[]T`
- `Display`/`Debug` traits — конверсия через `str.from`/string interpolation

---

## Зависимость: codegen-блокеры

**Pass-rate сегодня (2026-05-09):** 91/91 nova_tests PASS.

**Plan 14 paused, почти полностью закрыт:**
- ✅ Ф.1 (Iter[T] element-type / Option[T] full refactor)
- ✅ Ф.2 (const non-trivial)
- ✅ Ф.3 (free-fn-as-value)
- ✅ Ф.4 (fn-в-record)
- ✅ Ф.6 (D69 variadic + spread)
- ✅ Ф.7 (`int as char` literal-only)
- ❌ Ф.5 (cross-file resolve, низкий ROI / высокая стоимость)

**Накопленные блокеры std/** (вскрылись после прод-grade Ф.1+Ф.6, не входят в Plan 14):
- Generic specialization при monomorphization (`set.nv`)
- Array-type mangling (`vec.nv`)
- Fail-method return-type propagation (`range.nv`)
- Protocol-bound dispatch D72 (`hashmap.nv`) — требует Plan 15
- Tuple type system (mixed types в `(K, V)`)
- Ф.7-bis (binary-pattern `(CharLit + IntExpr) as char`)

Возможно объединение в **Plan 19** "stdlib-blockers-round-2". Работа codegen-агента.

---

## Открытые вопросы (требуют решения перед финализацией)

**Q1.** Как технически расширять built-in эффекты в std-модуле? Spec не пишет явно — `type Fs effect { ... }` в `std.fs` либо переопределяет, либо добавляет операции? Возможно через embed `type FsExt effect { use _ Fs, ... }` (D39 для эффектов?).

**Q2.** `std.os` — отдельный эффект `Os` (более узкая capability) или операции встраиваются в `Io`? Spec не специфицирует.

**Q3.** Effect handlers для production — где-то должны быть default handler'ы (real_fs, real_net, real_time). Кто их предоставляет — runtime (`nova_rt/`) или std? Граница ответственности?

**Q4.** Errors — единый `IoError`/`FsError`/`NetError` per-domain или иерархия общих enum'ов? Spec любит `Fail[E]` с конкретным E.

**Q5.** Что делать с накопленными блокерами std/? Открыть Plan 19 или расширять Plan 14? Это влияет на расписание перед стартом P0 stdlib работы.

**Q6.** `std.fmt.fmt(template str, ...args)` — нужен ли printf-style вообще, если есть string interpolation? Interpolation хватает для статических templates; printf нужен только для динамических (i18n, log formatters). Можно отложить в P1 или вообще не делать.

**Q7.** Auto-derive `str.from` для пользовательских record/sum — есть ли явное правило в D73? D70 говорил про auto-derive по структуре; миграция в D73 переносит поведение, но в самой D73 не уверен что записано explicitly. Нужно проверить.

**Q8.** Расширение существующего effect (`Fs`, `Net`, `Time`) через std-модуль — это revolutionary дизайн? Spec явно не описывает. Если механизма нет — нужно либо открыть D-блок, либо все ops размещать в одном месте (built-in spec).

---

## Verification (когда дойдёт до реализации)

Каждый модуль из P0 должен иметь:
1. Конформанс-тест: типичный use-case целиком на Nova (read-file, HTTP-server hello-world, TCP echo, spawn-child-and-wait, channel-producer-consumer с observable interleave order, не только итоговой суммой).
2. Property-тесты для алгоритмических (sort, parse/format round-trip).
3. Документация с одним рабочим примером в каждом public API.
4. Проверка прохода всех трёх режимов (interpreter / JIT / AOT) — обязательно для stdlib.

---

## Связь с другими планами

- [03-package-ecosystem-roadmap.md](03-package-ecosystem-roadmap.md) — package manager (v0.6+), от которого зависит P2 категория и AI-стек
- [14-stdlib-codegen-gaps.md](14-stdlib-codegen-gaps.md) — codegen-блокеры (paused, остался Ф.5 + накопленные блокеры)
- [15-generic-bounds-enforcement.md](15-generic-bounds-enforcement.md) — D72 enforcement, нужен для протоколов на дженериках
- [16-capability-enforcement.md](16-capability-enforcement.md) — D63 forbid + D64 realtime, влияет на capability ограничения P0
- [17-q-resolutions.md](17-q-resolutions.md) — string interpolation (Plan 17 ✅) — основа для `str.from(v)` в форматировании
- (потенциально) Plan 19 — stdlib-blockers-round-2 (накопленные блокеры std/)
