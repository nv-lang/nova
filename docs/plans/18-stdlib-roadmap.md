// SPDX-License-Identifier: MIT OR Apache-2.0
# План 18: Stdlib gap-анализ — что из Rust/Go нужно в Nova

**Статус:** proposal, не начат.
**Дата создания:** 2026-05-09.
**Цель:** определить, что из stdlib Rust и Go отсутствует в Nova, расставить приоритеты под backend/CLI нишу и зафиксировать ключевые дизайн-решения до начала реализации.

---

## Context

Nova позиционируется как backend/CLI/system-альтернатива Go и Rust — та же ниша, но с эффектами в типах, structured concurrency и одним кодом для interpreter/JIT/AOT.

Сейчас в [std/](../../std/) уже есть приличное покрытие "не-системных" категорий: collections (vec, hashmap, set, linkedlist, queue, deque, priority_queue, lru, bloom_filter), encoding (base64, hex, json, csv, toml, ini, url), crypto (md5, sha1, sha256, hmac, bcrypt, jwt), identifiers (uuid, ulid, snowflake), text (regex, diff, markdown_minimal), math (complex, statistics), time (duration, cron), data (semver, sql), checksums (fnv, crc32), runtime (string, math, char, string_builder, write_buffer, read_buffer), concurrency-helpers (retry, rate_limiter), testing (property).

При этом критическая прослойка для backend/CLI отсутствует: файлы, сеть, процессы, sync-примитивы для fibers, полноценное время, форматирование. Для языка, который заявляет "пишите серверы и CLI как на Go" — это блокеры реальной полезности.

---

## Приоритизация под нишу Nova

### P0 — must-have для v0.1–v0.2 (без них язык не годится для backend/CLI)

Все они аккуратно ложатся в существующую effect-систему (`Io`, `Net`, `Db`, `Fs`, `Time`, `Random`, `Log`).

| Категория | Rust analogue | Go analogue | Эффект | Что должно быть |
|---|---|---|---|---|
| **`std.fs`** | `std::fs` | `os` + `io/ioutil` + `path/filepath` | `Fs` | read/write_file, read/write_bytes, mkdir, mkdir_all, remove, rename, copy, exists, stat (size/mtime/mode), walk_dir, read_dir, temp_file/temp_dir, file open/close/seek (handle-based) |
| **`std.io`** | `std::io` (Read/Write/BufRead) | `io` + `bufio` | `Io` | Reader/Writer protocol-set, BufReader/BufWriter, copy(reader, writer), Scanner (line/token), stdin/stdout/stderr handles |
| **`std.net`** | `std::net` | `net` | `Net` | TcpListener, TcpStream, UdpSocket, SocketAddr, connect/dial, listen/accept, DNS resolve. Должно интегрироваться с fiber-scheduler (non-blocking под капотом) |
| **`std.http`** | (`hyper`/`reqwest` экосистема) | `net/http` | `Net` | HTTP/1.1 client (get/post/request с headers/body/timeout), server (mux/router, request/response, middleware). Это **главное** — Go в backend популярен именно из-за `net/http` в stdlib |
| **`std.os`** | `std::env` + `std::process` | `os` + `os/exec` + `os/signal` | `Io` + `Fs` | env vars (get/set/all), args, exit, hostname, getuid/getpid, exec.Command (spawn child, pipes, wait, kill), signal handlers |
| **`std.sync`** | `std::sync` (Mutex/RwLock/Arc) | `sync` + `sync/atomic` | — (compile-time / pure runtime) | **Fiber-aware** Mutex, RwLock, Semaphore, WaitGroup, Once, Channel (already planned), atomic ops (i32/i64/ptr — load/store/add/cas). Без них structured concurrency не закроет всех нужд |
| **`std.time`** (расширить) | `std::time` (Instant, SystemTime) + `chrono` | `time` (Time, Duration, Location, ticker, timer) | `Time` | Instant (monotonic), DateTime + Date + TimeOfDay, TimeZone (хотя бы UTC + local + offset), parse/format (RFC3339, custom layout), Ticker, Timer, sleep_until. У нас сейчас только duration + cron |
| **`std.fmt`** | `std::fmt` (Display, Debug, Formatter, write!) | `fmt` (Sprintf, Fprintf, Stringer) | — | Sprint/Sprintln, Sprintf (хотя бы базовый verbs), Display + Debug protocol для пользовательских типов. Сейчас непонятно как печатать произвольную структуру читаемо |
| **`std.flag`** | (`clap` — в stdlib нет) | `flag` (+ `os.Args`) | — | Парсер CLI-аргументов: short/long flags, subcommands, env-fallback, help-генерация. Для CLI-ниши обязательно |
| **`std.log`** | (`log` + `tracing` экосистема) | `log/slog` | `Log` (уже есть как эффект) | Реализация хэндлера для существующего `Log` эффекта: structured logging (key-value), уровни, JSON output, child loggers. Эффект есть — реализации нет |
| **`std.strconv`** | методы на типах (`parse::<i64>`) | `strconv` (Atoi, Itoa, ParseFloat, FormatFloat) | — | Уже частично через runtime — проверить полноту: parse_int (с base), parse_float, format_int, format_float (precision/notation) |
| **`std.sort`** / алгоритмы | `slice::sort_by` | `sort` | — | sort/sort_by/sort_stable, binary_search, min/max/by, partition, dedup. Можно методами на vec — но как-то должны быть |

