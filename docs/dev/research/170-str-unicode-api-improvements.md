# План улучшения API str + unicode

**Основан на:** [13-str-unicode-api-audit.md](13-str-unicode-api-audit.md)  
**Дата создания:** 2026-06-18  
**Статус:** Proposed

---

## Overview

Этот план систематизирует улучшения API строк и Unicode в Nova, выявленные в ходе глубокого аудита. Улучшения разделены по приоритетам от критичных (P0) до nice-to-have (P3).

---

## P0 — Критичные улучшения

### [P0-1] Рефактор UTF-8 decode логики

**Проблема:** UTF-8 decode дублируется в 3 местах (~90 строк идентичного кода):
- `std/runtime/string/core.nv:256-285` — `@to_chars()`
- `std/runtime/string/chars.nv:53-76` — `CharsIter @next()`
- `std/runtime/string/chars.nv:104-135` — `CharsIter @nth()`

**Решение:** Создать module-private helper функцию:

```nova
// std/runtime/string/chars.nv (добавить после is_cont)

// Module-private: decode one UTF-8 codepoint at bytes[pos].
// Returns (codepoint, step_bytes). Assumes valid UTF-8 (R-UTF8).
// For malformed sequences, returns the lead byte with step=1 (degrade gracefully).
fn decode_utf8_at(bytes ro []u8, pos int) -> (int, int) {
    ro b = bytes[pos] as int
    mut cp = b
    mut step = 1
    
    if b < 0x80 {
        // ASCII: 0xxxxxxx
        cp = b
        step = 1
    } else if (b & 0xE0) == 0xC0 && pos + 1 < bytes.len() {
        // 2-byte: 110xxxxx 10xxxxxx
        cp = ((b & 0x1F) << 6) | (bytes[pos+1] as int & 0x3F)
        step = 2
    } else if (b & 0xF0) == 0xE0 && pos + 2 < bytes.len() {
        // 3-byte: 1110xxxx 10xxxxxx 10xxxxxx
        cp = ((b & 0x0F) << 12) | 
             ((bytes[pos+1] as int & 0x3F) << 6) | 
             (bytes[pos+2] as int & 0x3F)
        step = 3
    } else if (b & 0xF8) == 0xF0 && pos + 3 < bytes.len() {
        // 4-byte: 11110xxx 10xxxxxx 10xxxxxx 10xxxxxx
        cp = ((b & 0x07) << 18) | 
             ((bytes[pos+1] as int & 0x3F) << 12) | 
             ((bytes[pos+2] as int & 0x3F) << 6) | 
             (bytes[pos+3] as int & 0x3F)
        step = 4
    }
    // Else: invalid lead byte or truncated sequence → return byte as-is, step=1
    
    (cp, step)
}
```

**Изменения:**

1. **chars.nv** — заменить decode в `@next()` и `@nth()`:
```nova
export fn CharsIter mut @next() -> Option[char] {
    ro bytes = @buf.as_bytes()
    ro n = @buf.byte_len()
    if @pos >= n { return None }
    
    ro (cp, step) = decode_utf8_at(bytes, @pos)
    @pos += step
    Some(cp_to_char(cp))
}

export fn CharsIter @nth(idx int) -> Option[char] {
    if idx < 0 { return None }
    ro bytes = @buf.as_bytes()
    ro n = @buf.byte_len()
    mut cp_idx = 0
    mut i = @pos
    
    while i < n {
        ro (cp, step) = decode_utf8_at(bytes, i)
        if cp_idx == idx {
            return Some(cp_to_char(cp))
        }
        cp_idx += 1
        i += step
    }
    None
}
```

2. **core.nv** — заменить decode в `@to_chars()`:
```nova
export fn str @to_chars() -> []char {
    ro bytes = @as_bytes()
    ro n = @byte_len()
    mut out = []char.with_capacity(n)
    mut i = 0
    
    while i < n {
        ro (cp, step) = decode_utf8_at(bytes, i)
        out.push(cp_to_char(cp))
        i += step
    }
    out
}
```

