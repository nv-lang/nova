<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 180 — I/O + Filesystem + OS: `io`-core (Read/Write/streams) + `Fs`-эффект (файлы/директории/пути) + `Os` (env/args/process)

> **Top-level umbrella-план.** Создан 2026-06-22. **Статус:** 📋 DRAFT (pre-hardening — будет прокачан cross-lang workflow,
> как 179; открытые вопросы §3.0 ещё НЕ закрыты).
> **Маркер:** `[M-180-io-fs-os]`. **Запуск:** «**выполни план 180**».
> **Эталон:** **Go / Rust / TS / Kotlin / Java** (io+fs+os). **Архитектура — по net-семейству** (`TcpNet`/`AddrNet`,
> [std/net/effect.nv](../../std/net/effect.nv)): эффект = внутренний плумбинг (libuv-backed, async, cancellable), юзер ходит
> через type-методы; ошибки — `Result[T, IoError]`.
> **Зависит/координация:** libuv `uv_fs_*`/`uv_spawn` + M:N park/wake (как net); **Plan 80 must-consume** (линейный `File`,
> обязан `.close()`); **Plan 179** `Timestamp` (метаданные mtime/atime); **Plan 83.3** `Blocking`-эффект (D50); value-ABI = 172.4;
> str/format = [91.18](91.18-str-unicode-api.md); промоут `std/_experimental/path/`. Закрывает `[M-91.10-fs-net-effects-formal]` (fs-часть).
> **Фоновые агенты:** §10. **Сквозной критерий (обязательный):** «**без упрощений, как для прода**» (крит §8.0).

---

## 1. Зачем

В Nova **нет `std/fs`, `std/io`, `std/os`** — только консольный `print`/`println` (эффект `Io`), `Write`-протокол (sink для
`@display`) и `ReadBuffer`/`WriteBuffer` (in-memory). Нельзя открыть файл, прочитать директорию, узнать env, спавнить процесс.
Это закрывает backend/CLI-нишу языка целиком (Plan 18 признал fs/os P0-для-0.2; followup `[M-91.10-fs-net-effects-formal]`).
Net уже сделан (Plan 91.12/83.12) — fs/io/os строятся по той же модели и переиспользуют её инфру (libuv-park, error-pattern).

## 1a. Где Nova может быть ЛУЧШЕ peers (differentiators — в доку)

- **`Fs`/`Os`/`Io` — мокабельные эффекты.** Тест подменяет файловую систему / env / часы handler'ом (`with Fs = mem_fs() { … }`) →
  **детерминизм без реального диска** и без DI-плумбинга. Go (нет DI — нужен `afero`/интерфейс вручную), Rust (нужен trait-abstraction),
  Java (нет встроенного), Node (`mock-fs`/jest-моки глобальны) — все слабее.
- **`File` — must-consume (Plan 80, линейный).** Компилятор **обязывает** закрыть файл, а `.close()` **возвращает `Result`**
  (ошибка close видна). Бьёт Rust (`Drop` глотает ошибку close), Go (`defer f.Close()` — ошибка игнорится, классический footgun),
  Java try-with-resources (suppressed-exceptions), Node (легко забыть).
- **Метаданные времени = `Timestamp`** (typed, из 179), не сырые epoch-int.
- **Структурный `IoError`** (kind + errno + path) + cancellable async-fs через M:N (как net).
- **Path — typed** (не голая строка как Go), но без OsString-боли Rust (решение §3.0-Q1).

## 2. Эталон (cross-lang — io / fs / os; уточнить hardening-workflow'ом)

