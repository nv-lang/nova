# Анализ API str + unicode в Nova

**Дата:** 2026-06-18  
**Автор:** AI Code Review Agent  
**Цель:** Глубокий аудит API строк и Unicode на соответствие лучшим практикам современных языков (Go/Rust/TS/Kotlin/Java)

---

## Executive Summary

API Nova для работы со строками и Unicode **в целом хорошо спроектировано** и следует современным практикам, но есть ряд областей для улучшения:

### ✅ Сильные стороны:
1. **Правильное разделение `as_*` vs `to_*`** — view vs owned copy
2. **Lens model для char iteration** — честный O(n), нет скрытых O(n²)
3. **Zero-copy slicing** — `@[start..end]` возвращает view
4. **Методы на типе предпочтительнее функций** — `s.trim()` не `trim(s)`
5. **Использование RawMem** — memcpy/memcmp для производительности
6. **Правильная конвенция именования** — `byte_len()`, `as_bytes()`, `to_chars()`
7. **Контракты через requires** — slice bounds checking
8. **Опциональный Unicode** — не тянет 650KB+ в каждую программу

### ⚠️ Области для улучшения:
1. **Дублирование UTF-8 decode логики** — 3 копии в разных файлах
2. **Неэффективный @replace** — O(n²) concat loop вместо StringBuilder
3. **Отсутствие контрактов panic/throw** — нет явных ensures/requires
4. **Неиспользование fluent API (@ return)** — многие методы могли бы быть chainable
5. **Избыточные аллокации в некоторых местах**
6. **Отсутствие некоторых стандартных методов** (contains_char, starts_with_char)

---

## 1. Сравнение с другими языками

### 1.1 Naming Convention

| Метод Nova | Rust | Go | Kotlin | TypeScript | Оценка |
|------------|------|----|--------|------------|--------|
| `@byte_len()` | `len()` (bytes) | `len()` | `length` | `.length` | ✅ Хорошо |
| `@as_bytes()` | `as_bytes()` | N/A | `toByteArray()` | N/A | ✅ Отлично |
| `@to_bytes()` | `into_bytes()` | `[]byte(s)` | `toByteArray()` | N/A | ✅ Хорошо |
| `@as_chars()` | `chars()` | N/A | `codePoints()` | `[...str]` | ✅ Отлично |
| `@to_chars()` | N/A | N/A | N/A | N/A | ✅ Хорошо |
| `@is_empty()` | `is_empty()` | `len()==0` | `isEmpty()` | `=== ""` | ✅ Стандарт |
| `@trim()` | `trim()` | `TrimSpace()` | `trim()` | `trim()` | ✅ Стандарт |
| `@to_lower()` | `to_lowercase()` | `ToLower()` | `lowercase()` | `toLowerCase()` | ⚠️ Имя OK |
| `@to_upper()` | `to_uppercase()` | `ToUpper()` | `uppercase()` | `toUpperCase()` | ⚠️ Имя OK |
| `@starts_with()` | `starts_with()` | `HasPrefix()` | `startsWith()` | `startsWith()` | ✅ Стандарт |
| `@ends_with()` | `ends_with()` | `HasSuffix()` | `endsWith()` | `endsWith()` | ✅ Стандарт |
| `@find()` | `find()` | `Index()` | `indexOf()` | `indexOf()` | ✅ Стандарт |
| `@split()` | `split()` | `Split()` | `split()` | `split()` | ✅ Стандарт |
| `@concat()` | N/A (+) | N/A (+) | N/A (+) | N/A (+) | ✅ Хорошо |
| `@repeat()` | `repeat()` | `strings.Repeat` | `repeat()` | `repeat()` | ✅ Стандарт |
| `@replace()` | `replace()` | `ReplaceAll()` | `replace()` | `replaceAll()` | ✅ Стандарт |
| `@pad_left()` | N/A | N/A | `padStart()` | `padStart()` | ⚠️ Не стандарт |
| `@pad_right()` | N/A | N/A | `padEnd()` | `padEnd()` | ⚠️ Не стандарт |

