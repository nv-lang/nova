# std/ — статус относительно bootstrap-codegen

Это **стандартная библиотека Nova** — spec-faithful код для базовых
структур данных, парсеров, криптопримитивов и data-форматов. Файлы
организованы по доменам (`collections/`, `crypto/`, `encoding/`,
`identifiers/`, `checksums/`, `time/`, `path/`, `math/`, `text/`,
`data/`, `concurrency/`).

Часть либ написана как **аспирационные** — демонстрируют как код
*должен* выглядеть в зрелом Nova, но bootstrap-codegen в текущей
итерации не покрывает все используемые фичи.

Запуск через `.\run_tests.ps1 -IncludeStdlib`.

## Текущий статус (2026-05-27, Plan 91 Ф.0+Ф.7.1+Ф.4 ✅ ЗАКРЫТЫ — branch `plan-91-stdmvp`)

**Ф.4 results (sort module):**

- Создан `std/sort.nv` (MVP surface: `[]int @sort`/`@sort_by`/
  `@binary_search`/`@min`/`@max`).
- Smoke `target/smoke_sort.nv` — build OK, run OK.
- Conformance `nova_tests/plan91/sort_basic.nv` — **15/15 PASS**.
- `nova check std/` после добавления: **25 PASS / 0 FAIL.**

**Ф.7.1 results (quarantine):**

- **`nova check std/`** (no `--skip`, full directory walk): **24→25 PASS / 0 FAIL.**
  Acceptance criterion Plan 91 §Ф.7.1 met (down from 12 FAIL pre-quarantine).
- **30 non-MVP files** перенесены в `std/_experimental/` (auto-skip через
  `should_skip_path_full` на underscore-prefixed component). Полная таблица
  per-domain — [std/_experimental/STATUS.md](_experimental/STATUS.md).
- **23 MVP files** остаются в `std/`: prelude/runtime/testing/collections (vec/
  hashmap/set/range)/encoding (json/base64)/time (duration)/net (Plan 83.12)/
  concurrency (cancellation/timer)/bench (Plan 57).
- **Compiler fix:** `is_stdlib_runtime_module` whitelist расширен для
  `std.net.*` (Plan 83.12 unblocks) + `std.bench` (Plan 57 benchmark DSL).
- **Source fixes (как побочный продукт Ф.7.1):**
  - D52 §2 в `std/encoding/json.nv` (10 violations) — Ф.3 work закрыт попутно.
  - `partial_prelude(core, runtime, errors)` в `std/collections/range.nv`
    (avoid std.prelude self-cycle при standalone check, Plan 62.F).
  - `module bench` → `module std.bench` (D29 rev-3 compliance).
  - `use net.addr` → `use std.net.addr` в `std/net/{tcp,udp}.nv` (Plan 83.12
    resolver fix).
- **Test imports updated:** 7 nova_tests файлов теперь импортируют
  `std._experimental.<domain>.<file>` вместо устаревшего `std.<domain>.<file>`.

### Ф.0 baseline (2026-05-27, до Ф.7.1)

**Прогон:** `nova check --color never std/ --skip std/encoding/toml.nv`
+ targeted `nova build` smoke на каждый MVP-модуль.
**Окружение:** worktree `nova-p91` от `github/main` (HEAD `32f3dd51392`,
post-Plan 83.10.4 merge `7b5b2fec8e0`).

### Сводка

- **std type-check (`nova check std/`, toml skipped):** **43 PASS / 12 FAIL** на 55 файлов.
  Numbers stable vs 2026-05-23 ревизии (12 FAIL не уменьшилось — Plan 100.x / 103.x / 104.x
  не трогали std/-источники).
- **std type-check on `std/` directory без `--skip`:** **stack overflow** —
  триггерится `std/encoding/toml.nv` (parser/checker глубокая рекурсия). Per-file `nova check`
  все остальные файлы работают; только TOML парсер сваливает stack. Workaround: `--skip` или
  карантин TOML в Ф.7.
- **std compile→exe (MVP modules smoke):** **8/9 категорий MVP компилируются и работают
  end-to-end** (PASS на realistic use-case). Главные блокеры — отсутствующие методы
  `[]str.join` / `str.parse_int` / `str.parse_float` / `str.pad_*` и отсутствующий модуль
  `std/sort.nv`; D52 §2 источниковые регрессии в `std/encoding/json.nv`.
