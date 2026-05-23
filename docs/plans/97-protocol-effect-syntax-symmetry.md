// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 97 — protocol/effect syntax: `.method` static + анон-литерал + `handler` → `effect` rename

> **Статус:** ✅ ЗАКРЫТ 2026-05-23 (Ф.0/Ф.1/Ф.2/Ф.3 + Ф.4 partial + Ф.6 spec sweep)
> **Регресс:** `nova test-all` — **PASS: 1076  FAIL: 0  SKIP: 56**
> **Коммиты (ветка `plan-97`):**
> - Ф.0 spec — `39c138ae824` D142+D143 + amendments
> - Ф.2 anon-protocol type-position — `69baaf148eb`
> - Ф.3 clean-break rename — `d5aa5bcc4c2`
> - Ф.4 protocol-literal parser/AST/type-check — `d4b82d95f9d`
> - Ф.6 spec sweep — `de73650c04c`
> **Deferred:** [M-protocol-literal-codegen-deferred] — runtime vtable
> для protocol-only типов отложен в Plan 100 (followup); все остальные
> элементы (parser, type-checker, capability-split factory parser
> + structural verify) — production-grade.
> **Приоритет:** P2 (закрывает два открытых вопроса спеки одной
> согласованной итерацией; разблокирует capability-split factory
> pattern для stdlib Plan 18)
> **Оценка:** ~6–8 dev-day (~3 кодовых блока + миграционный sweep + spec)
> **Зависимости:** D35 (`fn Type.name(...)` static-форма) ✅; D53
> (protocol keyword) ✅; D61 (`handler` literal) ✅; D87
> (`Handler[E, IRT]`) ✅; D77 (4-way `from`/`try_from` auto-derive) ✅;
> Plan 15 (generic bounds) ✅; Plan 56 D122 (эффекты в protocol-методах) ✅;
> Plan 08 (`From`/`Into`/`TryFrom`/`TryInto` инфра) ✅.
> **Закрывает:** `Q-static-method-protocol`
> (`spec/decisions/03-syntax.md:3247`), `Q-keyword-symmetry`
> (`spec/open-questions.md:5526`), D53 §628 bootstrap-status
> (`spec/decisions/02-types.md:903-905`).
> **Решения/гейты (приняты 2026-05-23):**
> - Static в protocol — `.method`-префикс (per spec hint).
> - Backwards-compat: bare-имена = instance (ничего не ломается).
> - `handler` keyword — **clean break** (без deprecated alias).
> - `Protocol[P]` first-class тип — **не** вводится (тривиальный `alias P`).
> - Литерал `protocol X { ... }` — **instance-only** (static не имеет
>   value-level реализации).
> **Источник:** обсуждение 2026-05-23.

## Зачем

Один план закрывает **два** связанных пробела в синтаксисе protocol/
effect. Раздельные планы 97 и 98 объединены 2026-05-23 — они трогают
одни и те же файлы, имеют общие D-блоки, и логика «один сквозной sweep»
дешевле двух разрозненных.

### Пробел 1: static-метод в protocol неотличим от instance

Спека `03-syntax.md:3247` явно фиксирует:
> Static-метод в protocol через `.method()`-префикс — `Q-static-method-protocol`.

`From[T] protocol { from(t T) -> Self }` — `from` это **статический**
метод по D35 (`Celsius.from(f)`), но в теле записан «голо»,
неотличимо от instance-методов (`Hashable.hash()`). Это:
- Делает декларацию неточной (теряется «static vs instance»).
- Делает doc-comment `protocols.nv` противоречивым (показано
  `fn Celsius @from(...)` — instance, противоречит D35).
- Блокирует корректное оформление `From`/`Into`/`TryFrom`/`TryInto`.

### Пробел 2: declaration ↔ literal asymmetry для effect/protocol

```nova
type Cron effect   { run() -> () }      // declaration
type Fan  protocol { run() -> () }      // declaration

let h = handler Cron { run() => () }    // literal — иной keyword
let p = ???                              // literal — НЕТ
```