| Аспект | Go | Rust | TS/Node | Kotlin | Java |
|---|---|---|---|---|---|
| io-абстракция | `io.Reader`/`Writer`, `bufio` | `Read`/`Write`/`Seek`+`BufReader` | `stream.Readable/Writable` | `okio` / java-interop | `InputStream`/`Reader`/`nio.channels` |
| файл | `os.File` (`Open`/`Create`) | `std::fs::File`+`OpenOptions` | `fs.promises.open`/`FileHandle` | `java.io.File`/`Path` | `Files`/`FileChannel`/`RandomAccessFile` |
| close | `defer f.Close()` (err игнор) | `Drop` (err глотается) | `await fh.close()` | use{} | try-with-resources |
| path | `path/filepath` (string) | `Path`/`PathBuf` (`OsStr`) | `path` (string) | `java.nio.file.Path` | `java.nio.file.Path` |
| convenience | `os.ReadFile`/`WriteFile` | `fs::read`/`write`/`read_to_string` | `fs.readFile` | `File.readText` | `Files.readString`/`readAllBytes` |
| atomic write | вручную (temp+rename) | вручную / `tempfile` crate | вручную | вручную | `Files.move(ATOMIC_MOVE)` |
| metadata | `os.Stat`→`FileInfo` | `fs::metadata`→`Metadata` | `fs.stat`→`Stats` | `Files.readAttributes` | `Files.readAttributes`/`BasicFileAttributes` |
| dir-iter | `os.ReadDir`/`WalkDir` | `read_dir`/`walkdir` | `fs.readdir`/`opendir` | `Files.list`/`walk` | `Files.list`/`walk`/`DirectoryStream` |
| env/args | `os.Environ`/`os.Args` | `env::var`/`env::args` | `process.env`/`argv` | `System.getenv`/args | `System.getenv`/args |
| process | `os/exec.Command` | `std::process::Command` | `child_process.spawn` | `ProcessBuilder` | `ProcessBuilder`/`Process` |
| error-model | `error` + `errors.Is(os.ErrNotExist)` | `io::Error`{`ErrorKind`} | `Error`{`code:'ENOENT'`} | `IOException` | `IOException` иерархия |
| async | goroutine+blocking | sync (или tokio::fs) | promises (libuv) | suspend (coroutines) | NIO async channels |

**Что взять:** Rust `ErrorKind` (структурный io::Error), `OpenOptions`-builder, `Path`-тип, единый `io::Error` для io+fs;
Go `os.ReadFile`/`WriteFile`-эргономика + `path/filepath` (portable); Java `Files.move(ATOMIC_MOVE)` (атомарность); Kotlin `use{}`
(но у нас лучше — must-consume). **Избегать:** Go silent-close-err + path-as-string; Rust `OsStr`-боль (если сможем); Node legacy
sync/callback дубли; Java checked-exception-шум (у нас Result).

## 3. Архитектура (effects-плумбинг + io-core протоколы + типы)

**Принцип (net-precedent):** `Fs`/`Os` — **внутренние плумбинг-эффекты** (юзер не зовёт; libuv-backed, async, cancellable),
user-API — type-методы + free-fns. `Io` (консоль) уже есть — расширяем (stdin-read). io-core (`Read`/`Write`/`Seek`) —
**протоколы**, обобщающие файлы / net / консоль / память.

**🔑 Byte-first (фундаментальный инвариант).** `str` в Nova — **UTF-8, immutable, validated** → это **НЕ байтовый буфер**
и **НЕ** контейнер для сырого чтения. Весь raw-I/O — **`[]u8`**: `Read.@read(buf mut []u8)`, `Write.@write([]u8)`,
`read(path)->[]u8`, `read_to_end->[]u8`. `str` появляется **только через явный fallible UTF-8-декод**: `read_to_string(path) ->
Result[str, IoError]` / `str.from_utf8(bytes) -> Result[str, Utf8Error]` (невалидный UTF-8 → ошибка, не паника, не lossy-по-умолчанию;
+ опц. `from_utf8_lossy`). `lines()` — **два варианта**: `byte_lines() -> []u8`-итератор (raw) и `lines() -> Result[str]`-итератор
(UTF-8-декод per-строка). Интегрируется с `ReadBuffer`/`WriteBuffer` (Plan 85 — уже байтовые). Это как Rust (`read`→bytes,
`read_to_string`→validated) / Go (`[]byte` везде); **никогда** не читать «в `str`».