- **Полный `nova test`:** не дозапускался в Ф.0 (4+ часа на full suite). Авторитативный
  baseline — последняя зафиксированная ревизия пред-83.10.4: **1158 PASS / 19 FAIL / 56 SKIP**
  (memory `project-plan83_10_3-status`); Plan 83.10.4 closure (merge `7b5b2fec8e0`,
  2026-05-27) перевёл concurrency-cluster в более зелёное состояние — фактический baseline
  ожидается ~1162-1165 PASS, но точная проверка делегирована Ф.5 (conformance) Plan 91.

### MVP по модулям (Plan 91 §Scope), компиляция → запуск → результат

| MVP-модуль | check | build→exe | run | Что показывает smoke |
|---|---|---|---|---|
| `Option` (prelude) | ✅ | ✅ | ✅ | `safe_div + .unwrap_or(-1) + .map(\|x\| x*2)` |
| `Result` (prelude) | ✅ | ✅ | ✅ | `type DivErr | DivByZero \| Other` + match Ok/Err |
| `Vec` (`[]T` ext methods `std/collections/vec.nv`) | ✅ | ✅ | ✅ | `map/filter/fold/any/all/first` chained |
| `HashMap` (`std/collections/hashmap.nv`) | ✅ | ✅ | ✅ | `new/insert/get/len/match Option` |
| `HashSet` (`std/collections/set.nv`) | ✅ | ✅ | ✅ | `new/insert/contains/len` |
| `Duration` (`std/time/duration.nv`) | ✅ | ✅ | ✅ | `from_secs/from_millis/plus/as_nanos/as_millis` |
| `math` (`std/runtime/math.nv`) | ✅ | ✅ | ✅ | `f64.sqrt/.pow/.ln` через runtime stubs |
| `text` basic (`std/runtime/string.nv`) | ✅ | ✅ | ✅ | `split/trim/to_upper/starts_with` + `for-in []str` |
| `text` extended (join/parse_int/pad) | n/a | ❌ | n/a | методы отсутствуют (см. ниже) |
| `json` (`std/encoding/json.nv`) | ❌ | ❌ | n/a | 5 D52 §2 источниковых ошибок в самом std/encoding/json.nv |
| `sort` (нет модуля) | n/a | n/a | n/a | модуля не существует — Ф.4 создаёт `std/sort.nv` |

**🎯 Update vs Plan 91's assumptions:** Plan 91 §Ф.1 ожидал крупных codegen-блокеров
для Vec/HashMap/HashSet (generic specialization, array-type mangling, D72 protocol-bound
dispatch, tuple types). **Реальность 2026-05-27 — все три коллекции работают
end-to-end на realistic smoke-программах.** Plan 95/99/101/103 закрыли эти блокеры
попутно. Ф.1 Plan 91 — теперь conformance-валидация и расширение API surface, не
снятие core-блокеров.

### Категоризация 12 текущих FAIL (`nova check`)

**A. D52 §2 shorthand violations** (parser-strict regression после ~50 планов):
6 файлов, тривиальный source-level fix (`name: name` → `name`, `name: @name` → `@name`):
- `std/encoding/json.nv:289:36, 443:47, 593:66` (line:@line, col:@col, text:text, key:key)
- `std/math/complex.nv:179:7` (re:@re, im:@im likely)
- `std/time/cron.nv:167:51, 167:61` (min:min, max:max)
- `std/text/regex.nv:204:27, 204:37, 371:37, 486:9` (min/max/c/start)
- `std/crypto/jwt.nv:133:77` (now_ms)

**B. Array literal parse error** ("expected `,` or `]` in array literal, got int literal"):
4 файла, **все non-MVP** (crypto):
- `std/crypto/hmac.nv:180:28`, `std/crypto/md5.nv:162:30`, `std/crypto/sha1.nv:84:30`,
  `std/crypto/sha256.nv:151:30`

**C. E_UNUSED_PREFIX_TYPEVAR** (Plan 101.1 / D145 strictness, post-2026-05-25):
1 файл, **non-MVP**:
- `std/concurrency/retry.nv:107:11` — `fn[…E]` declared but не используется в signature

**D. Parser stack overflow:** 1 файл, **non-MVP**:
- `std/encoding/toml.nv` — deep recursion в parser. Per-file и full-dir crash идентичны.
  Карантинировать в Ф.7.1.

**E. Standalone-check import cycle:** 1 файл (false-positive для full-dir):
- `std/collections/range.nv` — `collections.range → std.prelude → std.collections.range`.
  В составе `nova check std/` (full dir) загружается через prelude и работает; ломается
  только при per-file `nova check std/collections/range.nv` standalone. Не блокер
  shipping'а; задача для tooling polish (не Plan 91 scope).