**Тесты:** Добавить тесты для проверки корректности после рефактора:
```nova
// nova_tests/planXXX/refactor_utf8_decode.nv
module nova_tests.planXXX

test "decode_utf8_at handles all cases" {
    ro s = "aé中🎉"  // 1 + 2 + 3 + 4 byte chars
    ro chars = s.to_chars()
    assert(chars.len() == 4)
    assert(chars[0] == 'a')
    assert(chars[1] == 'é')
    assert(chars[2] == '中')
    assert(chars[3] == '🎉')
}
```

**Приемка:**
- [ ] Все существующие тесты проходят
- [ ] Новый тест проходит
- [ ] Code coverage decode логики 100%
- [ ] Нет регрессии производительности (benchmark)

**Оценка усилий:** 2-3 часа

---

### [P0-2] Оптимизировать @replace через StringBuilder

**Проблема:** Текущая реализация O(n²) из-за concat loop:
```nova
result = result.concat(to).concat(parts[i])  // каждая итерация alloc + copy
```

**Решение:** Использовать StringBuilder для O(n):

```nova
// std/runtime/string/transform.nv

export fn str @replace(from str, to str) -> str {
    if from.byte_len() == 0 { return @ }
    
    ro parts = @split(from)
    ro n = parts.len()
    if n == 0 { return @ }
    
    // Estimate capacity: original length + (replacements * size_diff)
    ro from_len = from.byte_len()
    ro to_len = to.byte_len()
    ro replacements = n - 1
    ro estimated = @byte_len() + replacements * (if to_len > from_len { to_len - from_len } else { 0 })
    
    consume sb = StringBuilder.with_capacity(if estimated > @byte_len() { estimated } else { @byte_len() })
    sb.append(parts[0])
    
    mut i = 1
    while i < n {
        sb.append(to).append(parts[i])
        i += 1
    }
    
    sb.as_str()
}
```

**Аналогично для @replacen:**
```nova
export fn str @replacen(from str, to str, n int) -> str {
    ro fn_len = from.byte_len()
    if n <= 0 || fn_len == 0 { return @ }
    
    consume sb = StringBuilder.with_capacity(@byte_len())
    mut rest = @
    mut count = 0
    
    while count < n {
        match rest.find(from) {
            Some(k) => {
                sb.append(rest[0..k]).append(to)
                rest = rest[k + fn_len .. rest.byte_len()]
                count += 1
            },
            None => break,
        }
    }
    
    sb.append(rest).as_str()
}
```

**Тесты:** Существующие тесты должны покрыть, но добавить performance test:
```nova
// nova_tests/planXXX/replace_perf.nv
test "replace scales linearly" {
    // Create string with many occurrences
    ro s = "abc".repeat(1000)  // "abcabcabc..."
    ro result = s.replace("abc", "xyz")
    assert(result.byte_len() == s.byte_len())
    assert(!result.contains("abc"))
    assert(result.contains("xyz"))
}
```

**Приемка:**
- [ ] Все существующие тесты проходят
- [ ] Performance test показывает O(n) вместо O(n²)
- [ ] Benchmark: replace на 10K replacements < 1ms

**Оценка усилий:** 1-2 часа

---

### [P0-3] Добавить базовые контракты (requires/ensures)

**Проблема:** Практически нет явных контрактов в коде.

**Решение:** Добавить requires/ensures для ключевых методов:

#### 1. Slice operations (`slice.nv`)

```nova
// Уже есть requires, добавить ensures:
export fn str @[r Range] -> str
    requires 0 <= r.start && r.start <= r.end && r.end <= @byte_len()
    ensures result.byte_len() == r.end - r.start
    ensures @is_char_boundary(r.start)  // runtime check
    ensures @is_char_boundary(r.end)
=> ...
```

#### 2. Concat (`transform.nv`)

```nova
export fn str @concat(other str) -> str
    requires @byte_len() >= 0  // always true for valid str
    requires other.byte_len() >= 0
    ensures result.byte_len() == @byte_len() + other.byte_len()
    ensures result.starts_with(@)
    ensures result.ends_with(other)
=> ...
```

#### 3. Find/search (`search.nv`)

