<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 180 — I/O + Filesystem + OS: `io`-core (Read/Write/Seek) + `Fs`-эффект + `Os` (env/args/cwd)

> **Top-level umbrella-план.** Создан 2026-06-22; production-hardened 2026-06-22 (cross-lang Go/Rust/TS/Kotlin/Java +
> adversarial-критика, workflow `plan180-harden`). **Статус:** 📋 READY (все Q закрыты §3.0).
> **Маркер:** `[M-180-io-fs-os]`. **Запуск:** «**выполни план 180**».
> **Эталон:** **Go / Rust / TS / Kotlin / Java**. **Архитектура — по net-семейству** ([std/net/effect.nv](../../std/net/effect.nv)):
> эффект = внутренний плумбинг (libuv-backed, async, park/wake как [net.c:1-24](../../compiler-codegen/nova_rt/net.c#L1)),
> юзер — через type-методы; ошибки — `Result[T, IoError]`.
> **D-блоки (NEW):** D322 (io-core), D323 (fs), D324 (os). ⚠ **D316–D321 (Plan 179/179.1) ещё НЕ в `spec/decisions/`** (committed
> только до D315) → Ф.0 verify/merge/renumber до присвоения D322+.
> **🔴 HARD-GATES:** (1) **Plan 80 must-consume** (Ф.3 consume-checker scope-exit) — **НЕ начат**; линейный `File`/`BufWriter`/`DirIter`
> требуют его → **гейтит Ф.2** (fallback: affine `consume` + runtime-checked close, если 80 слипнется). (2) **`str.from_utf8`→`Result`**
> отсутствует (91.18 deferred) → **Ф.0.5 prereq** для `read_to_string`. (3) **`uv_fs_*` C-wrappers** не написаны (net.c их не имеет) → новый `fs.c`.
> **Координация:** Plan 179 `Timestamp` (READY), 83.3 `Blocking` D50 (только CPU-bound-обёртки, не дефолт fs-путь), 172.4 value-ABI (READY), 91.18 str.
> **Закрывает** fs-часть `[M-91.10-fs-net-effects-formal]`. **Process → отдельный под-план 180.1** (Q5). **Фоновые агенты:** §10.
> **Сквозной критерий (обязательный):** «**без упрощений, как для прода**» (крит §8.0).

---

## 1. Зачем

В Nova **нет `std/io`, `std/fs`, `std/os`** — только консольный `print`/`println` (D69), `Write`-протокол (text-sink для `@display`),
`ReadBuffer`/`WriteBuffer` (in-memory). Нельзя открыть файл, прочитать директорию, узнать env. Это закрывает backend/CLI-нишу
языка (Plan 18: fs/os P0-для-0.2; `[M-91.10-fs-net-effects-formal]`). Net сделан (91.12/83.12) — fs/io/os по той же модели и инфре.

## 1a. Где Nova ЛУЧШЕ peers (differentiators — в доку)

- **🏆 must-consume `File`/`BufWriter` (Plan 80): `@close(self) -> Result` — ЕДИНСТВЕННЫЙ способ разрядить обязательство; незакрытый
  файл = compile-error, ошибка close (ENOSPC/EIO/quota — часто видна ТОЛЬКО на close) НЕ-игнорируема.** Это превращает **главный Go-footgun**
  (`defer f.Close()` глотает ошибку → тихая потеря данных) в compile-time-гарантию; бьёт Rust (`Drop` глотает), Java/Kotlin (suppressed на
  error-path), Node (await-using глотает). Самый крупный differentiator.
- **Мокабельные `Fs`/`Os`/`Io` эффекты:** `with Fs = mem_fs() { … }` → детерм. тест без диска и без DI. Go (нужен afero), Rust (trait-abstraction),
  Java/Node (global monkey-patch) — слабее.
- **byte-first by-necessity done RIGHT:** `str` UTF-8-validated → `read_to_string` **fallible** (`Result`, не Node-`U+FFFD`-порча, не скрытый decode).
- **Typed `Timestamp`** (179) для mtime/atime/ctime (каждый `Option[Timestamp]` — платформа может не записать); бьёт Node Date/ms/ns-триплет, Go `Sys() any`.
- **Структурный `IoError{kind,raw_os,op,path,source}`** с exhaustive (wildcard-forced) `ErrorKind` — бьёт Go/Node stringly-typed `err.code` (опечатка компилится) и Java checked-exception-шум.
- **Корректный `write_atomic`** (temp-в-той-же-директории + fsync-file + atomic-rename + fsync-parent-dir) одним примитивом — пробел, который Go/Rust/Node/Kotlin/Java оставляют (все хэндролят, обычно с багами).
- **byte-backed `Path`** — несёт реальные не-UTF-8 Unix / WTF-8 Windows имена, которые JVM не может назвать (`InvalidPathException`), а TS/Deno не представить.

## 2. Эталон (cross-lang io/fs/os)

| Аспект | Go | Rust | TS/Node | Kotlin | Java |
|---|---|---|---|---|---|
| io-абстракция | `io.Reader/Writer/Seeker` (всё `[]byte`), `bufio` | `Read/Write/Seek/BufRead`+`BufReader` | `stream.Readable/Writable` | okio/java | `InputStream`/`Reader`/nio |
| close | `defer f.Close()` (**err игнор**) | `Drop` (**err глотается**) | `await close()` | `use{}` (suppressed) | try-with-res (suppressed) |
| path | `string` (Unix=любые байты OK; Win=WTF-8) | `Path`/`OsStr` (non-UTF8) | string | nio.Path | nio.Path (**InvalidPathException** на non-UTF8) |
| error | `error`+sentinels `errors.Is(os.ErrNotExist)` | **`io::Error{ErrorKind}`** | `err.code==='ENOENT'` | `IOException` | `IOException`-иерархия |
| EOF/partial | `(n>0, io.EOF)` одновременно (**footgun**) | `Ok(0)`=EOF, partial→loop, `read_exact` | promise resolve | — | partial-read |
| atomic write | вручную | вручную (`tempfile` crate) | вручную | вручную | **`Files.move(ATOMIC_MOVE)`** |
| async | goroutine+blocking | sync / `tokio::fs` (threadpool) | promises (libuv) | suspend | NIO async |
| process | `os/exec` (`ErrDot`-fix) | `Command/Stdio` | `child_process` | `ProcessBuilder` | `ProcessBuilder` |

**Взять:** Rust `ErrorKind`/`OpenOptions`/`create_new`/`Path`/`read_at`-`write_at`/`Ok(0)=EOF`; Go `ReadFile`/`WriteFile`-эргономика +
portable `filepath`; Java `ATOMIC_MOVE`. **Избегать:** Go silent-close + `(n>0,EOF)` + path-as-string; Rust `Drop`-swallow; Node silent-`U+FFFD` + env-replace-drops-PATH; Java `InvalidPathException`.

## 3. Архитектура

**Принцип (net-precedent):** `Fs`/`Os` — **плумбинг-эффекты** (юзер не зовёт; libuv-backed, park/wake через `nova_sched_park`/libuv-cb/`nova_sched_wake`,
как [net.c](../../compiler-codegen/nova_rt/net.c)), user-API — type-методы + free-fns. `Io` (консоль) расширяем (stdin).

**🔑 Byte-first.** `str` = **UTF-8-validated immutable** → НЕ байтовый буфер. Весь raw-I/O — **`[]u8`**. `str` только через **fallible
UTF-8-декод** (`str.from_utf8(bytes) -> Result[str, Utf8Error]`, **Ф.0.5 — сейчас ОТСУТСТВУЕТ**; невалид → ошибка с byte-offset, не паника/не lossy;
+ `from_utf8_lossy`). Как Rust (`read`→bytes / `read_to_string`→validated) / Go (`[]byte`).

**io-core протоколы — в модуле `std.io`, имена `io.Read`/`io.Write`/`io.Seek`** (⚠ **отдельны** от prelude-`Write` text-sink `@display`:
коллизия имён, Q6/critique; byte-`Write` не prelude-export, ссылка квалифицированная; если W_PRELUDE_SHADOW мешает — Ф.0 переименует text-sink). Мост — явный `write_str`.

```nova
type io.Read  protocol { @read(buf mut []u8) -> Result[int, IoError] }     // Ok(0)=EOF (только при len(buf)>0); partial — норма
type io.Write protocol { @write(data []u8) -> Result[int, IoError]; @flush() -> Result[(), IoError] }  // partial-write легален
type io.Seek  protocol { @seek(pos SeekFrom) -> Result[u64, IoError] }
type SeekFrom | Start(u64) | End(int) | Current(int)
// default-хелперы (loop + EINTR-retry): read_exact -> UnexpectedEof; write_all -> WriteZero; read_to_end -> []u8; read_to_string -> Result[str]
```

## 3.0. Закрытые решения (Q1–Q11 — РЕШЕНЫ)

| # | Вопрос | РЕШЕНИЕ | Обоснование |
|---|---|---|---|
| Q1 | non-UTF8 Path | **`type Path value { ro bytes []u8 }`** (НЕ str, НЕ текущий experimental). Кодировка: raw OS-байты Unix / **WTF-8 Windows** (лосслесс round-trip UTF-16 incl. lone surrogates). `Path.from_str(str)->Path` (инфаллибельно), `@to_str()->Option[str]` (lossless), `@display()->str` (lossy U+FFFD, **print-only**), `@as_os_bytes()->[]u8`; lexical join/parent/file_name/extension/components/is_absolute на байтах, platform-separators; **reject NUL** → `InvalidInput`. `std/_experimental/path` — **ПЕРЕПИСАТЬ** (он str-based Unix-only). | str не несёт non-UTF8; одна задокументированная кодировка (WTF-8 Win) избегает Rust `as_encoded_bytes`-hazard |
| Q2 | File close | **must-consume линейный `File` (Plan 80)**: `@close(self) -> Result[(), IoError]` (финальный flush + ошибка) — единств. разрядка; незакрытый = **compile-error**; double-close невозможен (linearity). Сахар `with_file(path, opts) \|f\| { … }` — close-Result **сворачивается в Result блока** (не теряется). **HARD DEP Plan 80 Ф.3** (НЕ начат) → гейтит Ф.2; fallback affine `consume`+runtime-check. | главный differentiator; Go `defer Close` глотает (тихая потеря на ENOSPC), Rust Drop глотает, Kotlin/Java suppressed |
| Q3 | IoError единый | **ОДИН `IoError {kind ErrorKind, raw_os int, op str, path Option[Path], source Option[*IoError]}`** для io+fs+os(+process). net: `NetError` → alias/projection на `ErrorKind` (или deprecate), **отдельным byte-baseline-guarded коммитом ПОСЛЕ io-core**. Сохранить `NetError.@to_str()`-строки или обновить фикстуры. | Rust один `io::Error` доказан; текущий `NetError` (flat sum, без kind/errno/path) слабее |
| Q4 | async fs | **libuv `uv_fs_*` threadpool + fiber-park/wake** (точно net-паттерн); API blocking-looking `file.read(buf)->Result`. **Cancel — ЧЕСТНО best-effort:** queued → `uv_cancel` (чисто); in-flight syscall **не прерывается** (как Go/tokio/Java) → abandon-result + well-defined fd-state. `Blocking` (83.3) — только CPU-bound-обёртки. | консистентность с net (та же инфра); врать про mid-syscall-cancel — некорректно (все 5 peers честны) |
| Q5 | process | **отдельный под-план 180.1**, гейт ПОСЛЕ 180 Ф.1-Ф.3. 180 = io-core+fs+os(env/args/cwd/exit). `os.cwd/env` остаются в 180 (fs зависит от cwd для relative-paths). | subprocess огромен и ортогонален (pipes/PATH/PATHEXT/signals/cwd-trojan/deadlock) — не блокировать fs/os |
| Q6 | byte Write vs `@display` sink | **byte `io.Write` — SIBLING, отдельный** от text-sink `Write` (`@display`); module-qualified `io.Write`. Мост — явный `write_str(s)->encode-utf8->write`. | слияние навязало бы text-семантику на байт-стримы (Java InputStream-vs-Writer путаница) |
| Q7 | lines/CRLF | `lines()` — split `\n` + **strip trailing `\r`** (`\r\n`/`\n` → чистые), terminator не включён; `byte_lines()` — raw `[]u8`, без strip. Финальная строка без `\n` — yield. Embedded lone `\r` (old-Mac) — НЕ сепаратор. | Rust `BufRead::lines` (ожидаемо) / Go bufio; byte_lines верен для binary |
| Q8 | permissions | портабельный `Permissions{readonly bool}` (`@readonly()/@set_readonly`) cross-platform; Unix-mode через **unix-qualified** `@mode()->u32`/`from_mode` (Option/Unsupported на non-POSIX); `is_file/dir/symlink` — прямые предикаты (cheap, без extra stat); **нет portable ACL**. Windows: только readonly (0o200). | Rust Permissions + PermissionsExt::mode / Java Posix-vs-Dos-view; Unix-octal не универсален |
| Q9 | EOF/partial/EINTR | `read()` → `Ok(0)` EOF **только** при len(buf)>0; partial-read норма (loop / `read_exact`→`UnexpectedEof`); partial-write → `write_all` loop, `Ok(0)` mid → `WriteZero`; `Interrupted`(EINTR) — retry в std-хелперах. **НЕ Go `(n>0,EOF)`-одновременность.** | самый баг-генный контракт; Rust `Ok(0)`=EOF чище и матчит `read(buf)->Result[int]` |
| Q10 | BufWriter flush | **`BufWriter[W]` — must-consume линейный** (Plan 80); `@close(self)->Result` flush+ошибка; незакрытый = compile-error; **нет silent flush-on-drop**. Unbuffered `File.flush()` = no-op; durability — `sync_all`/`sync_data`. | убирает Go `bufio.Flush`-footgun + Rust BufWriter-drop-swallow на compile-time |
| Q11 | str.from_utf8 (MISSING) | **Ship `str.from_utf8(bytes []u8) -> Result[str, Utf8Error]` + `from_utf8_lossy` в 91.18/prelude — Ф.0.5, HARD PREREQ Ф.1.** `Utf8Error` несёт byte-offset первой невалидной последовательности. `read_to_string`/`lines` → through `from_utf8`, `Utf8Error`→`IoError{kind: InvalidData}`. `from_bytes_unchecked` (core.nv:167) — unsafe-only. | без неё `read_to_string` не звучен (пришлось бы lossy/unchecked) |

## 3b. `IoError` (структурный, Rust `ErrorKind`-precedent)

```nova
type IoError value { ro kind ErrorKind, ro raw_os int, ro op str, ro path Option[Path], ro source Option[*IoError] }
type ErrorKind | NotFound | PermissionDenied | AlreadyExists | NotADirectory | IsADirectory | DirectoryNotEmpty
    | WouldBlock | Interrupted | UnexpectedEof | WriteZero | InvalidInput | InvalidData      // InvalidData = UTF-8 decode fail
    | TimedOut | StorageFull | ReadOnlyFilesystem | CrossesDevices | BrokenPipe              // CrossesDevices = EXDEV (rename)
    | ConnectionRefused | ConnectionReset | ConnectionAborted | NotConnected | AddrInUse | AddrNotAvailable  // для net-унификации Q3
    | Unsupported | Other(int)                                                                // OPEN enum → wildcard-arm обязателен
fn IoError @to_str() -> str
```

## 3c. Семантика I/O (D322) + durability (D323)

- **EOF/partial (Q9):** см. таблицу. `read_exact`/`write_all`/`read_to_end` — loop + EINTR-retry в std; `Ok(0)`=EOF только при непустом буфере.
- **`write_atomic` (полный 5-шаговый рецепт, durable):** (1) temp **в ТОЙ ЖЕ директории** (избежать `EXDEV`/`CrossesDevices`), имя через `O_EXCL`;
  (2) `write_all`; (3) `fsync` файла (`sync_all`); (4) atomic `rename`/replace; (5) **best-effort `fsync` родительской директории** (no-op на Windows).
  Возврат-`Ok` обычной `write` **НЕ durable** без `sync_*`. Windows: rename-replace через `MoveFileEx`/`ReplaceFile` (может EPERM/EBUSY → retry).
- **TOCTOU:** `OpenOptions.create_new` (`O_EXCL`) → `AlreadyExists`; в доке «prefer open-and-match-NotFound над `exists()`-then-open»; `exists()` помечен racy.
- **SIGPIPE:** рантайм-init **игнорирует SIGPIPE** process-wide → запись в закрытый pipe → `BrokenPipe` (`EPIPE`/`WSAECONNRESET`), не убивает процесс.
- **symlink-races:** `remove_dir_all` — `openat`/`unlinkat` + NOFOLLOW где есть (Linux, anti-CVE); `metadata`(stat, follows) vs `symlink_metadata`(lstat, no-follow).
- **async-cancel (Q4):** park/wake; cancel best-effort (queued→`uv_cancel`; in-flight завершается, fd-state well-defined).
- **exit-flush:** `Os.exit(code)` **flush'ит** stdout/stderr (или документировать что НЕ — тогда требовать explicit flush; избежать exit-truncates-buffer footgun). `set_env`/`set_cwd` — process-global, racy с конкурентными relative-open (Rust сделал `set_var` unsafe в 1.84) → задокументировать / single-thread-контракт.
- **🔑 FFI-граница — байт-буферы, НЕ `str` (учитывая `str`=UTF-8).** extern-C-хуки рантайма берут пути/ключи/данные как **байт-буфер** (`*u8` + `int len`), **НЕ `str`**: `str` UTF-8-only и immutable, а пути (Q1 byte-`Path`: non-UTF8 Unix / WTF-8 Win), env-ключи и файл-данные — **произвольные байты**. Путь передаётся как `Path.@as_os_bytes()` (`ptr,len`); данные — `[]u8` (`ptr,len`). Шим на границе **NUL-terminate'ит + reject interior-NUL** (`InvalidInput`, Rust `CString::new`-стиль) перед libc/libuv (хотят `char*`). `str` через границу — только genuinely-text (на syscall-границе практически нет). Прямой биндинг libc `getenv(str)` запрещён.
- **stdin/stdout/stderr — fd-based байтовые хуки:** единые `io_read_fd(fd int, buf *mut u8, len int) -> int` (fd 0 = stdin) и `io_write_fd(fd int, buf *u8, len int) -> int` (fd 1 = stdout, 2 = stderr), byte-first; `Io`-эффект (`read_in`/`write_out`/`write_err`) оборачивает их в `[]u8`-API и мокабелен. (Не спец-`io_read_stdin` — симметричный fd-набор.)

## 4. Фазы

**Dep-chain:** Ф.0 → **Ф.0.5** → Ф.1 → **Ф.2 (гейт Plan 80 + `uv_fs_*`)** → Ф.3 → Ф.5. Process → **180.1**. net-миграции — отдельные коммиты. **Коммит после фазы** (§10).

- **Ф.0 — gate (без кода).** Закрыть §3.0 (готово); написать D322/D323/D324 (spec-first); **verify/merge/renumber D316–D321** (179/179.1 ещё не в `spec/decisions/`);
  подтвердить расписание Plan 80 Ф.3 (иначе affine-fallback). **GATE.**
- **Ф.0.5 — PREREQ `str.from_utf8` (Q11).** `str.from_utf8(bytes)->Result[str, Utf8Error]` + `from_utf8_lossy` + `Utf8Error{byte_offset}` в 91.18/prelude. **HARD-BLOCKER для `read_to_string`.**
- **Ф.1 — io-core.** `io.Read`/`io.Write`/`io.Seek` (sibling text-sink, Q6); `SeekFrom`; структурный `IoError`/`ErrorKind` (§3b); `BufReader`; **`BufWriter` must-consume** (Q10);
  `read_to_end`/`read_to_string`/`byte_lines`/`lines`/`read_exact`/`write_all`/`copy`; EOF/partial/EINTR-семантика (§3c/Q9); `stdin`/`stdout`/`stderr` через `Io`-эффект (мокабельны). DEP: Ф.0.5.
- **Ф.2 — fs.** `Fs`-эффект (новый `fs.c`: `uv_fs_open/read/write/close/stat/lstat/scandir/mkdir/unlink/rename/realpath/symlink/chmod/fsync/copyfile` + park/wake reuse net.c-паттерна);
  **`File` must-consume** + Read/Write/Seek + `create_new` + `read_at`/`write_at` + `sync_all`/`sync_data`; `OpenOptions`; **byte-backed `Path` ПЕРЕПИСАТЬ** (Q1);
  `Metadata`(→`Timestamp`, каждый `Option`); `DirEntry`/`read_dir`(lazy must-consume `DirIter`, cheap d_type)/`walk_dir`(per-entry-error + SkipDir);
  convenience incl. **`write_atomic`** (5-шаг §3c); portable `Permissions` + unix-mode-escape (Q8). **🔴 HARD-GATE: Plan 80 Ф.3** (иначе affine-fallback). **Закрывает fs-часть `[M-91.10-…]`.** DEP: Ф.1, 179.
- **Ф.3 — os.** `Os`-эффект: `args`/`env`/`set_env`/`vars`/`cwd`/`set_cwd`/`exit`(flush)/`temp_dir`/`home_dir`/`pid`/`hostname`; мокабельно; set_env/set_cwd — concurrency-контракт (§3c). DEP: Ф.1.
- **Ф.5 — тесты + spec/docs.** §7 pos+neg; D322-324; новый `docs/io-fs.md` (модель + «Nova↔Go/Rust/…» + §1a). DEP: all.

**Отдельные коммиты (byte-baseline-guarded, ПОСЛЕ io-core):** net `NetError`→`IoError` унификация (Q3); net `read`/`write` `str`→`[]u8` (Q6/gap-11, чтобы `io.Read`/`io.Write` единообразно покрывали File И TcpStream).
**DEFERRABLE → под-план 180.1:** process (`Command`/`Child`/`Output`/`ExitStatus`/`Stdio`, `uv_spawn`, pipe-drain на отд. фиберах, PATH-resolve incl. Windows PATHEXT/`ErrDot`, env-inherit-by-default, cancel/kill/wait, Windows arg-quoting). **Followups:** flock, mmap, `walk_dir`-filters, glob-промоут, fs-watch (+ write_atomic Windows rename-replace caveat).

## 5. Spec / D / Q / docs

- **Ф.0 prereq:** **смёржить/верифицировать D316–D321** (Plan 179/179.1) в `spec/decisions/` (сейчас только до D315) ИЛИ перенумеровать; затем D322+.
- **NEW D322** — io-core: `io.Read`/`io.Write`/`io.Seek` (sibling text-sink), `SeekFrom`, EOF/partial/EINTR-контракт (Q9), буферизация (BufWriter must-consume Q10), `IoError`/`ErrorKind`, stdin/stdout/stderr через `Io`, `str.from_utf8`/`Utf8Error`.
- **NEW D323** — fs: `Fs`-эффект (плумбинг libuv, best-effort-cancel Q4), `File` must-consume (Plan 80), byte-backed `Path` (Q1: WTF-8 Win), `Metadata`(→Timestamp), `write_atomic` 5-шаг + sync_all/sync_data, symlink/permissions (Q8), create_new/read_at/write_at.
- **NEW D324** — os: `Os`-эффект (args/env/cwd/exit-flush, set_env/set_cwd race-контракт). Process-модель → 180.1 D.
- error-index: `IoError`/`ErrorKind`-варианты + коды (`E_*`) для compile-проверок (must-consume leak/double-close); верифицировать при реализации.
- `docs/io-fs.md` — новый guide. Q-файл: §3.0 закрыть как RESOLVED.

## 6. Миграция

Аддитивно (`std/io`/`std/fs`/`std/os`). Отдельные byte-baseline-guarded коммиты: net `NetError`→`IoError`, net `str`→`[]u8` (mass compile-errors → per-file loop §10).
`std/_experimental/path` — **переписать** (не промоут — он str-based). Верификация против чистого бинаря; пересобрать `nova-cli` после `.nv` (`include_str!`).

## 7. Тесты (pos + neg; `nova_tests/io180/`, `fs180/`, `os180/`)

Раскладка как net/179 (pos standalone; neg `module neg.<name>` + `EXPECT_COMPILE_ERROR`); классификация по маркеру.

- **pos / контрактные (обязательные):**
  - **byte-roundtrip**: `write []u8` → `read` **байт-в-байт идентично**, включая **невалидный-UTF-8** контент;
  - **must-consume positive**: `File`/`BufWriter` `@close()` разряжает обязательство, его `Result` **наблюдается**; **close-error visible** (симул. ENOSPC → `close()` = `Err`, caller обязан обработать);
  - **non-UTF8 Path roundtrip** через `mem_fs`: `read_dir` отдаёт имя, которым **тот же файл переоткрывается** (лосслесс);
  - `read_to_string` на невалидном UTF-8 → `IoError{InvalidData}` (не паника/не lossy);
  - `read_exact`/`write_all` (partial+EINTR loop); `lines()` strip `\r\n`, `byte_lines()` raw; `copy(r,w)`;
  - `write_atomic` durability (fsync-file + same-dir-temp + fsync-dir; **torn-write neg — mandatory**); `EXDEV`→`CrossesDevices`;
  - `read_dir`/`walk_dir`(per-entry-error+SkipDir); `Metadata.len/modified`(`Timestamp`); `create_dir_all`/`remove_dir_all`(symlink-safe); `copy`/`rename`; symlink;
  - env get/set; args; cwd; **`with Fs = mem_fs()` детерминизм без диска** (одинаков между прогонами);
  - **cancellable-fs**: cancel in-flight → **не висит** + fd-state well-defined (НЕ mid-syscall-interrupt — best-effort, Q4).
- **neg (`EXPECT_COMPILE_ERROR`):** забыл `File.close()` → must-consume leak; double-close; use-after-consume (`File` после `close()`); `io.Write` без wildcard-арма на `ErrorKind` (если требуется).
- **neg (`IoError`/runtime):** open несуществующего → `NotFound`; permission → `PermissionDenied`; `create_new` существующего → `AlreadyExists`; `remove_dir` непустой → `DirectoryNotEmpty`; read после close; write в read-only → `ReadOnlyFilesystem`; NUL в Path → `InvalidInput`; `BrokenPipe` на закрытый pipe (процесс не падает).
- **big-tests** (вне дефолт-сэмпла): большие dir-обходы, large-file copy.

## 8. Критерии приёмки

0. **🔴 ОБЯЗАТЕЛЬНО: «без упрощений, как для прода».** Ни одного «решим потом» на критич. пути; каждая behavior-change — pos+neg + аргумент звучности.
1. io-core: `io.Read`/`io.Write`/`io.Seek` (sibling text-sink, без коллизии) + `BufReader` + **`BufWriter` must-consume** + `read_to_end`/`read_to_string`/`byte_lines`/`lines`/`read_exact`/`write_all`/`copy`; структурный `IoError`/`ErrorKind`; **EOF/partial/EINTR-контракт** реализован; stdin/stdout/stderr мокабельны; `str.from_utf8`→`Result` (Ф.0.5).
2. **byte-roundtrip** (incl non-UTF8) проходит; **close-error наблюдаема** (тест ENOSPC); **must-consume**: незакрытый `File`/`BufWriter` → compile-error, use-after-consume → compile-error.
3. fs: `File` must-consume (close→`Result`) + `OpenOptions`+`create_new`+`read_at`/`write_at`+`sync_all`/`sync_data`; **byte-backed `Path`** (non-UTF8 lossless roundtrip); `Metadata`(→`Timestamp`); `read_dir`/`walk_dir`; **`write_atomic`** (durability-тест: fsync-file+same-dir+fsync-dir, EXDEV→`CrossesDevices`); portable `Permissions`+unix-escape; best-effort-cancel (не висит, fd-state defined).
4. os: `args`/`env`/`cwd`/`exit`(flush)/`temp_dir`/… ; `Fs`/`Os`/`Io` **мокабельны** (`mem_fs` детерм. без диска).
5. Закрывает fs-часть `[M-91.10-fs-net-effects-formal]`. net `NetError`→`IoError` + net `str`→`[]u8` — отдельные коммиты (по Q3/Q6, byte-baseline-guarded).
6. **HARD-DEP-статусы честны:** Plan 80 (must-consume) — если не готов, `File`/`BufWriter` на affine-fallback с runtime-check, и это **явно** в плане/доке; D316–D321 в spec до D322+.
7. Полный регресс зелёный (батчами <10мин); большие fs-тесты вне дефолт-сэмпла.
8. spec: D322/323/324 (+ D316-321 смёржены); `docs/io-fs.md`; §1a differentiators.

## 9. Конвенции + координация

§1 (чекер), §3 (типы/эффекты из `.nv`), §5 spec-first (D-блоки до кода), §6 (коды + error-index), §7 (blast-radius + чистый бинарь), §8 (pos+neg, C-codegen).
**Координировать:** net-семейство (паттерн+инфра, миграция NetError/str), **Plan 80** (must-consume — HARD-GATE Ф.2), **179** (`Timestamp`), **83.3** (`Blocking` D50 — только CPU-обёртки), **172.4** (value-ABI), **91.18** (str+from_utf8 — Ф.0.5). После большой задачи — `project-creation.txt` + discussion-log + `simplifications.md`.

## 10. Фоновые агенты (если используются)

- **НЕ `git stash`** (worktree делят `.git` → repo-global коллизия, [[feedback-worktree-shared-stash]]); baseline — temp-worktree / commit+reset.
  Постоянный worktree `nova-p180` (naming `nova-pNN`) первой командой, самозарегистрироваться; cwd сбрасывается в main → **префикс абсолютным путём в каждой команде** ([[feedback_worktree_cwd_clarity]]).
- **Идемпотентность под rate-limit:** коммит после каждой фазы, без amend ([[feedback-commit-per-task]]); `git add` только конкретные файлы ([[feedback_git_add_specific]]);
  `git diff --cached --stat` перед commit ([[feedback-verify-index-before-commit]]); без `Co-Authored-By`; filter null перед действием.
- **Тесты:** `nova test` — не гейт корректности (byte-baseline), гейт = targeted pos+neg ([[feedback-nova-tests-not-correctness-gate]]); full `nova test` ~60-90мин > 10-мин cap → батчи <10мин ([[project-bash-timeout-10min-max]]);
  mass compile-errors (net str→[]u8) → **per-file loop** (`nova check FILE` → fix → re-check, [[feedback-test-fix-per-file-loop]]).
- **Worktree nova test:** env `NOVA_GC_LIB_DIR`/`INCLUDE_DIR` → main; libuv-submodule из main + удалить `libuv/.git` ([[project-worktree-nova-test-setup]]);
  **net/fs-тесты ОБЯЗАТЕЛЬНО с cwd=worktree** (libuv `repo_root=current_dir` — иначе fs-пути резолвятся неверно). **Пересобрать `nova-cli` после правок `.nv`** (`include_str!`). C-codegen only ([[feedback-no-interpreter]]). Не выдумывать синтаксис — `spec/decisions/` + `examples/` ([[feedback_nova_syntax]]).

## 11. Followup

`[M-180-io-fs-os]`. **Process → под-план 180.1** (Command/Child/Stdio/uv_spawn; гейт после 180 Ф.1-3). Отдельные коммиты: net `NetError`→`IoError`, net `str`→`[]u8`.
Followups: file-locking (advisory flock), mmap, `walk_dir`-filters, glob-промоут (`std/_experimental/path/glob.nv`), fs-watch (inotify/FSEvents), write_atomic Windows rename-replace-retry. Имена/детали — финал при реализации (после Ф.0).