### Кодоген-блокеры по MVP-модулям (Plan 91 §Ф.0.2 категоризация)

**Ф.2 — Text (real work):**

Отсутствуют методы (нужны и в `compiler-codegen/src/codegen/runtime_registry.rs`,
и в `nova_rt/string.c` C-runtime):

- `str @parse_int() -> Option[int]` (или `Result[int, ParseError]`)
- `str @parse_int_radix(radix int) -> Option[int]`
- `str @parse_f64() -> Option[f64]`
- `[]str @join(sep str) -> str`
- `str @pad_left(width int, fill char) -> str`
- `str @pad_right(width int, fill char) -> str`
- `str @repeat(n int) -> str`
- `str @replace(from str, to str) -> str`

Без них Ф.2 «text» — neither conformance-completable, ни getting-started-демонстрируемое.
**Effort:** 0.5-1 день (~8 методов через runtime_registry + C-stubs).

**Ф.3 — JSON (cheapest fix):**

5 D52 §2 источниковых ошибок в `std/encoding/json.nv`. Build→exe не пробовался — после
fix потребуется smoke compile→exe round-trip-теста.
**Effort:** 0.5 день (5 edits + smoke + один conformance test).

**Ф.4 — Time / Math / Sort:**

- `Duration` (Ф.4 time): ✅ работает end-to-end. `Instant` (now/elapsed) — не было в
  Ф.0.1 smoke; нужно проверить отдельно.
- `math` (Ф.4 math): ✅ runtime stubs работают (sqrt/pow/ln/sin/cos и др.). Free-fn
  `min`/`max`/`abs` — через if-else работают; чтобы дать canonical API через `int.min(b)`
  / `int.max(b)` — нужны методы в runtime_registry.
- `sort` (Ф.4 sort): **модуль не существует.** Нужно создать `std/sort.nv` со следующим
  surface (по образцу Go `slices` / Rust):
  - `fn[T] []T @sort()` где `T Ord`
  - `fn[T] []T @sort_by(cmp fn(T, T) -> Ordering)`
  - `fn[T] []T @binary_search(target T) -> Option[int]`
  - `fn[T] []T @min() -> Option[T]`, `@max() -> Option[T]` (Ord-bound)

Внутренний алгоритм: simple `pdq-sort` или `intro-sort` или даже `insertion-sort`
для MVP. Quick-sort/merge-sort работают через `D72 protocol-bound dispatch`,
который уже закрыт Plan 100.

**Effort:** Ф.4 = 1-2 дня (sort module + Instant + canonical min/max wrappers + 4 conformance tests).

**Ф.7.1 — Quarantine (cheap, unblocks shipping cleanly):**

Перенести в `std/experimental/` либо пометить markers в `std/nova.toml`:
- `std/encoding/toml.nv` (stack overflow)
- `std/encoding/csv.nv`, `std/encoding/ini.nv` (non-MVP)
- `std/text/regex.nv`, `std/text/diff.nv`, `std/text/markdown_minimal.nv` (non-MVP, regex check-fail)
- `std/math/complex.nv`, `std/math/statistics.nv` (non-MVP, complex check-fail)
- `std/crypto/*` (5 файлов, non-MVP, 4 check-fail)
- `std/identifiers/*` (4 файла, non-MVP)
- `std/checksums/*` (2 файла, non-MVP)
- `std/data/sql.nv`, `std/data/semver*.nv` (non-MVP)
- `std/path/*` (non-MVP — отложено в Plan 18)
- `std/concurrency/retry.nv` (non-MVP, retry check-fail)
- `std/concurrency/rate_limiter.nv`, `std/concurrency/cancellation.nv`,
  `std/concurrency/timer.nv` (опционально; cancellation в зависимостях Plan 83 fiber-api)

Что остаётся в `std/` MVP: prelude + runtime + testing + collections (vec, hashmap, set;
deque/lru/priority_queue/queue/linkedlist/bloom_filter/range можно либо MVP, либо
experimental — решить в Ф.7.1) + encoding (json, base64, hex, url) + time (duration; cron
→ experimental) + math (как папка остаётся пустой если complex/statistics ушли —
ребрейс через runtime/math.nv) + новый sort.

**Effort:** Ф.7.1 = 0.5 день (git mv + nova.toml update + `std/experimental/STATUS.md`).