```nova
export fn str @find(needle str) -> Option[int]
    ensures match result {
        Some(idx) => 
            idx >= 0 && 
            idx <= @byte_len() - needle.byte_len() &&
            @[idx .. idx + needle.byte_len()] == needle,
        None => !@contains(needle)
    }
=> ...
```

#### 4. Trim (`transform.nv`)

```nova
export fn str @trim() -> str
    ensures result.byte_len() <= @byte_len()
    ensures !result.starts_with(" ") && !result.ends_with(" ")  // simplified
=> ...
```

#### 5. Case conversion (`transform.nv`)

```nova
export fn str @to_lower() -> str
    ensures result.byte_len() == @byte_len()  // ASCII lowercase preserves length
=> ...

export fn str @to_upper() -> str
    ensures result.byte_len() == @byte_len()  // ASCII uppercase preserves length
=> ...
```

**Приемка:**
- [ ] Компилятор принимает все контракты
- [ ] Тесты проходят в debug mode (runtime checks active)
- [ ] Release mode компилируется без overhead
- [ ] Документировать semantics каждого контракта

**Оценка усилий:** 4-6 часов

---

## P1 — Важные улучшения

### [P1-1] Добавить contains_char / starts_with_char / ends_with_char

**Проблема:** Для проверки наличия char приходится создавать строку:
```nova
s.contains(str.from(c))  // unnecessary allocation!
```

**Решение:** Добавить char-specific методы:

```nova
// std/runtime/string/search.nv

/// True если строка содержит codepoint `c`. O(n) scan, no allocation.
/// Более эффективно чем `contains(str.from(c))` для single char.
export fn str @contains_char(c char) -> bool {
    ro bytes = @as_bytes()
    ro n = @byte_len()
    ro cp = c as int
    
    mut i = 0
    while i < n {
        ro (decoded, step) = decode_utf8_at(bytes, i)
        if decoded == cp { return true }
        i += step
    }
    false
}

/// True если строка начинается с codepoint'а `c`. O(1) для ASCII, O(1) avg для UTF-8.
export fn str @starts_with_char(c char) -> bool {
    if @is_empty() { return false }
    ro bytes = @as_bytes()
    ro (first_cp, _) = decode_utf8_at(bytes, 0)
    first_cp == (c as int)
}

/// True если строка заканчивается codepoint'ом `c`. O(n) worst case.
export fn str @ends_with_char(c char) -> bool {
    if @is_empty() { return false }
    ro bytes = @as_bytes()
    ro n = @byte_len()
    
    // Scan backwards to find last codepoint boundary
    mut i = n - 1
    while i > 0 && (bytes[i] as int & 0xC0) == 0x80 {
        i -= 1
    }
    
    ro (last_cp, _) = decode_utf8_at(bytes, i)
    last_cp == (c as int)
}
```

**Тесты:**
```nova
// nova_tests/planXXX/char_predicates.nv
module nova_tests.planXXX

test "contains_char works" {
    assert("hello".contains_char('e'))
    assert(!"hello".contains_char('z'))
    assert("Привет".contains_char('и'))
    assert("🎉party🎉".contains_char('🎉'))
}

test "starts_with_char works" {
    assert("hello".starts_with_char('h'))
    assert(!"hello".starts_with_char('e'))
    assert("Привет".starts_with_char('П'))
}

test "ends_with_char works" {
    assert("hello".ends_with_char('o'))
    assert(!"hello".ends_with_char('h'))
    assert("Привет".ends_with_char('т'))
}
```

**Приемка:**
- [ ] Методы работают для ASCII и multibyte UTF-8
- [ ] Performance лучше чем `str.from(c)` подход
- [ ] Тесты покрывают edge cases (empty string, emoji, etc.)

**Оценка усилий:** 2-3 часа

---

### [P1-2] Унифицировать ASCII whitespace check

**Проблема:** Два разных способа проверки ASCII whitespace:
```nova
// transform.nv: inline check
(bytes[start] as int) <= 32

// search.nv: function
fn is_ascii_ws(b int) -> bool => b == 32 || (b >= 9 && b <= 13)
```

**Решение:** Использовать единую функцию везде:

```nova
// std/runtime/string/search.nv (уже есть, перенести в общий модуль)

// Module-private: ASCII whitespace check (space + \t\n\v\f\r).
// These bytes are < 0x80, so they never appear as UTF-8 continuation bytes.
// Matches Rust `char::is_ascii_whitespace`.
fn is_ascii_ws(b int) -> bool => b == 32 || (b >= 9 && b <= 13)
```

**Изменить transform.nv:**
```nova
// Было:
while start < n && (bytes[start] as int) <= 32 { start += 1 }

// Стало:
while start < n && is_ascii_ws(bytes[start] as int) { start += 1 }
```

**Приемка:**
- [ ] Все trim методы используют is_ascii_ws
- [ ] split_whitespace использует is_ascii_ws
- [ ] Поведение не изменилось (тесты проходят)

**Оценка усилий:** 30 минут

---

### [P1-3] Заменить `i = i + 1` на `i += 1` везде

**Проблема:** Непоследовательный стиль —有些地方使用 `i = i + 1`, другие `i += 1`.

**Решение:** Mechanical refactor заменить все `x = x + N` на `x += N`:

```bash
# Найти все случаи
grep -rn "= .* + 1" std/runtime/string/*.nv
grep -rn "= .* - 1" std/runtime/string/*.nv

# Заменить (вручную или через sed)
sed -i 's/\([a-z_]\+\) = \1 + 1/\1 += 1/g' std/runtime/string/*.nv
sed -i 's/\([a-z_]\+\) = \1 - 1/\1 -= 1/g' std/runtime/string/*.nv
```

**Примеры изменений:**
```nova
// Было:
i = i + 1
count = count + 1
start = start + sep_len
end = end - 1

// Стало:
i += 1
count += 1
start += sep_len
end -= 1
```

**Приемка:**
- [ ] Все инкременты/декременты используют `+=`/`-=`
- [ ] Код компилируется без ошибок
- [ ] Все тесты проходят

**Оценка усилий:** 1 час (mechanical)

---

## P2 — Желательные улучшения

### [P2-1] Добавить split_at(byte_idx)

**Фича:** Split string by byte index, возвращает tuple of views:

```nova
// std/runtime/string/slice.nv

/// Split the string at byte index `idx`, returning (before, after) views.
/// Both views are zero-copy. Panics if `idx` is not on a codepoint boundary.
/// Similar to Rust `split_at`.
export fn str @split_at(idx int) -> (str, str)
    requires 0 <= idx && idx <= @byte_len()
    requires @is_char_boundary(idx)
=> (@[0..idx], @[idx..@byte_len()])
```

**Тесты:**
```nova
test "split_at works" {
    ro (before, after) = "hello world".split_at(5)
    assert(before == "hello")
    assert(after == " world")
    
    // UTF-8 aware
    ro (b, a) = "Привет".split_at(6)  // after 'П' (2 bytes)
    assert(b == "П")
    assert(a == "ривет")
}
```

**Оценка усилий:** 30 минут

---

### [P2-2] Документировать panic vs Option policy

**Задача:** Создать документацию когда использовать panic vs Option vs Result:

```markdown
# Error Handling Policy for String Operations

## Panic (programmer error)
Использовать `panic` когда нарушение invariant указывает на bug в коде:
- Out-of-bounds access: `s[100]` when `s.byte_len() == 5`
- Invalid UTF-8 boundary: `s[1..]` splitting a codepoint
- Contract violation: `requires` failed

Example:
```nova
export fn str @[idx int] -> char
    requires 0 <= idx && idx < @byte_len()
    requires @is_char_boundary(idx)
// Panics if requirements not met → programmer error
```

## Option (expected absence)
Использовать `Option[T]` когда отсутствие значения — normal case:
- Search not found: `find()`, `rfind()`
- Parse failure: `parse_int()`, `parse_float()`
- Optional extraction: `strip_prefix()`, `split_once()`

Example:
```nova
export fn str @find(needle str) -> Option[int]
// Returns None if not found → expected, not an error
```

## Result (recoverable error)
Использовать `Result[T, E]` для recoverable errors:
- IO operations: file read/write
- Network operations: HTTP requests
- Validation failures: user input validation

Example:
```nova
export fn str.parse_int() -> Result[int, ParseError]
// Can fail due to invalid format → caller should handle
```

## Summary
- **panic**: "This should never happen if code is correct"
- **Option**: "Value might not be there, and that's OK"
- **Result**: "Operation can fail, and caller needs to decide what to do"
```

