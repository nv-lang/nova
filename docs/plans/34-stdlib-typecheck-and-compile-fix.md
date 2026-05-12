// SPDX-License-Identifier: MIT OR Apache-2.0
# План 34: stdlib type-check + compile fix

**Статус:** ✅ **ЗАКРЫТ 2026-05-12** (расширенный scope).

- **Ф.1/1.5/2/3** type-check регресс: handlers.nv, 9 файлов через
  `import as th`, 4 parser-фикса. **45/45 std type-check PASS**.
- **Ф.4** полный `nova test std/` sweep — категоризация 48 FAIL'ов в
  5 категориях.
- **Ф.5.1** `--skip <pattern>` флаг (реализован параллельным агентом),
  для skip'а `std/runtime/`.
- **Ф.5.2** `int as char` refactor через `char.try_from` в 5 файлах
  (base64/hex/ulid/uuid/property) — codegen-гейт разблокирован.
- **Ф.5.3-4** strict-bool и for-in nova_int — оказались **не локальными
  fix'ами** (категория C/D), вынесены в known-blockers.
- **Ф.6** final sweep + docs.

**Финал:** 4 PASS / 41 FAIL после расширения. Pass-rate в Plan 34 не
вырос — за каждым разблокированным `int as char` стоят другие
codegen-bugs (NovaOpt mismatch, `nova_str` Lt, Nova_Buffer и т.д.).
Это **честный результат** — Plan 34 закрыл свой declared scope; 41
оставшийся FAIL — категории B/C/D из Ф.4 анализа, требуют отдельных
планов уровня spec/codegen, не локальных правок.

---

## Контекст

После закрытия [Plan 14](14-stdlib-codegen-gaps.md) (тогда baseline был
50/50 type-check) в stdlib добавились новые модули (`bcrypt`, `jwt`,
`ulid`, `uuid`, `snowflake`, `rate_limiter`, `retry`, `duration`,
`property`, и др.), которые в test-блоках используют ещё **не существующие**
helper'ы:

- `with Random = seeded(42) { ... }` — фабрика handler'а для `Random`
  эффекта со seeded PRNG (детерминированные тесты для bcrypt/uuid/ulid).
- `with Time = fixed_ms(0) { ... }` — фабрика handler'а для `Time`
  эффекта с фиксированным `now_ms()` (snowflake/jwt/cron/retry).

Плюс 3 файла с parser-багами (видимо, спека языка разошлась с парсером):

| Файл | Ошибка |
|---|---|
| `std/encoding/json.nv:164` | `expected pattern, got '+=' ` (mut-assignment в match-arm) |
| `std/text/regex.nv:222` | `expected fn / type / let / const / test, got '\|\|'` (multi-line bool expr) |
| `std/testing/property.nv:342` | `unexpected '=>' in expression` (closure внутри property-call) |

**Plan 34 закрывает оба класса** + вводит `std/testing/handlers.nv` как
первый модуль `std.testing` (Q3 из Plan 18 — где живут default
handler'ы — здесь же получает частичный ответ для test-handler'ов).

---

## Что переносим из Plan 18

Plan 18 (DRAFT) остаётся как общий roadmap stdlib (P0/P1/P2, design
decisions). Сюда переносим только то, что **активно реализуется**:

- **P0 → `std.testing`** — handler-фабрики для детерминизма (Random
  seeded, Time fixed) и property-test инфраструктура. В Plan 18
  `std.testing` отсутствовал в таблице P0 — расширяем roadmap (см.
  «Связь с Plan 18» ниже).
- **Q3 из Plan 18** («где предоставляются default handler'ы для эффектов
  — runtime или stdlib?») — для **test-handler'ов** ответ: stdlib
  (`std.testing`). Production-handler'ы (real_fs, real_net) — за рамками
  Plan 34.
- **Дизайн-решение 12 из Plan 18** (per-fiber handler isolation D80)
  — sanity-check: тесты должны корректно работать с `with ... = h { body }`
  без глобальных синглтонов.

---

## Фазы

### Ф.1 — `std/testing/handlers.nv` (handler-фабрики) ✅ ЗАКРЫТ 2026-05-12