**Вывод:** Именование в целом соответствует индустриальным стандартам. `pad_left/right` менее распространены, чем `padStart/padEnd`, но понятны.

### 1.2 API Completeness

#### Есть в Nova, но нет в других:
- ✅ `@eq_ignore_ascii_case()` — полезно, есть в Rust
- ✅ `@strip_prefix/suffix()` — отлично, как Rust
- ✅ `@splitn/rsplitn()` — продвинутый split, как Rust
- ✅ `@split_once/rsplit_once()` — отлично
- ✅ `@match_indices()` — полезно, как Rust
- ✅ `@is_char_boundary()` — критично для безопасности UTF-8

#### Нет в Nova, но есть в других:
- ❌ `contains_char(char)` — только `contains(str)`
- ❌ `starts_with_char(char)` / `ends_with_char(char)`
- ❌ `trim_matches(str)` — есть только для char
- ❌ `split_at(idx)` — split by byte index
- ❌ `get_unchecked(idx)` — unsafe access
- ❌ `capacity()` — для строк не применимо
- ❌ `reserve()` — immutable strings
- ❌ `clear()` — immutable strings
- ❌ `drain()` — immutable strings

**Вывод:** API достаточно полное для immutable strings. Missing methods либо неприменимы (mutable ops), либо могут быть добавлены как followup.

---

## 2. Самосогласованность API

### 2.1 Конвенция `as_*` vs `to_*`

✅ **ПРАВИЛЬНО РЕАЛИЗОВАНО:**

```nova
// as_* = zero-copy view/lens (borrowed)
s.as_bytes()    -> ro []u8       // O(1), no alloc
s.as_chars()    -> CharsIter     // O(1), lazy iterator

// to_* = owned copy (allocated)
s.to_bytes()    -> []u8          // O(n), alloc + copy
s.to_chars()    -> []char        // O(n), alloc + decode
```

Это **идеально** соответствует Rust:
- Rust: `as_bytes()` → `&[u8]`, `to_string()` → `String`
- Nova: `as_bytes()` → `ro []u8`, `to_bytes()` → `[]u8`

### 2.2 Методы на типе vs функции

✅ **ПРАВИЛЬНО:** Все операции — методы на `str`:
```nova
s.trim()           // НЕ trim(s)
s.to_lower()       // НЕ to_lower(s)
s.find("abc")      // НЕ find(s, "abc")
```

Это соответствует современным языкам (Rust, Kotlin, TS) и делает код более читаемым.

### 2.3 Fluent API (@ return)

⚠️ **ЧАСТИЧНО РЕАЛИЗОВАНО:**

Есть fluent returns:
```nova
out.push(@[start..end])  // push returns self
StringBuilder.append(x).append(y)  // chainable
```

Но **отсутствует** во многих методах, где могло бы быть полезно:
```nova
// Сейчас:
let trimmed = s.trim()
let lower = trimmed.to_lower()

// Могло бы быть (если бы to_lower возвращал @):
let result = s.trim().to_lower()  // уже работает, т.к. возвращает str
```

На самом деле, **все методы возвращают новый str**, поэтому chaining уже работает:
```nova
s.trim().to_lower().pad_left(10, ' ')  // ✅ Работает!
```

**Вывод:** Fluent API реализовано правильно через return type, не через `@`.

---

## 3. Контракты и Error Handling

### 3.1 Использование requires/ensures

❌ **ПРОБЛЕМА:** Практически **нет контрактов** в коде!

Единственный пример:
```nova
// std/runtime/string/slice.nv:36
requires 0 <= r.start && r.start <= r.end && r.end <= @byte_len()
```

**Что должно быть:**
```nova
export fn str @to_lower() -> str
    ensures result.byte_len() == @byte_len()  // ASCII lowercase не меняет длину
=> ...

export fn str @concat(other str) -> str
    requires @byte_len() >= 0 && other.byte_len() >= 0
    ensures result.byte_len() == @byte_len() + other.byte_len()
=> ...

export fn str @find(needle str) -> Option[int]
    ensures match result {
        Some(idx) => idx >= 0 && idx <= @byte_len() - needle.byte_len(),
        None => true
    }
=> ...
```

