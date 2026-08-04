---
source_rev: 07df7d2c9
source_date: 2026-08-02
---

[English](embed.md) | **Русский**

# Встраивание файлов и папок в бинарь: `embed` / `embed_dir`

> Пользовательский гайд по компайл-тайм интринсикам `embed("file")`
> ([D412](../../spec/decisions/03-syntax.md#d412), План 186) и `embed_dir("dir")`
> ([D412-амендмент](../../spec/decisions/03-syntax.md#d412) в том же файле,
> План 210). Оба — интринсики класса C (файловый ввод на
> этапе компиляции, прецеденты — Rust `include_bytes!`, Go `//go:embed`,
> Zig `@embedFile`, C23 `#embed`).

## TL;DR

```nova
ro logo  = embed("assets/logo.png")     // []u8 — the content of ONE file
ro site  = embed_dir("../frontend")     // EmbeddedDir — the WHOLE directory, recursively

assert(site.len() == 3)
assert(site.has("index.html"))
ro index = site.get("index.html")       // Option[[]u8]
```

- Аргумент — **только строковый литерал** (путь известен на компиляции);
  резолвится относительно `.nv`-файла вызова, граница — package-root вызова.
- Содержимое становится частью `.rodata` бинаря: **нулевая копия** payload'ов
  (`ro`-биндинг = вид над статикой, не куча).
- Встроенные файлы — зависимости сборки: изменение/добавление/удаление
  любого из них инвалидирует кэш инкрементальной сборки.
- `embed` → `[]u8`. `embed_dir` → иммутабельный `EmbeddedDir` (карта
  путь→байты, отсортирована, бинарный поиск).

## Содержание

- [`embed("path")` — один файл](#embedpath--один-файл)
- [`embed_dir("dir")` — вся папка рекурсивно](#embed_dirdir--вся-папка-рекурсивно)
- [API `EmbeddedDir`](#api-embeddeddir)
- [Материализация: нулевая копия](#материализация-нулевая-копия)
- [Детерминизм и сортировка](#детерминизм-и-сортировка)
- [dot-skip и symlink-skip](#dot-skip-и-symlink-skip)
- [Коды диагностик](#коды-диагностик)
- [NFC-нормализация путей](#nfc-нормализация-путей)
- [rodata-мина: не мутировать `data`](#rodata-мина-не-мутировать-data)
- [Взаимодействие с многофайловым codegen (План 209)](#взаимодействие-с-многофайловым-codegen-план-209)
- [CRLF и `.gitattributes`](#crlf-и-gitattributes)
- [`ReadFs` — один код для dev (диск) и prod (embedded)](#readfs--один-код-для-dev-диск-и-prod-embedded)
- [Кросс-языковое сравнение](#кросс-языковое-сравнение)
- [См. также](#см-также)

---

## `embed("path")` — один файл

```nova
test "embed(\"path\") round-trips the fixture bytes exactly" {
    ro data = embed("d412_embed_fixture.bin")   // path — relative to THIS .nv file
    ro want = x"48 69 00 FF 7F"                 // holds a NUL and a byte > 0x7F — these are raw bytes
    assert(data.len() == want.len())
}
```

(из `spec_tests/conformance/d412_hex_blob_embed.nv`).

- Путь резолвится относительно файла-исходника, где стоит вызов — модель
  Rust `include_bytes!`. Выход за пределы package-root вызова (`..` выше
  корня) — ошибка компиляции, не рантайм.
- Указан путь на директорию вместо файла → `E_EMBED_IS_A_DIR` («используй
  `embed_dir(...)`») — симметрично `embed_dir`'s `E_EMBED_NOT_A_DIR`.
- Сосед по D412 — hex-блоб литерал `x"48 69 00 FF"` (та же материализация;
  ведущие нули значимы, разделители `_`/пробел/перенос строки игнорируются,
  нечётное число цифр — `E_HEX_BLOB_ODD`). `embed(...)` — по сути «прочитать
  файл и подставить его байты как `x"…"`» на этапе компиляции.

## `embed_dir("dir")` — вся папка рекурсивно

```nova
ro assets = embed_dir("d412d_dir")     // recursively: alpha.txt, beta.txt, nested/gamma.txt

assert(assets.len() == 3)                                   // .hidden doesn't count (dot-skip)
assert(assets.paths() == ["alpha.txt", "beta.txt", "nested/gamma.txt"])   // sorted
assert(assets.has("nested/gamma.txt"))                       // recursion — nested paths are also a POSIX key
assert(!assets.has(".hidden"))

ro alpha = assets.get("alpha.txt").unwrap()                  // bytes "ABC", a zero-copy view
assert(assets.get("./alpha.txt") == None)                    // key WITHOUT a leading `./` — exact byte form
```

(адаптировано из `spec_tests/conformance/d412d_embed_dir.nv` — фикстура
`d412d_dir/` содержит `alpha.txt`("ABC")/`beta.txt`("XY")/
`nested/gamma.txt`("WXYZ")/`.hidden`).

- Тот же контракт аргумента, что у `embed`: строковый литерал, путь
  относительно `.nv`-файла вызова, package-root — граница; выход наружу
  (сама папка ИЛИ любой обойдённый внутри файл) — `E_EMBED_OUTSIDE_PROJECT`.
- **Рекурсивен по умолчанию.** Glob/фильтр — вне объёма (future); встраивается
  вся поддерево.
- Путь ведёт на файл, не папку → `E_EMBED_NOT_A_DIR` («используй `embed(...)`»).
  Папки не существует → `E_EMBED_DIR_NOT_FOUND`.
- **Ключ** записи = путь относительно embed-корня, разделитель POSIX `/`
  (Windows `\` при обходе диска конвертируется в `/`), **case-sensitive**,
  без ведущего `./`. `get`/`has` не нормализуют аргумент лексически: `..` не
  упрощается, `get("./x")` при существующем `x` честно даёт `None`.
- `\` (обратный слэш) в САМОМ строковом литерале аргумента (у `embed` И
  `embed_dir`) — `E_EMBED_PATH_BACKSLASH`: путь пишется POSIX-стилем `/`
  независимо от ОС компиляции (непортируемый исходник иначе).
- Пустая папка легальна → пустой `EmbeddedDir` (`len() == 0`).

## API `EmbeddedDir`

| Метод | Сигнатура | Семантика |
|---|---|---|
| `get` | `(path str) -> Option[[]u8]` | Байты файла по точному ключу. Бинарный поиск O(log N) по отсортированным записям. `None`, если пути нет — **не паника** |
| `has` | `(path str) -> bool` | Есть ли файл по пути (`get(path).is_some()`) |
| `paths` | `() -> []str` | Все встроенные пути, в отсортированном детерминированном порядке |
| `len` | `() -> int` | Число встроенных файлов |
| `entries` | `() -> ro []EmbeddedEntry` | Пары `(path, data)` без двойного lookup — `ro`-возврат (L2, read-only view, прецедент `str @bytes()`): мутация результата — ошибка компиляции |

`EmbeddedEntry { path str, data []u8 }` — одна запись; публично
конструируем (сам по себе инвариантов не несёт — используется и в ручных
тестах/моках).

**`EmbeddedDir` целиком иммутабелен** — нет мутирующих методов. Единственный
публичный конструктор — `EmbeddedDir.new(entries)` (тот же, что синтезирует
компилятор для `embed_dir(...)`): требует отсортированность+уникальность по
`path` (UTF-8 байтовый порядок == `str.compare`), нарушение — `panic`, не
тихий промах в `get`. Легально построить СВОЙ (не встроенный) каталог в
тестах — инвариант всё равно охраняется verify:

```nova
ro d = EmbeddedDir.new([
    EmbeddedEntry { path: "a.txt", data: a_bytes },
    EmbeddedEntry { path: "b.txt", data: b_bytes },
])
```

(`std/src/prelude/embed_test.nv` — конструирует вручную ДО того, как
резолвер умеет синтезировать `embed_dir`; доказывает контракт типа
независимо от компилятора.)

## Материализация: нулевая копия

Оба интринсика эмитятся в C как `static const uint8_t nova_blob_<hash>[]` в
`.rodata` — то же место, что str-литералы (интернирование по содержимому:
два одинаковых файла → один static, hash-коллизия → суффикс `_seq`).

- **`ro`-биндинг** (`ro img = embed("logo.png")`) — **нулевая копия**: `[]u8`
  с `data`, указывающим прямо на статику, `len == cap == N`.
- **`mut`-биндинг / consume в мутацию** — копия в GC-кучу в точке биндинга
  (обычный `Vec`-буфер дальше, `push` растёт как всегда).
- Boehm-сборщик мусора игнорирует указатели вне своей кучи — статический
  блоб никогда не собирается и не двигается.

`embed_dir("dir")` компилятор переписывает (пасс `embed_resolve`, ДО
type-check) в обычный вызов Nova:

```
EmbeddedDir.new([
    EmbeddedEntry { path: "app.js",     data: x"…" },   // sorted by path
    EmbeddedEntry { path: "index.html", data: x"…" },
])
```

— каждый `data` идёт через ТУ ЖЕ `HexBlobLit`-материализацию, что и
одиночный `embed`: **ноль правок в `emit_c.rs`**, только обход папки +
синтез AST в `embed_resolve.rs`. «Нулевая копия» в контракте — про
**payload'ы файлов**; сама таблица `entries` (заголовки + указатели) —
маленький one-time GC-alloc при вычислении выражения, O(N), пренебрежим
против байтов файлов.

**Совет:** биндить `embed_dir(...)` **один раз** (в `main()`, не на уровне
модуля — до закрытия `[M-codegen-emission-nondeterminism]`(c) static-init
topological order — и не в горячем пути): повторный вызов пересобирает
таблицу с нуля. Тот же нюанс есть у одиночного `embed` в теле функции
(дешёвый пересоздаваемый вид, но всё же пересоздаваемый).

## Детерминизм и сортировка

Записи `EmbeddedDir` **отсортированы по ключу** — UTF-8 байтовый порядок,
эквивалентный `str.compare` (D178, предпосылка корректности бинарного
поиска). Обход файловой системы сам по себе НЕ детерминирован между ОС —
резолвер сортирует результат явно, поэтому два билда (и билды на разных
ОС) дают идентичный порядок записей в сгенерированном `.c`.

## dot-skip и symlink-skip

- **Скрытые записи** (имя начинается с `.`) — пропускаются при обходе.
  Правило касается записей ВНУТРИ обхода, не самого аргумента:
  `embed_dir(".assets")` (корень назван явно) — встраивается целиком.
- **Символические ссылки** (файлы и папки) — НЕ следуются, пропускаются с
  `W_EMBED_DIR_SYMLINK_SKIPPED` (защита от escape через линк и от циклов
  обхода).
- Папка существует, но после dot/symlink-скипа встраивать нечего →
  `W_EMBED_DIR_EMPTY` — типичный симптом «навёлся не на ту папку», а не
  жёсткая ошибка (пустой `EmbeddedDir` легален).

## Коды диагностик

| Код | Класс | Когда |
|---|---|---|
| `E_EMBED_ARG_NOT_STR_LITERAL` | error | аргумент не строковый литерал / spread / named / арность ≠ 1 |
| `E_EMBED_NOT_FOUND` | error | (`embed`) файл не найден / не читается |
| `E_EMBED_IS_A_DIR` | error | (`embed`) путь ведёт на директорию — используй `embed_dir` |
| `E_EMBED_DIR_NOT_FOUND` | error | (`embed_dir`) папка не найдена |
| `E_EMBED_NOT_A_DIR` | error | (`embed_dir`) путь ведёт на файл — используй `embed` |
| `E_EMBED_OUTSIDE_PROJECT` | error | папка/файл выходит за package-root вызова |
| `E_EMBED_PATH_BACKSLASH` | error | `\` в строковом литерале пути (непортируемый исходник) |
| `E_EMBED_DIR_NFC_COLLISION` | error | два разных исходных имени нормализуются в один NFC-ключ (см. ниже) |
| `W_EMBED_DIR_SYMLINK_SKIPPED` | warning | симлинк пропущен при обходе |
| `W_EMBED_DIR_LARGE` | warning | суммарно > 16 MiB или > 4096 файлов |
| `W_EMBED_DIR_EMPTY` | warning | папка пуста после dot/symlink-скипа |
| `W_EMBED_DIR_NON_ASCII_PATH` | warning | не-ASCII имя файла (нормализовано в NFC — см. ниже) |

Большинство кодов проверено отдельной neg/standalone-фикстурой в
`spec_tests/conformance/{neg,standalone}/d412d_*` (конвенция §116: каждый
файл — свой compile-unit с `EXPECT_COMPILE_ERROR`/`EXPECT_COMPILE_WARNING`).
Исключение — `W_EMBED_DIR_SYMLINK_SKIPPED`: создание симлинков в
кросс-платформенной фикстуре само по себе непортируемо (на Windows требует
привилегий), поэтому этот код пока без выделенного теста — путь
`walk_embed_dir_rec` в `compiler-codegen/src/embed_resolve.rs` покрыт
только кодом, не фикстурой.

## NFC-нормализация путей

**Проблема:** macOS обычно хранит имена файлов в NFD (разложенная форма —
например, `é` как `e` + отдельный кодпоинт COMBINING ACUTE ACCENT U+0301),
тогда как Windows/Linux обычно дают NFC (предкомпонованная форма — `é` как
один кодпоинт U+00E9). Один и тот же git-чекаут на разных ОС мог раньше
давать РАЗНЫЕ байтовые ключи таблицы `embed_dir` — и, соответственно, разный
сгенерированный `.c` для идентичного содержимого репозитория.

**Решение ([D412-амендмент](../../spec/decisions/03-syntax.md#d412)):** каждый относительный путь записи
нормализуется в **NFC** при обходе. `get("café.txt")` с обычным
(предкомпонованным) строковым литералом в исходнике теперь находит файл
независимо от того, в какой форме файловая система физически хранила имя на
диске:

```nova
// Fixture: d412d_dir_nfc_normalize/ contains ONE file whose name on disk is
// NFD ("cafe" + U+0301 COMBINING ACUTE ACCENT + ".txt").
test "embed_dir NFC-normalizes an on-disk NFD file name" {
    ro d = embed_dir("d412d_dir_nfc_normalize")
    assert(d.has("café.txt"))     // the literal here is NFC ("é" = U+00E9, one code point)
}
```

(`spec_tests/conformance/standalone/d412d_embed_dir_nfc_normalize.nv`.)

**Коллизия форм** — если папка содержит ДВА РАЗНЫХ файла на уровне ФС
(разные байты имени — легально сосуществуют в одной директории), чьи
NFC-формы совпадают (например, предкомпонованный `café.txt` рядом с
разложенным `café.txt`) — это **жёсткая ошибка компиляции**
`E_EMBED_DIR_NFC_COLLISION`, а не тихая перезапись одной записи другой в
отсортированной таблице:

```nova
// EXPECT_COMPILE_ERROR E_EMBED_DIR_NFC_COLLISION
ro d = embed_dir("d412d_dir_nfc_collision")   // two files, one NFC form
```

(`spec_tests/conformance/neg/d412d_dir_nfc_collision_neg.nv`.)

`W_EMBED_DIR_NON_ASCII_PATH` (не-ASCII имя файла) остаётся — не-ASCII имя
всё ещё стоит внимания автора репозитория, но текст предупреждения теперь
говорит о том, что файл встраивается под НОРМАЛИЗОВАННЫМ NFC-ключом, а не
«как есть»; форм-коллизию ловит отдельная жёсткая ошибка выше.

**Реализация — zero новых Cargo-зависимостей.** Nova уже генерирует полные
таблицы Unicode 16.0 для `std.unicode.normalize_nfc`/`str @to_nfc()`
([План 152.4](../plans/152.4-std-unicode.md), файл `std/src/unicode/norm_data.nv`,
~113 КБ). Компилятор (Rust) не может вызвать эту Nova-функцию напрямую — она
исполняется В скомпилированной программе, а `embed_resolve` работает ДО
type-check, интерпретатора Nova в компиляторе нет. Вместо новой
Cargo-зависимости (`unicode-normalization` добавил бы ~762 КБ исходников
/ ~128 КБ сжатый `.crate` для NFD+NFKD+CCC+quick-check+stream-safe данных —
на порядок больше нужного) — `compiler-codegen/src/nfc.rs` парсит те же
`NFD_DATA`/`CCC_DATA`/`COMP_DATA` (NFKD не нужен для NFC — это ~45 КБ из
113 КБ файла) и повторяет ТОТ ЖЕ алгоритм canonical-decompose →
canonical-order → canonical-compose (UAX #15, включая алгоритмическую
Hangul-композицию), что `std/src/unicode/normalize.nv`. Одна каноническая
версия UCD на весь репозиторий, ноль добавленного веса в бинарь
компилятора.

Байты СОДЕРЖИМОГО файла (`data`) нормализация не затрагивает — она касается
только КЛЮЧА (пути) таблицы `embed_dir`; одиночный `embed(...)` не имеет
таблицы путей и потому не затронут вовсе.

## rodata-мина: не мутировать `data`

`data`/результат `get(...)` — **вид над `.rodata`**, не копия. `mut`-биндинг
результата с последующей записью НА МЕСТЕ (`d[0] = 5`) — неопределённое
поведение уровня чекера/рантайма (запись в read-only страницу памяти = SEGV
на большинстве платформ). Существующая защита D412 (копия при `mut`-биндинге)
ловит биндинг блоб-**литерала** напрямую (`mut x = x"01 02"`), но НЕ
значение, вернувшееся ИЗ функции/метода (`mut d = dir.get(p).unwrap()`) —
это унаследованный от одиночного `embed` хазард, отслеживаемый как
`[M-d412-blob-view-mut-write]` (backlog, P2, home D412; вне объёма Плана 210).

**Для мутации содержимого — явная копия:**

```nova
mut d = dir.get("config.json").unwrap().clone()   // now an ordinary GC buffer
d[0] = 0x7B                                        // legal — not .rodata
```

## Взаимодействие с многофайловым codegen (План 209)

Блоб рендерится в `.c` текстом — `0x%02X,` на байт
(`render_interned_blob_literals`, `emit_c.rs`) — то есть **≈×5.3 расширение**
относительно исходного размера байт (текстовое представление байта длиннее
самого байта). Из этого следуют два практических правила:

- **Порог `W_EMBED_DIR_LARGE`** — 16 MiB суммарного размера или 4096 файлов,
  а не изначально обсуждавшиеся 64 MiB: 64 MiB payload дал бы ~340 МБ
  `.c`-файла, с которым `clang` не совладает раньше, чем сборка станет
  практически неудобной.
- **Multi-TU (`NOVA_MULTI_TU=1`, План 209):** блоб-статики эмитятся в
  пролог — в multi-TU это означает `_common.h`, и КАЖДЫЙ `part` заново
  компилировал бы весь массив (прямо противоположно цели Плана 209 —
  снизить дублирование компиляции между `part`-ами). Определения блобов
  должны идти в ОДИН `part`, в `common` — только `extern`-декларация; блоб —
  неделимый юнит для `split_tu` (не режется между частями).

Future-выход из текстового рендеринга — C23 `#embed` (Option E, вне объёма
Плана 210): `.c` крошечный, компиляция почти мгновенная; требует `clang ≥19`
(доступен через WSL-clang; нативный windows-clang/MSVC — hex-fallback
остаётся).

## CRLF и `.gitattributes`

Байты встраиваются КАК ЕСТЬ из рабочей копии на диске, без нормализации
переносов строк. На Windows-чекауте с `autocrlf=true` текстовые ассеты
(`.html`/`.css`/`.js`) байтово ОТЛИЧАЮТСЯ от Linux-чекаута того же коммита →
разный `.c` / разный fingerprint между ОС для идентичного содержимого
репозитория. Для кросс-ОС воспроизводимости — явный `-text` (или `eol=lf`)
в `.gitattributes` на asset-папки, встраиваемые через `embed`/`embed_dir`.

## `ReadFs` — один код для dev (диск) и prod (embedded)

Частый кейс: раздавать статику веб-сервера — с диска в dev-режиме (живой
reload при правке файла) и вшитой в бинарь в prod. `ReadFs`
([D323-амендмент](../../spec/decisions/04-effects.md#d323), `std.fs`, План 210)
— read-only VFS-протокол, конформируемый **обоими** источниками:

```nova
import std.fs.{ReadFs, DirFs}

fn serve_assets[F ReadFs](mux mut ServeMux, assets F) -> () {
    mux.get("/{path...}", handler_fn(|req| {
        ro key = req.param("path").unwrap_or("index.html")
        match assets.read_file(key) {
            Ok(bytes) => ServerResponse.bytes(200, mime_of(key), bytes)
            Err(e)    => match e.kind {
                ErrorKind.NotFound => ServerResponse.empty(404)
                _                  => ServerResponse.empty(500)
            }
        }
    }))
}

fn main() {
    with Net = real_net(), Fs = real_fs() {
        mut mux = ServeMux.new()
        if dev_mode() {
            serve_assets(mut mux, DirFs.new("./frontend".to_path()))   // disk, live-reload
        } else {
            serve_assets(mut mux, embed_dir("../frontend"))                // baked into the binary
        }
        serve(mux, ":8080")
    }
}
```

`EmbeddedDir` конформит `ReadFs` через **extension-методы** (`std.fs`, не
трогая родной `prelude.embed`-API `@get`/`@has`/`@paths`); `DirFs` — обёртка
над реальной ФС с корнем-префиксом (та же escape-защита, что у `embed_dir`:
лексический `..`-фильтр + symlink-hard `canonicalize`). Протокол
**эффект-агностичен** (модель `io.Read`): `EmbeddedDir`-конформер — чистый,
`DirFs`-конформер несёт `Fs`, эффект всплывает транзитивно при mono. Nova не
поддерживает effectful-vtable dispatch, поэтому dev/prod-выбор — ветка
`if` НАД двумя mono-инстансами (в точке вызова), а не рантайм-переменная
одного `dyn`-типа. Подробности и почему `list`/directory-index сознательно
не входит в протокол — [`docs/guide/io-fs.md`](io-fs.md#readfs--one-vfs-protocol-over-the-disk-and-an-embedded-directory)
и [План 210 §6б](../plans/210-embed-dir.md).

## Кросс-языковое сравнение

| Аспект | Nova | Go `//go:embed` | Rust `include_bytes!`/`include_dir!` | Zig `@embedFile` | C23 `#embed` |
|---|---|---|---|---|---|
| Один файл | `embed("f")` → `[]u8` | `embed.FS` + `ReadFile` | `include_bytes!` → `&[u8]` | `@embedFile` → `[N]u8` | `#embed` → список int |
| Вся папка | `embed_dir("d")` → `EmbeddedDir` | `//go:embed dir` + `embed.FS` | `include_dir!` (крейт) | нет встроенного | нет |
| Рекурсия | да, по умолчанию | да | да (крейт) | — | — |
| Сортировка/бинарный поиск | да, сорт + O(log N) `get` | да (`embed.FS`) | линейный (крейт) | — | — |
| Скрытые файлы | скип (`.`-префикс) | скип (`.`/`_`-префикс) | конфигурируемо (крейт) | — | — |
| dev-режим (чтение с диска) | НЕТ intrinsic-подмены — явный `DirFs` через `ReadFs` | нет | `rust-embed` debug=disk (опция крейта) | — | — |
| NFC-нормализация путей | да + `E_EMBED_DIR_NFC_COLLISION` | нет (молчит) | нет (молчит) | — | — |
| Материализация | `.rodata`, zero-copy view | `.rodata`-подобно (Go binary) | `.rodata`, zero-copy | `.rodata` | `.rodata`, без hex-текст-раздутия |

**Nova берёт:** у Go — рекурсию, сортировку, бинарный поиск, dot-skip,
POSIX-пути, case-sensitive. У Rust `rust-embed` — `.get(path) -> Option`.
**Nova НЕ берёт:** dev-режим (чтение с диска в debug) — явный отказ (см.
[План 210 §2л](../plans/210-embed-dir.md)): вводит эффект в чистый по
конструкции тип и противоречит цели «один самодостаточный бинарь»; вместо
этого — явный `DirFs`/`ReadFs` opt-in (см. выше). **Nova идёт дальше обоих
эталонов** в NFC-нормализации: ни Go, ни Rust не решают NFD/NFC-ловушку
кросс-платформенной воспроизводимости имён файлов вообще.

## См. также

- [D412](../../spec/decisions/03-syntax.md#d412) —
  hex-блоб литерал `x"…"` + `embed("path")` (исходное решение, План 186).
- [D412-амендмент](../../spec/decisions/03-syntax.md#d412) —
  `embed_dir`, `EmbeddedDir`, коды диагностик, включая NFC-амендмент.
- [D323-амендмент](../../spec/decisions/04-effects.md#d323) — `ReadFs` (План 210).
- [План 210](../plans/210-embed-dir.md) — полная карта дизайна/решений/рисков
  (разведка, материализация Option R′, ревью-1/2/3).
- [`docs/guide/io-fs.md`](io-fs.md) — `std.io`/`std.fs`/`std.os` в целом, включая
  `ReadFs`.
- [`std/src/prelude/embed.nv`](../../std/src/prelude/embed.nv) — исходник
  `EmbeddedDir`/`EmbeddedEntry`.
- [`std/src/fs/readfs.nv`](../../std/src/fs/readfs.nv) — исходник `ReadFs`/`DirFs`.
- [`spec_tests/conformance/d412_hex_blob_embed.nv`](../../spec_tests/conformance/d412_hex_blob_embed.nv),
  [`d412d_embed_dir.nv`](../../spec_tests/conformance/d412d_embed_dir.nv) — референсные
  фикстуры для обоих интринсиков.