**Разместить в:** `docs/idioms/error-handling.md` или doc comment в `core.nv`

**Оценка усилий:** 1-2 часа

---

### [P2-3] Добавить больше ensures контрактов

Покрыть ensures контрактами больше методов, особенно:
- Transform methods (pad, repeat, replace)
- Search methods (match_indices, matches)
- Conversion methods (to_bytes, to_chars)

**Оценка усилий:** 3-4 часа (постепенно)

---

## P3 — Nice to have

### [P3-1] Добавить char_at_byte / byte_at_char helpers

```nova
/// Get the codepoint at byte offset `idx`. O(1) if idx is boundary, else panics.
export fn str @char_at_byte(idx int) -> char
    requires @is_char_boundary(idx)
=> ...

/// Get the byte offset of the i-th codepoint. O(n) scan.
export fn str @byte_offset_of_char(idx int) -> Option[int]
=> @as_chars().nth(idx).map(|_| /* need to track offset */)
```

**Оценка усилий:** 1-2 часа

---

### [P3-2] Unicode-aware trim_whitespace

Сейчас `trim()` только ASCII whitespace. Добавить Unicode-aware variant:

```nova
import std.unicode

export fn str @trim_unicode() -> str {
    // Use std.unicode.is_whitespace for full Unicode WS detection
    ...
}
```

**Оценка усилий:** 2-3 часа

---

### [P3-3] Benchmark suite для string ops

Создать benchmarks для detection performance regressions:

```nova
// bench/string_ops.nv
bench "concat_1000" {
    mut s = ""
    for i in 0..1000 {
        s = s.concat("x")
    }
}

bench "replace_many" {
    ro s = "abc".repeat(10000)
    s.replace("abc", "xyz")
}
```

**Оценка усилий:** 3-4 часа

---

## Implementation Roadmap

### Week 1: P0 items
- [P0-1] UTF-8 decode refactoring (2-3h)
- [P0-2] Optimize @replace (1-2h)
- [P0-3] Add basic contracts (4-6h)

**Total:** ~10 hours

### Week 2: P1 items
- [P1-1] Add char predicate methods (2-3h)
- [P1-2] Unify whitespace check (0.5h)
- [P1-3] Replace `= x + 1` with `+=` (1h)

**Total:** ~4 hours

### Week 3: P2 items
- [P2-1] Add split_at (0.5h)
- [P2-2] Document error handling policy (1-2h)
- [P2-3] Add more ensures contracts (3-4h)

**Total:** ~5 hours

### Week 4: P3 items (если есть время)
- [P3-1] char_at_byte helpers (1-2h)
- [P3-2] Unicode-aware trim (2-3h)
- [P3-3] Benchmark suite (3-4h)

**Total:** ~8 hours

---

## Success Metrics

1. **Code quality:**
   - [ ] Zero duplicate UTF-8 decode logic
   - [ ] All string methods have appropriate contracts
   - [ ] Consistent coding style (`+=` everywhere)

2. **Performance:**
   - [ ] @replace O(n) instead of O(n²)
   - [ ] contains_char faster than contains(str.from(c))
   - [ ] No performance regression in existing ops

3. **API completeness:**
   - [ ] Char-specific predicates available
   - [ ] split_at implemented
   - [ ] Error handling policy documented

4. **Testing:**
   - [ ] All new features have tests
   - [ ] All existing tests still pass
   - [ ] Performance benchmarks established

---

## Dependencies

- **P0-1** должен быть сделан первым (улучшает код для всех последующих изменений)
- **P0-3** может быть done incrementally (начать с slice/concat/find)
- **P1-1** зависит от P0-1 (использует decode_utf8_at helper)
- **P3 items** независимы, могут быть done в любом порядке

---

## Notes

- Все изменения должны быть backward compatible (не ломать existing code)
- Каждый PR должен включать тесты
- Performance-critical changes должны иметь benchmarks
- Documentation updates обязательны для API changes