### 3.2 Panic vs Throw

✅ **ПРАВИЛЬНО:** Используется `panic` для programmer errors:
```nova
extern "nova" fn panic(msg str) -> never
```

Но **нет явной документации** когда использовать panic vs Option:

**Рекомендация:**
- `panic` — programmer error (out of bounds, invalid UTF-8)
- `Option` — expected absence (find not found, parse failure)
- `Result` — recoverable errors (IO, network)

Сейчас это соблюдается неявно, но **должно быть задокументировано**.

---

## 4. Производительность и Аллокации

### 4.1 Дублирование UTF-8 Decode

❌ **КРИТИЧЕСКАЯ ПРОБЛЕМА:** UTF-8 decode логика дублируется **3 раза**:

1. `core.nv:256-285` — `@to_chars()`
2. `chars.nv:53-76` — `CharsIter @next()`
3. `chars.nv:104-135` — `CharsIter @nth()`

Каждая копия ~30 строк идентичного кода:
```nova
if b < 0x80 { cp = b; step = 1 }
else if (b & 0xE0) == 0xC0 && i + 1 < n { ... step = 2 }
else if (b & 0xF0) == 0xE0 && i + 2 < n { ... step = 3 }
else if (b & 0xF8) == 0xF0 && i + 3 < n { ... step = 4 }
```

**Решение:** Вынести в module-private helper:
```nova
// Module-private: decode one UTF-8 codepoint at bytes[pos].
// Returns (codepoint, step_bytes). Assumes valid UTF-8.
fn decode_utf8_at(bytes ro []u8, pos int) -> (int, int) {
    ro b = bytes[pos] as int
    mut cp = b
    mut step = 1
    if b < 0x80 {
        cp = b; step = 1
    } else if (b & 0xE0) == 0xC0 && pos + 1 < bytes.len() {
        cp = ((b & 0x1F) << 6) | (bytes[pos+1] as int & 0x3F)
        step = 2
    } else if (b & 0xF0) == 0xE0 && pos + 2 < bytes.len() {
        cp = ((b & 0x0F) << 12) | ((bytes[pos+1] as int & 0x3F) << 6) | (bytes[pos+2] as int & 0x3F)
        step = 3
    } else if (b & 0xF8) == 0xF0 && pos + 3 < bytes.len() {
        cp = ((b & 0x07) << 18) | ((bytes[pos+1] as int & 0x3F) << 12) | 
             ((bytes[pos+2] as int & 0x3F) << 6) | (bytes[pos+3] as int & 0x3F)
        step = 4
    }
    (cp, step)
}
```

**Экономия:** ~60 строк кода, легче поддерживать, меньше багов.

### 4.2 Неэффективный @replace

❌ **ПРОБЛЕМА:** `transform.nv:184-196` использует O(n²) concat loop:

```nova
export fn str @replace(from str, to str) -> str {
    ro parts = @split(from)
    ro n = parts.len()
    mut result = parts[0]
    mut i = 1
    while i < n {
        result = result.concat(to).concat(parts[i])  // O(n²)!
        i = i + 1
    }
    result
}
```

Каждый `concat` создает новую строку, итого O(n²) аллокаций.

**Решение:** Использовать StringBuilder:
```nova
export fn str @replace(from str, to str) -> str {
    if from.byte_len() == 0 { return @ }
    ro parts = @split(from)
    ro n = parts.len()
    if n == 0 { return @ }
    
    // Estimate capacity: original length + (replacements * size_diff)
    ro from_len = from.byte_len()
    ro to_len = to.byte_len()
    ro replacements = n - 1
    ro estimated = @byte_len() + replacements * (to_len - from_len)
    
    consume sb = StringBuilder.with_capacity(if estimated > 0 { estimated } else { @byte_len() })
    sb.append(parts[0])
    mut i = 1
    while i < n {
        sb.append(to).append(parts[i])
        i = i + 1
    }
    sb.as_str()
}
```