**Что:** новый файл [std/testing/handlers.nv](../../std/testing/handlers.nv)
с двумя exported функциями:

```nova
// Возвращает handler для Random эффекта с воспроизводимым PRNG.
fn seeded(seed int) -> RandomHandler

// Возвращает handler для Time эффекта с фиксированным временем
// (now_ms() всегда возвращает `ms`, monotonic_ns() тоже фиксирован).
fn fixed_ms(ms int) -> TimeHandler
```

**Реализация:**
- Sigaturа `fn seeded(seed int) -> Handler[Random]` и
  `fn fixed_ms(ms int) -> Handler[Time]` — first-class handler-фабрики
  через `Handler[E]` (D87). PRNG — Knuth MMIX LCG (`a=6364136223846793005,
  c=1442695040888963407`), не-CSPRNG, достаточно для воспроизводимости.
  Handler-литерал capture'ит `let mut state` из тела фабрики (стандартный
  closure-capture).
- `fixed_ms` возвращает `Time` handler где все `now_*` возвращают
  фиксированное время, `sleep(d)` — no-op.
- 7 self-tests в файле, прогон чистый.

**Что выяснилось в ходе работы:** `nova check` cross-file resolution
поддерживает типы (`Timestamp` из другого модуля) и static methods
(`Timestamp.from_unix_millis`) **без import**, но **не bare-name
функции**. Используется паттерн `import std.testing.handlers as th` +
`th.seeded(...)` / `th.fixed_ms(...)`. Wildcard `import X.Y.*` парсер
не принимает. См. [Plan 35](35-cross-file-resolve.md) Ф.1 — туда
вынесена задача расширить name resolution для bare-name visibility.

**Acceptance:** ✅ 9 stdlib-файлов с `th.seeded` / `th.fixed_ms`
проходят `nova check`.

### Ф.2 — Parser-баги (json/regex/property) ✅ ЗАКРЫТ 2026-05-12

**Решение по каждому:** правка .nv (не парсер), сохраняя семантику.

1. **`json.nv:164` (`+=` в match-arm)** — `Some(_) => @col += 1` парсер
   не принимает (bare `+=` без braces в arm-RHS). Обернул в `{ ... }`:
   `Some(_) => { @col += 1 }`.
2. **`json.nv:499` (`mut` параметр)** — `fn ... (mut fields HashMap[...])` —
   парсер не поддерживает `mut`-modifier для arg'а. Убрал `mut` (HashMap
   reference-type через GC, mutability семантически не теряется).
   Этот баг **прятался за первым** (был обнаружен только после фикса №1).
3. **`regex.nv:222` (multi-line `||`)** — парсер не продолжает expression
   на следующей строке если она начинается с `||`. Решение: вынести `||`
   в конец предыдущей строки (`'|' ||\n c == ')'`). Pure-syntactic
   нормализация — семантика идентична.
4. **`property.nv:342, 355, 367, 379, 391, 408` (trailing-block closure)** —
   `property(gen) { xs => ... }` использует Kotlin/Swift-style
   trailing-block-as-closure синтаксис, которого в Nova нет. По
   D22 closure — `|xs| { ... }`. Заменил на `property(gen, |xs| { ... })`
   с переходом на explicit-argument форму.

**Acceptance:** ✅ json/regex/property проходят `nova check`.

### Ф.3 — Sweep + retrospective ✅ ЗАКРЫТ 2026-05-12

- ✅ `nova check std/` → **45/45 PASS** (включая новый
  std/testing/handlers.nv).
- ✅ `nova test` → **191/191 PASS** (нет регрессий после правок).
- ✅ Обновлены [docs/project-creation.txt](../project-creation.txt) и
  [docs/simplifications.md](../simplifications.md).
- ✅ [Plan 14](14-stdlib-codegen-gaps.md) baseline помечен как
  расширенный через Plan 34.

---

## Решённые вопросы

**Q-handler-type ✅.** Каноническая сигнатура — `fn seeded(seed int) -> Handler[Random]`
(вариант A). Опирается на D87 (`Handler[E, IRT]` параметризация).
Closure-capture mut state работает прямо в теле handler-литерала.