### P1 — желательно для v0.3–v0.4 (production-ready backend)

| Категория | Что нужно |
|---|---|
| **`std.tls`** | TLS поверх TcpStream (mbedTLS bundle), client+server, ALPN. HTTPS без этого не сделать |
| **`std.crypto.aead`** / `std.crypto.aes` / `std.crypto.rsa` / `std.crypto.ecdsa` / **secure_random** | Сейчас только хэши + bcrypt + jwt — не хватает симметричного и асимметричного шифрования, secure RNG |
| **`std.compress`** | gzip, deflate, zlib, zstd — для HTTP, логов, артефактов |
| **`std.encoding.yaml`** | YAML — стандарт для конфигов в backend |
| **`std.encoding.binary`** | Big/little endian, varint, fixed-width — для бинарных протоколов |
| **`std.encoding.xml`** | XML минимум для legacy interop |
| **`std.archive`** | tar, zip — packaging, артефакты |
| **`std.mime`** | MIME-types (lookup по расширению + magic bytes), для HTTP |
| **`std.template`** | Простые text/html templates (Jinja-like). Для server-side rendering и code-gen |
| **`std.context`** | Если cancel_scope/structured concurrency не покроет передачу deadline/values поперёк fiber-границ — добавить явный Context, как в Go |
| **`std.db`** (драйверы) | SQL-модуль есть, но нужны драйверы: postgres, sqlite, mysql + connection pool. Через эффект `Db` |
| **macOS support** | Win + Linux — P0; macOS — P1, после стабилизации первых двух |

### P2 — nice-to-have, можно через сторонние пакеты после `nova add` (v0.6+)

- HTTP/2, HTTP/3, WebSocket, gRPC
- Image (PNG/JPEG decode), audio
- Date-time с полной IANA tz database
- ssh, ftp, smtp клиенты
- Templating engines посерьёзнее
- Markdown полноценный (CommonMark)
- AI-обвязка (ndarray/tensor, embeddings storage, tokenizers, LLM bindings) — через сторонние пакеты, не stdlib

---

## Что **не нужно** копировать из Rust/Go

- `std::iter` / Go range loops — это синтаксис языка, не stdlib
- `std::error` / `errors` — у Nova эффект `Fail` + `Result`/`Option` уже это покрывает
- `context.Context` — структурированная concurrency и cancel_scope в Nova должны это закрывать на уровне языка
- `sync.Pool`, `sync.Map` — зависит от GC; премат
- `async`/`await` инфраструктура (`tokio`/`futures`) — у Nova нет цвета функций, fibers + эффекты делают это ненужным
- Generics-helpers вроде `slice` пакета в Go — должны быть методами на встроенных коллекциях, не отдельным модулем

