---
source_rev: 07df7d2c9
source_date: 2026-08-02
---

# I/O, файловая система и ОС в Nova

[English](io-fs.md) | **Русский**

> Пользовательский гайд по `std.io`/`std.fs`/`std.os` (Plan 176). Модель, сравнение
> с другими языками (7 языков) и рецепт долговечной записи `write_atomic`.

## TL;DR

```nova
// byte-first protocols, one shared error, mockable effects
type io.Read  protocol { @read(buf mut []u8) -> Result[int, IoError] }   // Ok(0) = EOF, only when buf is non-empty
type io.Write protocol { @write(data []u8) -> Result[int, IoError]; @flush() -> Result[(), IoError] }

with Fs = mem_fs() {                    // deterministic test, no disk
    consume f = File.create("out.txt".to_path())!!
    f.write("hi".bytes())!!
    // f un-closed at scope-exit -> compile error (D133); explicit @close() needed
    // for close-Result to reach a `Result`-flavored happy path
}
```

- **Один структурный `IoError{kind, raw_os, op}`** для io/fs/os (по образцу `io::Error` из Rust).
- **`File`/`BufWriter` — must-consume** (D133): забытый `@close()` — ошибка
  компиляции, а не утечка во время исполнения — и ошибка на закрытии (`ENOSPC`,
  квота, …) никогда не может быть молча проглочена.
- **`Fs`/`Os`/`Io` — эффекты**: `mem_fs()`/`mock_os()`/`mock_io()` дают
  детерминированные тесты без реального диска, окружения или консоли.
- **`str` никогда не является типом сырого I/O.** Весь I/O — `[]u8`; `str`
  появляется только через падающий `str.from_bytes(bytes) -> Result[str, Utf8Error]`
  (модель Rust/Go/Zig, а не молчаливый `U+FFFD` из Node).

## Модель

### Байт-в-первую-очередь