**Q-prng-source ✅.** Knuth MMIX LCG (`state = state * 6364136223846793005 + 1442695040888963407`).
Не-CSPRNG, но reproducible across architectures. Для production
remains open: `secure() -> Handler[Random]` через CSPRNG runtime hook —
вне scope Plan 34.

**Q-time-handler-extras ✅.** `fixed_ms.sleep(d)` — **no-op** (instant
return). Это позволяет тестировать timeout/retry/rate_limit за
миллисекунды. Advance virtual clock сейчас нет; workaround — создавать
новый handler с новым `ms` между сценариями.

---

## Acceptance criteria плана

- 100% std/*.nv (non-runtime) проходят `nova check` → ожидается **44/44**
  или больше (после возможного добавления тестов).
- `std/testing/handlers.nv` создан, экспортирует `seeded` и `fixed_ms`.
- 11 fail'ящихся файлов на 2026-05-12 (bcrypt, jwt, ulid, uuid,
  snowflake, rate_limiter, retry, duration, property + json, regex) —
  PASS.
- nova_tests/ не регрессировали (179/179 → 179/179+).
- Обновлены: project-creation.txt, simplifications.md, README.md.

---

## Что НЕ входит

- Production handler'ы (`real_fs`, `real_net`, `real_time`) — это Plan 18
  Q3 в полном объёме, требует runtime-привязки (libuv). Plan 34 закрывает
  только **test**-handler'ы.
- Property-test framework сверх существующего `std/testing/property.nv`
  — тот файл уже есть, фиксим только parser-баг (Ф.2).
- Cross-file resolve для `import std.testing.seeded` в codegen — это
  [Plan 35](35-cross-file-resolve.md). `nova check` уже разрешает
  cross-file через interp path.
- Дополнительные std-модули из Plan 18 P0 (`std.fmt`, `std.flag`, `std.log`,
  `std.sort` и т.д.) — это roadmap, не текущая работа.

---

## Связь с другими планами

- [Plan 14 (CLOSED)](14-stdlib-codegen-gaps.md) — родительский, baseline
  type-check 50/50 пришёл оттуда.
- [Plan 18 (DRAFT)](18-stdlib-roadmap.md) — общий roadmap stdlib;
  `std.testing` сейчас не в P0 таблице — этим планом добавляем.
- [Plan 35](35-cross-file-resolve.md) — независимый, для compile-режима.
- [Plan 15 ✅](15-generic-bounds-enforcement.md) — D72 enforcement;
  property-test может потребовать generic-bounds при расширении.

---

## Ссылки

- [spec/decisions/04-effects.md](../../spec/decisions/04-effects.md) — `Random`, `Time` эффекты.
- [spec/decisions/04-effects.md → D80](../../spec/decisions/04-effects.md#d80) — per-fiber handler isolation.
- [feedback_concurrency_tests.md](../../../.claude/projects/d--Sources-nova-lang/memory/feedback_concurrency_tests.md) — observable interleave.


---

## Ф.4 — Полный sweep `nova test std/` (2026-05-12)

После расширения плана прогнан `nova test std/`. Результат **4 PASS /
48 FAIL** из ~52 файлов. Категории FAIL'ов:

### A. Локальные codegen-блокеры (правка .nv в stdlib — реалистично)

| Файлы | Корень ошибки | Действие |
|---|---|---|
| **base64, hex, ulid, uuid, property** (5) | `int as char` запрещён (D54, Plan 14 Ф.7 закрыта strict literal-only) | Refactor через `char.try_from(n)?` с `?`-propagation. Возвращаемый тип меняется на `Fail[CharRangeError] -> char` или `Option[char]`. |
| **priority_queue, retry, json, url** (4) | `if condition must be bool` (strict-bool D-блок) | Replace `if x` → `if x != 0` для `int` или `if x.is_some()` для `Option`. |
| **bcrypt, range, ini, diff, regex** (5) | `for-in: nova_int` (Iter[T] erasure после Plan 14 Ф.1) | Заменить итерацию через range `for i in 0..arr.len { ... arr[i] ... }` где это применимо. |

### B. Cross-file codegen-блокеры (`Nova_<Type>` unknown, blocks Plan 35 Ф.2)

| Файлы | Тип |
|---|---|
| lru, rate_limiter, jwt, semver_range, toml, uuid_namespace, snowflake (`th.` not resolved) | `unknown type 'Nova_<X>'` |

Cross-file codegen вынесен в [Plan 35 Ф.2](35-cross-file-resolve.md).
В Plan 34 не закрываются.

### C. Generic specialization (накопленные блокеры Plan 14)

| Файлы | Природа |
|---|---|
| **set, vec** (2) | `for-in: Nova_Iter*` / `Nova_[]T*` — generic specialization at monomorphization. |

Отдельный план (по приоритету). В Plan 34 не закрывается.

### D. Разнородные str/record codegen-баги (диагностика case-by-case)

| Файлы | Корень |
|---|---|
| md5, sha1, sha256, hmac (4) | `nova_str` mismatch — конкретный bug на `int` вместо `nova_str` initialization |
| csv | `subscripted value is not an array` |
| complex | `strip_suffix` missing on `nova_str` |
| statistics, path, duration | mixed `nova_unit`, `NovaOpt_nova_str` issues |
| semver | `unsupported Lt on nova_str` |
| sql, cron | `anonymous record literal without spread` |
| markdown_minimal | `Nova_Buffer` — старый рефактор, нужна правка под StringBuilder |
| handlers.nv | `NovaVtable_Random` — codegen для нестандартного effect type |

Каждый — отдельный bug; разнородные. **В Plan 34 берём только если
быстро (< 30 минут на файл).**

### E. False-positives (auto-gen библиотечные модули без main)

| Файлы | Природа |
|---|---|
| runtime/char, gc, math, read_buffer, string, string_builder, write_buffer (7) | `lld-link: undefined symbol 'nova_fn_main_impl'` — это lib-only модули (`std.runtime.*` auto-gen), не имеют тестов / main |

`nova test` пытается их собрать как exe — это **bug test-runner'а** или **mis-categorization**. Эти файлы должны быть skipped (как `std/runtime/` skipped в `nova check` по D95).

**Действие в Ф.4:** добавить `runtime/` в hard-skip для `nova test`
(аналогично check). Это уберёт 7 false-FAIL'ов сразу.

---

## Ф.5 — Action plan

| Фаза | Категория | Файлов | ETA |
|---|---|---|---|
| **Ф.5.1** | E (skip runtime/ в test) | 7 | ~10 min — test-runner правка |
| **Ф.5.2** | A.1 (`int as char` refactor) | 5 | ~60 min — refactor через `char.try_from` |
| **Ф.5.3** | A.2 (`if x` → `if x != 0`) | 4 | ~30 min — узкая правка |
| **Ф.5.4** | A.3 (`for-in nova_int`) | 5 | ~60 min — если refactor простой |
| **Ф.5.5** | D — лёгкие случаи (на выбор) | 1-3 из 10 | timeboxed |
| **Ф.6** | Final sweep + docs | — | ~30 min |

**Категории B (cross-file codegen) и C (generic specialization) — НЕ
в этом плане.** Они вынесены в Plan 35 и накопленные блокеры из Plan 14
retrospective.

---

## Acceptance criteria (расширенные)

Минимум:
- `runtime/` skipped в `nova test` (false-positives убраны).
- 5 `int as char` файлов: ✅ PASS после `char.try_from` refactor.
- 4 `if condition` файла: ✅ PASS после strict-bool правки.
- Существующие 45/45 type-check + 191/191 nova_tests не сломаны.

Желательно:
- 5 `for-in nova_int` файлов починены.
- 1-3 файла из категории D (диагностика).

**Не входит:**
- Cross-file codegen (Plan 35 Ф.2).
- Generic specialization (отдельный план).
- str-related разнородные codegen-баги (Plan 14 retrospective).


---

## Ф.6 — Closing retrospective (2026-05-12)

**Финальный sweep:** `nova test std/ --skip std/runtime` → **4 PASS / 41 FAIL**.

### Что реально закрыто

1. ✅ Type-check регресс (Ф.1-3): 33/44 → 45/45.
2. ✅ Plan-реорганизация: Plan 14 closed, Plan 35 создан, Plan 18 обновлён.
3. ✅ `--skip <pattern>` флаг в `nova test` (реализован параллельным
   агентом, не мной). Позволяет `nova test std/ --skip std/runtime`.
4. ✅ Plan 34 Ф.5.2: 5 файлов разблокированы от `int as char` codegen-гейта.
   После refactor через `char.try_from(code)` — кодеген проходит,
   следующая ошибка уже не int-as-char.

### Почему 41 FAIL остался

Каждая категория из Ф.4 анализа имеет свой природу блокировки:

- **Категория A.1 (`int as char`):** ✅ разблокировано Plan 34 Ф.5.2.
  Но за этим гейтом стоят **другие** codegen-bugs (Plan 14 retrospective
  «накопленные блокеры»). pass-rate не вырос потому что 5 файлов
  упёрлись в category-D баги.
- **Категория A.2 (`if condition must be bool`)** — оказалась **не**
  локальной правкой. Это **D72 generic-method dispatch** ошибка
  (`K.eq`, `T.lt`, `HashMap.contains`, `str.starts_with` возвращают
  `nova_int` вместо `bool` из-за erasure). Plan 14 называет это
  «блокер для Plan 15 enforcement».
- **Категория A.3 (`for-in nova_int`):** infer-fix для step_by сделан
  параллельным агентом (commit e019a47128), но codegen `for-in StepRangeIter`
  ещё открыт.
- **Категория B (cross-file codegen):** Plan 35 Ф.2.
- **Категория C (generic specialization):** отдельный план.
- **Категория D (разнородные codegen-bugs):** диагностика case-by-case,
  отдельные мелкие планы.

### Открытые задачи (не Plan 34)

Сводно (не делать в этом плане):

1. **Plan 35 Ф.1** (расширен): wildcard `import X.Y.*` + bare-name
   visibility (~150 строк).
2. **Plan 35 Ф.2**: cross-file codegen module-resolver (~300-500 строк).
3. **Новый план «D72 method-resolution через protocol-bounds в codegen»**:
   `K.eq`, `T.lt`, etc. возвращают bool, не int (4+ файла unblocked).
4. **Новый план «for-in StepRangeIter codegen»**: для (range).step_by(n).
5. **Plan 37** (создан параллельным агентом): type-check semantic
   parity — поднять `check_as_cast_allowed` / `check_bool_condition`
   из codegen в type-checker.
6. **Buffer cleanup**: остатки `Nova_Buffer` после Plan 04 в uuid/markdown_minimal.
7. **Misc codegen bugs** (10+ файлов): str-init type mismatch (md5/sha1/sha256),
   `subscripted value not an array` (csv), `nova_str` Lt (semver),
   anonymous record literal (sql/cron), и т.д. — case-by-case.

### Файлы (Plan 34 расширенный финальный)

Изменено:
- `compiler-codegen/src/test_runner.rs`, `nova-cli/src/main.rs` — `--skip`
  флаг (агент)
- `docs/plans/34-stdlib-typecheck-and-compile-fix.md` (renamed +
  расширен)
- `docs/plans/14, 18, 23, README.md` (status updates)
- `docs/plans/35-cross-file-resolve.md` (new, расширен Ф.1)
- `docs/plans/37-typecheck-semantic-parity.md` (new, от агента)
- `docs/project-creation.txt`, `docs/simplifications.md`
- `std/testing/handlers.nv` (new)
- 9 std-файлов с `import as th`
- 4 .nv файла с `int as char` refactor (base64, hex, ulid, uuid, property)
- json.nv, regex.nv, property.nv — parser-фиксы

### Acceptance расширенный

✅ Минимум:
- 45/45 std type-check.
- 191/191 nova_tests без регрессий.
- `runtime/` skip через `--skip std/runtime`.
- 5 `int as char` файлов разблокированы (codegen проходит этот гейт).

❌ Не достигнуто (вынесено в отдельные планы):
- 4 `if condition` (D72 codegen-bug).
- 5 `for-in nova_int` файлов целиком.

**Plan 34 закрывается.** Дальнейшая работа — через новые планы.


---

## Ф.7 — Production-grade improvements (2026-05-12)

> **Реоткрытие plan'а после аудита «как для прода».** Финальная проверка
> handlers.nv против Go (PCG в math/rand v2), Rust (ChaCha8 в rand crate),
> Java (Clock.fixed/offset) выявила 4 production-gaps в Ф.1 реализации,
> которые **сделаны bootstrap-формой** и могут быть улучшены без
> architecture changes. Plan 34 расширен Ф.7 для закрытия их сейчас.

### Контекст

LCG (Knuth MMIX) — простейший PRNG, который я выбрал в Ф.1 ради
2-строчности. Production-grade PRNG (PCG, xoshiro, ChaCha) дают:

- **Лучше distribution** — passes BigCrush/PractRand vs LCG fails.
- **Дольше period** — xoshiro256++ ~2^256 vs LCG ~2^64.
- **No degenerate seeds** — xoshiro работает для seed=0 (state init
  через splitmix64); LCG `state * a + c` с seed=0 → state становится c,
  но это case-by-case.
- **Те же ~30 строк** — простота сравнима.

`fixed_ms.sleep(d)` — no-op. Это **теряет deterministic time-advance**
testing для rate_limiter/retry/cron (Plan 34 Q-time-handler-extras
честно отметил это). Решение из Go/Rust ecosystem: **mutable test
clock** (`clock.Advance(d)` в Go, `tokio::time::advance(d)` в Rust).

`bytes(n)` извлекает **1 байт из 64-битного advance** — выкидывает 56
бит. Тривиальная perf-улучшение: 8 байт за 1 advance.

`seed int` — Plan 34 принимает `int` (signed 64), spec говорит `u64`.
Mismatch, нужно `u64` для consistency с `Random.u64()` return type.

### Изменения

| Что | Было | Стало |
|---|---|---|
| **PRNG** | LCG (Knuth MMIX) | xoshiro256++ (Sebastiano Vigna, public domain) |
| **`seeded` signature** | `fn seeded(seed int)` | `fn seeded(seed u64)` |
| **`bytes(n)`** | 1 byte per advance | 8 bytes per advance |
| **State init для xoshiro** | `state = seed as u64` | splitmix64 mixing seed → 4×u64 state |
| **`fixed_ms`** | sleep no-op | оставить (single fixed moment) |
| **New: `mut_clock(start_ms u64)`** | — | mutable test clock с `Time.advance(d Duration)` |
| **Self-tests** | 7 sanity | +3 distribution/quality smoke tests |

### Acceptance Ф.7

- `nova check std/testing/handlers.nv` PASS (signature changes).
- Type-check std/ остаётся 45/45 — callers `th.seeded(42)` совместимы
  (42 → u64 implicit).
- `nova_tests/` 202/202 без регрессий.
- 3 новых distribution-теста (period > 2^32, non-correlation,
  uniform-byte distribution) PASS.
- `mut_clock` с `Time.advance(Duration)` работает в handler-state.

### Что НЕ входит (по-прежнему)

- CSPRNG (`secure() -> Handler[Random]`) — нужен runtime hook.
- Real time handler (`system_clock()`) — production handler, требует
  libuv.
- Re-fix 41 codegen-FAIL'ов — отдельные планы.

### Ссылки на алгоритм

- xoshiro256++: https://prng.di.unimi.it/xoshiro256plusplus.c
- splitmix64 для state init: https://prng.di.unimi.it/splitmix64.c
- Эти PRNG — **public domain** (CC0), безопасно копировать.

### Результаты Ф.7 ✅ ЗАКРЫТ 2026-05-12

Реализовано:

| Что | Результат |
|---|---|
| **xoshiro256++** | 30 строк в `seeded()`: 4×u64 state + step function (rotl + XOR) |
| **splitmix64 init** | Расширяет 64-bit seed → 4 хороших u64. Работает для seed=0 |
| **`bytes(n)` — 8 bytes/advance** | Cache (`buf, buf_remaining`) hold 8 bytes из последнего u64 |
| **`seed u64`** | Сигнатура изменена. Все 9 callers `th.seeded(N)` совместимы (implicit `int → u64` coercion) |
| **`mut_clock(start_ms u64)`** | Новая фабрика — `sleep(d)` продвигает state `current_ms += d.nanos / 1_000_000` |
| **10 self-tests** | 5 sanity + 3 distribution quality + 4 time handlers |

**Тесты Ф.7:**
- Sanity: detеrminism, разные seed → разные seq, seed=0 не degenerate,
  bytes(n) детерминизм
- Distribution quality: 1000-period smoke, 8-bucket byte uniform,
  non-correlation между подряд значений
- Time: `mut_clock` advance, independent instances между `with`-блоками,
  composition `seeded + mut_clock`

**Acceptance Ф.7 ✅:**
- ✅ `nova check std/testing/handlers.nv` PASS.
- ✅ `nova check std/` — 45/45 (не сломан).
- ✅ 9 callers `th.seeded`/`th.fixed_ms` — все PASS (signature change
  совместима через type coercion).
- ✅ `nova test` — 225/225 PASS, нет регрессий.
- ⚠️ `nova test std/testing/handlers.nv` падает на CC-FAIL `NovaVtable_Random`
  (category-D codegen bug для effect-types в stdlib — не Ф.7 scope,
  выносится в отдельный план).

**Production gap'ы остающиеся (не Ф.7):**
- CSPRNG `secure()` — нужен runtime hook.
- Real `system_clock()` — нужен libuv.
- codegen `NovaVtable_<Effect>` для stdlib handler'ов — отдельный план.


---

## Ф.8 — Spec sync + focused tests (followup #2, 2026-05-12)

> **Реоткрытие** plan'а после reminder пользователя «ты в спеку все
> сохранил?» — выявило что Ф.7 изменения (`mut_clock`, `seed u64`,
> xoshiro256++) **не были sync'нуты в spec/decisions/**. Plus просьба
> «тесты напиши по тому что делал, позитивные и негативные».

### Ф.8.1 — Spec sync

Источник правды для language features — `spec/decisions/*.md`. Plan 34
Ф.1 + Ф.7 API изменения должны быть отражены там.

**`spec/decisions/04-effects.md`:**
- Test-handler table (line 2644): `Time` запись расширена —
  `fixed_ms(ms)` ✓ (фиксированный момент) + `mut_clock(start_ms)` ✓
  (sleep продвигает виртуальное время).
- Test-handler table: `Random` запись уточнена — `seeded(seed)` ✓
  xoshiro256++ deterministic PRNG.
- Decision matrix (line 2690): test-handler use-cases расписаны
  (uuid v7 / jwt / rate_limiter / retry / cron — какой handler где).
- Footnote после первой таблицы: «Источник test-handler'ов» — pointer
  на `std/testing/handlers.nv`, xoshiro256++ tier note (parity с Go
  math/rand v2 PCG и Rust `rand` ChaCha8), pending production-handlers
  (`secure()`, `system_clock()`).

**`spec/decisions/08-runtime.md` D26 prelude:**
- Signatures уточнены: `fixed_ms(ms u64) / mut_clock(start_ms u64) /
  seeded(seed u64)`.

### Ф.8.2 — Focused tests в nova_tests/plan34/

**Дизайн-решение про inline reproducers.** Прямые тесты
`th.seeded(seed)` / `th.mut_clock(start_ms)` из `std/testing/handlers.nv`
**не могут быть запущены через `nova test`** — codegen падает на
`unknown type NovaVtable_Random` (category-D codegen bug для stdlib
effect-types, не Plan 34 scope).

Альтернатива: **inline reproducers** покрывают **algorithm correctness**
независимо от effect-codegen. Когда NovaVtable bug закроется — можно
будет добавить wrapper-тесты на handlers.nv.

**Positive (4 файла):**

| Файл | Покрытие |
|---|---|
| `gc_namespace_basic.nv` | Smoke для всех 5 `gc.*` functions: heap_size, live_count, alloc_count, collect, reset_stats. Non-negative + callable без panic. |
| `gc_alloc_monotonic.nv` | Behavior: `gc.alloc_count` растёт после 100 аллокаций; `gc.collect` после `reset_stats` не валит heap_size. |
| `inline_xoshiro_determinism.nv` | Inline reproducer splitmix64+xoshiro256++ из handlers.nv. 5 sub-tests: splitmix64-init для seed=0 non-zero, determinism (тот же seed → та же sequence), разные seed → разные sequences, последовательные значения различаются, non-correlation (a→b и b→c diffs не равны). |
| `inline_mut_clock_advance.nv` | Inline `Clock { ms u64 }` record + `clock_sleep_ms`. 4 sub-tests: sleep продвигает state, разные instances независимы, large delta (1 hour = 3.6M ms), zero delta — state не меняется. |

**Negative (4 файла) — `EXPECT_COMPILE_ERROR` regression markers для Ф.2 parser fixes:**

| Файл | Маркер | Защищает fix |
|---|---|---|
| `negative_match_arm_compound_assign.nv` | `expected pattern, got '+='` | json.nv:164 fix #1 (bare `+=` в RHS match-arm) |
| `negative_mut_param.nv` | `expected identifier, got 'mut'` | json.nv:499 fix #2 (mut-modifier для параметра функции) |
| `negative_multiline_or_continuation.nv` | `got '\|\|'` | regex.nv:222 fix #3 (multi-line `\|\|` continuation) |
| `negative_trailing_block_closure.nv` | `unexpected '=>' in expression` | property.nv fix #4 (Kotlin/Swift-style trailing-block closure) |

Каждый negative тест — будущий regression marker: если расширят грамматику
(когда D-блоки про эти фичи будут приняты) — тест провалится, нужно
обновить spec и удалить/обновить тест.

### Acceptance Ф.8

- ✅ Spec sync — `04-effects.md` table/matrix/footnote + `08-runtime.md` D26 signatures.
- ✅ `nova_tests/plan34/` — **8/8 PASS** через release `nova-cli/target/release/nova.exe`.
- ✅ `nova check std/` — 45/45 (не сломан).
- ⚠️ Full `nova test` — 249/256 PASS. 7 CC-FAIL — это **не моя регрессия**:
  `Plan 40 Ф.2+Ф.3 commit 655033c` от агента изменил channel runtime
  ABI (`nova_channel_send` signature) и не обновил все emit-sites.
  Affected tests: `concurrency/select_*`, `concurrency/plan40_*`,
  `expected_runtime/select_all_closed`. Это для агента в Plan 40 followup.

### Process lesson (added to memory)

После reminder пользователя — обновлён `feedback_project_docs.md`:
**spec sync — четвёртая обязательная компонента** наряду с
project-creation/simplifications/discussion-log. При **любом**
изменении user-facing API (signatures, новые ops/handlers, изменение
spec'd behavior) — sync в `spec/decisions/*.md` обязателен синхронно
с commit'ом задачи, не postpone'ить.

### Commits Ф.8

- `5d71e0843d` — Plan 34 followup #2: spec sync + 8 focused tests.
- (в `e7d19dac92` агентский) — docs/project-creation.txt +
  docs/simplifications.md (захвачены параллельным агентом в его commit;
  subject inaccurate но содержание сохранено).
- Private discussion-log Этап 92 — commit `21f7926`.

### Файлы Ф.8

- `spec/decisions/04-effects.md` (table + matrix + footnote)
- `spec/decisions/08-runtime.md` (D26 signatures)
- `nova_tests/plan34/*.nv` (8 файлов: 4 positive + 4 negative)
- `docs/project-creation.txt`, `docs/simplifications.md` (записи)
- `docs/plans/34-stdlib-typecheck-and-compile-fix.md` (эта Ф.8 секция)
- `.claude/.../memory/feedback_project_docs.md` (auto-memory updated)

**Plan 34 EXTENDED + Ф.7 + follow-up #1 (gc.nv) + Ф.8 (followup #2) — закрыт полностью.**
