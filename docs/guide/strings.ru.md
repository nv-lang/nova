---
source_rev: 27d5dd055
source_date: 2026-08-02
---

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Строки в Nova — модель линз

[English](strings.md) | **Русский**

> Plan 152.1 (D249/D250). `str` — тонкий «кусок текста»; ты работаешь через
> **линзы представления**, а координаты **байтовые**. Цена всегда видима —
> под `[i]` или `len` нет скрытого O(n).

## Модель

`str` хранит UTF-8 как `(ptr *ro u8, len int)` и **всегда валидный UTF-8**
(инвариант R-UTF8). Она иммутабельна. Ты не индексируешь и не измеряешь `str`
напрямую — ты выбираешь **линзу**:

```
                          str  (thin: identity, slice s[a..b], search→byte-offset)
        as_bytes() ▼                              as_chars() ▼
   ro []u8  (Vec[u8] view)                   CharsIter  (decoding stream)
   O(1) [i] / len() / slice / iterate        next / count / nth / is_empty — O(n)
   ── byte layer (u8) ──                      ── codepoint layer (char) ──

        as_graphemes() ▼   (opt-in: import std.unicode)
   GraphemesIter  (UAX #29 cluster stream)
   next / count / is_empty — O(n);  no [i]
   ── grapheme layer (visible "character", a str slice) ──

        as_words() ▼   (opt-in: import std.unicode)
   WordsIter  (UAX #29 word segments; O(1) to create)
   next / count / is_empty;  no [i]
   ── word layer (words / spaces / punctuation, str slices) ──

        as_sentences() ▼   (opt-in: import std.unicode)
   SentencesIter  (UAX #29 sentence segments; O(1) to create)
   next / count / is_empty;  no [i]
   ── sentence layer (a sentence + its trailing whitespace, str slices) ──
```

- **`as_bytes()` — реинтерпретация** — байты физически лежат непрерывно, так что
  это настоящий `ro []u8` с O(1) `[i]`/`len()`. Zero-copy.
- **`as_chars()` — декодирующая линза** — codepoint-ы вычисляются на лету, так что
  это *поток* (итератор), а не коллекция: `count()`/`nth(i)` — O(n), и намеренно
  **нет позиционного `at(i)`/`len()`** (это приглашало бы `for i in 0..len {
  at(i) }` = O(n²)). Зеркалит `str::chars()` из Rust.