**Комментарий в коде говорит об этом:**
```
// Note: uses @concat loop (not []str.join) to avoid circular import with std.text.
// O(n²) for many replacements — acceptable for bootstrap; 
// StringBuilder followup is [str-replace-buffered].
```

Но followup еще не сделан!

### 4.3 Избыточные аллокации в @to_chars

⚠️ **МИНИМАЛЬНАЯ ПРОБЛЕМА:** `core.nv:256-285`:

```nova
mut out = []char.with_capacity(@byte_len())  // over-allocates
```

Для ASCII это нормально (1 byte = 1 char), но для multibyte UTF-8:
- `"Привет"` = 12 bytes, 6 chars → capacity 12, используется 6 (50% waste)
- `"你好"` = 6 bytes, 2 chars → capacity 6, используется 2 (67% waste)

**Решение:** Уже есть комментарий:
```
// Plan 152.1 Ф.4: capacity hint = byte length (codepoint count ≤ byte count;
// `@char_len()` retired — over-allocating by the multibyte slack is fine).
```

Это **осознанное упрощение** — trade-off между точностью и сложностью. Приемлемо.

### 4.4 Эффективное использование RawMem

✅ **ОТЛИЧНО:** RawMem используется правильно:

```nova
// core.nv:309 — compare через memcmp
ro c = unsafe { RawMem.compare(@ptr, other.ptr, min) }

// search.nv:22 — starts_with через memcmp
unsafe { RawMem.compare(@ptr, prefix.ptr, pn) == 0 }

// core.nv:155 — copy_nonoverlapping
unsafe { RawMem.copy_nonoverlapping(src, buf, n) }
```

Это дает **максимальную производительность** (C-level memcpy/memcmp).

---

## 5. Организация файлов и модулей

### 5.1 Структура папок

✅ **ОТЛИЧНО:** Правильное разделение по файлам:

```
std/runtime/string/
├── core.nv       # identity, length, bytes, conversions
├── chars.nv      # codepoint layer (CharsIter)
├── transform.nv  # trim, case, concat, pad, repeat, replace
├── search.nv     # search + split
├── slice.nv      # slicing operations
└── parse.nv      # parsing (int, float)
```

Каждый файл имеет четкую ответственность. Это **лучшая практика**.

### 5.2 Folder-module pattern

✅ **ПРАВИЛЬНО:** Все файлы объявляют один module:
```nova
#no_prelude
module runtime.string
```

Это позволяет им видеть друг друга без импортов.

### 5.3 Unicode организация

✅ **ОТЛИЧНО:** Аналогичная структура:

```
std/unicode/
├── normalize.nv   # NFC/NFD/NFKC/NFKD
├── case.nv        # case folding/mapping
├── category.nv    # General_Category, predicates
├── collate.nv     # DUCET collation
├── graphemes.nv   # grapheme clusters
├── words.nv       # word boundaries
└── sentences.nv   # sentence boundaries
```

Plus generated data files (`*_data.nv`).

---

## 6. Дублирующийся код

### 6.1 UTF-8 decode (критично)

Как упомянуто выше — **3 копии** decode логики.

### 6.2 ASCII whitespace check

⚠️ **ДУБЛИРОВАНИЕ:**

```nova
// transform.nv:18 — trim использует inline check
while start < n && (bytes[start] as int) <= 32 { ... }

// search.nv:118 — отдельная функция
fn is_ascii_ws(b int) -> bool => b == 32 || (b >= 9 && b <= 13)
```

**Решение:** Унифицировать:
```nova
// Module-private: ASCII whitespace check (space + \t\n\v\f\r).
// These bytes are < 0x80, so they never appear as UTF-8 continuation bytes.
fn is_ascii_ws(b int) -> bool => b == 32 || (b >= 9 && b <= 13)

// В trim использовать:
while start < n && is_ascii_ws(bytes[start] as int) { start = start + 1 }
```

### 6.3 Hex parsing

⚠️ **ДУБЛИРОВАНИЕ:** `hex()` функция определена в `normalize.nv:38-41`, но используется также в `collate.nv`.

