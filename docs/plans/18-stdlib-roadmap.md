// SPDX-License-Identifier: MIT OR Apache-2.0
# План 18: Stdlib roadmap для Nova

> ⚠️ **СТАТУС: PARTIALLY ACTIVE** (обновлено 2026-05-25). Шаг 1
> (`std.sync`) ✅ зашиплен — формализуется отдельно [Plan 103](103-sync-primitives-spec-formalization.md)
> (D167). Шаги 2-4 (fs/os/net) перенесены в [Plan 91](91-stdlib-mvp-for-0.1.md)
> std MVP roadmap (на горизонте 0.2–0.4). Шаг 5 (RwLock/Semaphore) —
> отдельные планы по запросу. Q1–Q8 ниже — частично актуальны (Q3
> частично закрыт Plan 34, Q5 закрыт через декомпозицию в Plan 14/15/35).

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
| **`std.testing`** | без эффекта (фабрики handler'ов) | `seeded(seed)`, `fixed_ms(ms)` — deterministic test-handler'ы для `Random`/`Time`. Property-test инфраструктура (`property(gen)`, generators). Закрывается в [Plan 34](34-stdlib-typecheck-and-compile-fix.md). |

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

**Plan 14 CLOSED (2026-05-12), 6 фаз закрыты:**
- ✅ Ф.1 (Iter[T] element-type / Option[T] full refactor)
- ✅ Ф.2 (const non-trivial)
- ✅ Ф.3 (free-fn-as-value)
- ✅ Ф.4 (fn-в-record)
- ✅ Ф.6 (D69 variadic + spread)
- ✅ Ф.7 (`int as char` literal-only)
- ⛔ Ф.5 вынесена в [Plan 35](35-cross-file-resolve.md) (cross-file resolve, низкий ROI)

**Накопленные блокеры std/** (вскрылись после прод-grade Ф.1+Ф.6, не входят в Plan 14):
- Generic specialization при monomorphization (`set.nv`)
- Array-type mangling (`vec.nv`)
- Fail-method return-type propagation (`range.nv`)
- Protocol-bound dispatch D72 (`hashmap.nv`) — требует Plan 15
- Tuple type system (mixed types в `(K, V)`)
- Ф.7-bis (binary-pattern `(CharLit + IntExpr) as char`)

Каждый блокер — отдельный план по приоритету (Plan 19 уже занят под
closure-rev + D85 error-ops). Работа codegen-агента.

---

## Открытые вопросы (требуют решения перед финализацией)

**Q1.** Как технически расширять built-in эффекты в std-модуле? Spec не пишет явно — `type Fs effect { ... }` в `std.fs` либо переопределяет, либо добавляет операции? Возможно через embed `type FsExt effect { use _ Fs, ... }` (D39 для эффектов?).

**Q2.** `std.os` — отдельный эффект `Os` (более узкая capability) или операции встраиваются в `Io`? Spec не специфицирует.

**Q3.** Effect handlers для production — где-то должны быть default handler'ы (real_fs, real_net, real_time). Кто их предоставляет — runtime (`nova_rt/`) или std? Граница ответственности?
> ▸ **Частичный ответ (2026-05-12):** для **test-handler'ов** (`seeded`,
> `fixed_ms`) ответственность — на stdlib (`std.testing`). См.
> [Plan 34](34-stdlib-typecheck-and-compile-fix.md). Production-handler'ы (real_fs/net/time)
> остаются открытыми.

**Q4.** Errors — единый `IoError`/`FsError`/`NetError` per-domain или иерархия общих enum'ов? Spec любит `Fail[E]` с конкретным E.

**Q5.** Что делать с накопленными блокерами std/? Расширять Plan 14 уже нельзя (CLOSED 2026-05-12) — каждый блокер формулируется отдельным планом по приоритету. Текущий регресс закрыт [Plan 34](34-stdlib-typecheck-and-compile-fix.md).

**Q6.** `std.fmt.fmt(template str, ...args)` — нужен ли printf-style вообще, если есть string interpolation? Interpolation хватает для статических templates; printf нужен только для динамических (i18n, log formatters). Можно отложить в P1 или вообще не делать.

**Q7.** Auto-derive `str.from` для пользовательских record/sum — есть ли явное правило в D73? D70 говорил про auto-derive по структуре; миграция в D73 переносит поведение, но в самой D73 не уверен что записано explicitly. Нужно проверить.

**Q8.** Расширение существующего effect (`Fs`, `Net`, `Time`) через std-модуль — это revolutionary дизайн? Spec явно не описывает. Если механизма нет — нужно либо открыть D-блок, либо все ops размещать в одном месте (built-in spec).

---

## Execution Plan (актуализирован 2026-05-14)

> Plan 18 статус: DRAFT → продвигается к **active**. Ниже — конкретные
> шаги в порядке приоритета. Каждый шаг = отдельный коммит/sub-plan.

### Шаг 1 — std.sync (M:N-correct примитивы) — ✅ ЗАКРЫТО (формализация: Plan 103)

> ✅ **Зашиплено.** `std/runtime/sync.nv` + `compiler-codegen/nova_rt/sync_primitives.h`.
> Формализация в spec — отдельный [Plan 103](103-sync-primitives-spec-formalization.md)
> (D167). Реальный состав отличается от исходного дизайна ниже:
>
> | Дизайн (ниже) | Реализация (sync.nv) | Причина |
> |---|---|---|
> | `Atomic[T]` generic | `AtomicInt` + `AtomicBool` моно-типы | generic specialization для primitive T ограничен ([M-fn-prefix-int-only-mono]) |
> | `Mutex[T]` data-carrying | `Mutex` без `T` (Go-style) | проще, M:N-safe; data-carrying — отдельный design Q |
> | `WaitGroup` | ✅ как планировалось | — |
> | (не было) | `Once` (run/done barrier) | exactly-once добавлен в Шаг 1 scope |
> | `RwLock`, `Semaphore` | НЕ реализованы | перенесено в Шаг 5 |
>
> Memory ordering жёстко acq_rel/acquire/release (configurable
> ordering — отдельный план). Mutex fair-FIFO, NOT reentrant; Once с
> acquire fast-path; WaitGroup Go-style (add happens-before wait).

**Почему сейчас (исторически):** M:N runtime работает (Plan 44.5). Shared mut между
workers без синхронизации = UB. std.sync нужен для честного M:N кода.

Реализация (исходный дизайн — оставлен для истории; реальный shipped — см. Plan 103 D167):

**`Atomic[T]`** — через C11 `_Atomic` / MSVC `_InterlockedExchange*`.
```nova
export external fn Atomic[int].new(val int) -> Atomic[int]
export external fn Atomic[int].@load() -> int
export external fn Atomic[int].@store(val int) -> ()
export external fn Atomic[int].@fetch_add(delta int) -> int
export external fn Atomic[int].@compare_exchange(expected int, desired int) -> bool
```
Runtime: `nova_rt/atomic.h` — wrapping `<stdatomic.h>` (C11) /
`<intrin.h>` (MSVC). T=int, bool, ptr (3 specializations).

**`Mutex[T]`** — через `uv_mutex_t` (libuv, кроссплатформенный).
```nova
export external fn Mutex[T].new(val T) -> Mutex[T]
export external fn Mutex[T].@lock() -> T  // park если занят
export external fn Mutex[T].@unlock() -> ()
export external fn Mutex[T].@try_lock() -> Option[T]
```
Park-while-locked: `nova_sched_park_with_unlock` + `uv_mutex_lock` —
паттерн уже есть в Channel implementation.

**`WaitGroup`** — счётчик с barrier.
```nova
export external fn WaitGroup.new() -> WaitGroup
export external fn WaitGroup.@add(delta int) -> ()
export external fn WaitGroup.@done() -> ()   // add(-1)
export external fn WaitGroup.@wait() -> ()   // park until count == 0
```

Тесты: atomic increment от N workers (final sum = N × iterations),
mutex producer-consumer, waitgroup join N fibers.

### Шаг 2 — std.fs базовый (File read/write) — MEDIUM (→ Plan 91 / релиз 0.2+)

**`File.open`, `File.read`, `File.write`, `File.close`** через
libuv `uv_fs_open/read/write/close` с park/wake integration
(async libuv callback → `nova_sched_wake`). Effect: `Fs`.

Минимум для v0.2: read whole file, write whole file, stat, list dir.
Blocking-pool для sync path (отдельный thread pool в libuv).

Тест: write temp file → read back → assert content. Cross-platform.

### Шаг 3 — std.os.args / env — SMALL (→ Plan 91 / релиз 0.2+)

```nova
export external fn os.args() -> []str
export external fn os.env(key str) -> Option[str]
export external fn os.exit(code int) -> ()
```

Runtime: `argc/argv` через main thread init; `getenv` / `GetEnvironmentVariable`.
Нет libuv: синхронные системные вызовы.

### Шаг 4 — std.net (TCP echo server) — MEDIUM-HIGH (→ Plan 91 / релиз 0.2+)

`TcpListener.bind(addr)` + `TcpStream.dial(addr)` через libuv `uv_tcp_*`.
accept loop → spawn fiber per connection → park/wake на read/write.

**Это первый end-to-end network test Nova.** Если работает —
языковой server на Nova возможен.

### Шаг 5 — std.sync остаток (RwLock, Semaphore) — DEFERRED

> **Обновление 2026-05-25:** `Once` перенесён в Шаг 1 и зашиплен (см.
> [Plan 103](103-sync-primitives-spec-formalization.md) D167).
> Остались `RwLock` и `Semaphore` — отложены до конкретного use case.
> Когда возникнет — отдельный sub-plan (103.1 / 103.2 candidate).

Эти нужны для продвинутых паттернов (read-heavy data, bounded
concurrency).

### Блокеры которые нужно закрыть ДО шагов выше

- **Generic specialization** (`Atomic[int]`, `Mutex[T]`) — Plan 15
  или targeted fix в codegen. Сейчас generics через void* erasure —
  может быть достаточно для `Atomic[int]` (фиксированный T=int).
- **external fn в user-defined modules** — std.sync/fs/net не в
  `std.runtime.*` → D82 whitelist надо расширить на `std.*`.

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
- [14-stdlib-codegen-gaps.md](14-stdlib-codegen-gaps.md) — codegen-блокеры (CLOSED 2026-05-12, 6 из 7 фаз)
- [15-generic-bounds-enforcement.md](15-generic-bounds-enforcement.md) — D72 enforcement, нужен для протоколов на дженериках
- [16-capability-enforcement.md](16-capability-enforcement.md) — D63 forbid + D64 realtime, влияет на capability ограничения P0
- [17-q-resolutions.md](17-q-resolutions.md) — string interpolation (Plan 17 ✅) — основа для `str.from(v)` в форматировании
- [34-stdlib-typecheck-and-compile-fix.md](34-stdlib-typecheck-and-compile-fix.md) — текущий регресс stdlib type-check + `std.testing` handlers (активный)
- [35-cross-file-resolve.md](35-cross-file-resolve.md) — Ф.5 из Plan 14, вынесена самостоятельно
- [91-stdlib-mvp-for-0.1.md](91-stdlib-mvp-for-0.1.md) — std MVP для релиза 0.1 (Option/Result/Vec/HashMap/HashSet/sort/json/time/math); наследует Шаги 2-4 этого плана для релизов 0.2+
- [103-sync-primitives-spec-formalization.md](103-sync-primitives-spec-formalization.md) — формализация Шаг 1 (D167)
