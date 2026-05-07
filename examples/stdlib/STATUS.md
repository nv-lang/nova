# examples/stdlib/ — статус относительно bootstrap-codegen

Это **демо-материалы** показывающие spec-faithful Nova код для базовых
структур данных, парсеров, криптопримитивов и data-форматов. Они
написаны как **аспирационные**: демонстрируют как код *должен* выглядеть
в зрелом Nova, но bootstrap-codegen в текущей итерации не покрывает
все используемые фичи.

Запуск через `.\run_tests.ps1 -IncludeStdlib`. **Сейчас: 0 из 43
компилируется.** Это ожидаемо — список причин ниже, для приоритезации
будущих compiler-задач.

## Закрытые блокеры (2026-05-07)

- **char-литералы** ('a' / '\n' / '\u{...}') — реализованы (Q-char-literals,
  commit 7852ced).
- **throw в expression position** (D25/D65) — реализован (commit cfa53ca).
- **Match scrutinee parsing** — `match foo() { ... }` (commit d467cd2).
- **Leading `||` / `&&` newline-tolerance** (commit 781bb43).

## Группы блокеров (по типу, для приоритезации)

### B. Bitwise операторы `&` и `^` (11 файлов)
**Файлы:** base64, bloom_filter, crc32, hashmap, hex, md5, set, sha1,
sha256, snowflake, ulid (для `&`); bcrypt, hmac, jwt (для `^`).

**Причина:** Парсер сейчас отвергает `&` как "single `&` is not used in
Nova" и `^` как "unexpected byte". Но bitwise-and / bitwise-xor нужны
для криптографии и хэш-функций — это фундаментальные операции.

**Решение:** Добавить bitwise операторы в lexer/parser:
`&`, `|`, `^`, `<<`, `>>` (и compound-assign `&=`, `|=`, etc.).
Spec-clarification: D-operators нужно дополнить.

### C. Multi-line if-else / continuation (8 файлов)
**Файлы:** complex (560), cron (273), duration (469), rate_limiter (87),
retry (85), semver (449), semver_range (117), statistics (237).

**Причина:** `expected '{', got newline` — парсер не справляется с
multi-line if-else в expression position. D49 newline-tolerance не
покрывает все cases.

**Решение:** Расширить newline-tolerance на ветви if/else и trailing
expressions.

### D. Generic syntax `[T]` (3 файла)
**Файлы:** vec (25), lru (16), priority_queue (15).

**Причина:** `expected identifier, got '['` или `expected ']', got identifier`.
Парсер не распознаёт `Type[T]` в некоторых позициях (импорт, тип-аннотация).

**Решение:** Парсер дженериков должен работать в большем числе позиций.

### E. Anonymous record literal без spread (2 файла)
**Файлы:** deque, range.

**Причина:** Codegen говорит "anonymous record literal without spread
not supported". Spec D55 описывает coercion в позиции с явным типом —
нужна inferred-type-context реализация в codegen.

### F. Большие integer literals (3 файла)
**Файлы:** fnv (30), uuid (77), uuid_v3_v5 (17).

**Причина:** "invalid int: number too large to fit". Hash-константы
типа FNV/UUID prime требуют u64-литералов; int64 переполняется.

**Решение:** Принять hex-литералы как u64 семантику в lexer (или
ввести `u64`-suffix и тип uint).

### G. Pattern parsing — comma в pattern (3 файла)
**Файлы:** csv (23), json (98), toml (60), ini (31).

**Причина:** "expected pattern, got `,`" / "expected `]`, got `,`".
Tuple-pattern с запятыми внутри композитного паттерна — не парсится.

**Решение:** Расширить pattern-parser на tuple-of-patterns.

### H. Match-arm syntax (2 файла)
**Файлы:** sql (295: `=>` after `==`), diff (104: `=>` after newline).

**Причина:** Match-arm с guard или multi-line arm-body не распознаётся.

### I. effect keyword в позиции type (1 файл)
**Файл:** linkedlist (48). `fn ... effect Fail[Error]` — `effect` как
keyword в типе сигнатуры. Возможно баг в файле (синтаксис устарел).

### J. Misc (отдельные мелкие баги в файлах)
- **glob (18):** `expected identifier, got match` — match в позиции id.
- **markdown_minimal (117):** `expected type, got m` — likely typo.
- **path (16):** `expected identifier, got ...` — spread где не ждут.
- **queue (26):** `in` keyword в expression — for-in в expression.
- **regex (149):** `expected identifier, got (` — anonymous fn?
- **url (140):** `expected type, got newline` — multi-line type signature.

## Приоритеты

1. **Bitwise операторы** (group B, 11 файлов) — самый широкий unblock,
   нужен для всей криптографии.
2. **Multi-line if-else** (group C, 8 файлов) — широкий unblock для
   data-парсеров и validation-логики.
3. **Большие integer literals** (group F, 3 файла) — мелкий fix, но
   разблокирует UUID/FNV/hash-константы во многих местах.
4. **Generic-syntax + anonymous records + pattern composition** —
   средние codegen/parser-задачи.
5. **Misc** (группы H/I/J) — точечные фиксы по одному.

После каждой группы — recompile и проверка через
`.\run_tests.ps1 -IncludeStdlib`. Финальная цель — **43/43 PASS**.
