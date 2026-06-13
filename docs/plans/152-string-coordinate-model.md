<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 152 — Строковая модель: линзы-типы (`as_bytes`/`as_chars`), байт-координаты, без int-индексации

> **Создан:** 2026-06-13.  **Статус:** 📋 **PLANNED**, P1.
> **Эстимат:** ~2–3 dev-day (новый тип `CharsView` + перенос char-слоя с `str` в
> линзу + sweep call-сайтов).  **Model:** Sonnet 4.6 + High + Thinking ON.
> **Зависит от:** Plan 138 (D238 `Index[K,V]`, D239 `[]T≡Vec[T]`), Plan 131 (`Vec`
> на raw ptr), Plan 139/139.1 (`str` lang-item), Plan 147 (D246), D58/D241/D242
> (`Next`/`Iter`).
> **Предложено пользователем:** «с чистого листа» → линзы `as_bytes()`/`as_chars()`,
> убрать бэар `len()` (2026-06-13).

---

## Идея

`str` хранится как UTF-8 `(ptr *ro u8, len int)` (Plan 139). При UTF-8-хранении
O(1)-доступа по codepoint **нет** — символ занимает 1–4 байта. Сейчас в Nova сидит
рассогласование: `len()` = байты (O(1)), а `str[i]` через `Index[int, char]` (D238)
= codepoint (O(n)); `@find`/`@rfind` отдают codepoint-offset, не композирующийся с
байтовым slice; `@pad_*` обещают «width codepoints», а считают байты
([string.nv:728](../../std/runtime/string.nv#L728)).

Решение — **линзы-типы (Swift-модель views), а не плоские `*_at`/`*_len` на `str`:**

> **`str` — это «кусок текста», тонкий. Чтобы работать с ним, выбираешь
> представление-линзу: `as_bytes()` для байтов, `as_chars()` для символов. Каждая
> линза несёт свои методы, согласованные по единице. Целочисленной индексации
> самого `str` нет — координаты байтовые, символы вычисляются.**

### Глубокая причина — асимметрия двух линз

- **`as_bytes()` — reinterpretation, не вычисление.** Байты UTF-8 физически лежат
  подряд → линза = настоящий `ro []u8` (≡ `Vec[u8]`, D239) с **O(1) `[i]`, `len()`,
  срезами, итерацией — всё бесплатно из `Vec[u8]`**. Ничего не декодируется.
- **`as_chars()` — decoding lens.** Codepoint'ы НЕ хранятся как массив — каждый
  вычисляется при проходе. Поэтому линза **не может** быть `[]char` (это была бы
  материализация, alloc) — она отдельный view-тип `CharsView`, декодирующий на лету.

Эта асимметрия — правда UTF-8, и хорошо, что её видно в типах: `[]u8`
(индексируемый слайс) vs `CharsView` (итерируемая линза). Байты — есть; символы —
вычисляются.

### Что это даёт

1. **Вопрос «в каких единицах `len`?» растворяется по конструкции** — один `len()`
   на каждой линзе, где он однозначен: `s.as_bytes().len()` (байты, O(1)) /
   `s.as_chars().len()` (codepoint'ы, O(n)). Разноимённых `byte_len`/`char_len`
   на `str` не нужно.
2. **Симметрия `as_` (линза, дёшево, алиас) vs `to_` (владеемая копия, alloc):**
   | | view (линза) | owned (копия) |
   |---|---|---|
   | байты | `as_bytes() -> ro []u8` | `to_bytes() -> []u8` |
   | символы | **`as_chars() -> CharsView`** (NEW) | `to_chars() -> []char` |
3. **`str` тонкий и концептуальный**; расширяемо — graphemes станут ещё одной
   линзой `as_graphemes()` из `std/unicode`, симметрично.

---

## D249. Строковая координатная модель: линзы вместо плоских методов

### Что

1. **`str` не индексируется целым числом.** `str` **не** реализует `Index[int, V]`
   (ни `→char`, ни `→u8`). Бэар `str[i]` → `E_STR_NO_INT_INDEX` с fix-it («символ —
   `s.as_chars().at(i)`; байт — `s.as_bytes()[i]`; срез — `s[a..b]`»).

2. **Единственный `[]` на `str` — byte-range slice `str[a..b]`**: zero-copy view,
   O(1), с проверкой границы на codepoint-boundary (panic при OOB / рассечении
   символа). `str` реализует только `Index[Range, str]`.

3. **Байтовая линза `@as_bytes() -> ro []u8`** (есть). Весь байтовый слой — оттуда
   и бесплатно из `Vec[u8]`: `s.as_bytes()[i]` (O(1) байт), `.len()` (O(1) байт),
   `for b in s.as_bytes()`, срезы. `@byte_at` ретайрится (избыточен → `as_bytes()[i]`).

4. **Символьная линза `@as_chars() -> CharsView`** (NEW, см. [D250](#d250)). Весь
   codepoint-слой — оттуда: `len()` (O(n)), `at(i)` (O(n)), итерация `Next[char]`,
   `indices()`. Плоские `@char_at`/`@char_len`/`@char_indices` со `str` **убраны**
   (переезжают в `CharsView`).

5. **Поиск возвращает байтовые смещения.** `@find`/`@rfind` → `Option[int]` =
   **байт**-offset (не codepoint) → композируется с `s[k..]` за O(1):
   ```nova
   ro i = s.find("=")
   match i { Some(k) => ro val = s[k+1..], None => ... }   // zero-copy, O(1)
   ```

6. **Итерация привилегирована, дефолт — `char`.** `for c in s` → `char` (делегирует
   в `s.as_chars().iter()`, чтобы частый случай — обход символов — остался коротким).
   O(n) тут не сюрприз (итерация и обязана быть линейной).

7. **Бэар `len()` на `str` — нет.** Длина — свойство представления, живёт на линзе:
   `s.as_bytes().len()` / `s.as_chars().len()`. `s.len()` → `E_STR_NO_LEN` с fix-it.
   **Поблажка (recommended):** оставить `@byte_len() -> int` (O(1), читает поле `len`)
   как шорткат — байтовая длина нужна повсюду для аллокаций, имя однозначно. Char-
   длины шортката нет (только `as_chars().len()`). Поле `len` в layout остаётся
   (storage-инвариант) — убирается публичный *метод* `@len()`.

8. **Grapheme-слой** — будущая линза `@as_graphemes() -> GraphemesView` из
   `std/unicode` (Unicode-таблицы, версионно-зависимо) — не ядро.

### Сводная таблица API

| Нужно | Как | Сложность |
|---|---|---|
| байт по индексу | `s.as_bytes()[i]` | O(1) |
| длина в байтах | `s.as_bytes().len()` или шорткат `s.byte_len()` | O(1) |
| итерация по байтам | `for b in s.as_bytes()` | O(n) |
| символ по индексу | `s.as_chars().at(i) -> Option[char]` | O(n) |
| длина в символах | `s.as_chars().len()` | O(n) |
| итерация по символам | `for c in s` или `for c in s.as_chars()` | O(n) |
| (byte-offset, char) пары | `s.as_chars().indices()` | O(n) |
| `s[i]` целым | **нет** (`E_STR_NO_INT_INDEX`) | — |
| срез | `s[a..b]` (byte-range, boundary-checked, zero-copy) | O(1) |
| бэар `len()` | **нет** (`E_STR_NO_LEN`) | — |
| поиск | `@find`/`@rfind` → **байт-offset** | O(n) |
| owned-копии | `to_bytes() -> []u8` / `to_chars() -> []char` | O(n) |
| graphemes | `as_graphemes()` (future, `std/unicode`) | O(n) |

### Почему

- Нет целого индекса строки, который одновременно дёшев, корректен и однозначен на
  UTF-8 (байт = обрывок, codepoint = O(n)-ложь, codepoint ≠ grapheme). Swift убрал
  int-subscript, Rust оставил только slice — Nova следует им.
- Линза отвечает на вопрос «в каких единицах» самим своим типом: `len()`/`at()`
  на линзе однозначны, плоские `byte_*`/`char_*` на `str` не нужны.
- Байт-координаты композируются (`find`→`slice` за O(1)); codepoint-offset'ы — нет.
- Соответствует этосу «никаких скрытых затрат» (D135): O(n) виден либо в линзе
  (взял `as_chars` — принял O(n)), либо в итерации (она и так линейна).

### Что отвергнуто

- **`str[i] -> u8` (Go) / `str[i] -> char` (Python).** Индексация *строки* любой
  единицей провоцирует `for i in 0..len`-антипаттерн; нужное доступно явно через
  линзу. Python-O(1) к тому же невозможен на UTF-8 (Plan 139 зафиксировал хранение).
- **Дефолт = grapheme (Swift).** Корректнее всего, но тащит Unicode-data в ядро →
  библиотека (`as_graphemes`).
- **Оставить плоские `char_at`/`char_len` + `len()=байты` на `str`.** Прагматично,
  но двусмысленность держится дисциплиной (источник бага `@pad_*`), и char-методы
  засоряют тонкий `str`. Отвергнуто в пользу линз (решение пользователя 2026-06-13).
- **Сделать `as_chars()` целочисленно-индексируемым (`chars[i]`).** Нет: это вернёт
  «O(n) под видом O(1)» на уровень ниже. `CharsView` — итерируемый + `at(i)`
  (имя сигналит скан), без `Index[int]`.

### Амендменты

- **[D26](../../spec/decisions/08-runtime.md#d26)** (str semantics): «нет int-
  индексации; нет бэар `len()`; работа через линзы `as_bytes`/`as_chars`; дефолт-
  итерация = `char`; поиск = байт-offset».
- **[D238](../../spec/decisions/03-syntax.md#d238)** (`Index[K,V]`): **RETRACT**
  `str | int | char`. Остаётся только `str | Range | str` (byte-range view).
- **[D58](../../spec/decisions/03-syntax.md#d58)** (`for x in c`): `for c in s`
  yields `char` через `s.as_chars().iter()`; `CharsView` реализует `Next[char]`.

---

## D250. `CharsView` (decoding lens) + конвенция `as_`/`to_`

### `CharsView`

Линза над буфером `str`, дающая codepoint-представление. Тот же layout, что у `str`
(алиасит тот же буфер, zero-copy):

```nova
// Линза: НЕ владеет буфером, алиасит str. value, 16 байт, copy.
type CharsView value priv {
    ptr      *ro u8     // тот же буфер, что у str (read-only)
    byte_len int        // длина буфера в БАЙТАХ
}
```

Методы (вся codepoint-алгебра — здесь, не на `str`):

| Метод | Смысл | Сложность |
|---|---|---|
| `@len() -> int` | число codepoint'ов | O(n) |
| `@at(i int) -> Option[char]` | i-й codepoint (имя сигналит скан) | O(n) |
| `mut @next() -> Option[char]` | курсор (реализует `Next[char]`) | амортиз. O(1) |
| `@iter() -> CharsView => self` | реализует `Iter[CharsView]` | O(1) |
| `@indices() -> CharIndicesView` | пары `(byte_offset, char)` для slice | O(n) |
| `@is_empty() -> bool` | `byte_len == 0` | O(1) |

- **`CharsView` НЕ реализует `Index[int, char]`** — целочисленной индексации нет
  (D249 §«Что отвергнуто»); только `at(i)` и итерация.
- Decode-курсор — тот же UTF-8-алгоритм, что в нынешних `@char_at`/`@to_chars`
  ([string.nv:219](../../std/runtime/string.nv#L219),[545](../../std/runtime/string.nv#L545)),
  просто переезжает в `CharsView`. `char.try_from` валидирует codepoint
  (невалид → U+FFFD).
- `@indices()` даёт `(byte_offset, char)`, чтобы после обхода резать `str[a..b]`
  по байтовому offset'у — мост между символьным проходом и байтовым slice.

### Конвенция `as_` / `to_`

Кодифицируется проектно-широко (не только для `str`):

- **`as_<repr>() -> <view>`** — линза/реинтерпретация существующего буфера:
  zero-copy, дёшево, **алиасит** источник (источник обязан пережить view). Источник
  иммутабелен на время жизни view. Примеры: `str.as_bytes() -> ro []u8`,
  `str.as_chars() -> CharsView`, (future) `str.as_graphemes()`.
- **`to_<repr>() -> <owned>`** — материализация: новый **владеемый** объект, alloc,
  без алиасинга. Примеры: `str.to_bytes() -> []u8`, `str.to_chars() -> []char`.

### Lifetime / GC

`CharsView` алиасит GC-буфер `str` (как уже делает `as_bytes()`). Безопасно: `str`
иммутабелен (R8), буфер GC-managed, view держит `ptr` → conservative GC (Boehm)
его видит; пока view жив, буфер не собран. Новых рисков сверх `str`/`as_bytes` нет.

---

## Фазы

- **Ф.0 — Spec.** D249 + D250 в `03-syntax.md`; амендмент-блоки к D26/D238/D58.
- **Ф.1 — Снять `Index[int, char]` со `str`.** Compiler str-index dispatch + checker:
  `str[i]` целым → `E_STR_NO_INT_INDEX` (fix-it: `as_chars().at(i)` / `as_bytes()[i]`
  / `s[a..b]`). `str[a..b]` (`Index[Range, str]`) сохранить.
- **Ф.2 — `@find`/`@rfind` → байт-offset.** Убрать cp-счётчик
  ([string.nv:144](../../std/runtime/string.nv#L144),[174](../../std/runtime/string.nv#L174)).
- **Ф.3 — `CharsView` + `str.@as_chars()`.** Новый тип `CharsView value priv {ptr,
  byte_len}` с методами `len`/`at`/`next`/`iter`/`indices`/`is_empty`; перенести
  decode-курсор из `@char_at`/`@to_chars`. `CharIndicesView` для `@indices()`.
  `str @as_chars() -> CharsView`.
- **Ф.4 — Тонкий `str`.** Убрать со `str`: `@len()`, `@char_at`, `@char_len`,
  `@get`(Index[int]), `@byte_at` (→ `as_bytes()[i]`). Оставить/добавить:
  `@as_bytes()`, `@as_chars()`, `@byte_len()` (O(1) шорткат, примитив над полем
  `len`), `@to_bytes()`, `@to_chars()`. `for c in s` → `s.as_chars().iter()`.
  `E_STR_NO_LEN` на `s.len()`. `@pad_*` → `as_chars().len()`
  ([string.nv:728](../../std/runtime/string.nv#L728),[736](../../std/runtime/string.nv#L736)).
- **Ф.5 — Фикстуры + миграция.** POS: `for c in s`, `s[a..b]`, `find`+slice,
  `as_chars().at/len/indices`, `as_bytes()[i]/len`. NEG: `s[i]`→`E_STR_NO_INT_INDEX`,
  `s.len()`→`E_STR_NO_LEN`, `chars[i]`→нет `Index[int]`. Мигрировать call-сайты:
  `s.char_at(i)`→`s.as_chars().at(i)`, `s.char_len()`→`s.as_chars().len()`,
  `s.len()`→`s.byte_len()`/`as_chars().len()` (по смыслу), `s[i]`→линза. Тесты через
  релизные `nova` + компилятор.
- **Ф.6 — Закрытие.** Финализировать D249/D250; README plans, project-creation.txt,
  simplifications.md, nova-private/discussion-log.md. Закоммитить пофазно.

---

## Критерии приёмки

- **A1.** `str[i]` где `i: int` → `E_STR_NO_INT_INDEX` с fix-it (линзы/slice).
- **A2.** `str[a..b]` — zero-copy byte-slice; panic при OOB и рассечении codepoint.
- **A3.** `@as_bytes() -> ro []u8` даёт O(1) `[i]`/`len()`/итерацию байтов; `@byte_at`
  ретайрнут.
- **A4.** `@as_chars() -> CharsView`; `CharsView` имеет `len()`(O(n))/`at(i)`/
  `next()`/`iter()`/`indices()`/`is_empty()`; **не** реализует `Index[int, char]`
  (`chars[i]` → ошибка).
- **A5.** `for c in s { ... }` итерирует `char` (= `s.as_chars()` обход), значения/
  порядок идентичны нынешнему `@to_chars()`.
- **A6.** `@find`/`@rfind` → **байт**-offset; `s.find(x)`→`s[k..]` композируется
  zero-copy без повторного скана.
- **A7.** У `str` нет `@len()` (`s.len()`→`E_STR_NO_LEN`) и нет плоских
  `@char_at`/`@char_len`/`@char_indices`. `@byte_len()` (O(1)) остаётся как шорткат.
- **A8.** `@pad_left/@pad_right(width,_)` считают width в **codepoint'ах** (тест не-
  ASCII: `"é".pad_left(3,'·') == "··é"`).
- **A9.** `str` реализует `Index[Range, str]` и не реализует `Index[int, _]`;
  `CharsView` реализует `Iter[CharsView]`/`Next[char]`.
- **A10.** Конвенция `as_`/`to_` соблюдена: `as_*` — zero-copy view, `to_*` — owned.
- **A11.** Полный `nova test` без новых FAIL vs baseline; plan152-фикстуры зелёные.

---

## Оценка миграции

| Точка | Объём |
|---|---|
| Compiler: снять `Index[int,char]` для str + `E_STR_NO_INT_INDEX` | малый |
| `string.nv`: find/rfind → байт | малый |
| **Новый `CharsView` (+ `CharIndicesView`) + перенос decode-курсора** | **средний** (1–2 Nova-типа; алгоритм уже есть) |
| **Тонкий `str`: убрать `len/char_at/char_len/get/byte_at`, добавить `as_chars`** | **средний** (методы + `Next/Iter` wiring + `E_STR_NO_LEN`) |
| Миграция call-сайтов (`char_at`/`char_len`/`str[i]`/`s.len()`) — stdlib + fixtures | **средний** (sweep + на каждом решить единицу) |
| Фикстуры pos/neg | малый |

Эстимат **~2–3 dev-day** (больше, чем плоский вариант: появляется новый тип-линза и
переезжает целый слой методов). Риск умеренный, но локализован в `str`/`string.nv` +
точечный compiler-dispatch; инфра `Index`/`Iter`/`Next`/`value`-records готова
(Plan 138/139/D58). Главный драйвер времени — sweep call-сайтов, где механически не
заменить (нужно решить «байт или символ»).

---

## Связанные D-блоки

| D | Связь |
|---|---|
| D249 (NEW) | строковая координатная модель — линзы, без int-index, char-итерация |
| D250 (NEW) | `CharsView` (decoding lens) + конвенция `as_`/`to_` |
| D26 AMEND | str semantics — линзы, нет `len()`/int-index, байт-offset find |
| D238 AMEND | RETRACT `str \| int \| char`; остаётся `str \| Range \| str` |
| D58 AMEND | `for c in s` → `char` через `as_chars().iter()` |
| D239 | `[]u8` ≡ `Vec[u8]` — байтовая линза бесплатно из `Vec` |
| D240 | `MutIndex` — `str`/`CharsView` иммутабельны, не реализуют (R8) |
| D241/D242 | `Next[T]`/`Iter[I]` — `CharsView` реализует `Next[char]`/`Iter[CharsView]` |