Закрывается единой решёткой:

```nova
let h = effect   Db  { query(q) => mock_rows() }    // keyword из declaration
let p = protocol Fan { run() => spin_blades() }     // keyword из declaration

fn db()  -> Effect[Db]                              // rename Handler → Effect
fn db2() -> Effect[Db, ShutdownSignal]
```

Use-case на котором режется design stdlib — **capability-split factory**:

```nova
type Locker   protocol { lock()   -> () }
type Unlocker protocol { unlock() -> () }

fn Lock.new() -> (Locker, Unlocker) {
    let state = MutexState { ... }
    let l = protocol Locker   { lock()   => state.lock() }
    let u = protocol Unlocker { unlock() => state.unlock() }
    (l, u)
}
```

Без анон-литерала — два named-типа-обёртки, использующиеся один раз.
Кандидаты в Plan 18: `Process.spawn -> (Stdin, Stdout, Stderr)`,
`HttpServer.bind -> (Acceptor, ShutdownHandle)`, `Db.transaction ->
(TxReader, TxWriter, Commit)`.

### Trade-off rename `handler` → `effect` (зафиксирован)

Аргумент против rename (из Q-keyword-symmetry):
> `let h = handler Logger { ... }` сразу читается «обработчик
> эффекта». `effect Logger { ... }` может читаться как «значение
> типа эффекта Logger».

