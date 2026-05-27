# Решения по дизайну Nova

Журнал принятых решений по дизайну языка с обоснованиями.

> **Структура.** Раньше всё лежало в одном файле `decisions.md` (~6000
> строк, 48 D-решений по хронологии). Теперь разбито по темам — каждый
> файл описывает одну область, решения внутри расположены логически
> (от общего к частному), а не хронологически.
>
> История эволюции (что менялось, что отменялось) — в `history/`.

## Тематические разделы

| # | Файл | Что внутри | D-решения |
|---|---|---|---|
| 01 | [01-philosophy.md](01-philosophy.md) | Цели, парадигма, AI-first | D1, D9, D10 |
| 02 | [02-types.md](02-types.md) | Record, sum-type, protocol, generic, поля, bounds, ghost state, hybrid dispatch, tuple mono, symmetry decl↔literal | D15, D17, D32, D36, D39, D42, D52, D53, D55, D66, D72, D110, D119, D122, D123, D142 |
| 03 | [03-syntax.md](03-syntax.md) | Объявления, литералы, операторы, методы, парсинг, defer/errdefer, атрибуты `#name`, default generics, select, named params, map-literal, doc-comments, size accessors, static-dot в protocol | D16, D19, D20, D22, D23, D27, D30, D33, D34, D35, D37, D38, D40, D43, D44, D45, D46, D48, D49, D54, D58, D59, D60, D69, D82, D83, D88, D90, D94, D96, D102, D104, D108, D117, D143 |
| 04 | [04-effects.md](04-effects.md) | Fail, Io, Db, effect-литерал, with-блоки, interrupt, forbid, realtime, ?, `Effect[E, IRT]`, contracts (#pure/axiom/trusted), axiom binder, Fail payload | D2, D3, D4, D11, D12, D18, D25, D28, D31, D61, D62, D63, D64, D65, D67, D68, D85, D86, D87, D115, D118, D120 |
| 05 | [05-memory.md](05-memory.md) | Managed GC, escape analysis, regions | D6, D21 (cancelled) |
| 06 | [06-concurrency.md](06-concurrency.md) | Fiber runtime, structured concurrency, spawn, detach, supervised(cancel:), channels (Channel revision capability-split), select, handler scoping, park/wake API, implicit main-scope, fiber stack allocation, work-stealing scheduler, preemption | D14, D50, D71, D75, D79, D80, D91, D92, D93, D97, D98, D103 |
| 07 | [07-modules.md](07-modules.md) | Модули, импорты (включая селективный `import X.{A, B}` и `export import` re-export), видимость, package tooling, `_module.nv`, межпакетные зависимости, version-диапазоны, effect-aware deps | D5, D29, D47, D78, D100, D138, D139, D140 |
| 08 | [08-runtime.md](08-runtime.md) | Panic, capability, deployment, prelude, From/Into, TryFrom, math, Mem, assert, primitive built-ins (hash/eq/ord), примитивы доступа к памяти | D7, D13, D26, D41, D70 (replaced → D73), D73, D74, D76, D77, D81, D109, D141 |
| 09 | [09-tooling.md](09-tooling.md) | Тесты, контракты, форматирование, CLI, EXPECT-маркеры, conditional compilation, `#doc`, doc-attrs, doc-tests, JSON schema, contracts verifier (assume/quantifiers/cache/Z3), bench DSL | D24, D89, D95, D99, D101, D105, D106, D107, D111, D112, D113, D114, D116, D121 |
| 10 | [10-overloading.md](10-overloading.md) | Перегрузка функций и методов: четыре оси, резолв | D84 |

### Свежие D-решения (по нумерации)

| D# | Файл | Что |
|---|---|---|
| D85 | 04-effects.md | Операторы `?` и `!!` — унифицированное поведение для `Result` и `Option` |
| D86 | 04-effects.md | `??` coalesce-оператор — fallback для `Result`/`Option` без `Fail` |
| D87 | 04-effects.md | `Handler[E, IRT]` — параметризация handler типом interrupt'а |
| D88 | 03-syntax.md | Default-значения generic-параметров (`Handler[E]` ≡ `Handler[E, never]`) |
| D89 | 09-tooling.md | Test-tooling конвенции — `EXPECT_*` маркеры для negative-тестов |
| D90 | 03-syntax.md | `defer` и `errdefer` — scope-level cleanup statements |
| D91 | 06-concurrency.md | Channel revision — capability-split на `ChanWriter` / `ChanReader` |
| D92 | 06-concurrency.md | Top-level `main` как implicit supervised scope |
| D93 | 06-concurrency.md | Park/wake — нормативный runtime primitive для блокирующих операций |
| D94 | 03-syntax.md | `select { ... }` — multiplexed channel operations |
| D95 | 09-tooling.md | CLI path конвенции — `nova check <path>` / `nova test <path>` |
| D96 | 03-syntax.md | Синтаксис атрибутов — `#name` без квадратных скобок (`#realtime`, `#pure`) |
| D97 | 06-concurrency.md | Fiber stack allocation — per-thread mmap arena (Linux/macOS) / calloc (Windows) |
| D98 | 06-concurrency.md | Work-stealing scheduler — per-worker runqueue, global queue fallback |
| D99 | 09-tooling.md | Conditional compilation — `#if platform(...)` / `#if feature(...)` атрибуты |
| D100 | 07-modules.md | `_module.nv` — entry-point файл для folder-module |
| D101 | 09-tooling.md | `#doc` атрибут — structured documentation comments |
| D102 | 03-syntax.md | Named parameters — `f(x: 1, y: 2)` синтаксис вызова |
| D103 | 06-concurrency.md | Preemption — cooperative yield-points в fiber tight loops |
| D104 | 03-syntax.md | Синтаксис doc-comment'ов — `///` outer, `//!` inner |
| D105 | 09-tooling.md | Doc-атрибуты — `#doc(section = "...", example = "...")` |
| D106 | 09-tooling.md | Семантика doc-test'ов — `nova doc --test` |
| D107 | 09-tooling.md | JSON output schema v1 — `nova doc --json` |
| D108 | 03-syntax.md | Map-literal `[k: v]` синтаксис + map-coercion (D55 rev) |
| D109 | 08-runtime.md | Встроенные методы примитивных типов — hash, eq, ord |
| D110 | 02-types.md | Ghost state — spec-only bindings для SMT-верификации |
| D111 | 09-tooling.md | `assume` / `assert_static` / `#trusted` external |
| D112 | 09-tooling.md | Bounded quantifiers (`forall`/`exists` по коллекции) |
| D113 | 09-tooling.md | `#must_verify_module` — strict mode на модуле |
| D114 | 09-tooling.md | SMT cache + parallel verification |
| D115 | 04-effects.md | Axiom binder — `BinderType` enum вместо `Option<TypeRef>` |
| D116 | 09-tooling.md | Z3 backend через собственные FFI-биндинги |
| D117 | 03-syntax.md | Size-like accessors require call syntax (`.len()` vs `.len`) |
| D118 | 04-effects.md | Typed `Fail[E]` codegen — payload preservation via fail-frame |
| D119 | 02-types.md | Method-level type parameters в generic methods |
| D120 | 04-effects.md | `#pure` views + axioms + `#verify`/`#trusted` handlers |
| D121 | 09-tooling.md | Benchmark DSL — `bench "..." { measure { ... } }` + `bench.*` namespace |
| D122 | 02-types.md | Hybrid dispatch для bound-K methods (vtable infra) |
| D123 | 02-types.md | Tuple monomorphization — per-concrete-combo structs |
| D124 | 06-concurrency.md | Monotonic vs Timestamp — раздельные типы для wall-clock и монотонных часов |
| D138 | 07-modules.md | Межпакетный импорт — только через объявленную `[dependencies]`-зависимость (Plan 03.1) |
| D139 | 07-modules.md | Version-диапазоны git-зависимостей — резолв по тегам репозитория (Plan 03.2) |
| D140 | 07-modules.md | Effect-aware зависимости — effect-surface, effect-diff, `forbid` на границе (Plan 03.4) |
| D141 | 08-runtime.md | Примитивы доступа к памяти — `str.byte_at`, bulk slice-операции `[]T`, `compare` (Plan 90); extend-family `extend_from`/`insert_from`/`reserve`, `copy_from` hardening + W_VIEW_EXTEND_DETACH lint (Plan 90.1 amend 2026-05-27) |
| D142 | 02-types.md | Симметрия effect/protocol declaration ↔ literal — keyword `handler`→`effect`, `Handler[E,IRT]`→`Effect[E,IRT]`, анонимный protocol-литерал, capture-rules (Plan 97) |
| D143 | 03-syntax.md | Static-method префикс `.method()` в `type X protocol { ... }` теле — симметрия с D35 instance-методом `fn T.method()` (Plan 97) |
| D144 | 02-types.md | Sub-slice views `arr[a..b]` / `str[a..b]` — `cap == len` model, 5 форм Range (RangeBounds parity), bounds-check для raw `arr[i]` (Plan 96) |
| D133 | 02-types.md | type-level `consume` — обязательная consume-семантика (Plan 100.1, proposed Ред. 2 2026-05-24); расширяет D131 affine с противоположной стороны (must-be-consumed). Унифицированная модель «view-default, `consume`-explicit». Foundation Plan 100 family |
| D156 | 02-types.md | Generic `[T consume]` bound + collection-aware iteration с 3 mode'ами (`for tx` view / `for mut tx` mut-view / `for consume tx` consume) — Plan 100.2 proposed Ред. 2 |
| D157 | 05-memory.md | Implicit view default + closure capture analysis + `match consume` syntax (Plan 100.3, proposed Ред. 2 2026-05-24). `view T` keyword отвергнут — view это no-qualifier default везде. Automatic closure-mode detection (view / mut-view / consume) |
| D158 | 03-syntax.md | Failable cleanup body (Plan 100.4.1, ✅ закрыт 2026-05-26) — amend D90 §4; defer/errdefer body может Fail; Plan 49 multi-error composition; MultiError prelude type; runtime helpers nv_compose_suppressed / nova_rethrow_with_suppressed |
| D159 | 03-syntax.md | Async/suspend в cleanup body (Plan 100.4.2, ✅ закрыт 2026-05-26) — amend D90 §5; checker сняла suspend-call ban; spawn/parallel for/supervised/detach/blocking keep banned (E D159-spawn-in-defer); cancel-shielding runtime — [M-100.4.2-cancel-shielding] followup |
| D160 | 03-syntax.md | `okdefer` + reason-aware `defer \|result\|` (Plan 100.4.3, proposed); complement к errdefer; mixed LIFO defer family |
| D161 | 03-syntax.md | Multi-defer LIFO error accumulation + panic-in-defer composition (Plan 100.4.4, ✅ закрыт 2026-05-26) — amend D90 §«panic»; codegen per-defer NovaFailFrame wrap; LIFO continues после partial failure; no Rust double-panic-abort; closes [M-100.4.1-emit-defer-wrap] |
| D162 | 03-syntax.md | Consume-integration final (Plan 100.4.5, ✅ закрыт 2026-05-26 bootstrap MVP) — check_consume + d162_mark_defer_cover распознаёт defer/errdefer/okdefer cover; leverage Plan 100.8 D166; D90 §7 amend (interrupt → errdefer) — P2 BREAKING [M-100.4.5-d90-§7-interrupt-errdefer] followup |
| D163 | 02-types.md | FFI consume integration — type-driven, без `external consume fn` keyword (Plan 100.5, proposed Ред. 2 2026-05-24). Унифицировано с regular fn: return-type carries consume-ness; `consume` keyword только на params/receivers (D131 semantic) |
| D164 | 02-types.md | Cross-module consume — visibility, mangling-bit extension Plan 81 D134, package contracts (Plan 100.6, proposed) |
| D165 | 09-tooling.md | Consume-types migration policy — `nova consume-migrate` CLI + edition versioning (Plan 100.7, proposed) |
| D166 | 09-tooling.md | Consume-types developer experience — perf budget <5%, LSP quick fixes (12 error codes), hover info, `nova doc` integration, structured diagnostic format (Plan 100.8, proposed) |

## История

- [history/rejected.md](history/rejected.md) — все отвергнутые альтернативы с причинами.
- [history/evolution.md](history/evolution.md) — как менялись решения по ходу разработки.

## Шаблон D-решения

Каждое D-решение в новых файлах следует единому формату:

```markdown
## DXX. Название

### Что
Одно предложение — суть решения.

### Правило
Подробные правила и примеры current syntax.

### Почему
Обоснование с прецедентами / trade-offs.

### Что отвергнуто
Краткий список альтернатив с причинами отказа.

### Связь
- DXX (зависит / уточняется)
- DYY (родственное)

### Эволюция (если применимо)
Краткая хронология изменений с указанием прежних формулировок.
```

Если решение **отменено** — в начале блока пометка `> ⚠️ ОТМЕНЕНО, см. DZZ`.

## Принципы записи

1. **Только current state** в основном тексте. Ссылки на устаревшие
   формулировки — только через раздел «Эволюция».
2. **Все примеры синтаксически валидны** по текущим правилам. Никаких
   `trait`/`impl`, lowercase эффектов, `:` в типах.
3. **Перекрёстные ссылки внутри `spec/decisions/`** — относительные
   пути (`02-types.md#d17`).
4. **Внешние ссылки** на `syntax.md`, `effects.md` etc. — относительные
   `../syntax.md` (они теперь рядом, в `spec/`).

## Миграция

Старый `decisions.md` в корне репозитория **удалён** после переноса
всех решений в эту директорию (`spec/decisions/`). Все cross-references
в живых документах (`spec/*.md`, `examples/`, `docs/articles/`,
`docs/plans/`, `docs/research/`, `README.md`, `CONTRIBUTING.md`,
`editors/vscode/README.md`) обновлены на новые пути.

См. процесс миграции — [history/evolution.md](history/evolution.md).