```nova
// io-core протоколы (Read — NEW; Write — расширить байтовым sink)
type Read protocol {  @read(buf mut []u8) -> Result[int, IoError] }        // bytes; 0 = EOF
type Write protocol { @write(data []u8) -> Result[int, IoError]; @flush() -> Result[(), IoError] }
type Seek protocol {  @seek(pos SeekFrom) -> Result[u64, IoError] }
type SeekFrom | Start(u64) | End(int) | Current(int)

// плумбинг-эффекты (юзер не трогает)
type Fs effect { open(path Path, opts OpenOptions) -> Result[File, IoError]
                 read(f File, buf mut []u8) -> Result[int, IoError]
                 write(f File, data []u8) -> Result[int, IoError]
                 close(f File) -> Result[(), IoError]
                 stat(path Path) -> Result[Metadata, IoError]   // + lstat/readdir/mkdir/unlink/rename/realpath/symlink/chmod/copyfile … }
type Os effect { args() -> []str; env(key str) -> Option[str]; set_env(key str, v str) -> ()
                 cwd() -> Result[Path, IoError]; exit(code int) -> never; temp_dir() -> Path; home_dir() -> Option[Path] }
```

**Типы (user-facing):**
- **`File`** — `consume`/must-consume (Plan 80): `File.open(path)`/`File.create(path)`/`File.options() -> OpenOptions`(builder);
  impl `Read`+`Write`+`Seek`; `@metadata()`, `@sync_all()` (fsync), `@close() -> Result[(), IoError]` (**обязателен**, close-err видна).
- **`Path`** (промоут `std/_experimental/path/`): immutable; `@join`/`@parent`/`@file_name`/`@extension`/`@components`/`@is_absolute`/`@to_str`; portable-сепараторы.
- **`Metadata`**: `@len() -> u64`, `@is_file/@is_dir/@is_symlink`, `@modified/@accessed/@created() -> Timestamp` (179), `@permissions()`.
- **`DirEntry`** + `read_dir(path) -> Result[DirIter]` (ленивый итератор) + `walk_dir` (рекурсивный).
- **`BufReader[R Read]`/`BufWriter[W Write]`** — буферизация; `read_to_end() -> Result[[]u8]` (raw), `read_to_string() -> Result[str]`
  (fallible UTF-8), `byte_lines()`/`lines()` (raw `[]u8` / fallible-`str`), `read_until(byte)`, `copy(r, w) -> Result[u64]`.
- **`Stdin`/`Stdout`/`Stderr`** — через `Io`-эффект (мокабельны).
- **`Command`/`Child`/`Output`/`ExitStatus`** (process, Ф.4): `Command.new(prog).arg(…).env(…).cwd(…).stdin/out/err(Stdio).spawn()/output()/status()`.

**Free-fns (convenience, Fs-эффект):** `read(path)->Result[[]u8]`, `read_to_string(path)`, `write(path, data)`, `append(path, data)`,
**`write_atomic(path, data)`** (temp+fsync+rename — production), `exists(path)->bool`, `metadata`/`symlink_metadata`, `copy`, `rename`,
`remove_file`, `remove_dir`, `remove_dir_all`, `create_dir`, `create_dir_all`, `canonicalize`, `read_link`, `symlink`, `set_permissions`,
`temp_file()`/`temp_dir()`.

**`Os`-free-fns:** `args()`, `env(key)`/`set_env`/`vars()`, `cwd()`/`set_cwd`, `exit(code)`, `temp_dir()`/`home_dir()`, `pid()`, `hostname()`.

## 3b. Модель ошибок — структурный `IoError` (Rust `ErrorKind`-precedent)