Контр-аргумент (**accept'нут** 2026-05-23): та же двусмысленность
есть для `protocol X { ... }`-литерала. Если вводим литерал для
протоколов с keyword из declaration — последовательно делаем то же
для эффектов. Симметрия побеждает локальную точность keyword'а.

## Сравнение с языками

### Static в interface/trait

| Язык | Static-метод в trait/interface |
|---|---|
| Rust | `trait T { fn associated() -> Self; }` — нет `self` → associated. `Self::associated()`. |
| Go | interface'ы не имеют static. |
| TS | interface'ы — инстансовые, static — отдельная конструкция (`abstract class`). |
| Nova (сейчас) | bare-имя — неотличимо от instance. |
| Nova (цель) | `.from(t T)` static vs `from(t T)` instance. |

### Anonymous protocol/interface literal

| Язык | Effect-literal | Anonymous protocol/interface |
|---|---|---|
| Nova (сейчас) | `handler X { ... }` | нет |
| Nova (цель) | `effect X { ... }` | `protocol X { ... }` |
| Koka / Eff | `handler { ... }` | нет |
| Java | — | `new Runnable() { ... }` ✓ |
| Kotlin | — | `object : Runnable { ... }` ✓ |
| TypeScript | — | object-literal structurally ✓ |
| Rust | — | **нет** (`impl Trait for Type` обязателен) |
| Go | — | **нет** (named receiver обязателен) |
| Swift | — | **нет** (`extension Type: Protocol`) |

Картина расколота; Nova движется в сторону Kotlin/Java/TS.

## Привязка к коду (сверено 2026-05-23)

- **Spec:**
  - `03-syntax.md:3247` — `Q-static-method-protocol` (закрывается).
  - `03-syntax.md:1262` — `fn str.from(i int) -> Self` (D35 static).
  - `02-types.md:640-657` — D53 §628 (анон-protocol в позиции типа).
  - `02-types.md:903-905` — D53 bootstrap-status: «требует
    `TypeRef::Protocol(...)` варианта».
  - `04-effects.md:1734` — D61 handler-literal.
  - `04-effects.md:1738-1747` — D87 `Handler[E, IRT]`.
  - `open-questions.md:5526-5797` — Q-keyword-symmetry (закрывается).
- **Implementation:**
  - `compiler-codegen/src/parser/` — protocol body парсер (Ф.1);
    type-position парсер (Ф.2); expression-литерал парсер (Ф.4);
    lexer keyword `handler` → удалить (Ф.3).
  - `compiler-codegen/src/types/` — `TypeRef::Protocol(...)` variant
    (Ф.2); static vs instance matching (Ф.1).
  - `compiler-codegen/src/prelude/` — rename `Handler` → `Effect` (Ф.3).
  - `nova_tests/`, `examples/`, `std/` — миграция Ф.3 sweep.
  - `std/prelude/protocols.nv` — `From`/`TryFrom` update (Ф.1).
- **`protocols.nv`** stale-моменты (под-задача Ф.1):
  - `From[T]` / `TryFrom[T, E]` — static без префикса.
  - Комментарий 101-108 («`Fail[E]` prohibited Plan 56 Ф.2.7») —
    stale (запрет снят 2026-05-20 D122 amended).
  - Doc-comment `fn Celsius @from(...)` — противоречит D35.
- **Записано как отложенное:**
  - `docs/simplifications.md:3375` — Plan 15 trade-off
    (`[P-15-anon-protocol-bound]`).
  - `docs/plans/15-generic-bounds-enforcement.md:185-189` — explicit.
  - `spec/decisions/06-concurrency.md:2289-2290` — D79 cross-ref.

## Scope

**Входит:**

*Static-dot:*
- `.method(...)`-префикс для static в `protocol {}` body
  (`.from(t T) -> Self`, `.try_from(t T) -> Result[Self, E]`).
- AST: `is_static: bool` на protocol-методе.
- Type-checker: static матчится против `fn Type.method`, instance —
  против `fn Type @method`.
- Backwards-compat: bare-имена = instance (ничего не ломается;
  `Iter`/`Hashable`/`Equatable`/`Comparable`/`Display`/`Into`/`TryInto`
  без правок).
- Update `protocols.nv`: `From`/`TryFrom` под новый синтаксис; destale
  комментариев 101-108 + `@from` doc-comment.

*Type-position анон-protocol (D53 §628):*
- `protocol { method-sig* }` в позиции типа (параметр, return,
  generic bound).
- AST: `TypeRef::Protocol(ProtocolSig)` variant.
- Body — **тот же** парсер, что у named-`protocol { ... }`: instance
  bare + static `.method` (из static-dot выше).
- Закрывает Plan 15 `[P-15-anon-protocol-bound]`.

*Effect/Handler rename — clean break:*
- Keyword `handler` (literal) → `effect`. Старый keyword **удаляется**,
  без deprecated alias.
- Builtin тип `Handler[E, IRT]` → `Effect[E, IRT]`. `Effect[E]` ≡
  `Effect[E, Never]` через D88 default (без изменений).
- Миграция `.nv`: `nova_tests/`, `examples/`, `std/` — scripted
  replace + ручная проверка collision.
- Миграция spec/docs/README — sweep.

*Protocol-литерал (expression-position):*
- `protocol X { method-impl* }` — value, реализующий контракт `X`.
- AST: `ProtocolLit { proto: TypeRef, methods: Vec<MethodImpl> }`.
- **Instance-методы только** (static не имеет value-level реализации:
  static — `Type.method` D35, у литерала нет «своего типа»).
- Type-checker: структурное соответствие; capture-rules как closure
  (D22 / D6 managed heap — без новых правил).
- Codegen: синтез anonymous-типа + методов (как handler сейчас).

*Spec:*
- Новый **D142** в `02-types.md` — symmetry effect/protocol
  declaration ↔ literal; rename handler→effect; anon literal.
- Новый **D143** в `03-syntax.md` (или amend D35/D58) — `.method`-
  префикс для static в protocol body.
- Закрытие Q-keyword-symmetry + Q-static-method-protocol.
- Update D53 §628 bootstrap-status (реализовано), D61 (literal-keyword
  `effect`), D87 (тип `Effect`).

**Не входит:**
- `@method`-префикс для явных instance в protocol — Q-open, отдельный
  followup (bare = instance закрывает практику).
- `Protocol[P]` first-class тип — **отвергнуто** (тривиальный `alias P`;
  у эффектов `Handler` нужен потому что значение передаётся в
  `with X = h`; у протокола значение — тип, реализующий контракт).
- Изменение D77 4-way auto-derive семантики — не трогаем.
- Перевод `TryFrom`/`TryInto` с `Result` на `Fail` — `try_`-prefix
  convention диктует `Result`.
- Capture-rules для protocol-литерала сверх closure (D22 / D6).
- Изменение семантики handler'ов — только keyword rename.

## Фазы

### Ф.0 — Дизайн + D142 + D143 (~0.5 д) — GATE

- **Ф.0.1** Локализовать parser-точки: (a) `protocol { }` body
  (для static-dot), (b) type-position (для анон-protocol), (c)
  expression-position (для handler-literal + protocol-literal),
  (d) lexer keyword `handler`.
- **Ф.0.2** Карта type-checker'а: (a) static vs instance matching,
  (b) `TypeRef::Protocol` через все use-sites.
- **Ф.0.3** Карта codegen: (a) static-bound-method dispatch через
  Plan 88 `apply_type_subst_to_ref`; (b) protocol-literal codegen
  как existing handler-literal (anonymous struct + methods).
- **Ф.0.4** Написать **D142** (`02-types.md`): symmetry, rename,
  anon literal, capture-rules, clean-break note.
- **Ф.0.5** Написать **D143** (`03-syntax.md`) или amend D58:
  `.method` static, bare = instance backwards-compat, matching
  rules.
- **Ф.0.6** Update D53 §628 bootstrap-status: «реализуется Ф.2».
  Update D61: literal keyword `effect`. Update D87: builtin
  `Effect[E, IRT]`.
- **Ф.0.7** Update `spec/decisions/README.md` D-index.

**Acceptance:** D142, D143, amends D53/D61/D87 написаны; Q-keyword-
symmetry + Q-static-method-protocol закрыты со ссылками.

### Ф.1 — Static-dot в protocol body (~1.2 д)

- **Ф.1.1** Парсер `protocol { }` body: принять `.identifier(...)`
  как static; bare `identifier(...)` остаётся instance.
- **Ф.1.2** AST: `is_static: bool` на protocol-методе (default `false`).
- **Ф.1.3** Type-checker: для `is_static = true` — матчить против
  `fn Type.method(...)`, не `fn Type @method`.
- **Ф.1.4** Codegen: вызов static protocol-метода в generic-bound
  контексте — через mono-substitution `T` → концретный тип
  (Plan 88 `apply_type_subst_to_ref`).
- **Ф.1.5** Update `std/prelude/protocols.nv`:
  - `From[T] protocol { .from(t T) -> Self }`.
  - `TryFrom[T, E] protocol { .try_from(t T) -> Result[Self, E] }`.
  - `Into[U]` / `TryInto[U, E]` — без изменений (instance).
  - Destale comment 101-108: переписать причину `Result`-формы
    через `try_`-prefix + D77 (не «ban»).
  - Destale doc-comment `fn Celsius @from(...)` → `fn Celsius.from(...)`.
- **Ф.1.6** Build + проверить, что stdlib `Type.from`/`Type.try_from`
  объявления продолжают удовлетворять обновлённым протоколам.

**Acceptance:** Q-static-method-protocol закрыт; `protocols.nv`
честный; D77 auto-derive продолжает работать.

### Ф.2 — Type-position анон-protocol (~1.0 д) — D53 §628

- **Ф.2.1** Парсер type-position: принять `protocol { method-sig* }`
  как четвёртую форму после `[]T`, `(A, B)`, `fn() -> T`.
- **Ф.2.2** AST: `TypeRef::Protocol(ProtocolSig)` variant (`ProtocolSig`
  — reuse существующего `ProtocolSpec` без имени).
- **Ф.2.3** Type-checker: обобщить `compute_protocol_satisfaction` на
  inline-protocol (был только для named).
- **Ф.2.4** Generic bound: `fn min[T protocol { @lt(other Self) -> bool }](xs []T)`
  — закрывает Plan 15 trade-off.
- **Ф.2.5** Body парсера inline-protocol — **переиспользовать** body
  парсера named (Ф.1) — единая точка, статика-dot работает там же.

**Acceptance:** D53 §628 fully implemented; `[P-15-anon-protocol-bound]`
снят с simplifications.md.

### Ф.3 — Clean-break rename `handler` → `effect` / `Handler` → `Effect` (~1.5 д)

- **Ф.3.1** Lexer: удалить keyword `handler`. Keyword `effect` уже
  есть (declaration), расширяется до literal через парсер.
- **Ф.3.2** Парсер expression-position: literal `effect X { ... }`
  (был `handler X { ... }`). Disambiguation: `effect IDENT {` →
  literal; в declaration context `type X effect {` — без изменений.
- **Ф.3.3** AST: внутреннее имя `HandlerLit` оставить (рефакторинг
  имён — отдельный noise); публичное keyword — `effect`.
- **Ф.3.4** Prelude/type-checker: rename builtin `Handler` → `Effect`.
- **Ф.3.5** Миграция `.nv` (scripted + ручная проверка):
  - `nova_tests/**/*.nv`, `examples/**/*.nv`, `std/**/*.nv`.
  - Замены: `handler X` (expr-ctx) → `effect X`; `Handler[`
    → `Effect[`.
  - Ручная: переменные/identifier'ы с именем `handler`/`Handler`
    (collision).
- **Ф.3.6** Spec sweep: `effects.md`, `04-effects.md`, `02-types.md`,
  README EN/RU, overview.md, examples примеры.
- **Ф.3.7** Парсер: hint при встрече `handler` в expr-ctx — diagnostic
  «`handler` keyword removed; use `effect` (D142)».

**Acceptance:** `handler` keyword не существует; `nova test`
зелёный после миграции; миграционный коммит атомарный.

### Ф.4 — Expression-position литерал `protocol X { ... }` (~1.2 д)

- **Ф.4.1** Парсер expression-position: literal `protocol X { method-impl* }`
  где `X` — имя именованного протокола ИЛИ inline (через Ф.2 type-anon).
- **Ф.4.2** AST: `ProtocolLit { proto: TypeRef, methods: Vec<MethodImpl> }`.
- **Ф.4.3** Type-checker:
  - Структурное соответствие `methods` сигнатуре `proto`.
  - **Instance-only** — попытка реализовать static → diagnostic
    «static methods cannot be implemented in protocol-literal; they
    belong to a type (D35) — use a named type».
  - Capture-rules — как closure (D22/D6).
- **Ф.4.4** Codegen: синтез anonymous-struct + methods (model handler-
  literal). Captured state — managed heap (D6), как у closure.

**Acceptance:** capability-split factory pattern работает на
`Lock.new() -> (Locker, Unlocker)` fixture'е.

### Ф.5 — Тесты pos/neg (~0.6 д)

*Static-dot (Ф.1):*
- **Ф.5.1** `protocol_static_from.nv` — pos: реализует `From[T]` через
  `fn MyT.from(t T)` (D35 static); bound `[T From[X]]` резолвит
  `T.from(v)`.
- **Ф.5.2** `protocol_static_try_from.nv` — pos: `TryFrom[T, E]` +
  bound dispatch.
- **Ф.5.3** `protocol_instance_unchanged.nv` — regress-pos:
  `Hashable.hash`/`Iter.next` работают как instance.
- **Ф.5.4** `neg_static_vs_instance_mismatch.nv` — тип `fn T @method`
  когда протокол требует `.method` → compile error.
- **Ф.5.5** `neg_instance_vs_static_mismatch.nv` — обратное.
- **Ф.5.6** `from_into_d77_autoderive.nv` — regress: D77 auto-derive
  с обновлёнными декларациями.

*Type-anon (Ф.2):*
- **Ф.5.7** `anon_protocol_param.nv` — pos: `c protocol { close() -> () }`
  параметр + вызов.
- **Ф.5.8** `anon_protocol_bound.nv` — pos: generic bound
  `[T protocol { ... }]`.
- **Ф.5.9** `neg_protocol_record_ambiguity.nv` — `{ ... }` без префикса
  в позиции типа → parse error.

*Rename (Ф.3):*
- **Ф.5.10** `effect_literal_basic.nv` — pos: `let h = effect Db { ... }`
  + `with Db = h { ... }`.
- **Ф.5.11** `effect_type_alias.nv` — pos: `fn() -> Effect[Db]` +
  `fn() -> Effect[Db, IRT]`.
- **Ф.5.12** `neg_old_handler_keyword.nv` — `handler X { ... }` →
  parse error с hint «use `effect` (D142)».

*Protocol-литерал (Ф.4):*
- **Ф.5.13** `protocol_lit_capability_split.nv` — pos: `Lock.new() ->
  (Locker, Unlocker)`.
- **Ф.5.14** `protocol_lit_closure_capture.nv` — pos: литерал +
  замыкание над state.
- **Ф.5.15** `protocol_lit_with_anon_type.nv` — pos: литерал inline-
  protocol типа (через Ф.2).
- **Ф.5.16** `neg_protocol_lit_missing_method.nv` — литерал без метода
  → diagnostic «missing method `X.foo`».
- **Ф.5.17** `neg_protocol_lit_wrong_signature.nv` — несовпадение
  сигнатуры.
- **Ф.5.18** `neg_protocol_lit_static_method.nv` — попытка реализовать
  static в литерале → diagnostic Ф.4.3.

**Ф.5.19** Полный `nova test .` — 0 новых FAIL после миграции Ф.3.

### Ф.6 — Sweep + закрытие (~0.5 д)

- **Ф.6.1** Закрыть Q-keyword-symmetry (`open-questions.md`): mark
  closed, link → D142.
- **Ф.6.2** Закрыть Q-static-method-protocol (`03-syntax.md:3247`):
  mark closed, link → D143.
- **Ф.6.3** `docs/plans/README.md` — Plan 97 → ЗАКРЫТ (с обновлённым
  заголовком).
- **Ф.6.4** `docs/simplifications.md`:
  - Снять `[P-15-anon-protocol-bound]`.
  - Если что-то отложили (`@method` явный instance) — маркер.
- **Ф.6.5** `docs/plans/15-generic-bounds-enforcement.md` — обновить
  trade-off секцию: «закрыто Plan 97 Ф.2».
- **Ф.6.6** `docs/project-creation.txt` — запись.
- **Ф.6.7** `nova-private/discussion-log.md` — запись (объединение
  Plan 97 + 98 + декомпозиция решений).
- **Ф.6.8** Merge `plan-97` → `main`.

**Acceptance:** spec/docs consistent, оба Q закрыты,
`[P-15-anon-protocol-bound]` снят, README обновлён.

## Итог Ф.0

> Заполняется по результатам аудита (Ф.0.1–Ф.0.3): parser-точки,
> карта type-checker'а, codegen-маршруты. До аудита пусто.

## Acceptance criteria

*Static-dot:*
- [ ] `protocol { .from(t T) -> Self }` парсится; `from` — static.
- [ ] `protocol { method() }` (bare) парсится; `method` — instance.
- [ ] `From[T] protocol { .from(t T) -> Self }` в `protocols.nv`;
      существующий stdlib продолжает удовлетворять.
- [ ] Type-checker матчит static против `fn Type.method`, instance —
      против `fn Type @method`.
- [ ] Compile error при static/instance mismatch.
- [ ] D77 4-way auto-derive работает.

*Type-anon:*
- [ ] `c protocol { close() -> () }` параметр работает.
- [ ] Generic bound `[T protocol { ... }]` работает.
- [ ] `[P-15-anon-protocol-bound]` снят.

*Rename:*
- [ ] Keyword `handler` не существует (parse error + hint).
- [ ] `effect X { ... }` литерал работает в expression-position.
- [ ] Builtin `Effect[E, IRT]` (был `Handler[E, IRT]`).
- [ ] Все `.nv`, spec, docs мигрированы.

*Protocol-литерал:*
- [ ] `protocol X { method-impl* }` литерал работает.
- [ ] capability-split factory fixture проходит.
- [ ] Static-метод в литерале → diagnostic Ф.4.3.

*Спека:*
- [ ] D142 в `02-types.md`; D143 в `03-syntax.md` (или amend D58).
- [ ] D53 §628 bootstrap-status: «реализовано Plan 97 Ф.2».
- [ ] D61 literal-keyword `effect`; D87 builtin `Effect[E, IRT]`.
- [ ] Q-keyword-symmetry + Q-static-method-protocol закрыты.

*Регресс:*
- [ ] Полный `nova test .` — 0 новых FAIL.

## Risks

1. **Парсер ambiguity `effect X { ... }` в expr-ctx.** `effect` —
   keyword, не identifier; не ломает валидные программы. **Низкий.**
2. **`protocol Fan { ... }` vs record-literal `Fan { ... }`.** Литерал
   protocol'а **обязательно** с keyword'ом `protocol` — парсер
   однозначен. **Низкий.**
3. **Type-position `protocol { ... }` vs anonymous record-тип.**
   Префикс `protocol` обязателен (D53 §659). **Низкий.**
4. **Миграция Ф.3 — масштаб.** ~30+ `.nv` + ~10 spec-доков + Rust-
   код. Атомарный коммит + scripted-replace + ручная проверка
   identifier-collision. **Средний.**
5. **Static-dot AST flag миграция.** AST-flag `is_static: bool` — все
   места обхода protocol-методов нужно обновить (default `false` для
   backwards-compat → safe). **Низкий.**

## Non-scope (deferred / explicit)

- `@method`-префикс для явных instance в protocol — Q-open, отдельный
  followup. Bare = instance закрывает практику.
- `Protocol[P]` first-class — отвергнуто (тривиальный `alias`).
- Изменение D77 auto-derive — не трогаем.
- Перевод `TryFrom`/`TryInto` на `Fail` — `try_`-prefix convention
  диктует `Result`.
- Полная сверка протокол-методов stdlib (`Iter`/`Hashable`/…) — уже
  корректно bare-instance, правок не нужно.

## Связь

- [D35](../../spec/decisions/03-syntax.md#d35) — static (`.`) vs
  instance (`@`) методы (фундамент Ф.1).
- [D53](../../spec/decisions/02-types.md#d53) §628 — анон-protocol
  в позиции типа (Ф.2).
- [D58](../../spec/decisions/03-syntax.md#d58) —
  `Q-static-method-protocol` (Ф.1, закрывается D143).
- [D61](../../spec/decisions/04-effects.md#d61) — handler-литерал
  (rename Ф.3).
- [D77](../../spec/decisions/08-runtime.md#d77) — 4-way auto-derive
  (фон Ф.1).
- [D87](../../spec/decisions/04-effects.md#d87) — `Handler[E, IRT]`
  (rename Ф.3).
- [D88](../../spec/decisions/03-syntax.md#d88) — default generics
  (`Effect[E]` ≡ `Effect[E, Never]`).
- [D122](../../spec/decisions/02-types.md#d122) amended 2026-05-20 —
  эффекты в protocol-методах разрешены (фон Ф.1).
- [Q-keyword-symmetry](../../spec/open-questions.md#q-keyword-symmetry)
  — закрывается D142 (Ф.0).
- [Plan 08](08-from-into-conversions.md) — `From`/`Into`/`TryFrom`/
  `TryInto` инфра (фон Ф.1).
- [Plan 15](15-generic-bounds-enforcement.md) — generic bounds; trade-
  off секция закроется Ф.2 (`[P-15-anon-protocol-bound]`).
- [Plan 18](18-stdlib-roadmap.md) — stdlib roadmap; разблокируется
  capability-split factory pattern (Ф.4).
- [Plan 56](56-vtable-dispatch-erased-generics.md) Ф.2.7 REVERTED
  2026-05-20 — D122 amended.
- [Plan 88](88-generic-static-method-on-typevar.md) —
  `apply_type_subst_to_ref` (фундамент Ф.1.4 codegen).
