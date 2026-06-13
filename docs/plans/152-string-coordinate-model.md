<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 152 (umbrella) — Production-grade строковая модель: линзы, координаты, Unicode-корректность

> **Создан:** 2026-06-13.  **Статус:** 📋 **PLANNED umbrella**, P1.
> **Цель:** строковый слой Nova не хуже (а где можно — лучше) Go / Rust / TS / Kotlin /
> Java — по корректности (Unicode), полноте API и предсказуемости стоимости.
> **Эстимат (весь umbrella):** ~14–21 dev-day, декомпозирован на 152.0–152.7
> (152.0 — реструктуризация модуля, идёт первой; 152.7 — интерполяция/форматирование).
> **Parallel-safe с Plan 140.2** (точка координации — `emit_c.rs` index-lowering, §7).
> **Model:** Sonnet 4.6 + High + Thinking ON (152.4 collation/normalization — Opus).
> **Зависит от:** Plan 138 (D238 `Index`, D239 `[]T≡Vec[T]`), Plan 131 (`Vec` raw-ptr),
> Plan 139/139.1 (`str` lang-item), Plan 147 (D246), D58/D241/D242 (`Next`/`Iter`),
> Plan 137 (`Compare`/`Equal`/`Hash` протоколы).
> **Предложено пользователем:** линзы `as_bytes`/`as_chars`, без бэар `len()`,
> «без упрощений, как для прода, не хуже Go/Rust/TS/Kotlin/Java» (2026-06-13).

---

## 0. Проблема и принцип

`str` хранится как UTF-8 `(ptr *ro u8, len int)` (Plan 139). Текущее состояние:

- **Рассогласование единиц:** `@len()` = байты (O(1)), но `str[i]` через
  `Index[int,char]` (D238) = codepoint (O(n)); `@find`/`@rfind` отдают **codepoint**-
  offset, не композирующийся с байтовым slice; `@pad_*` обещают «width codepoints»,
  считают байты ([string.nv:728](../../std/runtime/string.nv#L728)).
- **Спека противоречит коду:** [Q-string-indexing](../../spec/open-questions.md#q-string-indexing)
  ЗАКРЫТ в пользу «школы B» (всё codepoint-indexed, O(n)) — это надо
  **переоткрыть и развернуть** (см. §4).
- **Прод-пробелы:** `to_lower/to_upper` ASCII-only (баг для Unicode);
  `char`-классификация (`is_alphabetic`/`to_digit`/…) отсутствует; нет
  нормализации, grapheme-сегментации, case folding, collation.

**Принцип (Swift views-as-types):**

> **`str` — тонкий «кусок текста». Работа через линзы-представления: `as_bytes()`
> для байтов, `as_chars()` для codepoint'ов, (future) `as_graphemes()` для
> grapheme-кластеров. Каждая линза несёт методы, согласованные по своей единице.
> Целочисленной индексации самого `str` нет — координаты байтовые, codepoint'ы и
> graphemes вычисляются. Стоимость всегда видна (этос D135).**

Асимметрия линз (ключ): **`as_bytes()` — reinterpretation** (байты физически лежат
подряд → настоящий `ro []u8`, O(1) `[i]`/`len`); **`as_chars()`/`as_graphemes()` —
decoding lenses** (элементы вычисляются на проходе → отдельные view-типы, O(n),
не `[]char`).

**Инвариант R-UTF8 (как Rust, лучше Go).** Значение `str` **всегда** содержит
валидный UTF-8. Конструкторы либо валидируют (`from_bytes` checked → `Result`,
`from_bytes_lossy` → replace), либо `from_bytes_unchecked*` несут явный контракт
вызывающего. Это безопасно делает `as_chars()`-декодинг тотальным (нет «битых»
последовательностей в рантайме) и отличает Nova от Go, где `string` может держать
невалидные байты. Инвариант фиксируется в D26 AMEND.

---

## 1. Сравнительный анализ (что значит «не хуже»)

Матрица возможностей прод-языков. `core` = в ядре языка/stdlib, `lib` = в
официальной библиотеке (Rust crate / Go x/text / java.text / Intl), `—` = нет.

| Возможность | Go | Rust | TS/JS | Kotlin/Java | **Nova сейчас** | **Nova цель** |
|---|---|---|---|---|---|---|
| длина в байтах O(1) | core | core | — (UTF-16) | — | ✅ `len` | `byte_len()` / `as_bytes().len()` |
| длина в codepoint'ах | `utf8.RuneCount` | `.chars().count()` | `[...s].length` | `codePointCount` | ✅ `char_len` | `as_chars().len()` |
| int-индекс → байт | core `s[i]` | `as_bytes()[i]` | — | — | — | `as_bytes()[i]` |
| int-индекс → codepoint | — | — | — (codeUnit) | — | ⚠ `str[i]` O(n) | **убрать** |
| slice по диапазону | `s[a:b]` (байты) | `&s[a..b]` (байты) | `slice`(codeunit) | `substring`(codeunit) | ⚠ codepoint | `s[a..b]` байты |
| итерация codepoint'ов | `for range` | `.chars()` | `for..of` | `.chars()`/`codePoints` | ✅ | `for c in s` |
| итерация байтов | `[]byte` | `.bytes()` | — | — | ✅ `as_bytes` | `as_bytes()` |
| (offset, char) пары | `for i,r:=range` | `.char_indices()` | `Segmenter` | — | — | `as_chars().indices()` |
| find / rfind | core (байт-offset) | core (байт-offset) | `indexOf` | `indexOf` | ⚠ cp-offset | **байт-offset** |
| all matches | — | `.match_indices()` | `matchAll` | — | — | `match_indices()` |
| contains/starts/ends | core | core | core | core | ✅ | ✅ |
| split (sep/n/r) | `Split`/`SplitN` | `split`/`splitn`/`rsplit` | `split` | `split` | ⚠ только `split` | **полный** |
| split_whitespace | `Fields` | `split_whitespace` | `split(/\s+/)` | `split`+regex | — | core (Unicode WS) |
| lines | `bufio` | `.lines()` | `split(\n)` | `lineSequence` | — | core |
| trim / trim_matches | `Trim*`/`TrimFunc` | `trim`/`trim_matches` | `trim*` | `trim*` | ⚠ ASCII trim | **полный** |
| strip_prefix/suffix | `TrimPrefix` | `strip_prefix` | — | `removePrefix` | — | core |
| replace / replacen | `Replace` | `replace`/`replacen` | `replace*` | `replace` | ✅ `replace` | +`replacen` |
| pad / repeat | — / `Repeat` | — / `repeat` | `padStart/End`/`repeat` | `padStart`/`repeat` | ⚠ pad баг | **исправить** |
| join | `Join` | `.join()` | `join` | `joinToString` | ✅ `[]str.join` | ✅ |
| parse → число | `strconv` | `.parse()` | `parseInt` | `toInt` | ✅ | ✅ |
| **case (ASCII)** | core | core | core | core | ✅ | ✅ |
| **case (Unicode)** | `unicode` | core `char` | core | core | ❌ **ASCII-only** | **152.3/152.4** |
| **case folding (caseless eq)** | `x/text/cases` | crate | `localeCompare` | `equalsIgnoreCase` | ❌ | **152.5** |
| **char классификация** | `unicode.Is*` | `char::is_*` | `RegExp \p{}` | `Character.is*` | ❌ ad-hoc | **152.3** |
| **char → digit / numeric** | — | `to_digit` | — | `digitToInt` | ❌ | **152.3** |
| **нормализация NFC/NFD/NFKC/NFKD** | `x/text/unicode/norm` | crate | `normalize()` core | `Normalizer` | ❌ | **152.4** |
| **grapheme-сегментация (UAX#29)** | `x/text` | crate | `Intl.Segmenter` | `BreakIterator` | ❌ | **152.4** |
| **word/sentence boundaries** | `x/text` | crate | `Intl.Segmenter` | `BreakIterator` | ❌ | **152.4 (roadmap)** |
| **collation (locale order)** | `x/text/collate` | crate | `localeCompare` | `Collator` | ❌ (байт-lex) | **152.5** |
| UTF-8 validate (checked/lossy) | `utf8.Valid` | `from_utf8`/`lossy` | `TextDecoder` | `String(bytes)` | ✅ | ✅ |
| UTF-16/UTF-32 interop | `utf16` | `encode_utf16` | native | native | ⚠ частично | **152.6** |
| **StringBuilder** | `strings.Builder` | `String::push` | `+=`/array | `StringBuilder` | ✅ | ✅ |
| интерполяция строк | — | `format!` | `${}` template | `"$x"` | ✅ `${e}`/`${e:?}` | ✅ (152.7) |
| формат-спеки (width/prec/align/radix) | `fmt` verbs | `format!` mini-lang | слабо | `String.format` | ⚠ только `:?` | **152.7-B** |
| format → произвольный sink (`Write`) | `io.Writer` | `fmt::Write` | — | `Appendable` | ⚠ только StringBuilder | **152.7-B (opt)** |

**Вывод.** Координатная часть (синие строки) — у Nova сейчас неконсистентна, чиним в
152.1/152.2. По API-полноте (split/trim/strip/replacen/pad/match_indices) — пробелы,
закрываем в 152.2. По **Unicode-корректности** (case-Unicode, char-классификация,
нормализация, graphemes, collation) — Nova существенно отстаёт; это и есть «прод», и
выносится в 152.3 (`char`) + 152.4 (`std/unicode`) + 152.5 (сравнение/collation).
Без них Nova «на уровне C `<string.h>`», а не Go/Rust/TS/Kotlin/Java.

**Где Nova ≥ / лучше (часть «или лучше»):**
- **Прозрачность стоимости** — нет скрытого O(n) под `[i]`/`len` (лучше Python/JS,
  где `len` UTF-16-кодовые единицы ≠ символы — `"😀".length==2`; лучше прежней
  школы B самой Nova).
- **Типобезопасность единиц** — `u8` / `char` / (future) grapheme — разные типы;
  перепутать байт и символ нельзя (лучше Go, где `byte`/`rune` — числовые алиасы;
  лучше C).
- **Инвариант валидного UTF-8** (R-UTF8) — лучше Go (там `string` бывает невалиден).
- **Grapheme как полноценная линза** (152.4) — на уровне Swift, выше Go/Rust-core
  (там graphemes только во внешних crate/x/text).

---

## 2. Архитектура: слои

```
                      ┌───────────────────────────────────────────┐
   str (тонкий) ──────┤ identity (==/hash/clone/compare),          │
                      │ конструкция, conversions, slice s[a..b],   │
                      │ search→байт-offset, split/trim/replace/pad,│
                      │ линзы as_bytes()/as_chars()/as_graphemes() │
                      └───────────────────────────────────────────┘
        as_bytes() ▼            as_chars() ▼            as_graphemes() ▼ (152.4)
   ro []u8 (Vec[u8])      CharsView {ptr,byte_len}    GraphemesView (std/unicode)
   O(1) [i]/len/slice     decode lens, O(n)           UAX#29, O(n), Unicode-data
        ▲                        ▲                            ▲
   байтовый слой           codepoint-слой (char)        grapheme-слой
                                  │
                            char-тип (152.3): is_*, case, to_digit, category
                                  │
              std/unicode (152.4): normalize, fold, segment, collate (Unicode-data)
```

- **Ядро (всегда, без Unicode-таблиц):** байты, codepoint decode/encode, ASCII case/
  classification (fast-path), byte-lexicographic `Compare`.
- **`std/unicode` (Unicode-data, версионируемый):** полная нормализация, grapheme/word
  сегментация, case folding, Unicode case mapping, collation. Подключается явно — за
  размер таблиц (десятки–сотни КБ) платит тот, кто импортирует.

**Модульная раскладка реализации (152.0).** `str` переезжает из одного файла в папку
`std/runtime/string/` (module-per-file): `core`/`search`/`transform`/`parse`/`chars`
+ internal `_buffer.nv` (byte-builder на `RawMem`, `#no_prelude`) как единый дом
аллокаций — устраняет push-loop-копипаст и разрывает StringBuilder-цикл. Facade
`runtime.string` реэкспортит публичную поверхность.

---

## 2.5. Сквозные инварианты (self-consistency)

Контракт, который **обязан** соблюдать каждый sub-plan — чтобы план был
самосогласован, а не набором локально-верных кусков:

- **I1. Координаты — байтовые.** Все позиции/offset'ы (find/rfind/match_indices/
  slice `[a..b]`/get) — в **байтах**. codepoint-индекс не возвращается нигде, кроме
  явных `as_chars().at/indices` (O(n)).
- **I2. Никакого скрытого O(n).** Стоимость видна либо в линзе (взял `as_chars`/
  `as_graphemes` — принял O(n)), либо в имени метода. Под `[i]`/`len`/subscript —
  только O(1).
- **I3. Единица = линза.** Длина/доступ/итерация по байтам — `as_bytes()`; по
  codepoint'ам — `as_chars()`; по graphemes — `as_graphemes()` (152.4). На самом
  `str` — только `byte_len()` (O(1)-шорткат) + слой, не привязанный к единице
  (identity/search-byte-offset/slice/transform-возвращающий-str).
- **I4. R-UTF8.** Значение `str` всегда валидный UTF-8 (D26 AMEND); unchecked-
  конструкторы несут контракт; декодинг-линзы тотальны.
- **I5. `as_` vs `to_`.** `as_*` — линза (zero-copy, алиас, источник переживает);
  `to_*` — owned копия (alloc). Соблюдается во всём std.
- **I6. Без циклов модулей.** Граф `runtime/string/*` + `StringBuilder` + `Display`/
  `Debug` — ацикличный DAG; общий низкоуровневый дом — internal `_buffer` на RawMem
  (152.0). Интерполяция/форматтер не вводят обратных рёбер.
- **I7. Каждый метод — ровно в одном слое.** Нет дублей (плоский `char_at` на `str`
  И `at` на `CharsView`); удалённое со `str` живёт в линзе.
- **I8. Ядро без Unicode-таблиц собирается и работает** (ASCII-complete);
  `std/unicode` подключается явно. Отставание от полного Unicode до Фазы B — честно
  помечено `[M-152-unicode-*]`, не замаскировано.

Нарушение любого инварианта — баг плана, а не «деталь реализации».

---

## 3. Декомпозиция (sub-plans)

Каждый sub-plan — **отдельный файл** (созданы; полные scope/фазы/D-Q/тесты/критерии —
там). Ниже — краткое summary. Порядок: **152.0** (фундамент) → 152.1 → 152.2 →
(152.3a ∥ 152.6 ∥ 152.7-A) → **Фаза B:** 152.4 → 152.3b/152.5b/152.7-B.
(152.7-A = «не сломать интерполяцию» — контракт внутри 152.0.)

| Sub-plan | Файл | Фаза |
|---|---|---|
| 152.0 реструктуризация модуля | [152.0-module-restructure.md](152.0-module-restructure.md) | A |
| 152.1 координаты + линзы (D249/D250) | [152.1-coordinate-model-lenses.md](152.1-coordinate-model-lenses.md) | A |
| 152.2 полный str-surface (D251) | [152.2-string-surface-parity.md](152.2-string-surface-parity.md) | A |
| 152.3 `char`-тип API (D252) | [152.3-char-type-api.md](152.3-char-type-api.md) | A(ascii)/B(unicode) |
| 152.4 `std/unicode` (D253) | [152.4-std-unicode.md](152.4-std-unicode.md) | B |
| 152.5 сравнение + collation (D254) | [152.5-comparison-collation.md](152.5-comparison-collation.md) | A(core)/B(locale) |
| 152.6 UTF-16/32 interop (D255) | [152.6-utf16-encoding-interop.md](152.6-utf16-encoding-interop.md) | A |
| 152.7 интерполяция + форматирование (D258) | [152.7-interpolation-formatting.md](152.7-interpolation-formatting.md) | A(сохранить)/B(улучшить) |

### 152.0 — Реструктуризация модуля `str`: папка + internal `_buffer` + RawMem `[engineering]`

**Мотивация (запрос пользователя).** Сейчас весь `str` — один файл
[string.nv](../../std/runtime/string.nv) (~766 строк), и часть buffer-building
(`@trim`/`@to_lower`/`@to_upper`/`@concat`) реализована **push-loop-копипастом** по
`[]u8` в обход `StringBuilder` (исторически — из-за import-цикла
`prelude → … → string_builder`). Цель: разнести по файлам, переиспользовать `RawMem`
по максимуму, устранить копипаст через **internal-модуль**.

**Scope:**
- Папка `std/runtime/string/` (module-per-file, как `std/collections/`):
  - `core.nv` (`runtime.string.core`) — `len`-семейство (`byte_len`/`as_bytes`/
    `as_chars`)/`slice`/`is_empty` + identity (`eq`/`hash`/`clone`/`compare`).
  - `search.nv` — `find`/`rfind`/`contains`/`starts_with`/`ends_with`/split-семейство/
    `match_indices`.
  - `transform.nv` — `trim*`/case(ASCII)/`replace`/`replacen`/`pad*`/`repeat`/`concat`.
  - `parse.nv` — `parse_int`/`try_parse_int`.
  - `chars.nv` — `CharsView`/`CharIndicesView` + char-курсор.
  - `_buffer.nv` (`runtime.string._buffer`, **internal**, `#no_prelude`) — единственный
    дом низкоуровневого byte-builder'а **на `RawMem`** (`alloc`/grow/
    `copy_nonoverlapping`/`fill` + NUL-term + `finish() -> str`). Публично НЕ
    реэкспортируется.
  - facade: `string.nv` → `runtime.string` реэкспортит подмодули (`export import`),
    сохраняя существующие `import std.runtime.string.{...}` и prelude-wiring.
- **RawMem-max:** свести `alloc_copy`/`from_bytes_*` и все push-loop'ы в `_buffer` на
  прямые `RawMem.alloc`/`copy_nonoverlapping`/`fill`/`compare` (один memcpy-проход,
  меньше bounds-check'ов). str-методы строят результат через `_buffer`, не хардкодя
  alloc/grow/NUL.
- **Cycle-break:** `_buffer` (`#no_prelude`) зависит только от `RawMem` + ptr-ops →
  вне prelude-цикла. И str-`transform`, и `StringBuilder` строятся на `_buffer`
  (`StringBuilder` = тонкий публичный `consume`-wrapper над `_buffer`); push-loop-
  копипаст устранён — alloc/grow/NUL живут в одном месте.
- **Internal-видимость:** ключевого `internal` для модулей в Nova нет (проверено) →
  конвенция `_`-префикс (как `_experimental`); facade не реэкспортит `_buffer`.

**Валидация (Ф.0.0, до рефактора):** подтвердить, что методы типа `str` могут жить в
**нескольких** модулях (`runtime.string.core` + `.search` + …) с сохранением
type-method-privacy к priv-полям `@ptr`/`@len`. Прецедент: Plan 139.1 уже разнёс
decl (`core.nv`) и методы (`string.nv`) — 2 модуля работают; нужно подтвердить N>2 и
резолв метода из любого. Если резолвер ограничивает — fallback: один `methods.nv` +
internal `_buffer` (дробление слабее, но цель «RawMem + без копипаста» достигнута).

**Deliverables:** `docs/strings-internals.md` (структура папки, роль `_buffer`,
конвенция internal); опц. **Q-module-internal-visibility** (нужен ли языку `internal`).
**Тесты:** facade + подмодули компилируются, существующие импорты не сломаны; нет
prelude-цикла; **golden байт-эквивалентность** `trim`/case/`concat` до/после;
`StringBuilder` и str-`transform` дают идентичный результат на общих кейсах.
**Эстимат:** ~1.5–2 dev-day. **Идёт ПЕРВЫМ** — фундамент для 152.1–152.6.

### 152.1 — Координатная модель + линзы `as_bytes`/`as_chars` `[D249, D250]`

Ядро разворота. **Scope:**
- `str` **не** реализует `Index[int,V]`; `str[i]` целым → `E_STR_NO_INT_INDEX`
  (fix-it: `as_chars().at(i)` / `as_bytes()[i]` / `s[a..b]`). `str[a..b]` —
  byte-range zero-copy view (`Index[Range,str]`), panic при OOB/рассечении codepoint.
- Линза `@as_bytes() -> ro []u8` (есть) — байтовый слой бесплатно из `Vec[u8]`.
  `@byte_at` ретайрнут → `as_bytes()[i]`.
- Линза `@as_chars() -> CharsView` (NEW). `CharsView value priv {ptr *ro u8,
  byte_len int}`: `len()` O(n), `at(i)->Option[char]` O(n), `mut next()->Option[char]`
  (`Next[char]`), `iter()->CharsView` (`Iter`), `indices()->CharIndicesView`,
  `is_empty()`. **`CharsView` НЕ реализует `Index[int,char]`** (только `at`/iter).
- Бэар `@len()` со `str` убран → `E_STR_NO_LEN` (fix-it: `as_bytes().len()` /
  `as_chars().len()`). `@byte_len() -> int` (O(1)) остаётся как шорткат (имя
  однозначно). Поле `len` в layout — storage-инвариант, остаётся.
- `@find`/`@rfind` → **байт**-offset.
- `for c in s` → `char` (= `s.as_chars().iter()`).
- Конвенция `as_` (линза, zero-copy, алиас) vs `to_` (owned копия) — кодифицируется.

**D-блоки:** D249 (модель), D250 (`CharsView` + конвенция `as_`/`to_`).
**Эстимат:** ~2–3 dev-day. (детали — §«D249/D250» ниже).

### 152.2 — Полный str-surface (паритет Go/Rust/TS/Kotlin/Java) `[D251]`

**Scope** — добить методы до паритета, разместив по слоям, все позиции/длины в
**байтовых координатах** (или явных char через линзу):
- **split-семейство:** `@split(sep)`, `@splitn(n, sep)`, `@rsplit(sep)`,
  `@split_once(sep)->Option[(str,str)]`, `@split_whitespace()` (Unicode WS —
  ASCII-fast + делегат в 152.4 для не-ASCII), `@lines()`.
- **trim:** `@trim`/`@trim_start`/`@trim_end` (Unicode WS), `@trim_matches(pred)`,
  `@strip_prefix(p)->Option[str]`, `@strip_suffix(s)->Option[str]`.
- **search:** `@find`/`@rfind` (байт-offset), `@match_indices(needle)->[]int`
  (байт-offset'ы), `@contains`/`@starts_with`/`@ends_with`.
- **transform:** `@replace`, `@replacen(from,to,n)`, `@repeat`, `@pad_left`/
  `@pad_right`/`@pad_center` (ширина в **codepoint'ах** → `as_chars().len()`).
- **owned линзы:** `@to_bytes()->[]u8`, `@to_chars()->[]char`.
- **slice helpers:** `@get(a..b)->Option[str]` (safe slice, None при OOB/не-границе).

**Deliverables:** D251 (полный surface + размещение + байт-семантика); Q-string-len
RESOLVE; миграция codepoint→byte семантики find/split.
**Эстимат:** ~2–3 dev-day.

### 152.3 — `char`-тип: классификация / case / digit `[D252]`

**Scope** — `char` (u32 codepoint) получает прод-API, как `char` в Rust / `Character`
в Java / `unicode` в Go. **Двухуровнево:**
- **ASCII-fast в ядре** (без таблиц): `@is_ascii`, `@is_ascii_digit`,
  `@is_ascii_alphabetic`, `@is_ascii_alphanumeric`, `@is_ascii_whitespace`,
  `@is_ascii_uppercase/lowercase`, `@to_ascii_uppercase/lowercase`,
  `@to_digit(radix)->Option[int]`, `@is_ascii_hexdigit`.
- **Unicode-aware** (делегат в `std/unicode`, 152.4): `@is_alphabetic`, `@is_numeric`,
  `@is_alphanumeric`, `@is_whitespace`, `@is_uppercase`/`@is_lowercase`,
  `@is_control`, `@general_category()->GeneralCategory`, `@to_uppercase()->CharsView`/
  `@to_lowercase()` (могут давать НЕСКОЛЬКО codepoint'ов — ß→ss).
- Консолидировать ad-hoc `is_alpha`/`is_digit` из `json.nv`/`url.nv` на `char`-методы.

**Deliverables:** D252 (char API); `std/runtime/char.nv` расширение; миграция
приватных хелперов.
**Эстимат:** ~2 dev-day (ASCII-часть; Unicode-часть зависит от 152.4).

### 152.4 — `std/unicode`: нормализация / сегментация / folding / case-mapping `[D253, Q-unicode-data]`

**Scope** — новый модуль `std/unicode` с Unicode-data, закрывающий разрыв с
Java `Normalizer`/`BreakIterator` / JS `normalize`/`Intl.Segmenter` / Rust crates:
- **Нормализация:** `normalize_nfc/nfd/nfkc/nfkd(s)->str` (canonical/compat,
  decomposition + canonical ordering + composition).
- **Grapheme-сегментация (UAX #29):** `str.@as_graphemes()->GraphemesView`
  (extended grapheme clusters: combining marks, ZWJ-emoji, regional-indicator флаги).
- **Case folding:** `fold_case(s)->str` (для caseless matching, 152.5).
- **Unicode case mapping:** полные `to_uppercase`/`to_lowercase`/`to_titlecase`
  (multi-codepoint, без локали) — апгрейд ASCII-only `string.nv`.
- **Word/sentence boundaries (UAX #29):** `word_indices`/`sentences` — **roadmap**
  (может стать 152.4.1).
- **Unicode-data strategy (Q-unicode-data):** как генерируем/пиним таблицы
  (UnicodeData.txt → codegen в `*.nv`/бинарь), версия Unicode, размер, lazy-загрузка.

**Deliverables:** D253 (архитектура слоя + что core/lib); Q-unicode-data (RESOLVE
стратегию данных); модуль `std/unicode` (поэтапно — нормализация → graphemes →
case-mapping → folding). Большой; внутренняя декомпозиция 152.4.1–152.4.4.
**Эстимат:** ~4–6 dev-day (Unicode-data pipeline — главный драйвер). **Opus.**

### 152.5 — Сравнение и collation `[D254, Q-string-collation]`

**Scope:**
- **`Compare`/`Equal`/`Hash`** на `str` — byte-lexicographic (есть `@compare`, D178);
  закрепить как дефолт `Ord` (быстрый, детерминированный, не локале-зависимый —
  как Rust/Go).
- **Caseless eq:** `@eq_ignore_ascii_case(other)` (ядро) + `@eq_ignore_case(other)`
  (Unicode case folding, делегат 152.4).
- **Locale collation:** `std/unicode/collate` — `Collator` (UCA, locale-tailored
  ordering), аналог JS `localeCompare`/Java `Collator`. **Roadmap** (зависит от
  Unicode-data 152.4); Q-string-collation фиксирует стратегию (UCA DUCET, tailoring).

**Deliverables:** D254 (модель сравнения: байт-Ord дефолт + явный collation-слой);
Q-string-collation.
**Эстимат:** ~2 dev-day (ядро); collation — roadmap.

### 152.6 — Encoding interop (UTF-16 / UTF-32 / code points) `[D255]`

**Scope** — для FFI/JSON/протоколов: `@encode_utf16()->[]u16`,
`str.from_utf16(units)->Result`, `@code_points()->[]int` (raw int codepoints без
char-обёртки), surrogate-pair помощники. Дополняет существующие
`from_bytes_*`/`to_bytes`.
**Deliverables:** D255; `std/encoding/utf16.nv`.
**Эстимат:** ~1–1.5 dev-day.

---

## 3А. Фазы по приоритету — «сейчас» vs «позже»

Разбивка по обязательности. **Phase A** — связный, самодостаточный, обязательный
*сейчас* набор: после него строки координатно-корректны, ASCII-полны и по API на
уровне Rust/Go. **Phase B** — Unicode-корректность прод-уровня (нормализация,
graphemes, folding, locale): обязательна для «не хуже Java/JS/Kotlin», но
*отделяема* и шедулится позже за `[M-152-*]`-маркерами (Phase A без неё связна).

| Sub-plan | Фаза | Почему так |
|---|---|---|
| **152.0** реструктуризация модуля | **A (now)** | фундамент; без него остальное копится копипастом |
| **152.1** координаты + линзы (D249/D250) | **A (now)** | ядро-разворот; иначе модель остаётся в half-broken состоянии |
| **152.2** полный str-surface (D251) | **A (now)** | паритет API Go/Rust; байт-семантика find/split |
| **152.3a** `char` ASCII-core (D252 §ASCII) | **A (now)** | классификация/digit/case ASCII — без таблиц, нужно lexer'ам/parser'ам |
| **152.5a** сравнение-core (D254 §byte-Ord) | **A (now)** | byte-`Ord` + `eq_ignore_ascii_case`; детерминированная сортировка |
| **152.6** UTF-16/32 interop (D255) | **A (now)** | мал, нужен FFI/JSON; surrogate-обработка |
| **152.3b** `char` Unicode-методы | **B (later)** | зависят от данных 152.4 |
| **152.4** `std/unicode` (норм./graphemes/folding/case-map) | **B (later)** | Unicode-data pipeline; крупнейший лифт; отделяем |
| **152.5b** locale-collation | **B (later)** | зависит от 152.4; UCA/CLDR |
| **152.7-A** сохранить интерполяцию через рефактор | **A (now)** | DAG модуля без цикла str↔StringBuilder (контракт в 152.0) |
| **152.7-B** формат-спеки + Write-sink | **B (later)** | width/precision/align/radix + decouple Display от StringBuilder |

**Acceptance Phase A (минимальный shippable):** строка координатно-консистентна (нет
int-index, нет бэар `len`, find=байт-offset, линзы), ASCII-корректна, API-паритет
Rust/Go, без скрытого O(n); полный `nova test` зелёный; spec A-части закрыты
(D249/D250/D251/D255 + A-части D252/D254 + AMEND D26/D238/D58 + Q-string-indexing/
-len/-unicode-data*/-collation*/-module закрыты, где `*` = зафиксирована стратегия,
реализация — Phase B).

**Phase B — обязательна для полной цели, но не блокирует A.** Пока B не сделана,
`str` Unicode-неполна (ASCII case, нет нормализации/graphemes) — это честно
помечается `[M-152-unicode-*]`-маркерами и в `docs/strings.md` («ASCII-complete;
полный Unicode — Phase B»). Без B Nova **хуже** Java/JS/Kotlin по Unicode → B
обязательна к закрытию, просто позже.

---

## 4. Spec / D / Q / документация (обязательные deliverables)

**Решения (D):**
- **D249** (NEW) — координатная модель: линзы, без int-index/`len()`, char-итерация.
- **D250** (NEW) — `CharsView` (decoding lens) + конвенция `as_`/`to_`.
- **D251** (NEW) — полный str-surface, размещение по слоям, байт-координатная семантика.
- **D252** (NEW) — `char`-тип API (ASCII-core + Unicode-aware).
- **D253** (NEW) — `std/unicode`: что в ядре vs библиотеке; Unicode-data слой.
- **D254** (NEW) — модель сравнения: байт-`Ord` дефолт + явный collation.
- **D255** (NEW) — UTF-16/32 encoding interop.
- **D258** (NEW) — формат-спеки интерполяции (Rust-style mini-language) + опц.
  `Write`-sink для `Display`/`Debug` (152.7).
- **D44/D183/D229 AMEND** — расширение FormatSpec (`${e:spec}`) + (опц.) `@display/
  @debug(mut w Write)` вместо конкретного `StringBuilder` (152.7).
- **D26 MAJOR AMEND** ([08-runtime.md](../../spec/decisions/08-runtime.md#d26)) —
  развернуть «школу B (всё codepoint-indexed)» на линзы/байт-координаты; зафиксировать
  инвариант **R-UTF8** (значение `str` всегда валидный UTF-8); обновить список
  «базовых str-методов».
- **D238 AMEND** — RETRACT `str | int | char`; остаётся `str | Range | str`.
- **D58 AMEND** — `for c in s` → `char` через `as_chars().iter()`.

**Закрытие открытых вопросов (Q) — РЕШЕНИЯ (фиксируются в плане):**

- **Q-string-indexing** ([open-questions.md](../../spec/open-questions.md#q-string-indexing))
  — **ПЕРЕЗАКРЫТО: вариант «линзы / byte-coordinates»** (разворачивает прежнее
  закрытие «школа B / всё codepoint-indexed», 2026-05-07). `str` не индексируется
  целым (`E_STR_NO_INT_INDEX`); единственный `[]` — byte-range slice `str[a..b]`
  (boundary-checked); элементный доступ через линзы: `as_bytes()[i]` (байт, O(1)) /
  `as_chars().at(i)` (codepoint, O(n)). Обоснование: на UTF-8 codepoint-index =
  O(n)-ложь под видом O(1); байт-offset композируется со slice за O(1). Прецедент:
  Rust (нет int-index), Swift (нет int-subscript). В `open-questions.md` — переоткрыть
  и закрыть заново с этой записью.
- **Q-string-len** — **ЗАКРЫТО: нет бэар `len()`.** Длина — свойство представления:
  `str.byte_len() -> int` (O(1)) — единственный length-метод на самом `str` (шорткат
  для аллокаций); codepoint-длина — `as_chars().len()` (O(n)); бэар `s.len()` →
  `E_STR_NO_LEN` с fix-it. Расхождение с `Vec.len()` намеренное (у `str` три
  расходящиеся длины).
- **Q-unicode-data** (NEW) — **ЗАКРЫТО: build-time codegen из UCD, версия-пин, lazy
  range-таблицы, без ICU.** Таблицы генерируются из официального Unicode Character
  Database (`UnicodeData.txt`, `CaseFolding.txt`, `GraphemeBreakProperty.txt`,
  `DerivedNormalizationProps.txt`, `emoji-data.txt`) build-time-инструментом
  `nova-codegen unicode` в компактные range-кодированные Nova-таблицы
  `std/unicode/data/`, пин к версии (константа `UNICODE_VERSION`, напр. `16.0`),
  lazy-инициализация. Не хардкодим вручную, не зависим от ICU/ОС. Прецедент: Rust
  `unicode-*` (codegen), Go `maketables`.
- **Q-string-collation** (NEW) — **ЗАКРЫТО: дефолт = byte-`Ord`; locale-collation —
  отдельный opt-in UCA-слой (Phase B).** `str.compare`/`Ord` — byte-lexicographic
  (быстрый, детерминированный, locale-независимый, как Rust/Go); locale-aware —
  явный `std/unicode/collate.Collator` на UCA (DUCET) + опц. CLDR-tailoring. `str`
  никогда не делает collation молча. Прецедент: Rust (byte Ord + crate), Go (byte +
  `x/text/collate`).
- **Q-module-internal-visibility** (NEW) — **ЗАКРЫТО (на сейчас): конвенция
  `_`-префикс, без нового keyword.** Internal-модули помечаются `_`-префиксом
  (`_buffer.nv`), facade их не реэкспортит; язык НЕ получает `internal`-keyword в
  рамках Plan 152 (минимизируем surface). Накопится потребность — отдельный
  language-план.
- **Q-format-spec** (NEW, 152.7) — **ЗАКРЫТО: формат-спеки Rust-style
  mini-language** (`:[[fill]align][sign][#][0][width][.prec][type]]`), не printf/
  Python; `:?` (Debug) сохраняется; расширение — AMEND D44/D183/D229 без смены
  синтаксиса `${...}`. Обобщение sink `Display.@display(mut w Write)` (decouple от
  `StringBuilder`) — решение B2, может быть отложено за breaking-ценой.

**Документация (`docs/`):**
- `docs/strings.md` (NEW) — гайд: модель линз, байт vs codepoint vs grapheme, когда
  что, рецепты (find+slice, обход символов, Unicode-корректное сравнение), таблица
  «откуда метод».
- `docs/strings-internals.md` (NEW, 152.0) — внутренняя структура модуля
  `runtime/string/`, роль internal `_buffer` на RawMem, конвенция internal-видимости.
- `docs/formatting.md` (NEW, 152.7) — интерполяция `${...}`, формат-спеки
  (Rust-style mini-language), `Display`/`Debug`, `Write`-sink.
- `docs/migration/d249-string-lenses.md` (NEW) — миграция `str[i]`/`len()`/
  `char_at`/`find`-семантики.
- `nova doc` бейджи стоимости (O(1)/O(n)) на str/CharsView-методах (если поддержано).

---

## 5. Тесты (позитивные + негативные)

Фикстуры в `nova_tests/plan152*/`, через релизные `nova` + компилятор. По sub-plan:

**152.0 — структура модуля.**
- POS: facade `runtime.string` + подмодули компилируются; существующие
  `import std.runtime.string.{...}` работают; `_buffer` строит str из RawMem
  (alloc+memcpy+NUL) корректно; `StringBuilder` и str-`transform` на общем `_buffer`
  дают идентичный результат; **golden** — `trim`/`to_lower`/`to_upper`/`concat`
  байт-в-байт как до рефактора.
- NEG: prelude-цикл отсутствует (сборка prelude не висит/не падает); прямой import
  `_buffer` из prelude-пути запрещён конвенцией (аудит).

**152.1 — координаты/линзы.**
- POS: `for c in s` (ASCII+мультибайт: «héllo», эмодзи), `s[a..b]` zero-copy,
  `s.as_bytes()[i]`/`.len()`, `s.as_chars().at(i)`/`.len()`/`.indices()`,
  `s.find(x)`→`s[k..]` композиция.
- NEG: `s[i]` целым → `E_STR_NO_INT_INDEX`; `s.len()` → `E_STR_NO_LEN`; `chars[i]`
  целым → нет `Index[int]`; `s[a..b]` рассекает codepoint → panic;
  `as_chars()` пережил `str` (lifetime — должен держаться GC, регресс-тест).

**152.2 — surface.**
- POS: split/splitn/rsplit/split_once/split_whitespace/lines; trim/trim_matches/
  strip_prefix/suffix; replacen; pad_* (не-ASCII ширина: `"é".pad_left(3)`);
  match_indices.
- NEG: `splitn(0,...)`, пустой sep, OOB `get(a..b)`→None, pad width<len.

**152.3 — char.**
- POS: `'A'.is_ascii_alphabetic()`, `'7'.to_digit(10)==Some(7)`, `'f'.to_digit(16)`,
  `'ß'.to_uppercase()` → «SS» (multi-cp), `'Ω'.is_alphabetic()`.
- NEG: `'g'.to_digit(16)==None`, `'\n'.is_ascii_alphanumeric()==false`.

**152.4 — unicode.**
- POS: NFC(«e»+combining-acute)==NFC(«é»); NFD обратно; NFKC(ﬁ-лигатура)→«fi»;
  graphemes(«👨‍👩‍👧»)==1, («🇺🇸»)==1, («é»=e+◌́)==1; case-fold(«ß»)==«ss».
- NEG: невалидный UTF-8 в normalize → policy (replace/err); неизвестная Unicode-
  версия данных.

**152.5 — сравнение.**
- POS: byte-`Ord` сортировка детерминирована; `eq_ignore_ascii_case`;
  `eq_ignore_case(«STRASSE»,«straße»)` (folding); collator(locale) ordering (если в
  scope).
- NEG: collation без locale-данных → fallback на byte-`Ord` + диагностика.

**152.6 — encoding.**
- POS: roundtrip `s.encode_utf16()`→`from_utf16()`; surrogate-пары (эмодзи);
  `code_points()`.
- NEG: lone surrogate в `from_utf16` → Result::Err.

**152.7 — интерполяция/форматирование.**
- POS: `"${n}"`/`"${n:?}"` golden (после рефактора 152.0); формат-спеки
  `"${n:>8}"`/`"${pi:.2}"`/`"${n:#x}"`/`"${n:08}"`; интерполяция с новым `str`/линзами.
- NEG: `"${x:zz}"` → `E_BAD_FORMAT_SPEC`; интерполяция в const-инициализаторе →
  compile-error; (B2) sink-mismatch.

**Регрессия:** полный `nova test` без новых FAIL vs baseline на каждом sub-plan.

---

## 6. Критерии приёмки

**Глобальные (umbrella).**
- **G1.** Каждая «Nova цель»-ячейка матрицы §1 закрыта либо реализацией, либо явным
  roadmap-маркером `[M-152-*]` (никаких молчаливых пропусков).
- **G2.** Ни одна публичная строковая операция не прячет O(n) под видом O(1):
  стоимость видна в линзе или имени.
- **G3.** Unicode-корректность не хуже Java/JS на нормализации, grapheme-count и
  caseless-eq (или явный roadmap с обоснованием отставания).
- **G4.** Полный `nova test` зелёный; все plan152*-фикстуры PASS.
- **G5.** Spec обновлён: D249–D255 + **D258** + амендменты D26(MAJOR)/D238/D58/D44/
  D183/D229; все **6** Q-вопросов **закрыты** записями-решениями (§4): Q-string-
  indexing (заново), Q-string-len, Q-unicode-data, Q-string-collation, Q-module-
  internal-visibility, Q-format-spec; `docs/strings.md` + `docs/strings-internals.md`
  + `docs/formatting.md` + migration-гайд написаны.
- **G6.** Реализация структурирована: модуль `runtime/string/` разбит по слоям,
  buffer-building живёт в одном internal `_buffer` на `RawMem` (ноль push-loop-
  копипастов alloc/grow/NUL вне `_buffer` — grep-аудит), `StringBuilder` без
  дублирования.

**Per-sub-plan A-критерии** — в §«Критерии» соответствующего файла `152.N`. Ключевые:
- **152.0:** папка `runtime/string/`+facade компилируется, существующие импорты целы;
  все str-аллокации через `RawMem`-`_buffer`; ноль copy-paste push-loop'ов; нет
  prelude-цикла; golden байт-эквивалентность до/после.
- **152.1:** `str[i]`→`E_STR_NO_INT_INDEX`; нет `@len()`→`E_STR_NO_LEN`; `as_bytes()`
  O(1) индекс; `CharsView` без `Index[int]`; `for c in s`==`char`; `find`==байт-offset;
  `str`:`Index[Range,str]` ∧ ¬`Index[int,_]`; `as_/to_` соблюдены.
- **152.2:** полный split/trim/strip/replacen/pad/match_indices; pad ширина в
  codepoint'ах (`"é".pad_left(3,'·')=="··é"`).
- **152.3:** `char` ASCII-API + Unicode-делегаты; ad-hoc хелперы консолидированы;
  `'ß'.to_uppercase()` мультибайтовый.
- **152.4:** NFC/NFD/NFKC/NFKD корректны на эталонах UAX #15; grapheme-count
  корректен на UAX #29-эталонах (эмодзи/флаги/combining); Unicode-данные
  версионированы.
- **152.5:** byte-`Ord` дефолт; `eq_ignore_case` через folding; collation-стратегия
  зафиксирована.
- **152.6:** UTF-16 roundtrip + surrogate-обработка; lone-surrogate → Err.
- **152.7:** интерполяция не сломана рефактором (golden); формат-спеки B1
  (width/prec/align/radix) корректны; невалидный спек → `E_BAD_FORMAT_SPEC`.

---

## 7. Для исполнителя (execution)

**Подготовка.**
- Заведи постоянный worktree `nova-p152`. str-инфра живёт в ветке `plan-138.1`
  (139.x НЕ смёржены в main) — сверься, какая база нужна для старта.
- D-блоки **D249–D255 + D258** зарезервированы под этот план (+ AMEND D26/D238/D58/
  D44/D183/D229). **D256/D257 заняты Plan 140.2** (Part A / Part B.0) — НЕ использовать.
  Другие агенты — с **D259**. Решения по Q закрыты в §4 — перенеси их записями в
  `spec/decisions/` и `spec/open-questions.md` (Q-string-indexing переоткрыть и
  закрыть заново).

**Parallel-safety.** План **можно вести параллельно с Plan 140.2** (Vec bounds-as-
contract) — логически независимы, общих D-блоков нет, разные std-файлы. Единственная
точка координации — `compiler-codegen/src/codegen/emit_c.rs`: 152.1 правит **str**-
index dispatch, 140.2 — **Vec `@index`** codegen. Разные функции одного файла →
конфликты при мёрже механические, не логические; кто мёржится вторым, чинит
index-lowering в `emit_c.rs` руками. Общий `.git`: без `git stash` при конкурентных
агентах.

**Порядок и фазы.** 152.0 → 152.1 → 152.2 → (152.3a ∥ 152.6 ∥ 152.7-A) → **B:** 152.4
→ 152.3b/152.5b/152.7-B.
Сейчас обязательна **Фаза A** (152.0–152.2, 152.3a, 152.5a, 152.6 — shippable-минимум,
§3А); **Фаза B** (152.4, 152.3b, 152.5b — Unicode) — после, за `[M-152-*]`-маркерами.

**Definition of Done для каждого sub-plan** (без упрощений, прод-уровень):
1. Реализация по scope файла `152.N`.
2. Spec: D-блок(и) + амендменты в `spec/decisions/`; Q — записи-решения в
   `spec/open-questions.md`.
3. Доки: `docs/strings.md` / `docs/strings-internals.md` / `docs/migration/*` (как
   указано в §4).
4. Тесты: pos **и** neg фикстуры в `nova_tests/plan152_N/`, прогон через **релизные**
   `nova` + компилятор.
5. Критерии приёмки файла `152.N` + глобальные G1–G6 (§6) выполнены.
6. Полный `nova test` без новых FAIL vs baseline.
7. Коммит **пофазно** (каждая Ф.X — отдельный коммит).

**Конвенции репо.** `git add` только конкретных файлов (рядом работают другие
агенты — никогда `-A`/`.`); перед `git commit` всегда `git diff --cached --stat`;
после крупной задачи обновлять `project-creation.txt` + `simplifications.md` +
`nova-private/discussion-log.md`. Синтаксис Nova — только из `spec/`+`examples/`, не
выдумывать.

**Фоновые агенты (если используются).**
- **НИКОГДА `git stash`** — stash/refs/reflog repo-global, общий `.git` с
  конкурентными worktree → collision → потеря изменений. Для чистого baseline
  используй **temp-worktree** (`git worktree add` на нужный ref) или
  **commit-reset** (закоммить → измеряй → `git reset` назад), не stash.
- **Rate-limit устойчивость.** Фоновые/workflow-агенты иногда ловят серверный
  rate-limit и падают. Поэтому: работай **идемпотентно** и **пофазно** —
  каждая Ф.X завершается коммитом-чекпойнтом, чтобы упавший агент можно было
  перезапустить с последнего коммита без потери. Не держи много несохранённой
  работы в одном долгом проходе; не полагайся на «дожить до конца» — дроби.
- **Изоляция.** Каждый параллельный sub-plan — в своём worktree (`nova-p152*`);
  не переключай ветки в чужом worktree; регистрируйся в worktree первой командой.

---

## D249 / D250 — полные драфты

Полные драфты **D249** (координатная модель) и **D250** (`CharsView` + конвенция
`as_`/`to_`) живут в их владельце — [Plan 152.1](152.1-coordinate-model-lenses.md)
(чтобы не дублировать/не расходиться). Краткая суть — в §0 (принцип), §2
(архитектура), §3 (summary 152.1). Остальные D-блоки (D251–D255) — в файлах
соответствующих sub-plans.

---

## Связанные D / Q-блоки

| D / Q | Связь |
|---|---|
| D249 (NEW) | координатная модель — линзы, без int-index/`len()`, char-итерация |
| D250 (NEW) | `CharsView` + конвенция `as_`/`to_` |
| D251 (NEW) | полный str-surface (152.2) |
| D252 (NEW) | `char`-тип API (152.3) |
| D253 (NEW) | `std/unicode` слой (152.4) |
| D254 (NEW) | модель сравнения + collation (152.5) |
| D255 (NEW) | UTF-16/32 interop (152.6) |
| D258 (NEW) | формат-спеки + опц. `Write`-sink (152.7) |
| D44/D183/D229 AMEND | расширение FormatSpec + `Display` sink (152.7) |
| D26 MAJOR AMEND | развернуть школу B → линзы/байт-координаты |
| D238 AMEND | RETRACT `str \| int \| char` |
| D58 AMEND | `for c in s` → `as_chars().iter()` |
| D239 | `[]u8 ≡ Vec[u8]` — байтовая линза бесплатно |
| D240 | `MutIndex` — `str`/`CharsView` иммутабельны, не реализуют (R8) |
| D241/D242 | `Next`/`Iter` — `CharsView` реализует `Next[char]` |
| Q-string-indexing | **ЗАКРЫТО заново** (был «вариант B») → линзы/byte-coords |
| Q-string-len | **ЗАКРЫТО** — нет бэар `len`; `byte_len()` + `as_chars().len()` |
| Q-unicode-data (NEW) | **ЗАКРЫТО** — codegen из UCD, версия-пин, lazy, без ICU |
| Q-string-collation (NEW) | **ЗАКРЫТО** — дефолт byte-`Ord`; UCA-collation opt-in (Phase B) |
| Q-module-internal-visibility (NEW) | **ЗАКРЫТО** — конвенция `_`-префикс, без keyword |
| Q-format-spec (NEW) | **ЗАКРЫТО** — Rust-style формат-спеки; `Write`-sink opt (152.7) |