### Стек-overflow в TOML — что это

Per-file `nova check std/encoding/toml.nv` валит stack overflow. Файл ~700 строк
(не уникально большой), но содержит много вложенных рекурсивных конструкций (TOML AST:
inline-tables и arrays-of-tables). Не Plan 91 фикс-target — заведём отдельный bug-track
после quarantine. В Ф.7.1 TOML уходит в experimental, и `nova check std/` начинает работать
без `--skip`.

### Ф.0.4 — Decision point: ревизия Ф.1-Ф.4 по фактическим данным

**Старый порядок Plan 91** (от 2026-05-22): Ф.1 (collections) → Ф.2 (text) → Ф.3 (json) →
Ф.4 (time/math/sort). Предположение — Ф.1 это major codegen work.

**Ревизия Ф.0.4 (2026-05-27): реальный объём ~5-7 дней (не 3-5 недель). Новый порядок:**

1. **Ф.7.1 (quarantine, 0.5 дн)** — **переносим вперёд.** Чистит test surface, убирает
   noise в STATUS.md, разблокирует `nova check std/` без `--skip`. Без неё каждый
   следующий шаг возвращается к одному и тому же 12-FAIL шуму.
2. **Ф.3 (json D52 fix, 0.5 дн)** — 5 source-level edits + smoke compile→exe + round-trip
   conformance.
3. **Ф.4 (time/math/sort, 1-2 дн)** — sort module create (новая работа) + Instant smoke +
   canonical min/max wrappers + 4 conformance tests.
4. **Ф.2 (text join/parse/pad, 1-2 дн)** — 8 методов через runtime_registry + C stubs +
   conformance.
5. **Ф.1 (collections, 0.5 дн)** — **только conformance-валидация** — Vec/HashMap/Set
   уже работают. Нужно: realistic use-case test для каждой + cross-product (Vec[(K,V)]
   round-trip через HashMap, etc).
6. **Ф.5 (conformance integration, 1 дн)** — собрать все conformance из Ф.1-Ф.4 в
   единый `nova_tests/plan91/` set + property-style тесты.
7. **Ф.6 (getting-started + 5-7 examples, 1 дн)** — Ф.6.1 decision (in-memory или TCP-echo)
   зависит от 83.12 готовности; default — in-memory.
8. **Ф.7.2-Ф.7.4 (release checklist, 0.5 дн)** — STATUS.md final, plans README, doc updates.

**Итого:** ~5-7 рабочих дней (1 sequential dev-week, или ~2 дня wall-time с
parallel-agent split á la Plan 103.4).

**Параллелизация (Plan 103.4 pattern):** Ф.7.1 + Ф.3 + Ф.4 + Ф.2 — **independent**, можно
4 параллельных Sonnet 4.6 worktree-агента. Ф.1 / Ф.5 / Ф.6 — sequential (зависят от Ф.1-Ф.4).

Список ниже — историческая хронология раундов 1-5 (закрытые блокеры) +
оставшиеся группы блокеров для приоритезации новых compiler-задач.
**Группы B-M ниже устарели после Plan 91 Ф.0 re-baseline (2026-05-27).** Реальные
2026-05-27 блокеры собраны выше в «Категоризация 12 текущих FAIL» и «Кодоген-блокеры
по MVP-модулям».

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

### 2026-05-07 (раунд 5)
- **D79 Channel base** (commit c0cd4337, 3d0cc7e9, 0dc5421b).
  Bootstrap-runtime: bounded ring-buffer, send/recv c yield, close +
  drain семантика, try_send/try_recv. 11 sequential тестов.
- **Lint: export-fail-untyped** (commit 835473f66). D65 convention:
  `export fn ... Fail` без `[E]` → warning. `Fail[E]`/`Fail[any]` OK.
- **Lint: protocol-in-effect-position** (commit 1fedd158d). D62 matrix:
  `fn f() Hashable -> ()` → warning, должно быть generic-bound.
- **D28 effect inference для private fn** (commit 284b20743, 4ee684852).
  Private fn с throw, без явного Fail — auto-add `Fail` placeholder.
  Public fn не трогается (lint вместо).
- **`select` parser + concurrent channels** — отложено (требует
  spawn-block codegen-fix).

### Совокупный эффект 5-го раунда
- Channel разблокирует concurrent примитивы как только spawn-block fix.
- D28 inference уменьшает шум в private helper'ах.
- Lints выводят рекомендации (suppressable через `--no-lint`) для
  AI-friendly кода.

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
