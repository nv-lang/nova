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
  Lexer + Parser-приоритеты по spec/03-syntax.md уровни 7-10 + codegen.
  Тесты: tests-nova/types/bitwise.nv (22 теста).
- **u64 hex/bin литералы > i64::MAX** — wrap to i64 (commit d111415e).

### Совокупный эффект 2-го раунда
14 stdlib-файлов (base64, bloom_filter, crc32, hashmap, hex, md5, set,
sha1, sha256, snowflake, ulid, bcrypt, hmac, jwt) больше не блокируются
на bitwise. 4 файла (fnv, uuid, uuid_v3_v5, ulid) больше не блокируются
на large-int. Каждый продвинулся на следующий блокер — см. таблицу ниже.

## Группы блокеров (по типу, для приоритезации)

### A. with-handler-lambda + trailing-block (5+ файлов)
**Файлы (точно):** retry (85), semver (449), semver_range (117),
statistics (237), rate_limiter (87), snowflake (105).

**Форма:** `with E = (e) => interrupt Err(e) { body }` — handler-lambda
greedy ест `{ body }` как trailing-block после `interrupt Err(e)`.

**Причина:** `parse_expr` после `=>` не различает "lambda-body выражение"
от "следующий `{`-block — это блок with'а". Парсер видит `interrupt Err(e) { ... }`
как call-with-trailing-block.

**Решение:** парсить handler-выражение в режиме `no_trailing_block`,
либо ввести explicit-form (handler-lambda обязательно в скобках).

### B. Multi-line if-else (continuation) (5 файлов)
**Файлы:** complex (560), cron (273), duration (469), hex (137),
semver_range (117).

**Причина:** `expected '{', got newline` — multi-line if-else в
expression position. D49 newline-tolerance не покрывает все cases.

### C. Anonymous record literal без spread (4 файла)
**Файлы:** deque, range, fnv, bloom_filter.

**Причина:** Codegen "anonymous record literal without spread not
supported". Spec D55 описывает coercion в позиции с явным типом — нужна
inferred-type-context реализация в codegen.

### D. Generic syntax `[T]` в неподдерживаемых позициях (3 файла)
**Файлы:** vec (25), lru (16), priority_queue (15).

**Причина:** `expected identifier, got '['` / `expected ']', got identifier`.
Парсер не распознаёт `Type[T]` в некоторых позициях.

### E. Pattern parsing — composite tuple-patterns (4 файла)
**Файлы:** csv (23), json (98), toml (60), ini (31), jwt (49),
hashmap (65).

**Причина:** "expected pattern, got `,`" / "expected `]`, got `,`".
Tuple-pattern с запятыми внутри композитного pattern не парсится.

### F. mut-параметры в @method-сигнатурах (2 файла)
**Файлы:** uuid (221), uuid_v3_v5 (66).

**Причина:** `expected type, got 'mut'` — `mut`-marker в параметрах
методов не парсится. Исправлено для свободных fn, но не для @method.

### G. for-in: byte/array iterator (1 файл)
**Файл:** crc32 (codegen-error: "for-in: unsupported iterator type
'nova_int'").

**Причина:** Codegen for-in поддерживает только Range и Array. Когда
итерируемся по результату некоего expression, тип нельзя вывести как
Array — codegen падает.

### H. fixed arrays (1 файл)
**Файл:** hmac.

**Причина:** Codegen "fixed arrays not yet supported".

### I. \x escape в str literal (1 файл)
**Файл:** base64 (291).

**Причина:** Lexer не поддерживает `\xNN` в строках (только `\u{...}`).

### J. md5/sha256: top-level expr вне fn (2 файла)
**Файлы:** md5 (230), sha256 (220) — "expected fn / type / let / const".

**Причина:** На указанной строке статемент не распознаётся как top-level.
Возможно continuation от предыдущей сигнатуры.

### K. Match-arm syntax (2 файла)
**Файлы:** sql (295: `=>` after `==`), diff (104: `=>` after newline),
bcrypt (87 — `return false` в match-arm).

### L. Misc — single-file-блокеры
- **set (21):** `use map HashMap[T,()]` — D39 embed не реализован.
- **linkedlist (48):** `effect` keyword в типе — старый синтаксис.
- **glob (18):** `expected identifier, got match` в выражении.
- **markdown_minimal (117):** `expected type, got m` — likely typo.
- **path (16):** `expected identifier, got ...` — spread где не ждут.
- **queue (26):** `in` keyword в expression.
- **regex (149):** `expected identifier, got (` — anonymous fn?
- **sha1 (90):** `unexpected | in expression`.
- **url (140):** `expected type, got newline` — multi-line type signature.

## Приоритеты следующего раунда

1. **with-handler-lambda + trailing-block** (group A, 5+ файлов) —
   средний parser-fix, разблокирует ВСЕ retry/timer/scheduler stdlib.
2. **Multi-line if-else** (group B, 5 файлов).
3. **Anonymous record literal без spread** (group C, 4 файла) — codegen.
4. **Pattern composition в tuple/list** (group E, 6 файлов) — parser.
5. **mut в @method params** (group F, 2 файла) — точечный fix.
6. **Misc / single-file** (groups L-K) — точечные.

После каждой группы — recompile и проверка через
`.\run_tests.ps1 -IncludeStdlib`. Финальная цель — **43/43 PASS**.
