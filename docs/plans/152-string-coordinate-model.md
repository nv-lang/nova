<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 152 — Строковая модель координат: байты-координаты, без int-индексации, char-итерация

> **Создан:** 2026-06-13.  **Статус:** 📋 **PLANNED**, P1.
> **Эстимат:** ~1–1.5 dev-day (точечный refactor: dispatch-таблица + `string.nv`
> + новый `CharsIter` + миграция фикстур).  **Model:** Sonnet 4.6 + High + Thinking ON.
> **Зависит от:** Plan 138 (D238 `Index[K,V]`), Plan 139/139.1 (`str` lang-item),
> Plan 147 (D246 три оси мутабельности), D58/D241/D242 (`Next`/`Iter`).
> **Предложено пользователем:** «рассмотреть с чистого листа, как сделать лучше».

---

## Идея

`str` хранится как UTF-8 `(ptr *ro u8, len int)` (Plan 139). При UTF-8-хранении
O(1)-доступа по codepoint **не существует** — codepoint занимает 1–4 байта.
Сейчас в Nova сидит рассогласование: `len()` = **байты** (O(1)), а `str[i]` через
`Index[int, char]` (D238) = **codepoint** (O(n)). Это даёт:

- семантически кривой идиом `for i in 0..s.len() { s[i] }` (счётчик байт ↔ codepoint-индекс);
- footgun «индексация выглядит O(1), а на деле O(n)-скан»;
- латентный баг: `@pad_left`/`@pad_right` обещают «до width **codepoints**», а
  считают `width - @len()` в **байтах** ([std/runtime/string.nv:728](../../std/runtime/string.nv#L728));
- `@find`/`@rfind` возвращают **codepoint-offset**, который не композируется со
  slice'ом `s[a..b]` (байтовым) без повторного O(n)-скана offset→байт.

Решение (синтез Rust + Go + Swift, минус Python-O(1) на UTF-8 и минус UTF-16-грабли):

> **Строка — не random-access последовательность. Это UTF-8-буфер, который
> итерируют, ищут и режут по диапазону, но не индексируют целым числом. Система
> координат — байтовые смещения; codepoint и grapheme — это views поверх неё.**

Как только убираем целочисленную индексацию `str[i]`, вся дилемма «единиц»
исчезает по конструкции: `len` может быть байтами (дёшево, alloc-релевантно),
итерация — codepoint'ами (эргономично, ожидаемо линейно), и рассогласовываться
нечему — `len` и `[i]` больше не смешиваются.

---

## D249. Строковая модель координат (амендит D26, D238, D58)

### Что

`str` подчиняется единой координатной модели:

1. **Байтовый слой — правда, всегда открыт, дёшев.**
   `@byte_len() -> int` (O(1)); `@as_bytes() -> ro []u8` (view, O(1)); доступ к
   байту — `s.as_bytes()[i]` (явно «индексирую байты, O(1)»), а не `s[i]`.

2. **Целочисленной индексации `str[i]` нет.** `str` **не** реализует
   `Index[int, V]` — ни `int → char`, ни `int → u8`. Единственный `[]` —
   slice по байтовому диапазону.

3. **`str[a..b]` — byte-range slice**, zero-copy view, O(1), с проверкой границы
   на codepoint-boundary (panic, если `a`/`b` рассекают символ). `str` реализует
   только `Index[Range, str]`.

4. **Поиск возвращает байтовые смещения.** `@find`/`@rfind` → `Option[int]` =
   **байт**-offset (не codepoint). Тогда поиск + slice композируются за O(1):
   ```nova
   ro i = s.find("=")        // Option[int], байт-offset
   match i { Some(k) => ro val = s[k+1..], None => ... }  // zero-copy, O(1)
   ```

5. **Итерация привилегирована, её дефолт — codepoint (`char`).**
   `for c in s` → `char` (O(n) — не сюрприз, итерация и обязана быть линейной).
   Явные гранулярности: `@as_bytes()` (`u8`), `@chars()` (`char`, = дефолт),
   `@char_indices()` (`(int, char)` — байт-offset + char, чтобы резать после
   обхода). `@to_chars() -> []char` остаётся как allocating-форма.

6. **Codepoint-доступ — только явный, честно O(n).** `@char_at(i) -> Option[char]`
   и `@char_len() -> int` остаются; имя сразу сигналит про скан. «Дай N-й символ»
   целочисленным subscript'ом не выражается принципиально.

7. **Grapheme-слой — в `std/unicode` (future), не в ядре.** Требует Unicode-таблиц
   (десятки КБ), версионно- и локале-зависим. Как `unicode-segmentation` в Rust.

### `len()` — решение

`@len() = @byte_len()` (байты, O(1)) — **остаётся**. Безопасно, потому что
целочисленной индексации больше нет → смешивать единицы не с чем. Char-ориентированные
API (`@pad_left`/`@pad_right`/truncate/выравнивание) **обязаны** явно звать
`@char_len()`. См. «Что отвергнуто» про purist-вариант.

### Сводная таблица API

| Операция | Решение | Сложность |
|---|---|---|
| длина в байтах | `@byte_len()` (= `@len()`) | O(1) |
| длина в символах | `@char_len()` | O(n) |
| `s[i]` целым | **нет** (не `Index[int,_]`) | — |
| `s[a..b]` | byte-range, boundary-checked, zero-copy | O(1) |
| доступ к байту | `s.as_bytes()[i]` | O(1) |
| доступ к codepoint | `@char_at(i) -> Option[char]` (явно) | O(n) |
| `for c in s` | `char` (дефолт, через `@iter()`) | O(n) |
| views | `@as_bytes()` / `@chars()` / `@char_indices()` | — |
| поиск `@find`/`@rfind` | **байтовый** offset | O(n) |
| graphemes | `std/unicode` (future), не ядро | O(n) |

### Почему

- Нет целого индекса, который одновременно дёшев, корректен и однозначен на UTF-8;
  `str[i]` при любой единице — либо обрывок символа (байт), либо O(n)-ложь (codepoint),
  либо не-символ (codepoint ≠ grapheme). Swift убрал int-subscript, Rust оставил
  только slice — Nova следует им.
- Байтовые координаты композируются (`find` → `slice` за O(1)); codepoint-offset'ы — нет.
- Соответствует этосу Nova «никаких скрытых затрат» (D135): O(n) виден либо в
  имени (`char_*`), либо в итерации (которая и так линейна).

### Что отвергнуто

- **Go-вариант `str[i] -> u8` (байт, O(1)).** Индексация *строки* байтом
  провоцирует тот же `for i in 0..len`-антипаттерн; байт нужен редко и доступен
  явно через `s.as_bytes()[i]`. Subscript на строке резервируем под slice.
- **Python-вариант `str[i] -> char`, O(1).** Невозможен на UTF-8 без отказа от
  UTF-8-хранения (Plan 139 его зафиксировал); Python платит памятью (fixed-width
  rep).
- **Swift-вариант: дефолт = grapheme.** Максимально корректно, но тащит Unicode-data
  в ядро. Для системного языка с тонким рантаймом — библиотека.
- **Purist «убрать бэар `@len()`» (только `byte_len`/`char_len`).** Самый
  correctness-чистый (двусмысленности нет вообще), но дорогая миграция (каждый
  `@len()`-call) при малом выигрыше — индексации уже нет, ловушки «len vs index»
  тоже. Отложено; при желании — отдельный followup `[M-152-no-bare-len]`.

### Амендменты

- **[D26](../../spec/decisions/08-runtime.md#d26)** (str semantics): добавить
  «нет целочисленной codepoint-индексации; дефолт-итерация = `char`; поиск =
  байт-offset».
- **[D238](../../spec/decisions/03-syntax.md#d238)** (`Index[K,V]`): **RETRACT**
  строку std-реализаций `str | int | char (panic OOB)`. Остаётся
  `str | Range | str (byte-range view, panic OOB)`. `str` больше не реализует
  `Index[int, V]`.
- **[D58](../../spec/decisions/03-syntax.md#d58)** (`for x in c`): `str` реализует
  `@iter() -> CharsIter`, `CharsIter` реализует `Next[char]`; `for c in s` yields `char`.

---

## Фазы

- **Ф.0 — Spec.** D249 в `03-syntax.md` (рядом с D238) + амендмент-блоки к D26/D238/D58.
- **Ф.1 — Снять `Index[int, char]` со `str`.** Compiler built-in str-index dispatch
  (`emit_c.rs` + checker): `str[i]` целым → `E_STR_NO_INT_INDEX` с fix-it hint
  («использовать `s.char_at(i)` для символа или `s.as_bytes()[i]` для байта»).
  `str[a..b]` (`Index[Range, str]`) сохранить. `@char_at` остаётся явным методом.
- **Ф.2 — `@find`/`@rfind` → байт-offset.** Убрать cp-счётчик в
  [string.nv:144](../../std/runtime/string.nv#L144),[174](../../std/runtime/string.nv#L174);
  возвращать байтовый `i`. Обновить callers (`@replace` уже на `@split`, не на find).
- **Ф.3 — `CharsIter` + `str.@iter()`.** Новый Nova-тип `CharsIter` (поля:
  байт-view + cursor), `mut @next() -> Option[char]` (UTF-8 decode-курсор, тот же
  алгоритм, что `@char_at`/`@to_chars`), `@iter() -> CharsIter => self`. `str @iter()
  -> CharsIter`. `@char_indices() -> CharIndicesIter` (или allocating
  `[]( int, char )`) — даёт байт-offset для последующего slice.
- **Ф.4 — Починить единицы.** `@pad_left`/`@pad_right` → `@char_len()` вместо
  `@len()` ([string.nv:728](../../std/runtime/string.nv#L728),[736](../../std/runtime/string.nv#L736)).
  Audit прочих `@len()`-call-сайтов на «байт vs символ».
- **Ф.5 — Фикстуры + миграция.** Позитивные (`for c in s`, `s[a..b]`, `find`+slice
  композиция, `char_at`, `char_indices`) + негативные (`s[i]` целым →
  `E_STR_NO_INT_INDEX`). Мигрировать существующие `str[i]`-usages в fixtures/stdlib
  → `char_at(i)`/`as_bytes()[i]`. Тесты через релизные `nova` + компилятор.
- **Ф.6 — Закрытие.** Финализировать D249; обновить README plans, project-creation.txt,
  simplifications.md, nova-private/discussion-log.md. Закоммитить пофазно.

---

## Критерии приёмки

- **A1.** `str[i]` где `i: int` → compile-error `E_STR_NO_INT_INDEX` с fix-it.
- **A2.** `str[a..b]` (Range) работает как zero-copy byte-slice, паника при OOB и
  при рассечении codepoint-границы.
- **A3.** `for c in s { ... }` итерирует `char` (codepoint), порядок и значения
  идентичны `@to_chars()`.
- **A4.** `@find`/`@rfind` возвращают **байтовый** offset; `s.find(x)` → `s[k..]`
  композируется без повторного скана и даёт корректный zero-copy view.
- **A5.** `@char_at(i) -> Option[char]`, `@char_len()`, `@byte_len()`, `@as_bytes()`,
  `@chars()`, `@char_indices()` присутствуют и согласованы.
- **A6.** `@pad_left(width, _)`/`@pad_right(width, _)` считают ширину в **codepoint'ах**
  (тест с не-ASCII: `"é".pad_left(3,'·')` == `"··é"`, длина 3 символа).
- **A7.** `@len() == @byte_len()` (байты, O(1)) — без регрессий в существующих
  байт-ориентированных call-сайтах (parse_int и т.д.).
- **A8.** `str` структурно реализует `Index[Range, str]` и `Iter[CharsIter]`, и
  **не** реализует `Index[int, _]`.
- **A9.** Полный `nova test` без новых FAIL vs baseline; plan152-фикстуры зелёные.

---

## Оценка миграции

| Точка | Объём |
|---|---|
| Compiler dispatch (снять `Index[int,char]` для str + `E_STR_NO_INT_INDEX`) | малый (1 ветка в str-index lowering + checker-guard) |
| `string.nv`: find/rfind → байт, pad_* → char_len | малый (3 метода) |
| Новый `CharsIter` + `str.@iter` + `char_indices` | средний (1 Nova-тип + decode-курсор уже есть) |
| Миграция `str[i]`-usages (fixtures/stdlib) | малый-средний (sweep по `\bstr-var\s*\[`; точное число — в Ф.1 audit) |
| Фикстуры pos/neg | малый |

Риск низкий: координатная модель уже наполовину байтовая (`len`/`byte_at`/
`as_bytes`/`split`), убираем именно неконсистентный узел (`Index[int,char]` +
codepoint-offset find). Компилятор-инфра (Index/Iter/Next) готова из Plan 138/D58.

---

## Связанные D-блоки

| D | Связь |
|---|---|
| D249 (NEW) | строковая координатная модель |
| D26 AMEND | str semantics — нет int-index, char-итерация, байт-offset find |
| D238 AMEND | RETRACT `str \| int \| char`; остаётся `str \| Range \| str` |
| D58 AMEND | `str @iter() -> CharsIter`; `for c in s` yields `char` |
| D240 | `MutIndex` — `str` иммутабельна, не реализует (R8) |
| D241/D242 | `Next[T]`/`Iter[I]` — `CharsIter` реализует `Next[char]` |