`str` в Nova проверяется на UTF-8 и иммутабельна — это не байтовый буфер. Каждая
поверхность сырого I/O (`io.Read`/`io.Write`, `Fs`, значения окружения `Os`,
`Net`) — это `[]u8`. Текст входит только через явное, могущее упасть
декодирование (`str.from_bytes`) или lossy-вариант (`str.from_bytes_lossy`) —
никогда неявно. Мост из текста обратно в байты для байтового приёмника — явный
`write_str` (`std.io`), никогда не неявная связка `str`→`Write` (см.
[Протоколы vs текстовый приёмник](#protocols-vs-the-text-sink) ниже).

### Одна ошибка, а не по одной на домен

```nova
type IoError value { ro kind ErrorKind, ro raw_os int, ro op str }
type ErrorKind enum
    | NotFound | PermissionDenied | AlreadyExists | NotADirectory | IsADirectory | DirectoryNotEmpty
    | WouldBlock | Interrupted | UnexpectedEof | WriteZero | InvalidInput | InvalidData
    | TimedOut | StorageFull | ReadOnlyFilesystem | CrossesDevices | BrokenPipe
    | ConnectionRefused | ConnectionReset | ConnectionAborted | NotConnected | AddrInUse | AddrNotAvailable
    | Unsupported | Other(int)   // OPEN enum -> a `match` needs a wildcard arm
```

`io`, `fs` и `os` возвращают `Result[T, IoError]`. `kind` — категоризированная,
сопоставимая проекция; `raw_os` — точный errno/`GetLastError` (авторитетный:
`kind` — best-effort, `raw_os` никогда не врёт); `op` называет упавшую операцию
для диагностики. Редкие/малоизвестные errno уходят в `Other(raw_os)`, а не молча
превращаются в ближайшую категорию.

**Почему не пооперационные наборы ошибок (Zig)?** Рассмотрено и отклонено: точная
error-union на каждую операцию требует инфраструктуры error-union и дробит
обработку ошибок по местам вызова. Nova берёт модель Rust — один открытый
`ErrorKind` композируется через `match`, и (fs) цепочка `source` несёт причину
вместо своего набора на функцию.

**`net` остаётся отдельным типом.** `NetError` (`std.net`) не сливается в
`IoError` — у его собственных `#stable` `@to_str()`-строк сохраняется точная
формулировка. Вместо этого он получает *аддитивную* best-effort проекцию
`NetError.@to_error_kind() -> ErrorKind` / `@to_io_error(op) -> IoError`,
используемую, чтобы дать `TcpStream.@read`/`@write` структурное соответствие
`io.Read`/`io.Write` (так что `TcpStream` и `File` оба удовлетворяют
generic-ограничению `[R Read]`/`[W Write]`), не трогая остальные места вызова
эффекта `Net` (`write_all`/`read_to_vec`/раздельные половины остаются с
`NetError` без изменений).

### Must-consume `File`/`BufWriter` (D133)

```nova
type File consume { … }
fn File @close(consume self) -> Result[(), IoError]   // the ONLY explicit discharge
```

Незакрытый `File` или `BufWriter` на выходе из области видимости — это
`D133-not-consumed` — ошибка **компиляции**, а не утечка во время исполнения,
а двойное закрытие невозможно по построению. `consume f = open(…) { … }`
разряжается через `@cleanup` при выходе из области (ошибка там присоединяется
к цепочке suppressed, никогда не теряется); явный `@close()` — это `Result`-путь,
когда ошибка закрытия должна достичь happy path. Это самый большой отличительный
признак по сравнению со всеми сверстниками из таблицы ниже — ни один из них не
делает «забыл закрыть» или «проглотил ошибку закрытия» невозможным для написания.

### Мокабельные эффекты

`Fs`/`Os` — plumbing-эффекты (на libuv под `real_fs()`/`real_os()`; пользователь
никогда не вызывает их напрямую, только методы `File`/env и т.п., построенные
поверх — та же форма, что у `Net` в `std.net`). `mem_fs()`/`mock_os()`/`mock_io()`
дают in-memory обработчик для детерминированных тестов: ни диска, ни мутации
окружения, ни консоли, включая инъекцию ошибок (`ENOSPC`/`EIO` на `mem_fs()`,
для тестов ошибки закрытия и порванной записи).

### EOF / частичное чтение / EINTR

- `read()` → `Ok(0)` сигнализирует EOF **только когда буфер непустой** — никогда
  не футган Го `(n > 0, io.EOF)`, где чтение может нести одновременно и данные,
  и сигнал EOF.
- Короткое чтение (`0 < n < len`) — норма, не EOF; частичная запись допустима
  (`write_all` крутится до конца). `Ok(0)` в середине записи → `WriteZero`.
- `Interrupted` (EINTR) повторяется автоматически внутри каждого хелпера-цикла
  (`read_exact`/`read_to_end`/`read_to_string`/`write_all`).

### `write_atomic` — действительно долговечна, а не просто выглядит атомарной

```
1. create a temp file in the SAME directory (O_EXCL)
2. write_all the data
3. fsync the temp file        (sync_all)
4. atomic rename/replace over the target
5. best-effort fsync of the parent directory (no-op on Windows)
```

Обычный `write`, вернувший `Ok`, **не** долговечен без шагов 3 и 5 — данные всё
ещё могут быть потеряны при пропадании питания, даже если переименование уже
произошло.

> **Анти-прецедент.** Опция атомарной записи `.atomic` в Swift и `AtomicFile`
> в Zig делают только шаги 1/2/4 — временный файл + переименование, без `fsync`.
> Это атомарно *относительно читателей* (никто никогда не наблюдает наполовину
> записанный файл), но **не долговечно против потери питания**: переименование
> может быть переупорядочено журналом самой файловой системы раньше попадания
> данных на диск, а падение между переименованием и финальным сбросом может
> оставить «новый» файл пустым или обрезанным. `write_atomic` в Nova всегда
> делает полный рецепт из 5 шагов; варианта-сокращения нет.

### Протоколы vs текстовый приёмник

`io.Read`/`io.Write` — **байтовые** протоколы, квалифицированные модулем
(`import std.io`) и намеренно **соседние** с prelude-текстовым приёмником `Write`
(`@display`/`Debug`-форматирование, D258/D374). Слияние притащило бы текстовые
семантики в байтовый I/O — ровно та путаница, ради избежания которой существует
разделение `Writer` vs `OutputStream` в Java. Мост — явный `write_str(w, s)`.

### `ReadFs` — один VFS-протокол поверх диска и встроенного каталога

`ReadFs` (`std.fs`, [амендмент D323](../../spec/decisions/04-effects.md#d323), Plan 210) — read-only виртуальная
файловая система — `@read_file(path) -> Result[[]u8, IoError]` +
`@path_exists(path) -> Result[bool, IoError]` — которой соответствуют **`DirFs`**
(вид с корнем поверх реального диска, эффект `Fs`) и **`EmbeddedDir`**
(результат `embed_dir("dir")`, чистый). Классический кейс «dev отдаёт с диска
с live-reload, prod отдаёт вшитую в бинарь копию» становится одной
generic-функцией `fn serve[F ReadFs](assets F, ...)`, мономорфизируемой дважды —
без рантайм-переключения `dyn`, потому что в Nova нет effectful-vtable
диспетчеризации (амендмент D122), которая пронесла бы эффект `Fs` типа `DirFs`
через экзистенциальное значение `ReadFs`. Ветвление между `DirFs`/`EmbeddedDir`
живёт в месте вызова (какой mono инстанцировать), а не в переменной.
Соответствие `EmbeddedDir` — **метод-расширение** (D287, объявлен в `std.fs`,
а не в домашнем модуле `EmbeddedDir` — `prelude.embed`) — структурное
соответствие через generic-ограничение `[F ReadFs]` видит его точно как
собственный (`std/src/fs/readfs_test.nv`). `list`/индекс каталога намеренно
**не** в протоколе (сканирование реальной ФС эффектно, дорого и недетерминировано
там, где встроенная сторона бесплатна и стабильна) — см.
[`docs/plans/210-embed-dir.md`](../plans/210-embed-dir.md) §6б за полным дизайном.

## Сравнение с другими языками (7 языков)

| Аспект | Go | Rust | TS/Node | Kotlin | Java | Zig | Swift |
|---|---|---|---|---|---|---|---|
| io-абстракция | `io.Reader/Writer` | `Read/Write/Seek` + `BufReader` | `stream.*` | okio/java | `InputStream`/nio | `std.Io` Reader/Writer (0.14+) | `FileHandle`/swift-system `FileDescriptor` |
| close | `defer Close()` (**ошибка игнорируется**) | `Drop` (**проглатывает**) | `await close()` | `use{}` (подавляется) | try-with-resources (подавляется) | `close()->void` (**ошибке некуда попасть**) | `close() throws` (легко забыть) |
| путь | `string` | `Path`/`OsStr` (байты) | string | nio.Path | nio.Path (**`InvalidPathException`**) | `[]const u8` (байты) | `FilePath` (байты, swift-system) |
| ошибка | sentinels | **`io::Error{ErrorKind}`** | `err.code` string | `IOException` | иерархия | **пооперационные наборы ошибок** | типизированный `Errno` |
| EOF/частичное | `(n>0, io.EOF)` (**футган**) | `Ok(0)`=EOF, `read_exact` | promise | — | partial-read | `0`=EOF / наборы ошибок | на основе данных |
| атомарная запись | вручную | вручную | вручную | вручную | `ATOMIC_MOVE` | `AtomicFile` (**без fsync**) | `.atomic` (**без fsync**) |
| TOCTOU | — | — | — | — | — | **🏆 dir-scoped ops (openat by design)** | — |
| async | goroutine | sync/`tokio::fs` (пул) | libuv | suspend | NIO | sync/evented | actors |

**Nova берёт:** `ErrorKind`/`OpenOptions`/`create_new`/`Path`/`read_at`-`write_at`/
`Ok(0)=EOF` из Rust; эргономику `ReadFile`/`WriteFile` и переносимые склейки путей
из Go; `ATOMIC_MOVE` из Java; байтовый прецедент `FilePath` из swift-system.

**Nova избегает:** молча проглатывающий close и `(n>0, EOF)` из Go;
проглатывающий `Drop` из Rust; молчаливую замену `U+FFFD` из Node;
`InvalidPathException` из Java; атомарность-без-fsync из Swift/Zig;
`close()->void` из Zig.

**Followup, ещё не выпущено:** модель dir-scoped `openat`/`unlinkat` из Zig
(анти-TOCTOU по построению) — отслеживается как `[M-176-dir-scoped-ops]`; текущий
`remove_dir_all` — обычная рекурсия по путям.

## Где Nova превосходит каждого сверстника

- **Must-consume `File`/`BufWriter` (D133).** `@close()` — единственный явный
  разряд; незакрытый хендл — ошибка компиляции, а ошибка на закрытии не может
  быть молча уронена. Превосходит все 7: `defer Close()` в Go и `Drop` в Rust оба
  проглатывают; Java/Kotlin подавляют на ошибочном пути; `await using` в Node
  проглатывает; `close()` в Zig возвращает `void` (ошибке некуда деться), а его
  `Io.Writer` требует ручного `flush()` (дисциплина, а не компилятор); `close()
  throws` в Swift — но забыть его вызвать компилируется без ошибок.
- **Мокабельные `Fs`/`Os`/`Io`.** `with Fs = mem_fs() { … }` — детерминированный
  тест, без диска, без DI-фреймворка. Go нужен `afero`, Rust — абстракция трейта,
  Java/Node — monkey-patching, у Zig нет вообще никакой истории (ручной DI),
  Swift нужен протокол-вирусный mock `FileManager`.
- **Байт-в-первую-очередь сделано правильно.** `str` проверяется на UTF-8;
  `read_to_string` *падающий* (`Result`, а не молчаливое повреждение `U+FFFD`
  из Node). Паритет с Zig по байтовым срезам, но у Zig нет проверяемого
  строкового типа, на который можно опереться.
- **Типизированный `Timestamp`** (Plan 175) для mtime/atime/ctime, каждое —
  `Option[Timestamp]` — превосходит трио Date/ms/ns в Node, `Sys() any` в Go,
  голый `i128` наносекунд в Zig.
- **Структурный `IoError{kind, raw_os, op}`** с исчерпывающим
  (принудительно-через-wildcard) `ErrorKind` — превосходит stringly-typed
  `err.code` в Go/Node, шум checked-исключений в Java; паритет с `ErrorKind`
  в Rust и типизированным `Errno` в swift-system. (Пооперационные наборы ошибок
  в Zig — реальная альтернатива — рассмотрены и отклонены, см. выше.)
- **`write_atomic`, которая действительно долговечна**, один примитив, 5 шагов —
  пробел в *каждом* сверстнике (Go/Rust/Node/Kotlin/Java делают руками; Swift/Zig
  поставляют недолговечную «атомарность», не защищающую от потери питания).
- **Байтовый `Path`.** Несёт реальные не-UTF-8 Unix-имена / WTF-8 Windows-имена,
  которые JVM вообще не может представить (`InvalidPathException`) и TS/Deno не
  могут представить вовсе; паритет с `Path`/`OsStr` в Rust и `FilePath` в
  swift-system (тоже байтовый), байтовым `[]const u8` в Zig.

## См. также

- [`spec/decisions/04-effects.md`](../../spec/decisions/04-effects.md) — D322
  (io-core), D323 (fs), D324 (os), амендмент D302 (net-проекция).
- [`docs/guide/consume-types.md`](consume-types.md) — механика must-consume
  (D133/D180), лежащая в основе `File`/`BufWriter`.
- [`docs/plans/176-io-fs-os.md`](../plans/176-io-fs-os.md) — зонтичный план
  (таблица решений Q1-Q15, история фаз).
