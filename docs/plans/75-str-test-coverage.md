# Plan 75 — Полное тестовое покрытие встроенного типа `str`

> **Статус:** 📋 план 2026-05-20, не начат  
> **Приоритет:** P1 | **Оценка:** ~3–5 рабочих дней  
> **Зависимости:** нет (standalone)

## Зачем

`str` — ключевой встроенный тип Nova. Текущий gap-анализ:

**Хорошо покрыто (позитив):** `concat`/`+`, `len`, `byte_len`, `starts_with`, `ends_with`, `contains`, `trim`, `to_lower`/`to_upper`, операторы `==`/`<`/`<=`/`>`/`>=`, `find`/`rfind`/`slice`/`bytes`/`chars`/`split`, интерполяция `"${}"`, escape-последовательности, Unicode (кириллица, emoji).

Файлы с существующим покрытием:
- `nova_tests/types/strings.nv`
- `nova_tests/types/strings_extended.nv`
- `nova_tests/types/str_compare.nv`
- `nova_tests/types/str_search.nv` (402 строки)
- `nova_tests/types/string_interpolation.nv`
- `nova_tests/runtime/string_builder.nv`

**Пробелы — нет ни одного теста:**

| Область | Описание |
|---|---|
| `@char_at(idx int) -> Option[char]` | Единственный безопасный API для посимвольного доступа |
| `@hash() -> u64` | FNV-1a hash; используется в `HashMap[str, V]` |
| `@is_empty() -> bool` | Метод **отсутствует** в `string.nv` (есть у всех коллекций, у `str` — нет); добавить в Ф.1 как `@len() == 0` |
| Конверсии Err-branch | `int.try_from("abc")`, `char.try_from("")`, `f64.try_from("bad")`, `bool.try_from("maybe")` |
| Compile-error тесты | `str as int`, `42 as str` — запрещённые касты |
| `split` edge cases | Пустой sep, соседние sep'ы, пустая строка |
| `slice` / `char_at` OOB | Что происходит при выходе за границы (panic vs clamp) |
| Интерполяция `char`-var | Известная проблема: `"${c}"` для `c: char` выводит int-код, не символ |
| `str.try_from(invalid []u8)` | Error-path при невалидных UTF-8 байтах |

**Source of truth:** `std/runtime/string.nv` (auto-generated из `compiler-codegen/src/codegen/runtime_registry.rs`). Единственный явный static-метод: `str.from(c char)` в `std/runtime/char.nv:13`. Прочие конверсии (`str.from(int)`, `int.try_from(str)`, ...) — codegen-уровень, не объявлены в `.nv`.

## Решение

1. **Новый каталог `nova_tests/str/`** — все новые str-тесты (позитив + негатив) в одном месте. Существующие `types/str_*.nv` не трогаются.
2. **10 тест-файлов** по категориям (Ф.1–Ф.6).
3. **Добавить `str.@is_empty()`** в `std/runtime/string.nv` как обычный Nova-метод (1 LOC). Метод логичный, у всех коллекций есть.
4. **Все найденные баги фиксятся внутри этого плана** — тест коммитится зелёным. Нетривиальные баги — отдельная фаза Ф.5.
5. Декларации модулей: `module str.<test_name>` — аналогично `module types.strings`.

## Файлы тестов

| Файл | Что покрывает | Маркер |
|---|---|---|
| `01_char_at.nv` | `@char_at` ASCII, Unicode (кириллица, emoji), in-bounds | позитив |
| `02_char_at_oob.nv` | `@char_at` при idx ≥ len, idx < 0 → `None` | позитив |
| `03_is_empty_hash.nv` | `@is_empty()` + `@hash()` determinism | позитив |
| `04_split_edge.nv` | split с соседними sep'ами, пустая строка, пустой sep | позитив |
| `05_conversions_positive.nv` | `str.from(char/int/bool/f64)`, `int.try_from(str)` Ok, round-trip `[]u8` | позитив |
| `06_conversions_err.nv` | Err-ветки: `int.try_from("abc")`, overflow, `char.try_from("")`, `char.try_from("hello")`, `f64.try_from("bad")`, `bool.try_from("maybe")` | позитив (Err-matching) |
| `07_interpolation_char.nv` | `"${c}"` для `c: char` → символ, не int-код | позитив |
| `08_slice_oob.nv` | `slice(0, 100)` OOB — зафиксировать/исправить поведение | позитив / EXPECT_RUNTIME_PANIC |
| `09_neg_cast_str_as_int.nv` | `"hello" as int` → compile error | `EXPECT_COMPILE_ERROR` |
| `10_neg_cast_int_as_str.nv` | `42 as str` → compile error | `EXPECT_COMPILE_ERROR` |