```nova
type IoError { ro kind ErrorKind, ro raw_os int, ro path Option[Path] }   // value-record
type ErrorKind | NotFound | PermissionDenied | AlreadyExists | NotADirectory | IsADirectory
              | DirectoryNotEmpty | WouldBlock | Interrupted | UnexpectedEof | InvalidInput
              | TimedOut | StorageFull | ReadOnlyFilesystem | BrokenPipe | Other(int)        // mirror std io::ErrorKind
fn IoError @to_str() -> str
```
**ОТКРЫТО (Q):** единый `IoError` для io+fs **vs** отдельный `FsError`; и **унифицировать ли с net `NetError`** (Rust — один `io::Error`).

## 3.0. Открытые вопросы (ЗАКРЫТЬ при hardening — сейчас draft)

| # | Вопрос | Кандидаты | Склонность |
|---|---|---|---|
| Q1 | **Не-UTF-8 пути** (Unix=байты, Windows=UTF-16; Nova `str`=UTF-8) | (a) `Path`=байты + `@to_str()->Option`/`to_str_lossy`; (b) UTF-8-only + документ. лимит; (c) WTF-8 как Rust | (a) — байтовый Path + lossy-view (как Rust OsStr, но проще API) |
| Q2 | **`File`-close модель** | (a) must-consume (Plan 80) — обязан `.close()`; (b) `consume` + `with`-scope; (c) RAII-defer | (a) must-consume — Nova-differentiator (close-err видна); коорд. Plan 80 |
| Q3 | **`IoError` единый vs `FsError`/`NetError`** | (a) один `IoError` (Rust); (b) отдельные | (a) один `IoError` для io+fs; net мигрировать на него (или alias) — Rust-урок |
| Q4 | **async-модель fs** | (a) libuv `uv_fs_*` threadpool + fiber-park (как net, cancellable); (b) sync + `Blocking`-эффект D50 | (a) async-park (консистентно с net), `Blocking` для CPU-bound-обёрток |
| Q5 | **process (Ф.4) — в этом плане или под-план 180.1** | (a) под-план 180.1 (subprocess огромен: pipes/PATH-resolve/signals/env-inherit); (b) фаза здесь | (a) под-план 180.1 — гейтить отдельно |
| Q6 | **Read/Write протокол vs существующий `Write`(@display sink)** | (a) байтовый `Write` отдельен от text-sink; (b) объединить | уточнить — `@display`-sink text-ориентирован; байтовый io.Write — сибling |
| Q7 | **lines() и CRLF** | (a) split `\n`, strip trailing `\r` (Rust); (b) только `\n` | (a) strip `\r\n` (Rust-семантика) |
| Q8 | **permissions портабельно** | Unix-octal mode vs Windows readonly/ACL | портабельный subset + платформенный escape (как Rust) |
| Q9 | **FFI str-граница** (C НЕ понимает `str` как `char*`) | (a) nova_rt-шим берёт `nova_str` `{ptr,len}` (Plan 139 ABI) → внутри **NUL-terminate + reject interior-NUL** (`InvalidInput`, как Rust `CString::new`), т.к. `str` не-NUL-terminated и может нести NUL; libc/libuv хотят `char*`; (b) extern на `CStr` + конверсия `str→CStr` в Nova | (a) — наши шимы берут `nova_str` (net-паттерн `socket_addr_from_str(s str)`) + NUL-terminate/interior-NUL-check на границе. Прямой биндинг libc `getenv(str)` **запрещён** |

## 4. Фазы (декомпозиция)

**Dep-chain:** Ф.0 → Ф.1 (io-core) → Ф.2 (fs) → Ф.3 (os) → [Ф.4/180.1 process] → Ф.5 (tests/docs). **Коммит после фазы** (§10).

- **Ф.0 — gate (без кода).** Закрыть §3.0 Q1-Q8; D-блоки (D322 io-core, D323 fs, D324 os — после D321=179.1, сверить). Координация
  Plan 80 (must-consume) / 83.3 (Blocking) / 179 (Timestamp). **GATE.**