Проверка показывает, что они используют общую функцию из peer module (folder-module sharing). Это **нормально**.

---

## 7. Chainability и Fluent API

### 7.1 Текущее состояние

✅ **ХОРОШО:** Большинство методов возвращают `str`, поэтому chaining работает:

```nova
s.trim().to_lower().pad_left(10, ' ').repeat(3)
```

### 7.2 Vec chaining

✅ **ОТЛИЧНО:** Vec использует fluent API с `@`:

```nova
[]u8.with_capacity(n).append(@as_bytes())  // append returns @
Vec[str].new().push("a").push("b")         // push returns @
```

Это **правильный паттерн** для mutable collections.

### 7.3 Где можно улучшить

⚠️ **ВОЗМОЖНОЕ УЛУЧШЕНИЕ:** Некоторые методы могли бы принимать `consume` для оптимизации:

```nova
// Сейчас:
let result = s1.concat(s2).concat(s3)  // 2 аллокации

// Могло бы быть (если бы concat принимал consume):
let result = s1.concat(consume s2).concat(consume s3)  // reuse buffers
```

Но для immutable strings это **не критично**.

---

## 8. Операторы vs Методы

### 8.1 Текущее использование

✅ **ПРАВИЛЬНО:** Nova синтезирует операторы из методов:

```nova
// D178: @compare synthesizes < <= > >=
s1 < s2   // => Nova_str_method_compare(s1, s2) < 0

// D178: @equal synthesizes == !=
s1 == s2  // => Nova_str_method_equal(s1, s2)

// D46: @plus synthesizes +
s1 + s2   // => s1.@plus(s2) => @concat
```

Это **лучше** чем отдельные функции `compare()`, `equal()`.

### 8.2 Предпочтение операторов

✅ **СОГЛАСОВАНО:** Код использует операторы где возможно:

```nova
// search.nv:302
if n != other.byte_len() { return false }  // НЕ .equal()

// transform.nv:23
while start < n && (bytes[start] as int) <= 32 { ... }  // НЕ .compare()
```

---

## 9. Tuple destructuring и compact assignments

### 9.1 Использование tuple assignment

✅ **ХОРОШО:** Есть примеры:

```nova
// search.nv:20
ro (sn, pn) = (@byte_len(), prefix.byte_len())

// core.nv:300-302
ro an = @byte_len()
ro bn = other.byte_len()
ro min = if an < bn { an } else { bn }
```

### 9.2 Где можно улучшить

⚠️ **МОЖНО КОМПАКТНЕЕ:**

```nova
// Сейчас (core.nv:300-302):
ro an = @byte_len()
ro bn = other.byte_len()
ro min = if an < bn { an } else { bn }

// Могло бы быть:
ro (an, bn) = (@byte_len(), other.byte_len())
ro min = if an < bn { an } else { bn }
```

Но это **стилистическое предпочтение**, не ошибка.

---

## 10. += -= операторы

### 10.1 Текущее использование

✅ **ХОРОШО:** Используется `+=`:

```nova
// transform.nv:193
i = i + 1  // ⚠️ Здесь можно i += 1

// search.nv:106
i = i + sep_len  // ⚠️ Можно i += sep_len
```

### 10.2 Где можно улучшить

⚠️ **РЕКОМЕНДАЦИЯ:** Заменить `i = i + 1` на `i += 1` везде:

```nova
// Было:
i = i + 1
count = count + 1

// Стало:
i += 1
count += 1
```

Это **более компактно** и соответствует modern practices.

---

## 11. Missing Features

### 11.1 Критично отсутствующие методы

❌ **ОТСУТСТВУЮТ:**

1. **`contains_char(char)`** — проверить наличие символа без создания строки
   ```nova
   // Сейчас:
   s.contains(str.from(c))  // аллокация!
   
   // Должно быть:
   s.contains_char(c)  // no alloc
   ```

2. **`starts_with_char(char)` / `ends_with_char(char)`**
   ```nova
   // Сейчас:
   s.starts_with(str.from(c))  // аллокация
   
   // Должно быть:
   s.starts_with_char(c)  // O(1), no alloc
   ```