**Итого:** 10 файлов, ~70–90 отдельных `test "..."` блоков.

> **Примечание:** `10_neg_utf8.nv` (invalid UTF-8 bytes → Err/panic) — добавляется если `str.try_from([]u8)` доступен как вызываемый метод в Nova-коде. Проверить в Ф.0.

## Фазы

### Ф.0 — Baseline и аудит (0.5 ч)

- [ ] `nova test --filter str` + `nova test --filter string` — зафиксировать текущее состояние (PASS/FAIL)
- [ ] Убедиться что все 23 метода из `std/runtime/string.nv` соответствуют `runtime_registry.rs`
- [ ] Проверить: доступен ли `str.try_from([]u8)` в Nova-коде (для Ф.6)

### Ф.1 — `@char_at` + `@is_empty` + `@hash` (1 д)

**Добавить `@is_empty` в `std/runtime/string.nv`:**
```nova
// True если строка пустая. O(1) (через @len — O(n); приемлемо для bootstrap).
export fn str @is_empty() -> bool => @len() == 0
```

- [ ] Добавить метод в `std/runtime/string.nv` (после `@byte_len`)
- [ ] `nova_tests/str/01_char_at.nv`:
  - `"abc".char_at(0)` → `Some('a')`
  - `"abc".char_at(2)` → `Some('c')`
  - `"Привет".char_at(0)` → `Some('П')` (2-байтный codepoint)
  - `"Привет".char_at(5)` → `Some('т')`
  - `"😀abc".char_at(0)` → `Some('😀')` (4-байтный emoji)
  - `"abc".char_at(3)` → `None` (exactly len → OOB)
- [ ] `nova_tests/str/02_char_at_oob.nv`:
  - `"abc".char_at(100)` → `None`
  - `"".char_at(0)` → `None`
- [ ] `nova_tests/str/03_is_empty_hash.nv`:
  - `"".is_empty()` → `true`
  - `"x".is_empty()` → `false`
  - `"abc".hash() == "abc".hash()` (determinism)
  - `"abc".hash() != "abd".hash()` (разные строки → разный hash с высокой вероятностью)
  - `"".hash() == "".hash()` (пустая строка стабильна)
- [ ] Если `char_at` возвращает неверный тип или panic → debug + fix в `emit_c.rs` / `nova_rt/`
- [ ] `nova test --filter str/0` → PASS

### Ф.2 — `split` edge cases (0.5 д)