- **`as_graphemes()` — линза пользовательского восприятия** — extended grapheme
  clusters (UAX #29): то, что человек видит как один символ, даже когда он
  тянется на несколько codepoint-ов (`é` = `e`+◌́; `🇺🇸` = 2 regional indicators;
  `👨‍👩‍👧` = ZWJ-emoji последовательность — каждый — **один** графема). Тоже
  *поток* (`next`/`count`/`is_empty`, O(n), без `[i]`). Он **опт-ин**
  (`import std.unicode`), потому что нужны Unicode-таблицы — байтовый/codepoint-слои
  выше остаются без таблиц. См. [Unicode-операции](#unicode-operations-opt-in-stdunicode)
  ниже.
- **`as_words()` — word-segment линза** — границы слов UAX #29 (`import
  std.unicode`); итерирует слова, пробелы и пунктуацию как `str`-срезы. O(1) на
  создание (forward state-machine, лениво). Питает `to_titlecase`.
- **`as_sentences()` — sentence-segment линза** — границы предложений UAX #29
  (`import std.unicode`); итерирует предложения (каждое с хвостовым пробелом
  и терминатором) как `str`-срезы. O(1) на создание (forward state-machine,
  лениво). Примечание: алгоритм UAX #29 по умолчанию **не имеет словаря
  сокращений**, так что `"Mr. Smith"` разделяется после `Mr.` (заглавная буква
  после `.` — граница) — это документированное поведение спеки, не баг.

## Длина

| Ты хочешь | Используй | Цена |
|---|---|---|
| длину в байтах | `s.byte_len()` | O(1) |
| число codepoint-ов | `s.as_chars().count()` | O(n) |
| число графем | `s.as_graphemes().count()` (`import std.unicode`) | O(n) |
| число слов | `s.as_words().count()` (`import std.unicode`) | O(n) |
| число предложений | `s.as_sentences().count()` (`import std.unicode`) | O(n) |

Голого `s.len()` **нет** — у `str` три расходящиеся длины (байты, codepoint-ы,
графемы), так что единица всегда явная. `s.len()` → `E_STR_NO_LEN`.

## Доступ к элементу

| Ты хочешь | Используй | Цена |
|---|---|---|
| i-й байт | `s.as_bytes()[i]` → `u8` (panic OOB) | O(1) |
| i-й codepoint | `s.as_chars().nth(i)` → `Option[char]` | O(n) |
| codepoint + байтовый offset | `s.as_chars().indices()` → `CharIndicesIter` → `Option[(int, char)]` | O(n) на шаг |

Целочисленного индекса `s[i]` **нет** — codepoint-индексация UTF-8 — это O(n),
спрятанный за `[i]`. `s[i]` → `E_STR_NO_INT_INDEX`.

## Срезы

`s[a..b]` — **байтовый диапазон** zero-copy представление (разделяет буфер).
Границы — это `requires`-контракт (zero-cost, когда компилятор может их доказать,
Plan 140.2); срез через границу codepoint-а паникует (это сломало бы R-UTF8).
Безопасная, не паникующая форма — `s.get(a..b) -> Option[str]` (None на OOB /
расщеплении codepoint-а).

```nova
ro s = "héllo"           // byte_len 6: h(0) é(1,2) l(3) l(4) o(5)
ro head = s[0..1]        // "h"
ro e    = s[1..3]        // "é"  (the 2 bytes of é)
ro tail = s[3..]         // "llo"
// s[1..2] would panic — it cuts é in half
```

Байтовые offset-ы композируются с поиском за O(1):

```nova
match s.find("=") {           // find returns a BYTE offset
    Some(k) => ro rest = s[k+1..],
    None => ...,
}
```

## Итерация

```nova
for c in s { ... }            // char (codepoints) — the default unit
for b in s.as_bytes() { ... } // u8 (bytes) — explicit
```

## Владеющие копии

Линзы `as_*` заимствуют (zero-copy). Для независимого владеющего значения
используй `to_*`: `s.to_bytes() -> []u8`, `s.to_chars() -> []char` (обе
аллоцируют).

## Unicode-операции (опт-ин: `std/unicode`)

Ядровые линзы выше — **ASCII-полные и байт/codepoint-корректны без каких-либо
Unicode-таблиц**. Операции, которым нужна Unicode Character Database, живут
в отдельном модуле `std/unicode`, который ты импортируешь явно — так программа,
не делающая Unicode-нормализацию/сегментацию, никогда не платит за таблицы
(они range-кодированы и лениво инициализируются, припинены к `UNICODE_VERSION`,
генерируются из официального UCD командой `nova-codegen unicode`; без ICU /
OS-зависимости).

### Нормализация (UAX #15)

```nova
import std.unicode

ro a = "e\u{301}"            // "e" + combining acute
ro b = "é"                   // precomposed U+00E9
assert(normalize_nfc(a) == normalize_nfc(b))   // canonically equal
assert(normalize_nfkc("ﬁ") == "fi")            // compatibility fold of the ligature
```

- `normalize_nfc(s) -> str`, `normalize_nfd(s) -> str` — каноническая
  (де)композиция.
- `normalize_nfkc(s) -> str`, `normalize_nfkd(s) -> str` — compatibility-формы.

Полный алгоритм UAX #15 (декомпозиция + каноническое упорядочивание по CCC +
каноническая композиция с правилом блокировки + алгоритмический Hangul) —
проверено против официального `NormalizationTest.txt`.

### Графемные кластеры (UAX #29)

`str.@as_graphemes() -> GraphemesIter` — третья линза — итерируй по
пользовательски-воспринимаемым символам:

```nova
import std.unicode

assert("é".as_graphemes().count() == 1)        // e + combining acute → 1
assert("🇺🇸".as_graphemes().count() == 1)        // 2 regional indicators → 1 flag
assert("👨‍👩‍👧".as_graphemes().count() == 1)        // ZWJ-emoji family → 1

for g in "a🇺🇸b".as_graphemes() {              // g is a str slice of one cluster
    // "a", "🇺🇸", "b"
}
```

`GraphemesIter` зеркалит `CharsIter` (value-record поток): `next() -> Option[str]`,
`count()`, `is_empty()`, O(n), без позиционного `[i]`. Реализует правила extended
grapheme cluster GB1–GB13 **плюс GB9c** (Indic Conjunct Break, Unicode 15.1) —
проверено против официального `GraphemeBreakTest.txt`.

### Case-folding и Unicode case mapping

Локале-независимо, мульти-codepoint. Конвенция: **голый `to_upper`/`to_lower` =
полный Unicode-mapping** (под `import std.unicode`; нужны таблицы); **суффикс
`_ascii_` = ASCII-only, без таблиц, всегда доступен** (`to_ascii_upper`/
`to_ascii_lower` из prelude). Вызов `s.to_upper()` без `import std.unicode` →
ошибка компиляции (E7320) — компилятор не будет молча откатываться к ASCII.

```nova
import std.unicode

assert(fold_case("MASSE") == fold_case("masse"))   // caseless match
assert(fold_case("ß") == "ss")                      // full fold
assert("straße".to_upper() == "STRASSE")            // ß → SS (multi-cp)
assert("ﬁle".to_upper() == "FILE")                  // ligature ﬁ → FI
assert("ΟΔΟΣ".to_lower() == "οδος")                  // final Σ → ς, others → σ

// ASCII-only variants (always available, no import needed):
assert("hello".to_ascii_upper() == "HELLO")
assert("HELLO".to_ascii_lower() == "hello")
```

- `s.fold_case()` — полный case-folding (UCD `CaseFolding` C+F) для
  caseless-сопоставления. Не нормализация: для канонически-эквивалентного текста
  сначала нормализуй, потом fold.
- `s.to_upper()` / `s.to_lower()` — полный Unicode case mapping, включая
  контекстное правило **Final_Sigma** (греческая Σ → ς в конце слова, σ иначе).
  Без locale-tailoring (тюркский/литовский). Требуют `import std.unicode`.
- `s.to_ascii_upper()` / `s.to_ascii_lower()` — ASCII-only (только A–Z/a–z);
  без таблиц; всегда доступны из prelude.

### Классификация codepoint-ов (`char`) и регистр

ASCII-методы типа `char` (`is_ascii_alphabetic`, `to_digit`, `len_utf8`, … —
prelude, без таблиц) получают Unicode-aware коллег под `import std.unicode`.
Они **1:1 с UCD** (не ASCII-аппроксимация) и совпадают с `char` из Rust:

```nova
import std.unicode

assert('Ω'.is_alphabetic())          // U+03A9 GREEK CAPITAL OMEGA (Lu)
assert('٣'.is_numeric())             // U+0663 ARABIC-INDIC DIGIT THREE (Nd)
assert('½'.is_numeric())             // U+00BD VULGAR FRACTION (No)
assert('\u{A0}'.is_whitespace())     // NO-BREAK SPACE (Zs)
assert('A'.general_category() == Lu) // import std.unicode.{Lu}
assert('ß'.to_uppercase() == "SS")   // multi-code-point → str (not one char)
assert('ﬁ'.to_uppercase() == "FI")   // ligature ﬁ → "FI"
```

- `@is_alphabetic` / `@is_numeric` / `@is_alphanumeric` / `@is_whitespace` /
  `@is_uppercase` / `@is_lowercase` / `@is_control` — бинарные предикаты над UCD.
- `@general_category() -> GeneralCategory` — UCD General_Category (TR44, 30
  значений `Lu`…`Cn`); `Cn` (не назначен) для любого codepoint-а,
  отсутствующего в UCD.
- `@to_uppercase() -> str` / `@to_lowercase() -> str` — полный по-codepoint-овый
  case mapping. Возвращают **`str`** (не один `char`), потому что один codepoint
  может отобразиться в несколько (ß → `"SS"`, İ → `"i"` + ◌̇). Final_Sigma —
  правило строкового уровня, так что одинокая Σ приводится к σ
  (бесконтекстный ответ).

Они делегируют в codepoint-таблицы `std/unicode` (`category_data.nv`:
General_Category + Alphabetic + White_Space из UCD 16.0) и в case-карты
(`case_data.nv`). Как и линзы выше, они **опт-ин** — без `import std.unicode`
Unicode-классификация вне скоупа (ASCII-core методы `char` остаются доступны
из prelude).

> **Резолюция методов:** `s.to_upper()` и `s.to_lower()` определены только под
> `import std.unicode`. Без этого импорта имена не резолвятся → ошибка компиляции
> `E7320`. Молчаливого ASCII-фолбэка нет. `s.to_ascii_upper()` /
> `s.to_ascii_lower()` всегда доступны и являются правильным выбором, когда
> Unicode-таблицы не нужны.

### Сегментация слов и title-casing (UAX #29)

`str.@as_words() -> WordsIter` — четвёртая линза — итерируй UAX #29 word-сегменты
(слова, пробелы и пунктуацию — каждый кусок между границами). O(1) на создание
(forward state-machine, лениво — без жадной материализации границ).

```nova
import std.unicode

assert("can't 3.14".as_words().count() == 3)         // "can't" | " " | "3.14"
assert(to_titlecase("hello world") == "Hello World") // first cased char per word
assert(to_titlecase("ﬁle") == "File")                // ﬁ → "Fi" (title mapping)
```

- `as_words()` / `WordsIter` — `next()`/`count()`/`is_empty()`, правила границ
  UAX #29 WB1–WB16 (обрабатывает `can't`, `3.14`, regional-indicator флаги,
  ZWJ-emoji).
- `to_titlecase(s)` — титульные-регистр первого cased-символа каждого слова
  (используя **titlecase**-mapping, например ǆ → ǅ, не uppercase Ǆ) и lowercase
  остального с Final_Sigma. Локале-независимо.

### Сегментация предложений (UAX #29)

`str.@as_sentences() -> SentencesIter` — пятая линза — итерируй UAX #29
sentence-сегменты (каждое предложение вместе с хвостовым пробелом
и терминатором). O(1) на создание (forward state-machine, лениво;
SB8-lookahead ограничен пер-сегментом, амортизированный O(1)).

```nova
import std.unicode

assert("3.4".as_sentences().count() == 1)            // ATerm between digits (SB6)
assert("the resp. leaders are".as_sentences().count() == 1) // lowercase after "." (SB8)
{
    mut sv = "Hello! World".as_sentences()
    assert(sv.next() == Some("Hello! "))             // STerm + space + capital → split
    assert(sv.next() == Some("World"))
    assert(sv.next() == None)
}
```

- `as_sentences()` / `SentencesIter` — `next()`/`count()`/`is_empty()`, правила
  границ UAX #29 SB1–SB11 (+ SB998 default-no-break). Дефолтный UAX #29 **не
  имеет словаря сокращений**: `"Mr. Smith went home. He slept."` даёт три
  сегмента (`"Mr. "`, `"Smith went home. "`, `"He slept."`), потому что заглавная
  буква после `.` — граница. Это документированное поведение спеки.

### Коллация (UCA / DUCET-порядок)

Дефолтный `compare`/`<` у `str` — **байтово-лексикографический** (быстрый,
детерминированный, локале-независимый). Для Unicode-aware упорядочивания
`import std.unicode` даёт UCA (UTS #10) DUCET-коллятор — `str` никогда не
коллятит молча (D254):

```nova
import std.unicode

assert(collate_compare("apple", "Apple") < 0)   // case is tertiary, not primary
assert(collate_compare("café", "cafe") > 0)      // accent is secondary
ro key = collate_sort_key("naïve")               // Vec[u32] sort key (cache for sorting)
ro r = Collator.order("a", "b")                  // Collator.order/key/same (DUCET namespace)
```

- `collate_compare(a,b) -> int` (-1/0/+1), `collate_sort_key(s) -> Vec[u32]`,
  `collate_eq`, `Collator.order/key/same` (bodyless namespace, без инстанса).
  Мульти-уровневый (primary/secondary/tertiary + quaternary) **Shifted**
  variable-weighting; сначала NFD-нормализует; обрабатывает контракции (вкл.
  discontiguous UCA S2.1) и implicit-веса (CJK и т.д.).
- Скоуп: **DUCET (root, non-tailored)**. CLDR locale-tailoring + `eq_ignore_case`
  — в роадмапе (Plan 152.5b, `[M-152-collation-tailoring]`) — как DUCET-режим
  `unicode-collation` в Rust / root-коллятор ICU.

> **Почему свободные функции, а не методы `str`?** Строковые трансформации
> (`trim_ascii`, `to_ascii_lower`, `to_upper` и т.д.) — методы `str`, потому что
> они вписываются в идиому «преобразуй эту строку». Колляция (`collate_compare`,
> `collate_sort_key`, `Collator`) намеренно **не** `str @compare`/`@equal` —
> колляция никогда не должна молча заменять дефолтный байтовый `Ord`
> (решение D254). Асимметрия намеренная, не недосмотр.

## Encoding-interop (UTF-16 / codepoint-ы)

Для FFI / JS-interop / протоколов `import std.encoding.utf16` добавляет UTF-16
и сырые codepoint-конверсии (не в prelude — это interop-заботы, а не ежедневные
строковые операции):

- `s.encode_utf16() -> []u16` — UTF-16 code units (суплементарные codepoint-ы
  становятся суррогатными парами).
- `str.from_utf16(units []u16) -> Result[str, Utf16Error]` — проверенное
  декодирование; одинокий или обрезанный суррогат — это `Err`, так что результат
  всегда валидный UTF-8 (R-UTF8).
- `s.code_points() -> []int` — сырые `int` codepoint-ы (без `char`-обёртки),
  те же значения, что `as_chars()` приведён к `int`.

`from_utf16(s.encode_utf16()) == Ok(s)` round-trips на ASCII, BMP
и суплементарных (например `"😀"`). Суррогатные хелперы (`is_high_surrogate`/
`is_low_surrogate`/`decode_surrogate_pair`) живут в том же модуле.

## Где живёт каждая операция

| Операция | Метод | Примечания |
|---|---|---|
| длина в байтах | `str.byte_len()` | O(1), читает поле `len` |
| байтовая линза | `str.as_bytes() -> ro []u8` | O(1) `[i]`/`len()` |
| codepoint-линза | `str.as_chars() -> CharsIter` | `next`/`count`/`nth`/`is_empty` |
| графемная линза | `str.as_graphemes() -> GraphemesIter` | `import std.unicode`; UAX #29 |
| word-линза | `str.as_words() -> WordsIter` | `import std.unicode`; UAX #29 |
| нормализация | `normalize_nfc/nfd/nfkc/nfkd(s)` | `import std.unicode`; UAX #15 |
| case fold / map / title | `s.fold_case()`/`s.to_upper()`/`s.to_lower()`/`to_titlecase(s)` | `import std.unicode` |
| case (ASCII-only) | `s.to_ascii_upper()`/`s.to_ascii_lower()` | всегда доступны, без импорта |
| классификация char (Unicode) | `c.is_alphabetic`/`is_numeric`/`is_whitespace`/`general_category` | `import std.unicode`; 1:1 UCD |
| char case (Unicode) | `c.to_uppercase()`/`to_lowercase() -> str` | `import std.unicode`; multi-cp |
| codepoint + байтовый offset | `s.as_chars().indices() -> CharIndicesIter` | `next()->(int,char)` |
| срез | `str[a..b]` / `str.get(a..b)` | байтовый диапазон, zero-copy |
| поиск | `find`/`rfind`/`contains`/`starts_with`/`ends_with` | байтовые offset-ы |
| split/trim/replace/pad/repeat/concat | `transform`/`search` | см. std/runtime/string/ |
| владеющие байты/chars | `to_bytes`/`to_chars` | alloc |
| UTF-16 / codepoint-ы | `encode_utf16`/`from_utf16`/`to_code_points` | `import std.encoding.utf16` |
| identity | `==` / `compare` / `hash` / clone | на основе содержимого (байтовый `Ord`) |
| колляция (UCA) | `collate_compare`/`collate_sort_key`/`Collator` | `import std.unicode`; DUCET/UTS #10 |

> Нормализация (UAX #15) и графемная сегментация (UAX #29) поставляются в опт-ин
> модуле `std/unicode` — см. [Unicode-операции](#unicode-operations-opt-in-stdunicode).
> Ядровые линзы выше — ASCII-полные и байт/codepoint-корректны без каких-либо
> Unicode-таблиц.

## Политика ошибок

| Ситуация | Используй | Пример |
|---|---|---|
| Нарушение инварианта (баг программиста), выход за границы | **panic** | `s.as_bytes()[i]` OOB; `s[a..b]` через границу codepoint-а |
| Ожидаемое отсутствие (не найдено, пусто, индекс за концом) | **`Option`** | `s.find(needle) -> Option[int]`; `iter.next() -> Option[char]` |
| Восстановимая ошибка внешнего ввода | **`Result`** | `str.parse_int() -> Result[int, ParseIntError]`; `str.from_utf16() -> Result[str, _]` |
| Best-effort декод ненадёжных байт | **lossy U+FFFD** | `str.from_bytes_lossy`; `cps_to_str` (invalid cp → `\u{FFFD}`) |

Правила (источник: protocols.nv, D325/Plan 177, D25):
- **`parse_int(s)` возвращает `Result[int, ParseIntError]`** — каждая падающая
  операция — это `Result` (D325). Бросай на call-site через `!!`, получай
  `Option` через `.ok()`. Нет близнеца с голым броском, нет `_opt`.
- **Никогда** не возвращай пустую строку при падении — это неотличимо от пустого
  ввода. Используй `Option`/`Result` вместо этого.
- `*_lossy` функции всегда возвращают валидный UTF-8; они подставляют `U+FFFD`
  для каждой невалидной байтовой последовательности, никогда молча не дропают
  байты.

## Интерполяция и format-спеки

Интерполяция — это `${expr}` (Display) / `${expr:?}` (Debug). Формат-спека
в стиле Rust следует за двоеточием —
`${expr:[[fill]align][sign][#][0][width][.precision][type]}` (Plan 152.7-B,
D258):

```nova
assert("${42:5}" == "   42")        // min width, right-aligned (numbers)
assert("${42:<5}" == "42   ")       // left align
assert("${42:*^7}" == "**42***")    // fill + center
assert("${42:05}" == "00042")       // zero-pad
assert("${255:x}" == "ff")          // hex; X=upper, b=binary, o=octal
assert("${255:#x}" == "0xff")       // # alternate radix prefix (always lowercase)
assert("${3.14159:.2}" == "3.14")   // precision (f64); for str = truncate
```

Некорректная спека — **ошибка компиляции** (`E_FORMAT_SPEC_UNKNOWN` /
`E_BAD_FORMAT_SPEC`), никогда не молчаливый пропуск. (Обобщение форматтера на
запись в любой `Write`-приёмник — `@display(mut w Write)` — в роадмапе,
Plan 152.7.1.)

### Вложенные строки внутри `${...}` (Plan 102, D44-amendment)

Тело `${...}` — настоящее Nova-выражение, так что оно может легально содержать
свои строковые литералы — включая обычный символ кавычки `"` — без
экранирования. Это прецедент PEP 701 / JS template-literal: оказавшись внутри
`${`, лексер сканирует *выраженческий* синтаксис, а не *строковый*, так что
вложенный `"..."` — это просто ещё один строковый литерал, а не терминатор
внешнего.

```nova
ro name = req.param("name")           // plain code, for reference

"hello, ${req.param("name")}"         // method call with a string argument
"${m["key"]}"                         // index syntax with a string key (D238 @index)
"${f("x ${y} z")}"                    // interpolation nested inside a nested string
```

Record-литеральные/блочные скобки `{ }` внутри выражения тоже корректно
вкладываются (например `${ if cond { a } else { b } }`), и escape `\${` для
литерального `${` не меняется. Изменилось только то, что сканер, находящий
парную `}` интерполяции, теперь string/brace-aware (одна общая процедура,
`scan_interpolation_body` в `compiler-codegen/src/lexer/mod.rs`, используется
и лексером, и split-шагом парсера) вместо остановки на первой неэкранированной
`"`, которую он видит — старый одномерный скан трактовал эту внутреннюю `"` как
конец *внешнего* строкового литерала.
