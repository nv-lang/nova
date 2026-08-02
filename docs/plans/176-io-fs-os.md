<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 176 — I/O + Filesystem + OS: `io`-core (Read/Write/Seek) + `Fs`-эффект + `Os` (env/args/cwd)

> **Top-level umbrella-план.** Создан 2026-06-22; production-hardened 2026-06-22 (cross-lang Go/Rust/TS/Kotlin/Java,
> workflow `plan180-harden` — план авторингался под №180, переномерован в 176 при сдвиге std-блока).
> **Ред. 2 — 2026-07-03** (5-агентный аудит + верификация): планка расширена до **7 языков (+Zig/Swift)**;
> **🎉 ГЛАВНОЕ: HARD-GATE «Plan 80» СНЯТ — must-consume УЖЕ реализован** (D133, Plan 100.1 ✅ 2026-05-25:
> `type X consume {}` + `D133-not-consumed` на scope-exit, `compiler-codegen/src/types/mod.rs:18864+/:19746-19908`; боевой пример —
> `WriteGuard`/Mutex; `File`-пример прямо в спеке D133) → affine-fallback-ветка УДАЛЕНА; stale-номера
> 180/179 вычищены; двойное владение net-миграций разведено со 178; mem_fs получил носителя; введена Ф.4.
> **Статус:** 🟢 **Ф.0.5 + Ф.1 + Ф.2 (fs+Path) + Ф.3 (os) + Ф.4 (net-миграции) + Ф.5 (тесты/spec/docs/Q-sweep) — ВСЕ DONE (Ф.4/Ф.5: 2026-07-09). Plan 176 ЗАКРЫТ.** **Маркер:** `[M-176-io-fs-os]` (CLOSED — см. `docs/plans/backlog-followups.md`).
> **Прогресс Ф.4/Ф.5 (2026-07-09, sonnet):** **Ф.4(a) NetError→ErrorKind проекция (Q3):** аддитивная
> `NetError.@to_error_kind() -> ErrorKind` / `@to_io_error(op) -> IoError` (`std/net/error.nv`); `NetError`/его
> `#stable` `@to_str()`-строки **не тронуты** (выбран путь «сохранить строки» — меньший дифф, чем переписывать
> все net-фикстуры под общий `kind_to_str`). **Ф.4(b) io.Read/io.Write на TcpStream:** `TcpStream.@read`/`@write`
> (`std/net/tcp.nv`) мигрированы на `Result[int, IoError]` через эту проекцию (+ `@flush()` no-op, тот же
> unbuffered-контракт что `File`) — структурная conformance `io.Read`/`io.Write` поверх byte-surface D407
> (Plan 183, уже влитого — conditional-маркер `[M-176-tcp-io-conformance]` не понадобился). Остальной `Net`-эффект
> НЕ тронут: `write_all`/`read_to_vec`/`read_text`/`write_str`, `TcpReadHalf`/`TcpWriteHalf`, `UdpSocket`, `resolve`
> — все по-прежнему `NetError` напрямую (изменение — только на прямых `@read`/`@write` `TcpStream`, по тексту
> плана). Фикстуры `std/net/{tcp_test,mock_test,stress_test}.nv`: прямые `.read()`/`.write()`-вызовы на
> `TcpStream` — через новый `must_io` (twin `must`, `Result[_, IoError]`); 13 call-site правок, остальные
> Net-вызовы (bind/connect/accept/halves) не тронуты. **Координация 178:** `HttpError.ErrSource.Net(NetError)`
> разгейчен — `HttpError.from_net(kind, e)` несёт типизированный `NetError` вместо строки-плейсхолдера;
> `std/http/transport/real.nv` (dns/connect/write) использует. Опасавшийся namespace-shadow
> (`NetError.InvalidPort` vs `ParseUrlError.InvalidPort`, см. баннеры `transport/real.nv`/`servernet.nv`) при
> эмпирической проверке НЕ подтвердился — `ParseUrlError` с тех пор переименовал вариант в `MalformedPort`;
> `std.http` компилируется с прямым `import std.net.{NetError}` без коллизий (баннеры оставлены как есть — их
> исторический rationale для сепарации `http.transport`/`http.servernet` остаётся валидным независимо).
> D302 амендирован (04-effects.md + README). Новый conformance-тест **`std/net/d302_neterror_iokind_test.nv`**
> (module-beside-module, тот же паттерн что d322/d323/d324 — `spec_tests/conformance` остаётся cross-domain-only):
> projection spot-checks + структурная conformance через `io.write_all`/`read_exact` над `mock_net()`.
> **Codegen-находка (расширяет `[M-176-xmod-payload-variant-ctor]`):** `[P67-LEGACY] Path call return type
> unknown` триггерится не только на cross-module payload-ctor, но и на ЛЮБОМ chained-method-call сразу после
> свежесконструированного enum-значения (даже nullary, даже same-module) — обход тот же (`ro x = Variant`
> перед `.method()`), уже był конвенцией в `error_test.nv`. **Ф.5:** новый `docs/guide/io-fs.md` (модель + 7-язык.
> таблица §2 + differentiators §1a + `write_atomic` Swift/Zig антипример); `spec/open-questions.md` Q9 частично
> закрыт (Time/Net/Fs/Os/Io/Http строки → D-ссылки, включая ранее не отмеченную Time-строку 175 Ф.6);
> Q-stdlib-minimal-api `from_bytes`-форма уже была обновлена Ф.0.5 (verified, не regression). Гейты:
> `spec_tests/conformance` (--positive --compile-error) **67/0**; `std/net` (addr/tcp/udp/dns/error) PASS
> пофайлово (udp+dns одновременно — pre-existing порт-флак, последовательно PASS/PASS); `std/io` 1/0; `std/fs`
> 1/0; `std/http` 5/0 (+1 pre-existing skip). Process-часть (176.1) не тронута.
> **Прогресс Ф.3 (2026-07-06):** модуль **`std/os`** (effect.nv/ffi.nv/os.nv/mock.nv) — **`Os` effect как тонкий int/str-primitive слой** (тот же паттерн, что `Fs`: rich `Option`/`Result`/`Path`/`EnvVar` строятся в `os.nv`-обёртках ВНЕ effect-vtable); public API `args`/`get_env`(+`_bytes`)/`has_env`/`set_env`(+`_bytes`)/`remove_env`/`vars`(→`[]EnvVar`)/`current_dir`/`set_current_dir`/`temp_dir`/`home_dir`/`exit_process`(flush+terminate)/`pid`/`hostname`; **byte-first** (env-ключи/значения кросят `[]u8`, NUL-терминация через `os_cstr`; non-UTF-8 env-значение round-trip'ит лосслесс через `get_env_bytes`); **`real_os()`** над нативными non-blocking-хуками `nova_rt/os_env.h` (getenv/`_putenv_s`/setenv/getcwd/chdir/getpid/gethostname; header-only static-inline как io_console.h — НЕ libuv; argv захвачен `nova_os_set_args` в изменённом `int main(int argc, char** argv)`) + **`mock_os(MockOs)`** (in-memory env/args/cwd map; **`exit` записывается** — `did_exit()`/`exit_code()`, НЕ терминирует → наблюдаемо в тесте; env-значения raw `[]u8` + `from_bytes_unchecked` byte-transparent); set_env/set_cwd concurrency-контракт (§3c, documented). Спека **D324** (04-effects.md) + README-индекс дописан (D322/D323/D324 committed-строки). Тесты: `nova_tests/os` (mock env round-trip/byte-correctness/vars/args/cwd/exit + real_os smoke pid/cwd/temp/env-round-trip/args — 6/6 PASS) + **`spec_tests/conformance/d324_os_env_args_cwd.nv`** (свёрнут в положительный CU — conformance **53/0**). **Codegen-находки (переиспользуют Ф.1/Ф.2-маркеры, НЕ упрощения):** (1) `exit` — язык-builtin (D13) → public-fn `exit_process`; (2) free-fn coarse-by-name резолв (D323-нота #3) → приватные `os_cstr`/`os_wrap_unit` (не `c_path`/`wrap_unit`); (3) cross-module payload-variant `ErrorKind.Other(0)` ловит `[M-176-xmod-payload-variant-ctor]` → ошибки cwd/hostname через `IoError.from_os(0, op)` (строит `Other` внутри std.io). Zero-regression подтверждён (baseline=parent-бинарь 5bb1ead7 temp-copy; io/fs/effects/basics/concurrency sample — delta 0; `int main`-сигнатура-смена не регрессит).
> **Прогресс Ф.2 (2026-07-04):** byte-backed `Path` value (POSIX+Windows/UNC/drive, lexical join/parent/file_name/extension/stem/components/normalize/with_extension, non-UTF-8 round-trip, Q1); **`Fs` effect как тонкий int-primitive слой** (rich `IoError`/`Metadata`/`DirEntry` строятся в .nv-обёртках ВНЕ effect-vtable — vtable стирает value-`IoError`-error в `nova_str`, поэтому эффект несёт только int/str-коды, зеркалящие fs.c-хуки; §3/§0 «логика в .nv над тонким C-hook»); **`File` must-consume (D133)** + `OpenOptions`(read/write/append/truncate/create/create_new, Q13) + positioned read_at/write_at + seek + sync_all/sync_data; `Metadata`(→`Timestamp`)/`DirEntry`/`FileType`/`Permissions`(Q8/Q12); **`real_fs()`** над libuv (`nova_rt/fs.c` uv_fs_* park/wake как net.c, best-effort-cancel Q4) + **`mock_fs()`/`MemFs`** (in-memory byte-Path-дерево, ENOSPC-инъекция для close-error/torn-write); convenience read/write/read_text/write_atomic(5-шаг durable §3c)/create_dir_all/remove_dir_all/copy_file/rename/read_dir/canonicalize/symlink/set_permissions/try_exists; `c_path` interior-NUL-reject (§3c(1); libuv сам конвертит UTF-8→UTF-16 на Windows → CWStr не нужен на этом бэкенде, `[M-176-cwstr-direct-winapi]`). Тесты: nova_tests/fs pos (path POSIX+Windows, mock_fs round-trip/metadata/seek/OpenOptions/dir/write_atomic/torn-write, real_fs temp-dir через spawn) + neg (D133 leak/double-close/use-after — через consume-param; match-extract tracking = `[M-176-consume-through-result-match]`) — ALL PASS; D-покрытие в **`spec_tests/d323`** (ОТДЕЛЬНЫЙ module, НЕ в `spec_tests/conformance`) — path_bytes/file_must_consume/write_atomic + neg, PASS. Main conformance = **38/0** чист. **Codegen-находки:** (1) value-record литералы требуют typed-форму в блок-позиции / anon в `=>`; (2) effect-op с rich `Result` стирается в `nova_int`/`nova_str` → int-primitive-эффект; (3) free-fn имена std.fs не должны коллидить с std.io generic-хелперами (read_text/write_text/copy_file, не read_to_string/write_str/copy); (4) **добавление std.fs-файла в folder-module `spec_tests.conformance` (один большой CU) ломает codegen map-closure в d102 (`undeclared 'f'`) — pre-existing CU-content-dependent closure-env баг, `[M-176-conformance-cu-map-closure]`** → d323 conformance вынесены в свой module; (5) mock_fs 10-тестовый binary flaky под GC-давлением → тесты разбиты ≤3/файл, `[M-176-memfs-gc-pressure]`.
> **Прогресс (2026-07-04):** **Ф.0.5** — `str.from_bytes -> Result[str, Utf8Error]` (Nova-body, D325-канон; ретайр
> интринзика `str.try_from([]u8)`; миграция всех потребителей + byte_offset-тест). **Ф.1 io-core** — `io.Read`/`io.Write`/
> `io.Seek` + `SeekFrom`; структурный `IoError`/`ErrorKind` (heap record); хелперы `read_exact`/`read_to_end`/
> `read_to_string`/`write_all`/`write_str`/`copy`/`lines`/`byte_lines`; конформеры `BytesReader`/`BytesWriter`;
> **`BufWriter` must-consume (D133, pos+neg)** + `BufReader`; `Io` effect + `stdin`/`stdout`/`stderr` + `mock_io`
> (capture/scripted) + `real_io` (C fd-хуки `nova_rt/io_console.h`). Спека D322 (04-effects.md); conformance
> `d322_io_read_write_seek.nv` (38/38); `nova_tests/io` pos+neg. **`IoError` = `value` (D322 §3b канон, 2026-07-04):**
> heap-обход снят — закрыт codegen keystone-gap «value-record в error-позиции generic `Result` / protocol-vtable»
> (protocol-vtable теперь forward-declare'ит референсимый `NovaRes_<ok>_NovaValue_<E>` mono до своей struct'ы;
> `emit_c.rs::emit_protocol_box_typedef`). **Оставшиеся codegen-обходы (followup-маркеры, НЕ упрощения семантики):**
> инлайн циклов в хелперах (форвард bounded-generic не проносит bound — checker `[M-176-io-forward-bounded-generic]`);
> `BufReader`/`BufWriter` с ЯВНЫМИ type-args (`BufWriter[BytesWriter].new` — inference-конструкция generic-wrapper'а
> = **отдельный** от value-record корень, эмпирически подтверждён value-record-независимым: NULL-stub падает и на
> heap-error; `[M-176-generic-wrapper-mono-inference]`); статические `SeekFrom.start/end/current` (cross-module
> payload-variant-литерал — checker-gap `[M-176-xmod-payload-variant-ctor]`) — backlog. Zero-regression подтверждён
> (baseline=parent-бинарь e50fcc6d, value-record/generic/io/str sample).
> **Запуск:** «**выполни план 176**».
> **Эталон:** **Go / Rust / TS / Kotlin / Java / Zig / Swift**. **Архитектура — по net-семейству**
> ([std/net/effect.nv](../../std/net/effect.nv)): эффект = внутренний плумбинг (libuv-backed, async, park/wake как
> [net.c:1-24](../../compiler-codegen/nova_rt/net.c#L1)), юзер — через type-методы; ошибки — `Result[T, IoError]`.
> **D-блоки (NEW):** D322 (io-core), D323 (fs), D324 (os) — резерв подтверждён 2026-07-03
> (`spec/decisions/README.md:140`: D316-D324 за 175/175.1/176; в спеке committed D315/D325-D328; внесение
> D316-D321 = задача Ф.0 планов 175/175.1, для 176 — dep-verify).
> **Оставшиеся гейты:** (1) **`str.from_bytes`→`Result[str, Utf8Error]`** отсутствует; НО fallible-декод УЖЕ
> есть как интринзик `str.try_from([]u8) -> Result[str, str]` (emit_c.rs:28166+, тесты nova_tests/str/utf8_invalid.nv)
> → Ф.0.5 = ПЕРЕОФОРМЛЕНИЕ (typed Utf8Error + канон-имя), не работа с нуля; (2) **`uv_fs_*` C-wrappers** не написаны (подтверждено:
> в nova_rt только комментарии) → новый `fs.c`.
> **Координация:** Plan 175 `Timestamp` (READY, Ред. 2); **Plan 178** (владелец net str→[]u8 + SocketAddr —
> §3.0-Q6); **Plan 173** (Cleanup[E]/@cleanup — канон scope-exit, §3.0-Q2); **Plan 174.6** (CWStr в C_ABI-грамматику);
> 83.3 `Blocking` (✅ закрыт; только CPU-bound-обёртки); 172.4 (design-lock, Ф.2-срез D328 реализован).
> **Закрывает** fs-часть `[M-91.10-fs-net-effects-formal]`. **Process → под-план 176.1** (файл НЕ создаётся до
> старта работ; `[M-176.1-process]` в OPEN-view — Ф.0). **Фоновые агенты:** §10.
> **Очередность (граф 173-176 — [README планов §Очередность](README.md), 2026-07-03):** Волна 0 = Ф.0 + Ф.0.5;
> Волна 1 трек D = Ф.1 (io-core — независим, стартует сразу). **Входящие гейты:** Ф.2 ← **Plan 175** (Timestamp)
> + 173 Ф.2 (Cleanup[IoError]-мост) + CWStr в 174.6 §2; Ф.4 ← Ф.2+Ф.3 (+178 byte-surface — conditional);
> 176.1 — Волна 3 (после Ф.1-Ф.3).
> **Сквозной критерий (обязательный):** «**без упрощений, как для прода**» (крит §8.0).

---

## 1. Зачем

В Nova **нет `std/io`, `std/fs`, `std/os`** — только консольный `print`/`println`, `Write`-протокол (text-sink для
`@display`, D258), `ReadBuffer`/`WriteBuffer` (in-memory). Нельзя открыть файл, прочитать директорию, узнать env.
Это закрывает backend/CLI-нишу языка (Plan 18: fs/os P0-для-0.2; `[M-91.10-fs-net-effects-formal]`). Net сделан
(91.12/83.12) — fs/io/os по той же модели и инфре.

## 1a. Где Nova ЛУЧШЕ peers — планка 7 языков (differentiators — в доку)

- **🏆 must-consume `File`/`BufWriter` (D133 — УЖЕ в языке): `@close(self) -> Result` — единственная ЯВНАЯ разрядка;
  незакрытый файл = compile-error, ошибка close (ENOSPC/EIO/quota — часто видна ТОЛЬКО на close) НЕ-игнорируема.**
  Бьёт **все 7**: Go `defer f.Close()` глотает (тихая потеря данных); Rust `Drop` глотает; Java/Kotlin — suppressed
  на error-path; Node `await-using` глотает; **Zig `close()` возвращает `void`** (ошибку физически некуда деть) и
  новый `Io.Writer` требует ручной `flush()` (дисциплина, не компилятор); **Swift** `FileHandle.close()` throws, но
  забыть можно. Самый крупный differentiator.
- **Мокабельные `Fs`/`Os`/`Io` эффекты:** `with Fs = mem_fs() { … }` → детерм. тест без диска и без DI. Go (afero),
  Rust (trait-abstraction), Java/Node (monkey-patch), **Zig (НЕТ вообще — ручной DI)**, **Swift (FileManager-mock
  через протокол — виральный DI)** — слабее.
- **byte-first by-necessity done RIGHT:** `str` UTF-8-validated → `read_to_string` **fallible** (`Result`, не
  Node-`U+FFFD`-порча). Zig-паритет (byte-slices везде), но Zig не имеет валидированного str-типа.
- **Typed `Timestamp`** (Plan 175) для mtime/atime/ctime (каждый `Option[Timestamp]`); бьёт Node Date/ms/ns-триплет,
  Go `Sys() any`, Zig голые `i128`-наносекунды.
- **Структурный `IoError{kind,raw_os,op,path,source}`** с exhaustive (wildcard-forced) `ErrorKind` — бьёт Go/Node
  stringly-typed `err.code`, Java checked-exception-шум; паритет Rust `ErrorKind` и Swift-system `Errno` (typed).
  **Zig per-op error sets** (точное множество ошибок на операцию) — сильная альтернатива: considered/rejected-нота
  в D322 (per-op sets требуют error-union-инфраструктуры и дробят обработку; один открытый `ErrorKind` + `raw_os` —
  Rust-модель, проще композируется через `source`-chain).
- **Корректный `write_atomic`** (5 шагов §3c) одним примитивом — пробел ВСЕХ peers: Go/Rust/Node/Kotlin/Java
  хэндролят; **Swift `.atomic` и Zig `AtomicFile` — tmp+rename БЕЗ fsync** (не durable после power-loss —
  антипример в доку: «атомарно против читателей ≠ durable»).
- **byte-backed `Path`** — несёт реальные не-UTF-8 Unix / WTF-8 Windows имена, которые JVM не может назвать
  (`InvalidPathException`), TS/Deno не представить; паритет Rust `Path`/`OsStr` и **Swift-system `FilePath`**
  (байтовый — прецедент Q1), Zig (байтовый `[]const u8`).

## 2. Эталон (cross-lang io/fs/os — 7 языков)

| Аспект | Go | Rust | TS/Node | Kotlin | Java | Zig | Swift |
|---|---|---|---|---|---|---|---|
| io-абстракция | `io.Reader/Writer` | `Read/Write/Seek`+`BufReader` | `stream.*` | okio/java | `InputStream`/nio | `std.Io` Reader/Writer (0.14+) | `FileHandle`/swift-system `FileDescriptor` |
| close | `defer Close()` (**err игнор**) | `Drop` (**глотает**) | `await close()` | `use{}` (suppressed) | try-with-res (suppressed) | `close()->void` (**некуда деть err**) | `close() throws` (можно забыть) |
| path | `string` | `Path`/`OsStr` (bytes) | string | nio.Path | nio.Path (**InvalidPathException**) | `[]const u8` (bytes) | `FilePath` (bytes, swift-system) |
| error | sentinels | **`io::Error{ErrorKind}`** | `err.code` string | `IOException` | иерархия | **per-op error sets** | typed `Errno` |
| EOF/partial | `(n>0, io.EOF)` (**footgun**) | `Ok(0)`=EOF, `read_exact` | promise | — | partial-read | `0`=EOF / error sets | Data-based |
| atomic write | вручную | вручную | вручную | вручную | `ATOMIC_MOVE` | `AtomicFile` (**без fsync**) | `.atomic` (**без fsync**) |
| TOCTOU | — | — | — | — | — | **🏆 Dir-scoped ops (openat by design)** | — |
| async | goroutine | sync/`tokio::fs` (pool) | libuv | suspend | NIO | sync/evented | actors |

**Взять:** Rust `ErrorKind`/`OpenOptions`/`create_new`/`Path`/`read_at`-`write_at`/`Ok(0)=EOF`; Go `ReadFile`/`WriteFile`-эргономика +
portable `filepath`; Java `ATOMIC_MOVE`; **Zig Dir-scoped/openat-семантика (followup `[M-176-dir-scoped-ops]`)**;
**Swift-system FilePath-прецедент**. **Избегать:** Go silent-close + `(n>0,EOF)`; Rust `Drop`-swallow; Node
silent-`U+FFFD`; Java `InvalidPathException`; **Swift/Zig atomic-без-fsync**; Zig close-void.

## 3. Архитектура

**Принцип (net-precedent):** `Fs`/`Os` — **плумбинг-эффекты** (юзер не зовёт; libuv-backed, park/wake через
`nova_sched_park`/libuv-cb/`nova_sched_wake`, как [net.c](../../compiler-codegen/nova_rt/net.c)), user-API —
type-методы + free-fns. `Io` (консоль) расширяем (stdin).

**🔑 Byte-first.** `str` = **UTF-8-validated immutable** → НЕ байтовый буфер. Весь raw-I/O — **`[]u8`**. `str` только
через **fallible UTF-8-декод** (`str.from_bytes(bytes) -> Result[str, Utf8Error]` — **ОТСУТСТВУЕТ → Ф.0.5**;
невалид → ошибка с byte-offset). `str.from_bytes_lossy`/`from_bytes_unchecked` **уже есть**
([core.nv:176/167](../../std/runtime/string/core.nv#L167)). Как Rust / Go / Zig.

**io-core протоколы — в модуле `std.io`, имена `io.Read`/`io.Write`/`io.Seek`** (⚠ **отдельны** от prelude-`Write`
text-sink `@display` D258: коллизия имён = `W_PRELUDE_SHADOW` — это **warning с suppress-механизмом** (`#allow(shadow)`),
не блокер (verified `lints.rs:1549+`); byte-`Write` не prelude-export, ссылка квалифицированная; если шум мешает —
Ф.0 решает rename text-sink с amend D258). Мост — явный `write_str`.

```nova
type io.Read  protocol { @read(buf mut []u8) -> Result[int, IoError] }     // Ok(0)=EOF (только при len(buf)>0); partial — норма
type io.Write protocol { @write(data []u8) -> Result[int, IoError]; @flush() -> Result[(), IoError] }  // partial-write легален
type io.Seek  protocol { @seek(pos SeekFrom) -> Result[int, IoError] }   // позиция = int (i64)
type SeekFrom | Start(int) | End(int) | Current(int)   // ВСЁ int (i64) — Nova-конвенция. Start<0 → InvalidInput на seek
// default-хелперы (loop + EINTR-retry): read_exact -> UnexpectedEof; write_all -> WriteZero; read_to_end -> []u8; read_to_string -> Result[str]
```

## 3.0. Закрытые решения (Q1–Q15 — РЕШЕНЫ; Ред. 2: Q2/Q3/Q6/Q11 обновлены, Q12-Q14 добавлены; Q15 — 2026-07-03)

| # | Вопрос | РЕШЕНИЕ | Обоснование |
|---|---|---|---|
| Q1 | non-UTF8 Path | **`type Path value { ro bytes []u8 }`** (НЕ str). Кодировка: raw OS-байты Unix / **WTF-8 Windows** (лосслесс round-trip UTF-16 incl. lone surrogates). `Path.from_str(str)->Path` (инфаллибельно), `@to_str()->Option[str]` (lossless), `@display()->str` (lossy U+FFFD, print-only), `@as_os_bytes()->[]u8`; lexical join/parent/file_name/extension/components/is_absolute на байтах; **reject NUL** → `InvalidInput`. `std/_experimental/path` — **ПЕРЕПИСАТЬ** (str-based Unix-only, подтверждено). | Прецеденты: Rust `OsStr`, **Swift-system `FilePath`**, Zig `[]const u8`; одна задокументированная кодировка (WTF-8 Win) |
| Q2 | File close | **must-consume линейный `File` через D133 (`type File consume {}`) — УЖЕ В ЯЗЫКЕ (Ред. 2: гейт Plan 80 снят, fallback удалён)**: `@close(self) -> Result[(), IoError]` — единственная **ЯВНАЯ** разрядка; незакрытый = compile-error `D133-not-consumed`; double-close невозможен. **Канон scope-exit — 173/D188:** `consume f = open(...) { body }` разряжает через `@cleanup(o)` (= `self.close()`, ошибка → suppressed-chain, НЕ теряется — 173 Ф.4 MultiError); explicit `@close()` — Result-путь, когда ошибка close нужна в happy-path. `File impl Cleanup[IoError]` — координация 173 Ф.2. Сахар `with_file(path, opts) \|f\| { … }` — Result-flavored: close-Result сворачивается в Result блока (оставлен с rationale: отличие от consume-блока = ошибка close в `Result`, не в suppressed). | главный differentiator (§1a: бьёт все 7); машинерия shipped (Plan 100.1, боевой пример WriteGuard) |
| Q3 | IoError единый | **ОДИН `IoError {kind, raw_os, op, path, source}`** для io+fs+os(+process). net: `NetError` → alias/projection на `ErrorKind` — **Ф.4 (Ред. 2: получил фазу-владельца)**, byte-baseline-guarded, ПОСЛЕ io-core. Сохранить `NetError.@to_str()`-строки или обновить фикстуры. **Координация 178 (ноты при его сверке):** (i) `HttpError.ErrSource.Net(NetError)` обновляется ЭТИМ коммитом; (ii) D327-D332 в 178 коллидируют с committed D327/D328 → перенумеровать (174 §6-прецедент); (iii) ложный from_bytes-green (Q11). | Rust один `io::Error` доказан; текущий NetError (flat sum, подтверждено) слабее |
| Q4 | async fs | **libuv `uv_fs_*` threadpool + fiber-park/wake** (точно net-паттерн); API blocking-looking. **Cancel — ЧЕСТНО best-effort:** queued → `uv_cancel`; in-flight syscall не прерывается (как Go/tokio/Java) → abandon-result + well-defined fd-state. | консистентность с net; врать про mid-syscall-cancel некорректно |
| Q5 | process | **отдельный под-план 176.1**, гейт ПОСЛЕ **176 Ф.1-Ф.3** (Ред. 2: stale «180» исправлен). 176 = io-core+fs+os; `os.cwd/env` остаются в 176. Файл 176.1 не создаётся до старта; `[M-176.1-process]` в OPEN-view. | subprocess огромен и ортогонален |
| Q6 | byte Write vs `@display` sink | **byte `io.Write` — SIBLING** (module-qualified). Мост — явный `write_str`. **net str→[]u8: ВЛАДЕЛЕЦ = Plan 178** (owner-sign-off 2026-06-26: additive byte-surface Ф.0.5 178-го + демоут str после HTTP byte-path); **176 добавляет ТОЛЬКО conformance-коммит: `impl io.Read/io.Write` на TcpStream поверх 178-byte-surface** (Ф.4). Правило: кто приземляется вторым — делает адаптацию. | слияние навязало бы text-семантику (Java-путаница); двойное владение с 178 разведено Ред. 2 |
| Q7 | lines/CRLF | `lines()` — split `\n` + strip trailing `\r`; terminator не включён; `byte_lines()` — raw. Финальная строка без `\n` — yield. Embedded lone `\r` — НЕ сепаратор. | Rust `BufRead::lines` / Go bufio |
| Q8 | permissions | портабельный `Permissions{readonly bool}`; Unix-mode через unix-qualified `@mode()->int`/`from_mode(int)` (Option/Unsupported на non-POSIX); `is_file/dir/symlink` — прямые предикаты; нет portable ACL. Windows: только readonly. | Rust Permissions + PermissionsExt / Java Posix-vs-Dos |
| Q9 | EOF/partial/EINTR | `read()` → `Ok(0)` EOF **только** при len(buf)>0; partial-read норма; partial-write → `write_all` loop, `Ok(0)` mid → `WriteZero`; `Interrupted`(EINTR) — retry в std-хелперах. НЕ Go `(n>0,EOF)`. | самый баг-генный контракт; Rust-модель чище |
| Q10 | BufWriter flush | **`BufWriter[W]` — must-consume (D133)**; `@close(self)->Result` flush+ошибка; **нет silent flush-on-drop**. Consume-поля выразимы (`consume field` в AST — подтверждено) → `BufWriter[File]` ок. Unbuffered `File.flush()` = no-op; durability — `sync_all`/`sync_data`. | убирает Go `bufio.Flush`-footgun + Rust drop-swallow + Zig ручной flush |
| Q11 | fallible byte→str | `from_bytes_lossy`/`from_bytes_unchecked` есть (core.nv:176/167). **Добавить fallible `str.from_bytes(bytes []u8) -> Result[str, Utf8Error]` + тип `Utf8Error{byte_offset}` — Ф.0.5, HARD PREREQ Ф.1.** Ред. 2 (verify-фактчек): fallible-декод УЖЕ существует как интринзик `str.try_from([]u8) -> Result[str, str]` (Err = строка БЕЗ byte_offset; emit_c.rs:28166-28176, string_builder.h:185, тесты nova_tests/str/utf8_invalid.nv) → **Ф.0.5 = переоформление**: канон-имя `from_bytes` (D325: обычное имя = Result; `try_from` без infallible-пары `from` нарушает D77-симметрию) + typed `Utf8Error{byte_offset}` (дом — рядом с Utf16Error, utf16.nv:23) + миграция utf8_invalid.nv-фикстур; судьба `str.try_from([]u8)` — deprecate/удалить тем же коммитом (НЕ дубль-API). **NB: 178 утверждает «from_bytes уже есть» по call-site в std/_experimental/crypto/jwt.nv:75/109 — ЛОЖНЫЙ green, определения нет (проверено); владелец = 176 Ф.0.5** (нота в 178 при его сверке). | без fallible-варианта `read_to_string` не звучен |
| Q12 | create-mode/umask (Ред. 2, Zig/Swift-аудит) | **Задокументировать**: default create-mode = `0o666 & ~umask` (POSIX-конвенция); `OpenOptions.mode(int)` — unix-qualified escape; Windows — N/A. Unix-тест: `mode(0o600)` применяется. | все peers наследуют POSIX-семантику молча — Nova документирует |
| Q13 | append-mode (Ред. 2) | **`OpenOptions.append` в скоупе Ф.2** (полный набор: read/write/append/truncate/create/create_new); neg: `append+truncate` → `InvalidInput`; append-семантика = atomic-EOF-write (O_APPEND), `write_at` на append-fd → задокументировать/InvalidInput. | Rust OpenOptions-паритет; без append файл-логгер не написать |
| Q14 | per-op error sets (Ред. 2) | **Considered/REJECTED** (Zig-модель): точные множества ошибок per-операция требуют error-union-инфраструктуры и дробят обработку; Nova = один открытый `ErrorKind` + `raw_os` + `source`-chain (Rust-модель). Нота в D322. | явное решение вместо молчания; композиция через source важнее точности множества |
| Q15 | io-протоколы × эффекты конформеров (узел 176↔178; ✅ решён 2026-07-03, исследование spec+checker) | **Протоколы `io.Read`/`io.Write` остаются эффект-агностичными; конформер несёт СВОЙ плумбинг-эффект** (File→`Fs`, TcpStream→`TcpNet`, BodyReader→`Http`) — это УЖЕ легально: **D122-amended** (2026-05-20: эффекты в protocol-методах разрешены; mono-dispatch пробрасывает их как у обычной effectful-fn; **vtable-путь для effectful-bounds запрещён** → generic-вызовы через io-bounds mono-only), **D62** (транзитивный не-Fail эффект = suppressable warning, не ошибка; io-семейство на `Result` → строгое Fail-правило не срабатывает by construction), runtime handler-стек (живой прецедент: nova_tests/plan91_12 зовёт `conn.write()` (TcpNet) из spawn без декларации — нужен лишь активный `with`-handler). **Ф.1-остаток:** (a) амендмент **D42/D15** — «impl не может привнести `Fail` сверх объявленного; НЕ-Fail эффекты конформера допустимы и всплывают транзитивно при мономорфизации (D62)» — узаконивает фактическое поведение чекера (конформанс эффекты сейчас не сверяет молча); (b) правило в **D322**; (c) spec_test `d322`: generic `copy(r,w)` с эффектным (File/mem_fs) И безэффектным (WriteBuffer) конформером. **Honest-дыра v1:** `forbid`/D63 и effect-surface НЕ видят эффекты через generic protocol-bound → `[M-effect-forbid-generic-bound]` (backlog). Отвергнуто: фикс-эффект-носитель `Io` (ломает номинальные handler-ы/forbid/мокабельность); scope-out (убил бы `copy_to`-в-File и BufWriter[File] — главные deliverables); effect-var `effects(W)` (Q6 open-questions, v0.7+ — для v1 не нужен) |

## 3b. `IoError` (структурный, Rust `ErrorKind`-precedent)

```nova
type IoError value { ro kind ErrorKind, ro raw_os int, ro op str, ro path Option[Path], ro source Option[*IoError] }
type ErrorKind | NotFound | PermissionDenied | AlreadyExists | NotADirectory | IsADirectory | DirectoryNotEmpty
    | WouldBlock | Interrupted | UnexpectedEof | WriteZero | InvalidInput | InvalidData      // InvalidData = UTF-8 decode fail
    | TimedOut | StorageFull | ReadOnlyFilesystem | CrossesDevices | BrokenPipe              // CrossesDevices = EXDEV (rename)
    | ConnectionRefused | ConnectionReset | ConnectionAborted | NotConnected | AddrInUse | AddrNotAvailable  // для net-унификации Q3
    | Unsupported | Other(int)                                                                // OPEN enum → wildcard-arm обязателен
fn IoError @to_str() -> str
// ENAMETOOLONG и прочие редкие errno → Other(raw_os) — приемлемо, задокументировать в error-index
```

## 3c. Семантика I/O (D322) + durability (D323)

- **EOF/partial (Q9):** см. таблицу. `read_exact`/`write_all`/`read_to_end` — loop + EINTR-retry; `Ok(0)`=EOF только при непустом буфере.
- **`write_atomic` (5-шаговый рецепт, durable):** (1) temp **в ТОЙ ЖЕ директории** (`O_EXCL`); (2) `write_all`;
  (3) `fsync` файла (`sync_all`); (4) atomic `rename`/replace; (5) **best-effort `fsync` родительской директории**
  (no-op на Windows). Возврат-`Ok` обычной `write` НЕ durable без `sync_*`. Windows: rename-replace через
  `MoveFileEx`/`ReplaceFile` (может EPERM/EBUSY → retry). **Anti-precedent (в доку): Swift `.atomic` и Zig
  `AtomicFile` делают tmp+rename БЕЗ шагов 3+5 — «атомарно», но НЕ durable после power-loss.**
- **TOCTOU:** `OpenOptions.create_new` (`O_EXCL`) → `AlreadyExists`; в доке «prefer open-and-match-NotFound над
  `exists()`-then-open»; `exists()` помечен racy. **Dir-scoped ops (Zig openat-модель — anti-TOCTOU by design) —
  followup `[M-176-dir-scoped-ops]`.**
- **SIGPIPE:** рантайм-init игнорирует SIGPIPE process-wide → запись в закрытый pipe → `BrokenPipe`, не убивает процесс.
- **symlink-races:** `remove_dir_all` — `openat`/`unlinkat` + NOFOLLOW где есть (anti-CVE); `metadata`(follows) vs `symlink_metadata`(lstat).
- **async-cancel (Q4):** park/wake; cancel best-effort.
- **exit-flush:** `Os.exit(code)` **flush'ит** stdout/stderr. `set_env`/`set_cwd` — process-global, racy (Rust сделал
  `set_var` unsafe в 1.84) → задокументировать/single-thread-контракт.
- **🔑 FFI-граница — разделять путь и данные, `str` НЕ передавать.** **(1) Путь/env-ключ → `CStr`** (NUL-terminated):
  **НОВЫЙ API `CStr.from_bytes(Path.@as_os_bytes()) -> Result[CStr, IoError]`** с reject interior-NUL → `InvalidInput`
  (Ред. 2: в cstr.nv сейчас ТОЛЬКО паникующий `str.as_cstr()` — from_bytes добавляет Ф.2, форма = D325 Result;
  NB cstr.nv — от Plan 118.1/D26, не D282). Лайфтайм через async — `CStr` GC-rooted на стеке фибры до resume.
  **Кодировка pathname:** POSIX `open(const char*)` — непрозрачные байты; **Unix** — verbatim в `uv_fs_open`;
  **macOS** — VFS требует UTF-8; **Windows** — нативно UTF-16, **WTF-8→UTF-16 в Nova** (переиспользуя
  `std/encoding/utf16.nv` — все нужные функции подтверждены: is_high/low_surrogate :31/:34,
  decode_surrogate_pair :38, encode_utf16 :49, Utf16Error :23), Windows-extern принимает **`(*u16, int len)` /
  `CWStr`** (Ред. 2: НЕ `[]u16` — Vec не C-ABI по 174.6-грамматике!), C-шим форвардит в `_wopen`/`CreateFileW`.
  **CWStr** (newtype над `*u16`) — вводит 176 Ф.2 рядом с CStr; **координация: ВНЕСТИ CWStr в C_ABI-грамматику 174.6 §2** (сейчас там ни CWStr, ни newtype-правила НЕТ — проверено; правило «positional newtype над C_ABI-типом = C-ABI» покроет CStr+CWStr единообразно; фазы M0-M3 в 174.6 вносятся его же реконсиляцией — 174 §3.6). Прецедент: Rust
  `OsStr::encode_wide`→`CreateFileW`, Zig `wtf8ToWtf16Le`→wide-API. Long-path: авто-префикс `\\?\` на абсолютных
  >260 (Rust-прецедент) — строка в D323. FFI-слой platform-split (Plan 42.12/D99 `_posix.nv`/`_windows.nv`+`#cfg`).
  **(2) Данные (read/write payload) → `(*u8, int len)`** — как у всех peers. Прямой биндинг libc `getenv(str)` запрещён.
- **stdin/stdout/stderr — fd-based байтовые хуки:** `io_read_fd(fd int, buf *mut u8, len int) -> int` (fd 0 = stdin) и
  `io_write_fd(fd int, buf *u8, len int) -> int` (1 = stdout, 2 = stderr); `Io`-эффект (`read_in`/`write_out`/`write_err`)
  оборачивает в `[]u8`-API, мокабелен.

## 4. Фазы

**Dep-chain:** Ф.0 → **Ф.0.5** → Ф.1 → {**Ф.2** ∥ Ф.3} → **Ф.4 (net-миграции)** → Ф.5. Process → **176.1**.
**Коммит после фазы** (§10). *(Ред. 2: дыра «Ф.4» в нумерации заполнена net-миграциями — раньше они висели
«отдельными коммитами» без фазы-владельца.)*

- **Ф.0 — gate (без кода).** (a) написать D322/D323/D324 (spec-first; содержание §3/§3.0/§3b/§3c, вкл. Q12-Q14-ноты);
  (b) **verify D-резерва**: README:140 держит D316-D324; D316-D321 вносят 175/175.1 (их Ф.0) — для 176 dep-verify,
  НЕ merge/renumber (high-water D328+); NB: README-индекс решений отстаёт от 02-types.md (D327/D328 committed,
  но в индексе нет) — дописать строки при verify; (c) **координация 173**: `File impl Cleanup[IoError]`
  (`@cleanup` = `self.close()`, ошибка → suppressed-chain) — согласовать с 173 Ф.2 rename-волной; (d) **координация
  174.6**: ВНЕСТИ CWStr/newtype-правило в C_ABI-грамматику 174.6 §2 (сейчас отсутствуют — проверено; «M0» в 174.6 ещё не существует, фазы вносит 174 §3.6); (e) решить Q6-rename text-sink `Write` (если
  W_PRELUDE_SHADOW-шум неприемлем — amend D258 + обновить существующий `spec_tests/conformance/d258_write_sink_decouple.nv`
  В ТОМ ЖЕ изменении); (f) зарегистрировать `[M-176-io-fs-os]`, `[M-176.1-process]`, `[M-176-dir-scoped-ops]`,
  `[M-176-create-temp]` в `docs/plans/backlog-followups.md` (OPEN-view); conditional `[M-176-tcp-io-conformance]`
  — только если 178 byte-surface не приземлится к Ф.4; (g) пометить Plan 80 в README планов как
  superseded-by-D133 (Plan 100.1) + **amend статус-ноту D133 в 02-types.md** (:4978 всё ещё «proposed;
  implementation pending» — stale, реализация shipped Plan 100.1 ✅ 2026-05-25) + **обновить строку 176 в README
  планов под Ред. 2** (там остались 179/Plan 80/affine-fallback/from_utf8 — выполнено 2026-07-03 в этой сверке). **GATE.**
- **Ф.0.5 — PREREQ fallible byte→str (Q11).** `str.from_bytes(bytes)->Result[str, Utf8Error]` + `type Utf8Error{byte_offset}`
  — **переоформление существующего интринзика `str.try_from([]u8)`** (typed-ошибка + канон-имя + миграция
  utf8_invalid.nv + deprecate try_from — Q11). **HARD-BLOCKER для `read_to_string`.** DEP: Ф.0.
- **Ф.1 — io-core.** `io.Read`/`io.Write`/`io.Seek` (sibling text-sink, Q6); `SeekFrom`; структурный `IoError`/`ErrorKind`
  (§3b); `BufReader`; **`BufWriter` must-consume (D133)** (Q10); `read_to_end`/`read_to_string`/`byte_lines`/`lines`/
  `read_exact`/`write_all`/`copy`; EOF/partial/EINTR (§3c/Q9); `stdin`/`stdout`/`stderr` через `Io`-эффект;
  **Io-mock deliverable** (capture stdout/stderr, scripted stdin — без него §8.4 не имеет носителя). DEP: Ф.0.5.
- **Ф.2 — fs.** `Fs`-эффект (новый `fs.c`: `uv_fs_open/read/write/close/stat/lstat/scandir/mkdir/unlink/rename/realpath/
  symlink/chmod/fsync/copyfile` + park/wake reuse net.c); **`File` must-consume (D133)** + Read/Write/Seek +
  **OpenOptions полный набор** (read/write/**append**/truncate/create/create_new — Q13) + `read_at`/`write_at` +
  `sync_all`/`sync_data`; **byte-backed `Path` ПЕРЕПИСАТЬ** (Q1); `Metadata`(→`Timestamp`, каждый `Option`);
  `DirEntry`/`read_dir`(lazy must-consume `DirIter`)/`walk_dir`(per-entry-error + SkipDir); convenience incl.
  **`write_atomic`** (5-шаг §3c) и **`with_file`** (Q2 Result-flavored сахар); portable `Permissions` + unix-mode (Q8/Q12); **`CStr.from_bytes` + `CWStr`** (§3c);
  FFI platform-split (`_posix.nv`/`_windows.nv`); **mem_fs deliverable**: `export fn mem_fs() -> Effect[Fs]`
  (module-conventions-канон; in-memory byte-Path-дерево, metadata, read_dir, OpenOptions-семантика, **инъекция
  ошибок ENOSPC/EIO** для close-error/torn-write тестов; дом — `std/testing/handlers.nv`, прецедент 175
  fixed/mut_clock). Тесты Ф.2 используют относительные пути worktree (temp_dir приходит в Ф.3 — задокументировать).
  DEP: Ф.1, 175. **Закрывает fs-часть `[M-91.10-…]`.**
- **Ф.3 — os. ✅ DONE (2026-07-06).** `Os`-эффект: `args`/`env`/`set_env`/`vars`/`cwd`/`set_cwd`/`exit`(flush)/`temp_dir`/`home_dir`/`pid`/
  `hostname`; **mock Os deliverable**: `export fn mock_os(MockOs) -> Effect[Os]` (env/args/cwd map + recorded-exit; дом — `std/os/mock.nv`, тот же канон `mock_*`, что `mock_fs`/`mock_io` — плановый «std/testing/handlers.nv»/`mem_os` заменён на codebase-precedent домен-модуль/`mock_os`); set_env/set_cwd — concurrency-контракт (§3c, documented). См. progress-ноту в шапке. DEP: Ф.1.
- **Ф.4 — net-миграции (Ред. 2: новая фаза; byte-baseline-guarded).** (a) `NetError`→`IoError`-унификация (Q3):
  projection на ErrorKind, `@to_str()`-строки сохранить/обновить фикстуры; координация 178 ErrSource; (b) conformance:
  `impl io.Read/io.Write` на `TcpStream` поверх byte-surface 178 (**полный str→[]u8 демоут — владелец 178**, Q6).
  DEP: **Ф.2, Ф.3** (rationale: byte-baseline-guarded миграции идут ПОСЛЕ основной работы — NetError-фикстуры
  не гоняются параллельно fs-коммитам); для (b) дополнительно byte-surface 178 Ф.0.5 — если 178 ещё не приземлил,
  (b) откладывается под `[M-176-tcp-io-conformance]` (conditional-маркер), НЕ блокируя Ф.5.
- **Ф.5 — тесты + spec/docs + Q-sweep.** §7 pos+neg+rt+spec_tests; D322-324 финал; новый `docs/guide/io-fs.md` (модель +
  7-языковая таблица §2 + §1a + write_atomic-антипример Swift/Zig); **Q-sweep** (§5). DEP: all.

**DEFERRABLE → под-план 176.1:** process (`Command`/`Child`/`Output`/`ExitStatus`/`Stdio`, `uv_spawn`, pipe-drain,
PATH-resolve incl. PATHEXT/`ErrDot`, env-inherit, cancel/kill/wait, Windows arg-quoting) — гейт после 176 Ф.1-Ф.3.
**Followups (§11):** flock, mmap, `walk_dir`-filters, glob-промоут, fs-watch, write_atomic Windows retry,
**dir-scoped ops (openat, Zig-модель)**, **create_temp/O_TMPFILE**, **copy fast-path** (`io.copy` File→File
sendfile-специализация; `fs.copy` уже получает copy_file_range бесплатно через uv_fs_copyfile).

## 5. Spec / D / Q / docs

- **NEW D322** — io-core: протоколы (sibling text-sink; **эффект-агностичны — эффект конформера всплывает при mono, Q15**; generic-io-bounds = mono-dispatch-only по D122), `SeekFrom`, EOF/partial/EINTR-контракт (Q9), BufWriter
  must-consume (Q10, D133), `IoError`/`ErrorKind` (+ **considered/rejected нота per-op error sets Q14**),
  stdin/stdout/stderr через `Io`, `str.from_bytes`/`Utf8Error`.
- **NEW D323** — fs: `Fs`-эффект (плумбинг, best-effort-cancel Q4), `File` must-consume (D133 + Cleanup[IoError]-мост
  173), byte-backed `Path` (Q1: WTF-8 Win; long-path `\\?\`), `Metadata`(→Timestamp), `write_atomic` 5-шаг (+
  антипример Swift/Zig), symlink/permissions (Q8) + create-mode/umask (Q12), OpenOptions (Q13), create_new/read_at/write_at.
- **NEW D324** — os: `Os`-эффект (args/env/cwd/exit-flush, set_env/set_cwd race-контракт). Process → 176.1-D.
- **amend prelude `Io`-decl** (расширение stdin/read_in/write_err); **amend D302** (NetError-унификация Q3/Ф.4);
  **amend D258** — только если Ф.0 решит rename text-sink.
- **spec_tests/conformance — ОБЯЗАТЕЛЬНОЕ D-покрытие (методология 2026-06-28; Ред. 2):** NEW
  `d322_io_read_write_seek.nv` (протоколы, SeekFrom, EOF/partial/EINTR, IoError/ErrorKind),
  `d323_file_must_consume.nv` + `d323_write_atomic.nv` + `d323_path_bytes.nv`, `d324_os_env_args_cwd.nv`;
  compile-error-сторона (must-consume leak/double-close/use-after) → `spec_tests/conformance/neg/`;
  amend D258 (если rename) → обновить существующий `d258_write_sink_decouple.nv` в том же изменении; amend D302
  (Ф.4) → `d302_neterror_iokind.nv`. Все `module spec_tests.conformance`, локалы с префиксами d322_/d323_/d324_;
  прогон `nova test spec_tests`.
- error-index: `IoError`/`ErrorKind`-варианты + E-коды must-consume (уже D133-семейство); ENAMETOOLONG→Other-нота.
- `docs/guide/io-fs.md` — новый guide (7-языковая таблица). **Q-sweep (Ред. 2, конкретизирован):** (1) open-questions
  **Q9** («стандартные эффекты не определены») — добавить/закрыть строки **Fs/Os/Io = D322-D324** (симметрично 175
  Ф.6 Time-строке); (2) **Q-stdlib-minimal-api:5551** — устаревшая форма `str.from_bytes Fail[Utf8Error]` → обновить
  на D325-канон `-> Result` тем же коммитом Ф.0.5. *(Q1-Q14 живут в этом плане — в open-questions их нет.)*

## 6. Миграция

Аддитивно (`std/io`/`std/fs`/`std/os`). Ф.4 — byte-baseline-guarded (NetError→IoError; conformance TcpStream).
`std/_experimental/path` — **переписать** (str-based, подтверждено). Верификация против чистого бинаря (temp-worktree
baseline §10); пересобрать `nova-cli` после `.nv` (`include_str!`); mass compile-errors → per-file loop §10.

## 7. Тесты (pos + neg + rt + spec_tests; раскладка по test-conventions — Ред. 2)

**Раскладка — ТРИ темы** (три std-модуля, durable; os-тесты (racy set_env) не смешивать в один CU с fs):
- **`nova_tests/io/`**, **`nova_tests/fs/`**, **`nova_tests/os/`** — folder-module `module nova_tests.io|fs|os`;
  **позитивы = peer-файлы с test-блоками** (описательные имена: `d322_read_exact_eintr.nv`, `plan176_write_atomic_exdev.nv`);
  **NB (Ред. 2): проверки «open несуществующего → Err(NotFound)» и т.п. — это ПОЗИТИВНЫЕ test-блоки с assert на
  Result, НЕ neg/-файлы**;
- **`fs/rt/`, `io/rt/`** — standalone `fn main` для runtime-фикстур: BrokenPipe («процесс не падает», EXPECT_STDERR/
  EXPECT_EXIT_CODE), ENOSPC-close сценарий;
- **`neg/`** — ТОЛЬКО `EXPECT_COMPILE_ERROR` (без двоеточия после маркера): must-consume leak, double-close,
  use-after-consume;
- **big-tests** — **`_slow.nv`-суффикс** (D277/D298: default-прогон пропускает, `--include-slow`; коммитятся —
  нерегенерируемые; fast-variant с малым N — peer-файл): большие dir-обходы, large-file copy;
- **spec_tests/conformance** — файлы §5.

**pos / контрактные (обязательные):**
- **byte-roundtrip**: `write []u8` → `read` байт-в-байт, включая невалидный-UTF-8;
- **must-consume positive**: `File`/`BufWriter` `@close()` разряжает, Result наблюдается; **close-error visible**
  (mem_fs ENOSPC-инъекция → `close()` = `Err`); **consume-блок**: `consume f = open(...) { … }` → `@cleanup`-разрядка
  (координация 173); **`with_file`**: close-Err (ENOSPC-инъекция) виден как Err результата блока при Ok-body (Q2);
- **non-UTF8 Path roundtrip** через mem_fs; `read_to_string` на невалидном UTF-8 → `IoError{InvalidData}`;
- **`str.from_bytes` unit**: точность `Utf8Error{byte_offset}` (Ред. 2 — прямой тест, не только косвенный);
- `read_exact`/`write_all` (partial+EINTR; **`Ok(0)` на пустом буфере НЕ EOF; WriteZero**); `lines()` edge-кейсы
  (финальная строка без `\n` — yield; embedded lone `\r` НЕ сепаратор); `byte_lines()` raw; `copy(r,w)`;
- **`write_str`-мост** io.Write ↔ text (Q6) + отсутствие коллизии io.Write/prelude-Write (компилируется без suppress
  или с задокументированным `#allow(shadow)`);
- `write_atomic` durability (fsync-file + same-dir-temp + fsync-dir; **torn-write neg через mem_fs-инъекцию —
  mandatory**); `EXDEV`→`CrossesDevices`;
- `read_dir`/`walk_dir`(per-entry-error+SkipDir); `Metadata.len/modified`(`Timestamp`); `create_dir_all`/
  `remove_dir_all`(symlink-safe); `copy`/`rename`; symlink;
- **Permissions API** (Ред. 2): `@readonly`/`@set_readonly` портабельно; unix `@mode()`/`from_mode` (`mode(0o600)`
  применяется — Q12); `Unsupported` на non-POSIX; **append-mode** (Q13): append-после-seek пишет в EOF;
- env get/set; args; cwd; `with Fs = mem_fs()` детерминизм; `with Os = mock_os()`; Io-mock (stdin scripted);
- **cancellable-fs**: cancel in-flight → не висит + fd-state defined;
- **NetError-строки** после Ф.4 (`@to_str()` сохранены / фикстуры обновлены осознанно).

**neg (`EXPECT_COMPILE_ERROR`):** забыл `File.close()` → `D133-not-consumed`; double-close; use-after-consume;
`append+truncate` → InvalidInput (если compile-time; иначе runtime-pos).

**rt:** BrokenPipe (процесс не падает); NUL в Path → `InvalidInput` (runtime-pos если не compile-time).

## 8. Критерии приёмки

0. **🔴 ОБЯЗАТЕЛЬНО: «без упрощений, как для прода».** Ни одного «решим потом» на критич. пути; каждая
   behavior-change — pos+neg + аргумент звучности.
1. io-core: протоколы + `BufReader` + **`BufWriter` must-consume (D133)** + хелперы; структурный `IoError`/`ErrorKind`;
   EOF/partial/EINTR-контракт; stdin/stdout/stderr мокабельны (**Io-mock существует**); `str.from_bytes`→`Result` (Ф.0.5).
2. byte-roundtrip (incl non-UTF8); close-error наблюдаема (mem_fs ENOSPC); must-consume: незакрытый → compile-error,
   use-after → compile-error; **consume-блок работает через Cleanup[IoError] (координация 173)**.
3. fs: `File` must-consume + **OpenOptions полный (incl. append Q13)** + `read_at`/`write_at` + `sync_*`; byte-`Path`
   (non-UTF8 roundtrip); `Metadata`(→`Timestamp`); `read_dir`/`walk_dir`; `write_atomic` (durability-тест);
   Permissions + mode/umask (Q8/Q12); best-effort-cancel; **`mem_fs()` deliverable с ошибко-инъекцией**;
   `CStr.from_bytes` + `CWStr` (координация 174.6).
4. os: `args`/`env`/`cwd`/`exit`(flush)/…; `Fs`/`Os`/`Io` мокабельны (носители — Ф.1/Ф.2/Ф.3 deliverables).
5. **Ф.4**: `NetError`→`IoError`-projection (координация 178 ErrSource); conformance `io.Read/Write` на TcpStream
   (поверх 178 byte-surface; при отсутствии — отложено с маркером, НЕ блокер).
6. **Гейт-статусы честны (Ред. 2):** must-consume = D133 (shipped) — БЕЗ affine-fallback; D322-D324 присвоены в
   рамках README-резерва (D316-D321 — критерий 175/175.1, для 176 dep-verify).
7. **Гейт корректности:** spec_tests/conformance зелёный (d322/d323/d324 + amended d258/d302) + **nova_tests
   baseline-delta = 0** (baseline = parent-коммит, тот же бинарь, temp-worktree/commit+reset — §10); батчи <10мин
   с `--results-file`/`--rerun-failed`; big-tests = `_slow.nv` вне дефолт-прогона.
8. spec: D322/323/324 + амендменты §5 + **spec_tests-файлы**; `docs/guide/io-fs.md` (7-языковая таблица, антипример
   Swift/Zig atomic); §1a differentiators; Q-sweep выполнен; followup-маркеры зарегистрированы в OPEN-view;
   Plan 80 помечен superseded-by-D133 + статус-нота D133 амендирована (Ф.0g).

## 9. Конвенции + координация

§1 (чекер), §3 (типы/эффекты из `.nv`), §5 spec-first, §6 (коды + error-index), §7 (blast-radius + чистый бинарь),
§8 (pos+neg, C-codegen). **Координировать:** net-семейство (паттерн+инфра); **Plan 178** (владелец net byte-surface +
SocketAddr + AddrNet-retract — НЕ пересекать коммиты; ErrSource-нота; ложный from_bytes-green — поправить при сверке
178); **Plan 173** (Cleanup[IoError]-мост, suppressed-chain); **Plan 175** (`Timestamp`); **Plan 174.6** (внести CWStr в C_ABI-грамматику §2); 83.3 (`Blocking` ✅ — только CPU-обёртки); 172.4 (design-lock; Ф.2-срез D328 реализован);
**Plan 42.12/D99** (platform-split FFI); **Plan 152.6/D255** (`utf16.nv` — подтверждён полный набор функций).
После большой задачи — `project-creation.txt` + discussion-log + `simplifications.md`.

## 10. Фоновые агенты (если используются)

- **НЕ `git stash`** (worktree делят `.git` → repo-global, [[feedback-worktree-shared-stash]]); baseline —
  **temp-worktree** (`git worktree add ../nova-176-base <parent>`) / commit+reset. Постоянный worktree **`nova-p176`**
  (naming nova-pNN) первой командой, самозарегистрироваться; cwd дрейфует → **префикс абсолютным путём в каждой
  команде** ([[feedback_worktree_cwd_clarity]]).
- **Git:** add только конкретные файлы; `git diff --cached --stat` перед commit; **DCO `git commit -s`** (CI-гейт);
  без `Co-Authored-By`; коммит после каждой фазы, без amend; **bidirectional sync в main после фазы**.
- **Идемпотентность под rate-limit** (workflow-агенты падают mid-run): шаги идемпотентны + checkpoint (commit per
  task); скрипты `.filter(Boolean)`, `resumeFromRunId`; не зависеть от успеха каждого агента.
- **Тесты:** только C-codegen ([[feedback-no-interpreter]]); **`nova test` требует ЯВНЫЙ путь**. **Батч-канон:**
  циклом `nova test nova_tests/<dir1> nova_tests/<dir2> … --results-file rN.json` (<10мин/батч,
  [[project-bash-timeout-10min-max]]), хвост `--rerun-failed`; ОТДЕЛЬНО `nova test spec_tests` и `nova test std`;
  флака ≠ регрессия (тот же бинарь). **Гейт корректности = spec_tests + pos+neg фикстуры + baseline-delta**
  ([[feedback-nova-tests-not-correctness-gate]]). Mass compile-errors (Ф.4) → per-file loop
  ([[feedback-test-fix-per-file-loop]]).
- **Worktree setup:** env `NOVA_GC_LIB_DIR`/`INCLUDE_DIR` → main; libuv-submodule из main + удалить `libuv/.git`
  ([[project-worktree-nova-test-setup]]); **net/fs-тесты ОБЯЗАТЕЛЬНО с cwd=worktree**; **mtime-touch `.rs`** перед
  cargo build; **пересобрать `nova-cli` после правок `.nv`** (`include_str!`). Не выдумывать синтаксис —
  `spec/decisions/` + `examples/` ([[feedback_nova_syntax]]).

## 11. Followup

`[M-176-io-fs-os]` (регистрация в OPEN-view — Ф.0). **Process → 176.1** (`[M-176.1-process]`; файл при старте работ;
гейт после 176 Ф.1-Ф.3). **NEW (Ред. 2):** `[M-176-dir-scoped-ops]` (Zig openat-модель — anti-TOCTOU by design);
`[M-176-create-temp]` (O_TMPFILE/anonymous temp); `[M-effect-forbid-generic-bound]` (Q15-дыра: forbid/effect-surface через generic-bound); conditional `[M-176-tcp-io-conformance]` (Ф.4b при
отсутствии 178 byte-surface). Прочие: flock, mmap, `walk_dir`-filters, glob-промоут
(`std/_experimental/path/glob.nv` существует), fs-watch (inotify/FSEvents), write_atomic Windows rename-replace-retry,
copy fast-path (sendfile). Имена/детали — финал при реализации (после Ф.0).
