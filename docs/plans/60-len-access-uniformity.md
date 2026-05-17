# Plan 60: size-accessor uniformity (`.len()` / `.is_empty()` / `.cap()` — method-only across all collections)

> **Status:** proposed (2026-05-17, revised). Production-grade unification, не quick clean-up. Реализует **D-блок D112** (новый) — «size-like accessors всегда method-вызов».

---

## Цель в одной фразе

Сделать так, чтобы для **любого** container'а (built-in array `[]T`, built-in `str`, любой user-type — `HashMap`, `Lru`, `Set`, `Range`, `Deque`, `Queue`, `Vec` и т.д.) запись `c.len()` была единственным легальным способом узнать размер, а `c.len` без скобок — compile-error с machine-applicable fix-it. То же — для `cap`, `byte_len`, `is_empty`.

Желаемое состояние = Rust (где `vec.len()`, `slice.len()`, `str.len()` — все method'ы, и никакого field-access нет). Альтернативный подход — TS-style property (no parens) — мы **сознательно отвергаем**, потому что Nova spec [syntax.md:820](../../spec/syntax.md#L820) и [D-блок computed property](../../spec/decisions/03-syntax.md) явно запрещают «property с эффектами / O(n) скрытой стоимостью» (см. §«Открытые вопросы», п. 5).

---

## Problem (что не так сейчас — verified)

### Inconsistency по типам

| Тип | `.len` | `.is_empty` | `.cap` / capacity | Источник |
|---|---|---|---|---|
| `[]T` (built-in array) | **field** | **field** (D38 «built-in sugar») | — | `emit_c.rs:9159-9165` |
| `str` (built-in string) | **field + method** (оба пути в codegen → `nova_str_char_len`) | **field + method** | — | `emit_c.rs:9155-9170`, `runtime_registry.rs:67` |
| `StringBuilder` / `WriteBuffer` | method `@len()` | method | — | `runtime_registry.rs:658, 805` |
| `HashMap`, `Set`, `Lru`, `Deque`, `Queue`, `Range`, `Vec` | method `@len()` | method `@is_empty()` | `cap()` для HashMap | std/* |

Это **точное соответствие Java-патологии** — массивы field, коллекции method. Хуже Rust (всё method), хуже TS (всё property), хуже Go (`len(x)` builtin для всего одинаково).

### Прямой подсчёт occurrences (regex `\.len(?!\()`, PCRE2)

| Зона | Field-style `.len` | Method-style `.len()` |
|---|---|---|
| `std/` | **290** | 33 |
| `nova_tests/` | **115** | 136 |
| `spec/` | **34** | — |
| `docs/` | **52** | — |
| `examples/` | **27** | — |
| **Total** | **~518** | ~170 |

План 60-v1 заявлял ~280 — занижено почти в 2×. Реальный migration scope — ~518 lines.

### Прочие size-accessors

| Accessor | Field в кодовой базе | Method в кодовой базе | Inconsistency? |
|---|---|---|---|
| `.cap` | 1 (`std/collections/hashmap.nv:145`) | 0 | **да** (одна inconsistency) |
| `.byte_len` | 0 | 31 (tests) | нет |
| `.size` | 0 | (user-types) | нет |
| `.count` | 0 | (user-types) | нет |
| `.is_empty` | **(D38 built-in field-sugar для `[]T` и `str`)** | повсеместно для user-types | **да** (built-in vs user) |
| `.empty` | 0 | 0 | n/a (не существует) |
| `.first` / `.last` | 0 | 6 | нет |

**Реальные точки рассинхрона:** `.len` (массово), `.cap` (1 случай), `.is_empty` (D38 sugar). Остальное уже consistent.

### Симптом для пользователя / LLM

```nova
fn report(vec []int, map HashMap[str, int]) -> () {
    println("vec  ${vec.len}")        // works (field)
    println("map  ${map.len}")        // compile error — нужно .len()
    println("vec? ${vec.is_empty}")   // works (D38 sugar)
    println("map? ${map.is_empty}")   // compile error
}
```

LLM-сгенерированный код регулярно перемешивает обе формы — это попадает на ревью, ломает CI, требует ручного fix. AI-first language такого допускать не должен (см. spec/overview.md §«Killer use-case»).

---

## Что НЕ делаем (rejected alternatives)

| Alternative | Почему отвергнут |
|---|---|
| **Field-style для всех типов** | Не выразимо для user-types — HashMap внутри `_count` + invariant'ы; exposing field ломает encapsulation. |
| **TS/Swift-style property (no parens)** | Противоречит [syntax.md:820](../../spec/syntax.md#L820) «скобки обязательны для вызова». Принципиальное решение Nova — predictable cost: `()` = «здесь происходит вычисление, возможно O(n)». Property-syntax спрячет O(n) `nova_str_char_len`. |
| **`len(x)` builtin (Go-style)** | Грязная зона — добавляет global-function-namespace конфликт с user-types; не работает с method-chaining `vec.map(f).len()`; противоречит D29 «один способ» (уже есть method-call syntax). |
| **Migration через feature-flag / GrowthBook** | Compile-error даёт мгновенный, видимый migration signal — ровно тот случай, когда flag вреден. |
| **Оставить как есть** | Java-style inconsistency. Минимум одна public-API ошибка, видная LLM с первой строки stdlib. AI-first → unacceptable. |

---

## Решение

### Принцип

Один **D-block D112** (см. §«Spec changes»): для любого типа `T` любой size-accessor (`len`, `cap`, `byte_len`, `is_empty`, плюс будущие `count`, `size` если они появятся как built-in) — **только** через method-call `t.accessor()`. Запись `t.accessor` (без скобок) разрешена **только** как bound method value (`fn() -> int`), что — по convention'у Nova — почти всегда user error. Compiler выдаёт error + fix-it, кроме явно arg-position контекстов где method-value легитимен.

### Codegen changes (compiler-codegen)

1. **Регистрация `[]T.@len()` / `[]T.@cap()` / `[]T.@is_empty()`** в `runtime_registry.rs`:
   - `[]T.@len() -> int` — lowers в `(arr->len)`, zero-cost; emits как inline expression, не C-fn.
   - `[]T.@cap() -> int` — lowers в `(arr->cap)`, zero-cost (точно так же).
   - `[]T.@is_empty() -> bool` — lowers в `((arr->len) == 0)`, zero-cost.
   - Receiver type — `[]T` (any T); реализуется через special-case на receiver-pattern (как уже сделано для `str` в registry).

2. **Регистрация `str.@is_empty() -> bool`** — lowers в `((s.len) == 0)` через `nova_str_byte_len(s) == 0` (O(1) проверка байтового размера; byte-length == 0 ⇔ codepoint-length == 0 для UTF-8).
   `str.@len()` / `str.@byte_len()` уже есть.

3. **Удаление field-access lowering** в `emit_c.rs`:
   - **9155-9157**: `str.len` (field) → удалить, оставить только method-path (15865).
   - **9159-9161**: `NovaArray.len` (field) → удалить.
   - **9163-9165**: `NovaArray.is_empty` (D38 field-sugar) → удалить.
   - **9167-9170**: `str.is_empty` (field) → удалить.
   - **12634-12636** (print-path type inference): удалить branch для field-access.
   - **17405-17414** (general type inference): удалить hard-coded field type для `len/is_empty`.

4. **Diagnostic в type-checker** на field-access `T.accessor` где `T` имеет одноимённый метод и accessor ∈ {`len`, `cap`, `byte_len`, `is_empty`, `count`, `size`}:
   ```
   error[E0xxx]: size-like accessor `len` is method-only
     --> file.nv:42:23
        |
   42  |     println("${vec.len}")
        |                    ^^^ help: append `()` — `vec.len()`
        |
        = note: bare `.len` is bound method value `fn() -> int`,
                rarely intended in argument position
        = see Plan 60 / D112 for migration rationale
   ```
   Diagnostic emit'ится в **type-checker**, не codegen — соответствует [Plan 37](37-typecheck-semantic-parity.md) принципу «семантические ошибки в type-checker, codegen не fallback».

5. **Method-value form `let f = vec.len` (без скобок)** — намеренно остаётся легальной (D-блок method-values Plan 11). Diagnostic уровня **warning**, не error, в **non-argument-position** контекстах — потому что иногда method-value легитимен (`fns.map(.len)` style API). См. §«Open questions» п. 1.

### Spec changes

**Новый D-блок D112** в [spec/decisions/03-syntax.md](../../spec/decisions/03-syntax.md):

> **D112. Size-like accessors require call syntax.** Для любого типа `T` методы, возвращающие размер/cardinality/capacity (`len`, `cap`, `byte_len`, `size`, `count`, `is_empty`), вызываются ТОЛЬКО как `t.method()`. Запись `t.method` (без скобок) допустима как bound method value, но компилятор выдаёт diagnostic level warning в non-arg-position, и error в argument-position где expected тип не `fn() -> T`. Rationale: predictable cost (см. D29 — Nova никогда не скрывает вычисление за property-syntax); consistency между built-in (`[]T`, `str`) и user-defined (`HashMap`, `Set`) collections; AI-friendly (LLM не должен запоминать «для какого типа какая форма»).

**Amend D38** в `spec/decisions/03-syntax.md` — удалить «built-in sugar `.is_empty` field-access for `[]T` / `str`» (это и был источник inconsistency).

**Amend D32** в `spec/decisions/02-types.md` — добавить:
> Поля `len` и `cap` структуры array (`(ptr, len, cap)`) **не exposed** в user-language как field-access. Доступ исключительно через method calls `@len()` / `@cap()`, lower'ящиеся в zero-cost field-read.

**Amend D26** в `spec/decisions/08-runtime.md` — обновить список prelude-методов:
- Добавить `[]T.@len() -> int`, `[]T.@cap() -> int`, `[]T.@is_empty() -> bool`, `str.@is_empty() -> bool` в зафиксированный prelude-API.

### Migration

Поскольку это **compile-error** (после удаления field-path в codegen + добавления diagnostic), миграция должна быть **mechanical и atomic в одном PR** — иначе CI падает между коммитами. Подход:

1. **Phase A (preparation, no behaviour change):** добавить method'ы в registry, оставить field-path active. Все тесты PASS.
2. **Phase B (auto-migration script):** Rust-утилита `nova-cli/src/bin/migrate_plan60.rs` (новый bin, не публичный — только для этой миграции):
   - Parse Nova-файл через существующий compiler-codegen lexer/parser (re-use AST).
   - Find `MemberAccess { obj, name }` где `name ∈ {len, cap, byte_len, is_empty}` И где expr используется НЕ в method-value контексте (т.е. не `let f = x.len` и не `fns.map(.len)`).
   - Re-emit с `()` append.
   - **Не** trust regex — он даёт false-positives (`let len = 0` локальные).
   - Verify roundtrip: parse → modify → emit → reparse → AST equiv (modulo added call).
   - Применить ко всем директориям: `std/`, `nova_tests/`, `examples/`, `spec/` (markdown code-blocks через extraction), `docs/`.
3. **Phase C (atomic switch):** в одном коммите — удалить field-path lowering в emit_c.rs, добавить type-checker diagnostic, прогнать `nova test` (должно PASS после Phase B).
4. **Phase D (post-migration verification):** прогнать `grep -rn '\.\(len\|cap\|byte_len\|is_empty\)\b\([^(]\|$\)' std/ nova_tests/ examples/` — 0 hits ожидается (кроме legitimate method-value cases, заwhite-list'енных).

### Migration of `.cap` (1 случай)

`std/collections/hashmap.nv:145` — `@_buckets.cap` → `@_buckets.cap()`. Тривиально.

### Migration of D38 `.is_empty` sugar

Глобально `arr.is_empty` → `arr.is_empty()`. Auto-migration script покрывает.

---

## Sub-tasks (фазы)

### Ф.0 — Audit baseline (½ day)

- [ ] Прогнать `nova test` на main. Зафиксировать baseline (557 PASS).
- [ ] Прогнать regex-audit: count'ы `.len` / `.cap` / `.byte_len` / `.is_empty` по зонам std/, nova_tests/, spec/, docs/, examples/. Записать exact counts (для Acceptance §«Stability»).

### Ф.1 — Registry additions (1 day)

- [ ] `runtime_registry.rs`: add `[]T.@len`, `[]T.@cap`, `[]T.@is_empty`, `str.@is_empty`.
- [ ] `emit_c.rs`: method-dispatch для этих методов lowers в zero-cost expressions (mirror'ит существующий field-path, но через method-call path).
- [ ] Regression: `nova test` — 0 fails (field-path всё ещё active как fallback).
- [ ] Smoke: один тест `nova_tests/plan60/methods_zero_cost.nv` — `let a = [1,2,3]; assert a.len() == 3 && !a.is_empty()`. PASS под Plan 60 codegen.

### Ф.2 — Auto-migration tool (2 days)

- [ ] `nova-cli/src/bin/migrate_plan60.rs` — Rust binary, использует `nova_codegen::lexer` + `parser` (already lib-exposed).
- [ ] Walk AST, find target MemberAccess (with whitelisting non-rewritable contexts).
- [ ] Re-emit Nova source (preserves comments, formatting — через `ropey`-style token-level rewrite поверх original bytes; **не** AST→source pretty-print, чтобы избежать reformatting noise).
- [ ] Dry-run mode (`--dry-run`) для review. Real run на `std/`, `nova_tests/`, `examples/`.
- [ ] Spec / docs migration — отдельный markdown-aware sub-tool: extract ```nova blocks, run main tool, re-inject. Plus simple sed-style для inline `\`code\`` ссылок.
- [ ] Run migration. Verify: `nova test` — 0 fails.

### Ф.3 — Atomic switch (1 day)

- [ ] `emit_c.rs`: remove field-path lowering для `.len/.cap/.is_empty/.byte_len` на `[]T`/`str`. Type-inference branches тоже remove.
- [ ] Type-checker: add diagnostic `E_SIZE_ACCESSOR_FIELD` с fix-it suggestion.
- [ ] Regression: `nova test` — 0 fails (после Ф.2 миграция уже завершена).
- [ ] Negative tests `nova_tests/plan60/bare_len_field_rejected.nv`, `bare_is_empty_rejected.nv` — оба с `// EXPECT_COMPILE_ERROR size-like accessor .* is method-only`.

### Ф.4 — Method-value disambiguation (1 day)

- [ ] В **argument-position** где expected тип НЕ `fn() -> T`, bare `.len` → error «expected `int`, found `fn() -> int` (bound method value). help: append `()`». Это даёт fix-it хороший AI-feedback signal.
- [ ] В **non-argument-position** (`let f = x.len`) → warning «bound method value of size-accessor; usually `.len()` intended».
- [ ] Whitelist для legitimate cases: `fns.map(.len)` — здесь method-value легитимен. Distinguishable по expected-type: если context требует `fn() -> T`, no warning.

### Ф.5 — Spec sync + idiom docs (½ day)

- [ ] D112 new block в `spec/decisions/03-syntax.md`.
- [ ] D38 amend (remove built-in sugar wording).
- [ ] D32 amend (поля array не exposed).
- [ ] D26 amend (prelude API methods list).
- [ ] `docs/idioms/size-accessors.md` (new): convention, when to use method-value, examples.
- [ ] `docs/migration/plan-60.md`: для пользователей внешних проектов (когда они появятся) — что изменилось, fix-it snippet.

### Ф.6 — Cross-reference cleanup (½ day)

- [ ] grep всех `*.md`, `*.nv` для `.len` field-style — 0 hits.
- [ ] CI gate: `nova check std/ nova_tests/ examples/` — clean.
- [ ] Update README, getting-started examples.

**Total: 5-6 dev-days** (увеличено с 2-4 из v1 — реальный scope ~518 occurrences, +auto-migration tool +diagnostic infra +spec sync 4 D-blocks).

---

## Acceptance criteria (production-grade)

### Корректность

- [x] `arr.len` (field) → compile error E_SIZE_ACCESSOR_FIELD с fix-it `arr.len()`.
- [x] `str.len` (field) → то же.
- [x] `arr.is_empty` (field) → то же.
- [x] `arr.cap` (field) → то же.
- [x] `arr.len()` (method) → zero-cost C-code idiomatic: `(arr->len)` без function-call overhead. Verified through emitted-C inspection в `nova_tests/plan60/zero_cost_check.nv` + grep emitted.c для `nova_array_len_helper(` (должно быть 0 hits).
- [x] `str.len()` → `nova_str_char_len(s)` (existing path).
- [x] `let f = arr.len` (method-value) — legal, warning level.
- [x] `fns.map(.len)` (method-value в expected fn-context) — legal, no warning.

### Migration completeness

- [x] grep `\.len\b[^(]` в `std/`, `nova_tests/`, `examples/`, `spec/`, `docs/` → 0 hits в Nova-code-blocks (false-positives типа `length` отфильтрованы).
- [x] grep `\.is_empty\b[^(]` → 0 hits.
- [x] grep `\.cap\b[^(]` → 0 hits (1 → 0).
- [x] grep `\.byte_len\b[^(]` → 0 hits (уже 0; defensive).

### Stability

- [x] `nova test` — 562 PASS / 0 FAIL после Plan 59 baseline (или текущий baseline + 0 регрессий + Plan-60-specific tests).
- [x] Cross-toolchain: PASS на Clang (default), MSVC, GCC (см. [Plan 58](58-cross-toolchain-msvc-verification.md)).
- [x] Performance: `nova bench` (Plan 57) для `arr.len()` в hot loop — ≤ 1% regression vs field-access. Если больше — inlining bug в codegen, fix перед merge.

### Diagnostic quality

- [x] Diagnostic для bare `.len` включает: (a) sample fix snippet, (b) link на D112 / Plan 60, (c) note про bound method value, (d) span подсвечивает имя метода.
- [x] При наличии Plan 36.D ergonomics — `nova explain E_SIZE_ACCESSOR_FIELD` показывает full migration rationale.

### Spec parity

- [x] D112 принят в `spec/decisions/03-syntax.md`.
- [x] D38 amended (built-in sugar wording removed).
- [x] D32 amended (array fields non-exposed).
- [x] D26 amended (prelude API).
- [x] `docs/migration/plan-60.md` опубликован.

---

## Open questions

1. **Method-value form `let f = x.len`** — error или warning?
   **Decision (proposed):** warning. Argument-position где expected `int` — error (с fix-it). Non-argument-position с explicit type annotation `let f fn() -> int = x.len` — silent (программист знает что делает). Иначе warning.

2. **`vec.first` / `vec.last`** (option-returning) — тоже подпадают под D112?
   **Decision (proposed):** **нет** — это не size-accessors, это element-accessors. Семантика разная (могут возвращать `Option[T]`, могут throw — D112 только про cardinality). Оставить как method-only по convention'у, но без жёсткого D-block enforcement.

3. **`vec.length` (alias)** — не вводить? `vec.size`?
   **Decision (proposed):** **не вводить**. Plan 60 — про consistency существующего, не про bikeshedding. `.len()` everywhere, никаких aliases. Spec D29 «один способ».

4. **`for i in 0..arr.len()` — расширяет ли syntactic noise?**
   **Decision (proposed):** да на 2 символа, нет семантически. Альтернатива `for x in arr` (D58 Iter[T]) уже доступна и предпочтительна. Loop с индексом — anti-pattern в Nova; D112 это **усиливает**, что хорошо.

5. **TS-style property syntax — закрытая дверь навсегда?**
   **Decision (proposed):** да, для bootstrap. Через 2-3 года, после стабилизации языка, можно вернуться к D-block «const-property» (computed property с compile-time гарантией purity + O(1)), но **не сейчас** — слишком много неопределённости в effect-system interaction. Запись в `spec/open-questions.md` как Q-const-property.

6. **`.len` для тип-параметрических контейнеров с bound `Sized`?**
   Не нужно — Nova не имеет `Sized` bound (D32 layout фиксирован для всех типов). N/A.

---

## Связь с другими планами

- **[Plan 11](11-method-values-and-overload.md)** — закрыт. Plan 60 использует method-overload infrastructure для регистрации `[]T.@len()` без special-case в emit_c.rs.
- **[Plan 37](37-typecheck-semantic-parity.md)** — Plan 60 диагностика живёт в type-checker (НЕ в codegen). Plan 60 — пример «семантические ошибки в type-checker», который Plan 37 хочет систематизировать.
- **[Plan 45](45-nova-doc.md)** — после Plan 60 stdlib doc-comments обновятся consistent `.len()` ссылками. `nova doc` extractor должен использовать `.len()` форму в `Examples:` секциях.
- **[Plan 56](56-vtable-dispatch-erased-generics.md)** — registry methods `[]T.@len()` подхватываются bound-K dispatch tables, если когда-нибудь нужен будет erased-generic вызов size-method. Не блокер для Plan 60.
- **[Plan 57](57-perf-benchmark-infrastructure.md)** — `nova bench` для `len-in-hot-loop` микро-бенч; см. Acceptance §«Performance».
- **[Plan 58](58-cross-toolchain-msvc-verification.md)** — Plan 60 включается в cross-toolchain matrix gate (zero-cost lowering должен работать одинаково под Clang/MSVC/GCC).

---

## Сравнение с state-of-the-art

| Language | Array size | String size | Map size | Inconsistency? |
|---|---|---|---|---|
| **Rust** | `vec.len()` method | `s.len()` method (bytes) | `map.len()` method | none — D112-like |
| **Go** | `len(slice)` builtin | `len(s)` builtin (bytes) | `len(map)` builtin | none — функция everywhere |
| **TS** | `arr.length` property | `s.length` property (UTF-16) | `map.size` property | none — property everywhere |
| **Swift** | `arr.count` property | `s.count` property | `dict.count` property | none — property everywhere |
| **Java** | `arr.length` field | `s.length()` method | `m.size()` method | **inconsistent** (Nova сейчас) |
| **Python** | `len(arr)` builtin | `len(s)` builtin | `len(m)` builtin | none |
| **C++** | `vec.size()` method | `s.size()` method | `m.size()` method | none |

**Nova после Plan 60 = Rust паритет.** Best-in-class: AI-first language с predictable cost + consistency.

Уникальное преимущество Nova над Rust: D112 — это **explicit spec'ed contract**, в Rust это implicit convention (никакого rustc-error если ты определишь field `len` на своём struct). LLM, читающий `spec/decisions/03-syntax.md#D112`, имеет однозначный сигнал; LLM в Rust-проекте может ошибиться и определить публичное поле `len`.

---

## Ссылки

- [spec/syntax.md:820](../../spec/syntax.md#L820) — «Скобки обязательны для вызова».
- [spec/decisions/02-types.md#d32](../../spec/decisions/02-types.md#d32) — array layout `(ptr, len, cap)`.
- [spec/decisions/03-syntax.md#d38](../../spec/decisions/03-syntax.md#d38) — built-in sugar (будет amended).
- [spec/decisions/08-runtime.md#d26](../../spec/decisions/08-runtime.md#d26) — prelude API.
- [spec/decisions/history/rejected.md:464](../../spec/decisions/history/rejected.md#L464) — per-field export rejected.
- [compiler-codegen/src/codegen/emit_c.rs:9155-9170](../../compiler-codegen/src/codegen/emit_c.rs#L9155) — текущий field-path lowering.
- [compiler-codegen/src/codegen/runtime_registry.rs:60-90](../../compiler-codegen/src/codegen/runtime_registry.rs#L60) — registry pattern.
- [docs/plans/11-method-values-and-overload.md](11-method-values-and-overload.md) — method-value semantics.
- [docs/plans/37-typecheck-semantic-parity.md](37-typecheck-semantic-parity.md) — где diagnostic живёт.