- **Ф.1 — io-core.** `Read`/`Write`/`Seek` протоколы; `SeekFrom`; `BufReader`/`BufWriter`; `read_to_string`/`read_to_end`/`lines`/`copy`;
  структурный `IoError`/`ErrorKind`; `stdin`/`stdout`/`stderr` через `Io`-эффект (мокабельны). DEP: 91.18 (str), value-records.
- **Ф.2 — fs.** `Fs`-эффект (libuv `uv_fs_*`, async-park, cancellable); `File` (must-consume, Read/Write/Seek) + `OpenOptions`;
  `Path` (промоут experimental); `Metadata` (→`Timestamp`); `DirEntry`/`read_dir`/`walk_dir`; convenience-fns (read/write/**write_atomic**/
  copy/rename/remove*/create_dir*/canonicalize/symlink/temp); permissions (портабельный subset). **Закрывает fs-часть `[M-91.10-…]`.** DEP: Ф.1, 179, 80.
- **Ф.3 — os.** `Os`-эффект: `args`/`env`/`set_env`/`vars`/`cwd`/`set_cwd`/`exit`/`temp_dir`/`home_dir`/`pid`/`hostname`; мокабельно. DEP: Ф.1.
- **Ф.4 — process (кандидат на под-план 180.1).** `Command`/`Child`/`Output`/`ExitStatus`/`Stdio`; `uv_spawn`; pipes (stdin/out/err);
  PATH-resolve; env-inherit/override; cancel/kill/wait. ⚠ Самая тяжёлая часть. DEP: Ф.1-Ф.3.
- **Ф.5 — тесты + spec/docs.** §7 pos+neg; D-блоки; новый `docs/io-fs.md` (модель + «Nova↔Go/Rust/…» + §1a differentiators).

## 5. Spec / D / Q / docs

- **NEW D322** — io-core: `Read`/`Write`/`Seek` протоколы, `SeekFrom`, буферизация, `IoError`/`ErrorKind`, stdin/stdout/stderr через `Io`.
- **NEW D323** — fs: `Fs`-эффект (плумбинг, libuv, cancellable), `File` (must-consume), `Path`-модель (§3.0-Q1), `Metadata`(→Timestamp),
  атомарная запись, symlink/permissions-семантика.
- **NEW D324** — os: `Os`-эффект (args/env/cwd/exit/…), process-модель (или → 180.1 D).
- error-index: коды/`IoError`-варианты (верифицировать при реализации).
- `docs/io-fs.md` — новый guide. Q-файл: §3.0 Q закрыть как RESOLVED после Ф.0.

## 6. Миграция

Аддитивная поверхность (новые модули `std/io`, `std/fs`, `std/os`). Возможная миграция: net `NetError` → единый `IoError` (Q3);
промоут `std/_experimental/path/` → `std/fs/path`. Верификация против чистого бинаря; пересобрать `nova-cli` после правок `.nv` (`include_str!`).

## 7. Тесты (pos + neg; раскладка `nova_tests/io180/`, `fs180/`, `os180/`)

- **pos:** Read/Write/Seek round-trip (memory + file); BufReader.lines(); copy(r,w); `read_to_string`/`write`/`write_atomic` файла;
  `read_dir`/`walk_dir`; `Metadata.len/modified`(Timestamp); `create_dir_all`/`remove_dir_all`; `rename`/`copy`; symlink; env get/set;
  args; cwd; `Command.output()` (echo); **мок `with Fs = mem_fs()`** (детерминизм без диска); **must-consume**: незакрытый `File` → compile-error.
- **neg (`EXPECT_COMPILE_ERROR`):** забыл `File.close()` → must-consume compile-error; double-close.
- **neg (`IoError`/runtime):** open несуществующего → `NotFound`; permission → `PermissionDenied`; read после close; write в read-only;
  `remove_dir` непустой → `DirectoryNotEmpty`; cancel в полёте fs-операции.
- **контрактные:** не-UTF-8 путь (Q1); атомарность `write_atomic` (нет torn-file при краше — по возможности); cancellable-fs не висит.

## 8. Критерии приёмки