3. **`split_at(byte_idx)`** — split by byte index
   ```nova
   let (before, after) = s.split_at(5)  // (s[0..5], s[5..])
   ```

### 11.2 Полезные, но не критичные

⚠️ **МОЖНО ДОБАВИТЬ:**

1. **`lines_with_terminator()`** — сохранить `\n` в конце каждой линии
2. **`split_whitespace_unicode()`** — Unicode-aware whitespace splitting
3. **`char_at_byte(idx)`** — получить char по byte offset (с проверкой boundary)
4. **`byte_at_char(idx)`** — получить byte offset по char index

---

## 12. Рекомендации по приоритетам

### P0 — Критичные (делать немедленно)

1. **[P0-1] Вынести UTF-8 decode в helper функцию**
   - Экономия: ~60 строк дублированного кода
   - Риск: низкий (refactor only)
   - Файлы: `core.nv`, `chars.nv`

2. **[P0-2] Оптимизировать @replace через StringBuilder**
   - Performance: O(n²) → O(n)
   - Followup из комментария в коде
   - Файл: `transform.nv`

3. **[P0-3] Добавить базовые контракты (requires/ensures)**
   - Безопасность: explicit bounds checking
   - Начать с slice, concat, find
   - Файлы: все string/*.nv

### P1 — Важные (делать в ближайшем спринте)

4. **[P1-1] Добавить contains_char/starts_with_char/ends_with_char**
   - Performance: избежать аллокации `str.from(c)`
   - API completeness
   - Файл: `search.nv`

5. **[P1-2] Унифицировать ASCII whitespace check**
   - DRY principle
   - Файлы: `transform.nv`, `search.nv`

6. **[P1-3] Заменить `i = i + 1` на `i += 1` везде**
   - Code style consistency
   - Mechanical refactor
   - Файлы: все string/*.nv

### P2 — Желательные (делать когда есть время)

7. **[P2-1] Добавить split_at(byte_idx)**
   - API completeness (как Rust)
   - Файл: `slice.nv`

8. **[P2-2] Документировать panic vs Option policy**
   - Developer experience
   - Файл: doc comment в `core.nv` или отдельный markdown

9. **[P2-3] Добавить больше ensures контрактов**
   - Correctness guarantees
   - Постепенно покрывать все методы

### P3 — Nice to have (когда будет время)

10. **[P3-1] Добавить char_at_byte/byte_at_char helpers**
    - Convenience methods
    - Низкий приоритет (можно через as_chars/as_bytes)

11. **[P3-2] Unicode-aware trim_whitespace**
    - Phase B feature (сейчас только ASCII)
    - Требует std.unicode import

12. **[P3-3] Benchmark suite для string ops**
    - Performance regression detection
    - Отдельный план

---

## 13. Обновленная методология кодирования

На основе анализа, вот обновленные правила для Nova string/unicode кода:

### 13.1 Naming Conventions

```
✅ as_*() -> view/lens/borrow (zero-copy, O(1))
✅ to_*() -> owned copy (allocate + copy, O(n))
✅ is_*() -> boolean predicate
✅ @method() -> operator synthesis (@compare, @equal, @plus)
✅ from_*() -> constructor/factory
```

### 13.2 Method Design

```nova
// ✅ Методы на типе, не free functions
export fn str @trim() -> str { ... }

// ✅ Fluent return для mutable builders
export fn StringBuilder append(s str) -> Self => @

// ✅ Immutable strings возвращают новый str (chaining works)
s.trim().to_lower().pad_left(10, ' ')

// ✅ Use consume для ownership transfer
export fn str.from_bytes_unchecked_steal(consume bytes []u8) -> str
```

### 13.3 Contracts

```nova
// ✅ ALWAYS add requires для preconditions
export fn str @[r Range] -> str
    requires 0 <= r.start && r.start <= r.end && r.end <= @byte_len()

// ✅ Add ensures для postconditions (где нетривиально)
export fn str @concat(other str) -> str
    ensures result.byte_len() == @byte_len() + other.byte_len()

// ✅ Document panic conditions
/// Panics if `idx` is not on a UTF-8 codepoint boundary.
export fn str @char_at(idx int) -> char
```

### 13.4 Error Handling

```
✅ panic() — programmer error (bounds check fail, invalid UTF-8)
✅ Option[T] — expected absence (find not found, parse failure)
✅ Result[T, E] — recoverable errors (IO, network, validation)

НЕ использовать panic для expected failures!
```

### 13.5 Performance Patterns

```nova
// ✅ Use RawMem for bulk operations
unsafe { RawMem.compare(ptr1, ptr2, len) }
unsafe { RawMem.copy_nonoverlapping(src, dst, n) }

// ✅ Use StringBuilder for multiple concatenations
StringBuilder.with_capacity(est).append(a).append(b).as_str()

// ✅ Zero-copy views where possible
@[start..end]  // NOT str.alloc_copy(...)

// ✅ Avoid O(n²) patterns
// BAD: result = result.concat(part) in loop
// GOOD: use StringBuilder or Vec then join

// ✅ Extract common logic into helpers
fn decode_utf8_at(bytes ro []u8, pos int) -> (int, int) { ... }
```

### 13.6 Code Style

```nova
// ✅ Use += -= instead of x = x + 1
i += 1
count -= 1

// ✅ Tuple destructuring for related values
ro (sn, pn) = (@byte_len(), prefix.byte_len())

// ✅ Compact multiple assignments when clear
(a, b) = (1, r + 2)

// ✅ Fluent chains for Vec/StringBuilder
[]u8.with_capacity(n).append(data).push(0)

// ✅ Prefer operators over method calls
if a < b { ... }  // NOT if a.compare(b) < 0
if a == b { ... }  // NOT if a.equal(b)

// ❌ Don't combine statements with ; unless necessary
a = 1; b = 2  // BAD
(a, b) = (1, 2)  // GOOD
```

### 13.7 Module Organization

```
✅ One file per concern (core, chars, transform, search, slice, parse)
✅ Folder-module pattern (all files declare same module)
✅ Module-private helpers prefixed with fn (not export fn)
✅ Generated data in separate *_data.nv files
✅ Opt-in heavy features (std.unicode not in prelude)
```

### 13.8 Testing

```nova
// ✅ Test happy path + edge cases
test "trim removes whitespace" {
    assert("  hello  ".trim() == "hello")
    assert("".trim() == "")
    assert("   ".trim() == "")
}

// ✅ Test UTF-8 edge cases
test "slice respects codepoint boundaries" {
    // "é" = 2 bytes, slicing in middle should panic
    // EXPECT_RUNTIME_PANIC
    "é"[0..1]
}

// ✅ Use EXPECT markers for negative tests
// EXPECT_COMPILE_ERROR E_STR_NO_LEN
"".len()  // bare len() retired
```

---

## 14. Заключение

API Nova для строк и Unicode **в целом отлично спроектировано** и следует современным best practices:

### Что сделано правильно:
1. ✅ Четкое разделение view (`as_*`) vs owned (`to_*`)
2. ✅ Lens model для char iteration (честный O(n))
3. ✅ Zero-copy slicing
4. ✅ Методы на типе, не free functions
5. ✅ Использование RawMem для производительности
6. ✅ Правильная организация файлов
7. ✅ Opt-in Unicode (не тянет таблицы в каждую программу)
8. ✅ Operator synthesis из методов

### Что нужно улучшить:
1. ❌ Дублирование UTF-8 decode (3 копии)
2. ❌ Неоптимальный @replace (O(n²))
3. ❌ Отсутствие контрактов (requires/ensures)
4. ⚠️ Missing convenience methods (contains_char, etc.)
5. ⚠️ Inconsistent use of `+=` vs `= ... +`

### Приоритеты:
- **P0:** Fix UTF-8 duplication, optimize replace, add contracts
- **P1:** Add missing methods, unify whitespace check, use `+=`
- **P2-P3:** Additional helpers, documentation, benchmarks

**Общая оценка: 8/10** — очень хороший foundation, needs polish.