- [ ] `nova_tests/str/04_split_edge.nv`:
  - `"a,,b".split(",")` → `["a", "", "b"]` (пустой элемент между sep'ами)
  - `"aXbXc".split("X")` → `["a", "b", "c"]`
  - `"abc".split("X")` → `["abc"]` (sep не найден)
  - `"".split(",")` → `[""]` или `[]` — задокументировать фактическое поведение
  - `",".split(",")` → `["", ""]`
  - `"abc".split("")` → задокументировать (пустой sep — panic или посимвольно)
- [ ] Если поведение сломано → fix; если design choice → comment в тесте

### Ф.3 — Конверсии Ok-branch (0.5 д)

- [ ] `nova_tests/str/05_conversions_positive.nv`:
  - `str.from('A')` → `"A"`
  - `str.from('Ю')` → `"Ю"` (2-байтный codepoint)
  - `str.from('😀')` → `"😀"` (4-байтный emoji)
  - `str.from(42)` → `"42"`
  - `str.from(-7)` → `"-7"`
  - `str.from(0)` → `"0"`
  - `str.from(true)` → `"true"`, `str.from(false)` → `"false"`
  - `str.from(3.14)` — содержит `"3.14"` (format может варьироваться)
  - `int.try_from("42")` → `Ok(42)`
  - `int.try_from("-100")` → `Ok(-100)`
  - `bool.try_from("true")` → `Ok(true)`, `bool.try_from("false")` → `Ok(false)`
  - Round-trip: `let s = "Привет"; assert(str.try_from(s.bytes()) == Ok(s))`

### Ф.4 — Конверсии Err-branch (0.5 д)

- [ ] `nova_tests/str/06_conversions_err.nv`:
  - `int.try_from("abc")` → `Err(...)` (не число)
  - `int.try_from("")` → `Err(...)` (пустая строка)
  - `int.try_from("9999999999999999999999")` → `Err(...)` (overflow)
  - `f64.try_from("bad")` → `Err(...)`
  - `bool.try_from("maybe")` → `Err(...)`
  - `bool.try_from("True")` → `Err(...)` (case-sensitive)
  - `char.try_from("")` → `Err(...)` (пустая строка)
  - `char.try_from("hello")` → `Err(...)` (multi-char)
  - Все через `match` или `if let Err(_) = ...`

### Ф.5 — Интерполяция `char`-переменных (0.5–1 д)

- [ ] `nova_tests/str/07_interpolation_char.nv`:
  - `let c char = 'A'; assert("${c}" == "A")` — ожидаем символ, не `"65"`
  - `let cy char = 'Ю'; assert("${cy}" == "Ю")`
  - char-literal: `assert("${'Z'}" == "Z")`
  - emoji: `let e char = '😀'; assert("${e}" == "😀")`
- [ ] Если `"${c}"` выводит `"65"` (int-код) → fix в `emit_c.rs`:
  - Interpolation dispatch: для переменных типа `char` → `nova_char_to_str()`, не `nova_int_to_str()`
  - Найти место: секция `InterpolatedStr` / `InterpStrPart` в `emit_c.rs`

### Ф.6 — OOB и compile-error тесты (0.5 д)

- [ ] `nova_tests/str/08_slice_oob.nv`: 
  - `"abc".slice(0, 100)` — зафиксировать поведение. Если panic → добавить `EXPECT_RUNTIME_PANIC slice`. Если clamp → assert результат.
- [ ] `nova_tests/str/09_neg_cast_str_as_int.nv` (EXPECT_COMPILE_ERROR):
  ```
  // EXPECT_COMPILE_ERROR
  module str.neg_cast_str_as_int
  fn main() Io { let _ = "hello" as int }
  ```
- [ ] `nova_tests/str/10_neg_cast_int_as_str.nv` (EXPECT_COMPILE_ERROR):
  ```
  // EXPECT_COMPILE_ERROR
  module str.neg_cast_int_as_str
  fn main() Io { let _ = 42 as str }
  ```
- [ ] Убедиться в читаемости сообщений об ошибках

### Ф.7 — Финал: regression + README (0.5 д)

- [ ] `nova test` полный прогон → **0 новых FAIL** (baseline 0 FAIL сохранён или улучшен)
- [ ] Добавить строку в `docs/plans/README.md`
- [ ] Коммит: `feat(75): str test coverage — Ф.1–Ф.6`

## Acceptance criteria

- [ ] `nova test` — 0 FAIL (baseline не ухудшен)
- [ ] `nova_tests/str/` содержит ≥ 9 test-файлов
- [ ] `@char_at` — ≥ 6 тестов (ASCII, Unicode 2b, emoji 4b, OOB-None, empty-None)
- [ ] Err-ветки конверсий — ≥ 8 Err-сценариев
- [ ] `@is_empty` добавлен в `std/runtime/string.nv` и протестирован
- [ ] Интерполяция `char`-var: рабочая ИЛИ зафиксирован баг с fix в codegen
- [ ] Compile-error тесты: ≥ 2 файла с `EXPECT_COMPILE_ERROR`
- [ ] Каждый найденный баг: зафиксирован (тест зелёный) ИЛИ documented known limitation

## Non-scope

- `StringBuilder` — хорошо покрыт в `nova_tests/runtime/string_builder.nv`, не трогаем
- Производительность / бенчмарки str — Plan 57
- Unicode collation (сортировка по locale) — будущее
- Raw strings, multiline literals — не реализованы в bootstrap
- `nova_tests/types/str_*.nv` — не переносим, оставляем как есть
- Contracts/SMT для str — Plans 33.x