---

## Зафиксированные дизайн-решения (2026-05-09)

1. **HTTP-клиент и server в одном модуле** `std.http` (как Go `net/http`) — упрощает discoverability.
2. **`std.sync` — M:N-готовый API сразу, упрощённая single-thread реализация на bootstrap.**

   **Что сейчас:** все fiber'ы крутятся на одном OS-потоке через minicoro. Mutex тривиален: если один fiber взял замок, другой ждёт; атомики не нужны.

   **Что в v0.4+:** M:N scheduler, fiber'ы по нескольким OS-потокам, реальная concurrency между fiber'ами на разных потоках. Нужны atomic CAS и memory ordering.

   **Выбор:**

   | Вариант | Сейчас | При M:N |
   |---|---|---|
   | A. Простой API | `lock.with(...)`, `mut counter` под замком | **Ломается user-код** — нужно переписать на `Atomic[i64]` |
   | B. M:N-готовый API | `Atomic[i64].fetch_add(1)`, `Mutex[T]` с memory ordering | Меняется только impl, **API стабилен** |

   Берём **B**. Сейчас чуть многословнее, зато при M:N пользовательский код не ломается. Реализация атомиков на bootstrap может быть наивной (без CAS — поскольку нет реальной concurrency), API уже M:N-correct.

   Затронутые типы: `Mutex[T]`, `RwLock[T]`, `Atomic[I]` (int/bool/ptr), `Channel[T]`, `WaitGroup`, `Once[T]`, `Semaphore`.
3. **`fmt` — compiler-generated default Debug** для всех типов. Явный override через **protocol**-impl (механизм nova, аналог Swift, не Rust traits). Macros — отложить.
4. **TLS — bundled mbedTLS** (C-нативная, статически линкуется в runtime). rustls не подходит: codegen pipeline = Nova → C → MSVC/clang, а rustls — Rust-крейт (потребовал бы cargo в build chain). mbedTLS: ~50KB, CMake-сборка кладётся рядом с libuv, Apache 2.0. Альтернатива на будущее — BoringSSL (production-grade, но Bazel-сборка сложнее). Wrapped-OpenSSL не подходит: на Windows системный OpenSSL почти никогда не установлен → AOT-бинарь не запустится.
5. **C-слой для IO/Net — libuv** как dependency (cross-platform: epoll/IOCP/kqueue под капотом). Не патчим (правило "сторонние библиотеки не трогать" из [project-philosophy.md](../project-philosophy.md) §4) — обёртки только в наших runtime-файлах. Экономия 6+ месяцев vs ручной event-loop.
6. **Платформы:** Windows + Linux first для P0 (Windows = main dev environment). macOS — в P1.
7. **stdlib на Nova vs C** — гибрид как сейчас: алгоритмика на Nova, syscall-обёртки на C. Для P0-IO/Net слой libuv-обёрток на C обязателен.
8. **AI-стек вне stdlib** — vector ops, embeddings, tokenizers, LLM bindings уйдут в сторонние пакеты после v0.6+ package manager. Stdlib фокусируется на backend/CLI.

---

## Зависимость: разблокировка codegen

**Pass-rate сегодня (2026-05-09):** 91/91 nova_tests PASS.

**Plan 14 закрыт почти полностью** (paused):
- ✅ Ф.1 (Option[T] full refactor) — коммит 304ec2b, тест `for_iter_typed.nv`
- ✅ Ф.2 (const non-trivial)
- ✅ Ф.3 (free-fn-as-value)
- ✅ Ф.4 (fn-в-record)
- ✅ Ф.6 (D69 variadic + spread) — коммит 6a54922, тест `variadic.nv`
- ✅ Ф.7 (`int as char` literal-only)
- ❌ Ф.5 (cross-file resolve) — низкий ROI / высокая стоимость, единственный открытый