0. **🔴 ОБЯЗАТЕЛЬНО: «без упрощений, как для прода».** Ни одного «решим потом» на критич. пути; каждая behavior-change — pos+neg + аргумент звучности.
1. io-core: `Read`/`Write`/`Seek` + `BufReader`/`BufWriter` + `read_to_string`/`lines`/`copy`; структурный `IoError`/`ErrorKind`; stdin/stdout/stderr мокабельны.
2. fs: `File` (must-consume, close→`Result`) + `OpenOptions`; `Path`; `Metadata`(→`Timestamp`); `read_dir`/`walk_dir`; полный convenience-набор + **`write_atomic`**; async+cancellable.
3. os: `args`/`env`/`cwd`/`exit`/`temp_dir`/… ; `Fs`/`Os`/`Io` **мокабельны** через handler (детерм. тест без диска/env).
4. process (Ф.4 или 180.1): `Command.spawn/output/status` + pipes + cancel.
5. paths: решение Q1 реализовано (не-UTF-8 обрабатывается, не падает).
6. Закрывает fs-часть `[M-91.10-fs-net-effects-formal]`; net-error унификация (Q3) — по решению.
7. Полный регресс зелёный (батчами <10мин); большие fs-тесты вне дефолт-сэмпла.
8. spec: D322/323/324; `docs/io-fs.md`; §1a differentiators.

## 9. Конвенции + координация

§1 (чекер), §3 (типы/эффекты из `.nv`, не хардкод), §5 spec-first (D-блоки до кода), §6 (коды + error-index), §7 (blast-radius + чистый
бинарь), §8 (pos+neg, C-codegen). **Координировать:** net-семейство (паттерн+инфра), **Plan 80** (must-consume `File`), **179** (`Timestamp`),
**83.3** (`Blocking` D50), **172.4** (value-ABI), **91.18** (str). После большой задачи — `project-creation.txt` + discussion-log + `simplifications.md`.

## 10. Фоновые агенты (если используются)

- **НЕ `git stash`** (worktree делят `.git` → repo-global коллизия, [[feedback-worktree-shared-stash]]); baseline — temp-worktree / commit+reset.
  Постоянный worktree `nova-p180` (naming `nova-pNN`) первой командой, самозарегистрироваться; cwd сбрасывается в main → префикс абсолютным путём.
- **Идемпотентность под rate-limit:** коммит после каждой фазы, без amend ([[feedback-commit-per-task]]); `git add` только конкретные файлы
  ([[feedback_git_add_specific]]); `git diff --cached --stat` перед commit; без `Co-Authored-By`; filter null перед действием.
- **Тесты:** `nova test` — не гейт корректности (byte-baseline), гейт = targeted pos+neg ([[feedback-nova-tests-not-correctness-gate]]);
  full `nova test` ~60-90мин > 10-мин cap → батчи <10мин ([[project-bash-timeout-10min-max]]); mass compile-errors → per-file loop.
- **Worktree nova test:** env `NOVA_GC_LIB_DIR`/`INCLUDE_DIR` → main; libuv-submodule из main + удалить `libuv/.git` ([[project-worktree-nova-test-setup]]);
  **net/fs-тесты нужны cwd worktree** (libuv repo_root=cwd). **Пересобрать `nova-cli` после правок `.nv`** (`include_str!`). C-codegen only ([[feedback-no-interpreter]]).
  Не выдумывать синтаксис — `spec/decisions/` + `examples/` ([[feedback_nova_syntax]]).

## 11. Followup

`[M-180-io-fs-os]`. process → возможно под-план **180.1** (Q5). Возможные followups: file-locking (advisory flock), mmap, `walk_dir`-фильтры,
glob (`std/_experimental/path/glob.nv` промоут), watch (inotify/FSEvents). Имена/детали — финал при реализации (после Ф.0 + hardening).
**Перед стартом — прокачать cross-lang workflow'ом (Go/Rust/TS/Kotlin/Java), как 179** → закрыть §3.0 Q1-Q8.
