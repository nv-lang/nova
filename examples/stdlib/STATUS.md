# examples/stdlib/ — статус относительно bootstrap-codegen

Это **демо-материалы** показывающие spec-faithful Nova код для базовых
структур данных, парсеров, криптопримитивов и data-форматов. Они
написаны как **аспирационные**: демонстрируют как код *должен* выглядеть
в зрелом Nova, но bootstrap-codegen в текущей итерации не покрывает
все используемые фичи.

Запуск через `.\run_tests.ps1 -IncludeStdlib`. **Сейчас: 0 из 43
компилируется.** Это ожидаемо — список причин ниже, для приоритезации
будущих compiler-задач.

## Закрытые блокеры

### 2026-05-07 (раунд 1)
- **char-литералы** ('a' / '\n' / '\u{...}') — реализованы (commit 7852ced).
- **throw в expression position** (D25/D65) — реализован (commit cfa53ca).
- **Match scrutinee parsing** — `match foo() { ... }` (commit d467cd2).
- **Leading `||` / `&&` newline-tolerance** (commit 781bb43).

### 2026-05-07 (раунд 2)
- **Bitwise операторы** `& | ^ << >>` — реализованы (commit db5bc95f).
- **u64 hex/bin литералы > i64::MAX** — wrap to i64 (commit d111415e).

### 2026-05-07 (раунд 3)
- **Handler-expr non-greedy в `with`** (commit 9dc7c23c).
  `with E = (e) => interrupt Err(e) { body }` — handler-lambda больше
  не "ест" `{ body }` как trailing-block.
- **mut-маркер на параметре fn** (commit 82767261). `fn f(buf mut Buffer)`
  — D6 mutable-marker теперь парсится (игнорируется в bootstrap).
- **D55 anonymous record literal с inferred type** (commit 94c76822).
  `fn make_point() -> Point => { x:7, y:11 }` — codegen использует
  declared return type как target struct.

### Совокупный эффект 3-го раунда
~15 stdlib-файлов продвинулись на следующие блокеры:
- complex/cron/duration/retry/semver/semver_range/snowflake/statistics/
  rate_limiter/ulid — handler-lambda больше не блокирует.
- range/fnv/snowflake/statistics/rate_limiter/bloom_filter — anon record
  больше не блокирует.
- uuid/uuid_v3_v5 — mut-params больше не блокирует.

### 2026-05-07 (раунд 4)
- **str.bytes() / chars() / split()** в codegen+runtime (commit f5a744f4,
  faa37299, e1f1b561). Eager bootstrap-имплементация Iter[char]/Iter[str]:
  - `nova_str_bytes(s)` → `[]byte` копия UTF-8 байтов
  - `nova_str_chars(s)` → `[]int` decoded codepoints
  - `nova_str_split(s, sep)` → `[]str` разбиение
  - `array_element_types[var] = nova_byte` для `let xs = s.bytes()`,
    чтобы for-in типизировал как byte.
- **Pattern alternation `|` в match arms** (commit e64b3b5e, e5befbbb).
  `Some(A) | Some(B) => body` собирается в Pattern::Or; codegen
  emit'ит disjunction условий; bindings из первого варианта.

### Совокупный эффект 4-го раунда
- crc32/fnv/snowflake/statistics — продвинулись через codegen, упёрлись
  в C-stage (assert/sqrt/Timestamp).
- bloom_filter — продвинулся, упёрся на `f64.ln()` (отсутствующий метод).
- cron/semver/semver_range — продвинулись через for-in блок на
  следующие блокеры.
- complex (562 → 563) — pattern alternation сработал, ушёл на
  следующий синтаксический блокер.

## Группы блокеров (после раунда 3)

### A. for-in: codegen iterator type-inference (5 файлов)
**Файлы:** bloom_filter, crc32, cron, range, semver, semver_range.

**Причина:** Codegen говорит "for-in: unsupported iterator type
'nova_int'/'Nova_Range*'". For-in поддерживает только когда тип
итерируется явно как Range или Array. Если результат expression
(например `xs.bytes()` или `0..n` через variable) — type-inference
падает.

### B. Pattern composition в tuple/list (5 файлов)
**Файлы:** csv (23), hashmap (65), ini (31), json (98), jwt (49),
toml (60).

**Причина:** "expected pattern, got `,`" / "expected `]`, got `,`".
Парсер pattern не поддерживает запятые внутри composite-паттернов.

### C. Multi-line if-else (3 файла)
**Файлы:** complex (562: `expected '=>', got '|'` — pattern alternation),
hex (137), retry/duration (handler issue не до конца).

### D. Pattern alternation `Some | None` (1+ файлов)
**Файл:** complex (562). `Some(InvalidFormat) | Some(NotANumber) =>`
не парсится — `|` в pattern.

### E. Generic syntax `[T]` в неподдерживаемых позициях (3 файла)
**Файлы:** vec (25), lru (16), priority_queue (15).

### F. for-in over str byte sequence (codegen) (1+ файлов)
**Файл:** ulid: "unsupported operator Lt on nova_str".

### G. fixed arrays (1 файл)
**Файл:** hmac.

### H. \x escape в str literal (1 файл)
**Файл:** base64 (291).

### I. md5/sha256: top-level expr вне fn (2 файла)
**Файлы:** md5 (230), sha256 (220) — "expected fn / type / let / const".

### J. Match-arm syntax (2 файла)
**Файлы:** sql (295), diff (104), bcrypt (87).

### K. uuid/uuid_v3_v5: остаточные блокеры (2 файла)
- uuid (332): `expected =>, got newline` — match-arm.
- uuid_v3_v5: `non-constant expression in const declaration`.

### L. CC-FAIL (codegen прошёл, MSVC ругается) (4 файла)
- fnv: `nova_str.bytes` — отсутствующий метод.
- hex: `Nova_char` — char-тип не имеет C-определения.
- rate_limiter: `Nova_Timestamp` — отсутствующий тип.
- snowflake: `assert: необъявленный идентификатор`.
- statistics: `.sqrt` метод не на правильном типе.

### M. Misc — single-file-блокеры
- **set (21):** `use map HashMap[T,()]` — D39 embed.
- **linkedlist (48):** `effect` keyword в типе.
- **glob (18):** `expected identifier, got match`.
- **markdown_minimal (134):** `expected pattern, got ...`.
- **path (16):** `expected identifier, got ...`.
- **queue (26):** `in` keyword в expression.
- **regex (149):** `expected identifier, got (`.
- **sha1 (90):** `unexpected | in expression`.
- **url (140):** `expected type, got newline`.
- **deque:** anonymous record иногда (?) — checking.

## Приоритеты следующего раунда

1. **Pattern alternation `|`** (group D) + **for-in expr-typing**
   (group A, 5 файлов) — parser/codegen.
2. **Pattern composition** в tuple/list (group B, 5 файлов).
3. **CC-FAIL fixes** (group L, 5 файлов) — runtime/std-types missing.
4. **Misc / single-file** (group M).

После каждой группы — recompile + проверка через
`.\run_tests.ps1 -IncludeStdlib`. Финальная цель — **43/43 PASS**.