**Накопленные блокеры std/** (открыты после прод-grade Ф.1 + Ф.6, не входят в Plan 14, см. [14-stdlib-codegen-gaps.md](14-stdlib-codegen-gaps.md) → раздел «Накопленные блокеры std/»):

| Блокер | Затронутые std-файлы | Природа |
|---|---|---|
| Generic specialization при monomorphization | `collections/set.nv` | Abstract `Iter[T]` erasure |
| Array-type mangling | `collections/vec.nv` | `Nova_[]T*` вместо `NovaArray_<T>*` |
| Fail-method return-type propagation | `collections/range.nv` | `step_by(3)` infer'ится как `nova_int` |
| Protocol-bound dispatch (D72) | `collections/hashmap.nv` | Generic-erased `K.eq(key)` — это блокер для Plan 15 |
| infer fallback для нестандартных iter | `text/diff.nv`, `crypto/bcrypt.nv` | `nova_int` fallback |
| Ф.7-bis (binary-pattern `(CharLit + IntExpr) as char`) | `identifiers/{uuid,ulid}`, `encoding/{base64,hex}` | Ф.7 strict literal-only |
| Tuple типизация — mixed types | `HashMap[K,V]`, `Iter[(K,V)]` | `_NovaTupleN` hardcoded на nova_int |

Каждый — отдельная задача. Возможно объединение в **Plan 19** "stdlib-blockers-round-2" или открытие отдельных планов под каждый паттерн — это работа codegen-агента.

После закрытия `hashmap.nv` (через Plan 15 D72 enforcement) и tuple-types откроется большая часть encoding/data модулей. После этого имеет смысл начинать P0 stdlib (`std.fs`, `std.io`, `std.net`).

---

## Критические файлы для будущей реализации

- [std/](../../std/) — корень stdlib, добавлять новые модули здесь
- [std/STATUS.md](../../std/STATUS.md) — статус компиляции (требует обновления)
- [14-stdlib-codegen-gaps.md](14-stdlib-codegen-gaps.md) — paused: Ф.1/Ф.2/Ф.3/Ф.4/Ф.6/Ф.7 ✅, остался Ф.5 + накопленные блокеры std/
- [compiler-codegen/src/codegen/runtime_registry.rs](../../compiler-codegen/src/codegen/runtime_registry.rs) — реестр C-runtime функций; новые intrinsic'и регистрировать сюда
- [compiler-codegen/nova_rt/](../../compiler-codegen/nova_rt/) — место для libuv-обёрток (свои `.c/.h`, **не править** `minicoro.h` и Boehm)
- [docs/project-creation.txt](../project-creation.txt) и [docs/simplifications.md](../simplifications.md) — обновлять после каждой крупной задачи
- effect handlers для `Io`/`Net`/`Fs`/`Time` — их сейчас нет в реализации; P0 модули требуют их в первую очередь

---

## Verification (когда дойдёт до реализации)

Каждый модуль из P0 должен иметь:
1. Конформанс-тест: типичный use-case целиком на Nova (read-file, HTTP-server hello-world, TCP echo, spawn-child-and-wait, mutex-counter с observable interleave order, не только итоговой суммой).
2. Property-тесты для алгоритмических (sort, parse/format round-trip).
3. Документация с одним рабочим примером в каждом public API.
4. Проверка прохода всех трёх режимов (interpreter / JIT / AOT) — обязательно для stdlib.

---

## Связь с другими планами

- [03-package-ecosystem-roadmap.md](03-package-ecosystem-roadmap.md) — package manager (v0.6+), от которого зависит P2 категория и AI-стек
- [14-stdlib-codegen-gaps.md](14-stdlib-codegen-gaps.md) — codegen-блокеры, должны быть закрыты до старта P0
- [13-runtime-stdlib-and-autogen.md](13-runtime-stdlib-and-autogen.md) — auto-gen std/runtime/*.nv из реестра компилятора (закрыт)
