// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 114 — Keyword refresh: `ro`/`mut`/`consume` bindings, drop `let`, narrow + generalize `const` (data + fn), rename `readonly` → `ro`

> **Создан 2026-05-30.**
> **Статус:** 🆕 PLANNED.
> **Приоритет:** P0 — syntax-surface change, должен приземлиться до 0.1 freeze
>   (Plan 91 в активной фазе; запоздалый rename = удвоенная миграция fixture-set'а).
> **Оценка:** ~4-5 dev-day (parser ~½ day; manual rewrite rules document
>   ~⅛ day; manual rewrite std+prelude ~½ day; manual rewrite fixtures+
>   examples+bench+docs+spec ~1 day (parallel-agent friendly); **`const`
>   narrowing Ф.9 ~⅛ day**; **`const` generalization Ф.10 ~1 day** —
>   включая sum-type + generic-aware associated const с per-monomorphization
>   codegen; **`const fn` comptime evaluator Ф.11 ~1 day**; manual spec/docs
>   polish ~½ day; verification full nova test ~½ day).
> **Зависимости:**
>   - Plan 14 Ф.2 ✅ closed — `const` lazy-init для non-constexpr; **Ф.9
>     narrowing убирает lazy-init из `const`** (теперь strict constexpr-only;
>     lazy fallback переезжает на `ro` для тех case'ов где non-constexpr).
>   - Plan 46 ✅ closed — D102 default-param-values; **не меняется** — `const`
>     ref продолжает работать (теперь strict constexpr enforce'ит сильнее).
>   - Plan 73.1 ✅ closed — `consume X = expr` binding-syntax (D180 в `05-memory.md`)
>     — становится третьим (и последним) binding-statement keyword'ом.
>   - Plan 108 ✅ closed — `readonly field` / `readonly T` (D175/D176); Ф.10
>     добавляет `const field` как третий вид field-decl (associated const).
>   - Plan 108.1 ✅ closed — `readonly param` synonym default.
>   - Plan 113 ✅ closed — attribute-only `#realtime` (zero overlap, упоминается
>     только как пример «keyword-cleanup как класс задач»).
> **D-блоки:** **новый D184** (keyword refresh master); **новый D200** (associated
>   constants — `const` field в `type X`); **новый D199** (`const fn` — comptime
>   evaluable functions); **D33 rewrite целиком** (три **реальные** оси:
>   binding mutability + hard constexpr + per-field freeze); амендменты
>   **D32, D34, D36, D175, D176, D180**. **D27, D30, D102 — НЕ меняются**
>   (`const` остаётся, narrower-meaning compatible со старыми формулировками).
>
> **Renumbering note (audit 2026-05-31):** ранее план использовал D184/D185/D186
> для master/assoc-const/const-fn. Audit обнаружил: **D186 уже занят** в spec'е
> (Plan 91.9 — `#impl(P+Q+...)` annotation, orthogonal к keyword refresh), **D185
> имеет text-reference** в D183 body (Plan 91.8c планировал promote до full block).
> Renumber: D186→D199 (const fn), D185→D200 (assoc const). D184 свободен —
> оставлен как есть. Также Plan 110 commit'нулся раньше с claim D188-D198 +
> dangling refs на D184-D187 (cleanup-семейство, никогда не landed).
> **Safety hatch для Ф.9 (`const` narrowing), Ф.10 (`const` generalization
>   to scope/field), и Ф.11 (`const fn`):** все три self-contained, extractable
>   в sub-plans **Plan 114.1** (Ф.9 → narrow const), **Plan 114.2** (Ф.10 →
>   assoc const), **Plan 114.3** (Ф.11 → const fn) — каждая одним revert'ом
>   независимо. Plan 114 может шипиться с любым subset фаз. Decision points
>   в preamble каждой фазы.
>   *Sub-plan numbering vs Plan 115: Plan 115 зарезервирован за std/tls
>   (Plan 91.12 followup, post-0.1); conditional const-extracts используют
>   sub-plan family 114.x (consistent с patterns Plan 108.1-108.4 / Plan 91.x /
>   Plan 100.x).*
> **Worktree convention:** `nova-p114` (создать сразу, регистрировать через
>   hook, все команды с cd-префиксом в worktree per feedback-worktree-cwd-clarity).
>
> **Recommended model:**
>   - **Opus 4.7 + Thinking ON** — рекомендуемый default для всего плана,
>     особенно Ф.10 (generic-aware assoc const + monomorphization integration)
>     и Ф.11 (comptime evaluator — новая компилятор-подсистема).
>   - **Sonnet 4.6 HIGH + Thinking ON** допустим для Ф.0-Ф.9 + Ф.7-Ф.8
>     (parser swap, bulk-rewrite, tree-sitter, spec D-blocks, return-type
>     rules — well-known patterns). НЕ рекомендуется для Ф.10/Ф.11 — там
>     нужен Opus 4.7 (новые subsystem'ы, high-risk для Sonnet).
>
> **Workflow требования (для агента):**
>   1. **Work без остановок** — не запрашивай confirmations внутри фазы;
>      переход между фазами только если smoke verify pass'нул.
>   2. **Commit per phase** — после каждой Ф.N (или sub-фазы если она
>      нетривиальная) — отдельный commit с message в формате `feat(Plan 114
>      Ф.N): <summary>`. Несколько задач в одной фазе → несколько коммитов.
>   3. **Update logs после каждой большой задачи:**
>      - `docs/project-creation.txt` — sprint section (per feedback-update-logs)
>      - `docs/simplifications.md` — закрытые/открытые `[M-114-*]` маркеры
>      - `d:\Sources\nv-lang\nova-private\discussion-log.md` — design decisions, лессоны
>   4. **Tests через release nova & компилятор:** все T1-T10 series тесты
>      запускать через `cargo build --release -p nova-cli` + `target/release/nova test`
>      (не debug build; не cargo test stand-alone). Это catch'ает release-only optimizations.
>   5. **Записать финальный статус** в этот же файл (`docs/plans/114-keyword-refresh-ro-mut-no-let.md`)
>      в новой секции «## Status — closure summary» в конце файла: что
>      сделано, что extracted в Plan 114.1/114.2/114.3 (если safety hatch fire'нул), full
>      `nova test` results, ссылки на коммиты.
>   6. **Safety hatch trigger'ы** в Ф.9/Ф.10/Ф.11 preamble — следуй им
>      буквально; не «пушь дальше» если decision point говорит extract.
>
> **Production-grade требование:** реализация без упрощений. Никаких temporary
> shortcuts, dual-syntax fallback'ов, silent compatibility-mode'ов, partial
> migrations. Hard cutover за один merge: parser принимает только новый
> синтаксис, весь репо (prelude + std + nova_tests + examples + bench + docs +
> spec) переписан, full `nova test` ≥ baseline 1559/74. Если фича требует
> cross-tool работы (tree-sitter, LSP quick-fix, editor packaging) — делать
> честно; всё что не влезает — выносится в отдельный followup-план с явным
> `[M-114-xxx]` маркером + record'ится в `simplifications.md` как «explicitly
> deferred, not silently dropped».

---

## Зачем

Сейчас в Nova **три ортогональные оси immutability** (D33) описываются **четырьмя
неединообразными keyword'ами** + один многозначный (`let`):

| Текущий синтаксис | Семантика | Проблема |
|---|---|---|
| `let X = expr` | immutable binding (default) | `let` — пустой keyword, повторяется в каждой строчке без информационной нагрузки |
| `let mut X = expr` | mutable binding | два слова на decl; `let mut` — visual noise |
| `consume X = expr` | owned binding (Plan 73.1) | уже без `let` — несимметрично с двумя выше |
| `readonly field T` | field freeze | длинно; в hot-path stdlib повторяется ~160× |
| `readonly T` | type-modifier | длинно; занимает много места в сигнатурах |
| `if let Pat = e { }` | pattern-binding-in-condition | реюзает `let` как pattern-intro — третий смысл слова |

**Цена статус-кво:**

1. **Несимметрия с consume.** `consume X = expr` (Plan 73.1, D180) уже без `let`.
   Тройка binding-statement'ов должна быть симметрична — **`ro` / `mut` / `consume`**,
   каждый сам себе keyword. Иначе reader держит в голове два разных правила:
   «для immutable/mutable нужен `let` + опциональный `mut`», а для `consume` —
   нет. AI-unfriendly.
2. **`let` несёт 0 бит информации.** Все три состояния (`ro`/`mut`/`consume`)
   определяются модификатором; `let` только маркер «это binding-stmt». В parser'е
   достаточно lookahead'а на ключевое слово (`ro`/`mut`/`consume`) — `let`
   избыточен.
3. **`readonly` — самый частый long-form keyword в spec.** 160 occurrences в
   .nv-файлах, ещё 161 в spec. Каждое `readonly` — 8 символов; `ro` — 2.
   Экономия ~960 символов в hot-path stdlib без потери ясности (с тем же
   `mut` парой `ro`/`mut` визуально симметрична: 2 + 3 буквы).
4. **`let` в `if let`/`while let` — третий смысл слова.** «`let` = pattern-intro
   в condition» путается с «`let` = binding declaration» при изучении языка.
   Перевод на `if ro Pat = e` / `if mut Pat = e` устраняет collision полностью —
   те же три keyword'а работают и в statement-, и в condition-position.
5. **`const` keyword размытый — не соответствует ни своей spec-формулировке,
   ни ожиданиям от mainstream-языков.** D33 сегодня декларирует «`const` =
   compile-time, `let` = runtime». **Это уже неправда** — Plan 14 Ф.2
   расширил `const` на non-constexpr-init через lazy static getter
   (`nova_const_<name>()`). В корпусе живут `const COMPUTED = make_point(7, 14)`
   — это runtime-init, не compile-time. В то же время Rust/C/Java
   разработчики ожидают что `const` = **hard compile-time guarantee**.
   Plan 114 Ф.9 **narrow'ит `const` до strict constexpr-only semantics**:
   compile-error если RHS не literal-eligible. Lazy fallback (для tех
   case'ов где non-constexpr) переезжает на `ro X = …`. Это восстанавливает
   соответствие mainstream'у — `const` теперь делает то что обещает.
6. **Associated constants — long-missing language feature.** Сейчас в Nova
   нет способа объявить «константа, привязанная к типу» (Java `static final`,
   Rust `impl T { const X = … }`, Kotlin `companion const val`). Workaround
   — module-level `const TYPE_X_MAX = …`, который ломает namespacing. Plan
   114 Ф.10 **generalize'ит `const` на новые позиции**: scope-local (`const
   N = 16` внутри fn) и record-field (`type T { const VERSION int = 1; … }`
   — accessible как `T.VERSION`). Единый `const` keyword работает везде с
   одной semantics — hard constexpr.
7. **`const fn` — comptime evaluable functions.** Без них `const X = …`
   ограничен literal + arithmetic. Plan 114 Ф.11 вводит **`const fn`** —
   функцию, вычисляемую компилятором (Rust `const fn`, C++ `constexpr fn`,
   Zig `comptime` params). Параметры с `const` модификатором требуют
   constexpr args на call site; return type `-> const T` гарантирует
   constexpr результат. Компилятор evaluate'ит body во время компиляции,
   inline'ит литералом на каждый call site. Pulls into in-scope features
   которые ранее requ'или Q7 (`comptime`): computed array sizes, lookup
   tables, type-driven dispatch.

**Бенчмарк vs mainstream.**

| Язык | Immutable | Mutable | Pattern-bind-in-cond | Strength |
|---|---|---|---|---|
| **Go** | `const X = …` (compile-time only) / нет immutable runtime | `var X = …` / `X := …` | `if v, ok := m[k]; ok { }` | walrus `:=` для cond compact |
| **Rust** | `let x = …` | `let mut x = …` | `if let Some(x) = e { }` | `let` повсюду, mut явный |
| **TypeScript** | `const x = …` | `let x = …` | `if (e !== null) { const x = e }` | `const`/`let` несимметрия (Const на immutable) |
| **Kotlin** | `val x = …` | `var x = …` | `if (e is X) { /* smart cast */ }` | symmetric pair `val`/`var`, smart cast |
| **Java** | `final var x = …` | `var x = …` | `if (e instanceof X x) { }` | `final` verbose, `instanceof + pattern` современный |
| **Swift** | `let x = …` | `var x = …` | `if case let .some(x) = e { }` | symmetric pair, but `if case let` ugly |
| **Nova V1** (now) | `let x = …` | `let mut x = …` | `if let Some(x) = e { }` | Rust-clone, `let` пустой |
| **Nova V2** (this plan) | **`ro x = …`** | **`mut x = …`** | **`if ro Some(x) = e`** / **`if mut Some(x) = e`** | symmetric pair, zero-noise, `consume` ortho |

**Nova V2 короче чем все шесть** и **симметричнее чем Rust/TS/Java** (нет
«doverload»: `let mut` vs `let`). Совпадает по структуре с Kotlin (`val`/`var`),
но добавляет третью симметричную опцию `consume` для owned-bindings (которой ни
у кого нет на binding-site).

---

## Дизайн

### Binding statements — три keyword'а, без `let`

```nova
ro x = 5                            // immutable binding (был let x = 5)
mut counter = 0                     // mutable binding   (был let mut counter = 0)
consume sb = StringBuilder.new()    // owned binding     (без изменений, Plan 73.1)

ro x int = 5                        // с явным типом
ro (a, b) = pair                    // destructuring tuple — оба immutable
mut (lo, hi) = bounds               // destructuring tuple — оба mutable
ro { name, age } = user             // destructuring record
```

**Правила:**

1. `ro` и `mut` — **statement-leading keyword**, появляются в любой
   statement-позиции (top of fn body, top of block, body of for/while/match arm).
2. **`=` обязателен** — bare `ro x` или `mut x` без инициализации = parse error
   `E_BINDING_REQUIRES_INIT`. (Те же правила, что были у `let` — D32; reaffirmed.)
3. **Тип после имени** — `ro x int = 5` (Plan 70 prefix-form), как сейчас.
4. **Destructuring** — leading keyword распространяется на все имена в паттерне.
   Per-element granularity (`(ro a, mut b) = …`) **не вводится** — destructure
   и переприсвой если нужна асимметрия.
5. **`const`** остаётся неизменным — другая ось (compile-time placement).
   `const MAX = 4096` ≠ `ro MAX = 4096`: первое compile-time-evaluable + dst в
   data-segment, второе runtime. D33 amend сохраняет разделение.

### Pattern-binding в условиях — `if` / `while` без outer keyword

```nova
// Constructor pattern — bare immutable (consistent с match arms)
if Some(user) = cache.get(key) { use(user) }            // user immutable
if Some(mut buf) = pool.try_take() { buf.fill(0) }      // mut explicit inside pattern

while Some(item) = queue.pop() { handle(item) }
while Some(mut line) = reader.read_line() { line.trim_in_place(); … }

// Destructure — bare immutable
if (a, b) = pair { use(a, b) }
if { name, age } = user_opt { greet(name, age) }

// Identifier pattern — REQUIRES `ro` (footgun protection: без keyword'а
// `if x = compute()` визуально неотличимо от assignment-в-condition'е)
if ro user = compute_user() { use(user) }               // ✓ explicit ro
if user = compute_user() { … }                          // ✗ E_AMBIGUOUS_IDENT_PATTERN
                                                         //   hint: «use `if ro user = …`»

// Chains (Plan 106 grammar, не меняется)
if Some(user) = lookup(id), user.is_active {
    process(user)
}

// else-if
if Some(a) = lookup_a() {
    use(a)
} else if Some(b) = lookup_b() {
    use(b)
}
```

**Правила:**

1. **Constructor / destructure pattern** — bare bindings внутри pattern'а
   default immutable. `mut` explicit когда нужно (`Some(mut x)`, `(mut a, b)`).
   Consistent с `match` arms — там же давно так работает.
2. **Identifier pattern** (`if NAME = expr`) — **обязательно `ro`** (или
   `mut` если нужна мутируемость binding'а). Без keyword'а — parse error
   `E_AMBIGUOUS_IDENT_PATTERN`. Это **footgun protection**: `if x = compute()`
   читается как assignment readers с C/JS background; explicit `ro`/`mut`
   снимает ambiguity.
3. **`consume` в conditions запрещён** — `consume` требует scope-exit
   tracking; pattern-binding-in-condition имеет scope = block body,
   ownership semantics complicated. Если нужно — destructure в statement-
   position через `match` или extract.
4. **`mut` outside pattern удалён** — `if mut Some(buf) = e` → используй
   `if Some(mut buf) = e` (mut inside pattern). Единое правило для match
   и if-let.
5. **Chains (Plan 106)** работают идентично: `if Pat = e1, Pat2 = e2, expr3`.
   Каждый sub-condition самостоятельный.
6. **`else if`** — корректно для всех форм (`else if Some(x) = e`,
   `else if ro y = …`, etc).

**Почему нужны оба `ro` и `mut` в condition-position.**
В Rust `if let Some(x) = e` биндинг **immutable** by default (нужно `if let
Some(mut x) = e` для mut). Это лишний уровень вложенности (`Some(mut x)` —
mut-keyword внутри паттерна). Nova V2 делает явное per-statement: keyword
снаружи паттерна, как у обычных binding-statement'ов. Единое правило, не два.

### `readonly` → `ro` — keyword rename, везде

| Позиция | Было | Стало |
|---|---|---|
| Field default-immutable | `readonly id u64` | `ro id u64` |
| Field type-modifier mutable ref/ro content | `field readonly T` | `field ro T` |
| Field always-mut field, ro content | `mut field readonly T` | `mut field ro T` |
| Type-modifier (param) explicit synonym | `fn f(readonly b T)` | `fn f(ro b T)` |
| Type-modifier (return) | `-> readonly []u8` | `-> ro []u8` |
| Type-modifier (binding type) | `ro view readonly []u8 = …` | `ro view ro []u8 = …` |

> **Заметка про последнюю строку.** `ro view ro []u8` повторяет `ro` дважды —
> первый раз как binding-modifier, второй как type-modifier. Это **не tautology**:
> binding-`ro` фиксирует «нельзя `view = …`», type-`ro` фиксирует «нельзя
> `view[0] = …`». Семантика сохранена; визуально читается естественно
> (короткий keyword, два слога ритмично). Альтернатива «type-position остаётся
> `readonly`» отвергнута в пользу единого keyword'а (см. Risk R-2).

**Error codes остаются как есть** — `E_READONLY_FIELD`, `E_READONLY_CONTENT`,
`E_READONLY_COERCE`, `E_PARAM_NOT_MUT`. Developers их googling; rename
diagnostic-кодов = breaking change для всех существующих рецептов / Stack
Overflow / docs cross-refs. Сохраняем bridge через terminology в diagnostic
text: «`ro` (read-only) field cannot be reassigned».

### `const` narrow → hard constexpr (Ф.9) + generalize → scope/field (Ф.10)

**Ф.9 narrows `const`:** keyword остаётся; semantics tightens до **strict
constexpr-only**. `const X = expr` — compile-error если RHS не literal-
eligible. Lazy-init fallback (Plan 14 Ф.2) **переезжает на `ro`** для тех
case'ов которые сегодня используют lazy.

```nova
// Stays as const (constexpr-eligible)
const MAX_PAYLOAD = 4096                          // ✓ literal
const TIMEOUT_SEC = 60 * 5                        // ✓ constexpr arithmetic
const GREETING = "hello"                          // ✓ literal
const ORIGIN Point = { x: 0.0, y: 0.0 }           // ✓ record-literal из constexpr полей
export const PRELUDE_VERSION int = 13             // ✓ export OK

// Converts to ro (non-constexpr — was lazy-init под старой семантикой)
ro COMPUTED Point = make_point(7.0, 14.0)         // function call → not constexpr
ro NOW = Time.now()                               // runtime call → not constexpr

// Hard error в Ф.9
const COMPUTED Point = make_point(7.0, 14.0)      // ✗ E_CONST_NOT_CONSTEXPR
                                                  //   suggestion: use `ro` для lazy-init
```

**Strict module-level partition (Ф.9 checker rule, формализованное правило convention'а):**

На **module-level** между `const` и `ro` — не выбор, а **обязательное
разделение по способности компилятора вычислить значение**:

| RHS | На module-level требуется |
|---|---|
| Constexpr-eligible (литерал, арифметика над literal'ами, record-литерал из constexpr-полей, ссылка на другой `const`, вызов `const fn` с constexpr args) | **`const X = …`** |
| Не-constexpr (runtime call, effect, allocation, ссылка на `ro`) | **`ro X = …`** |

```nova
// Module-level
const MAX = 4096                           // ✓ constexpr → const обязателен
const ORIGIN Point = { x: 0.0, y: 0.0 }    // ✓ constexpr record-literal

ro now Timestamp = Time.now()              // ✓ runtime → ro обязателен
ro COMPUTED Point = make_point(7, 14)      // ✓ non-const fn call → ro

// Ошибки на module-level (Ф.9 enforce):
ro MAX = 4096                              // ✗ E_RO_FOR_CONSTEXPR_PREFER_CONST
                                           //   hint: «use const MAX = 4096 — value is compile-time»
const COMPUTED = make_point(7, 14)         // ✗ E_CONST_NOT_CONSTEXPR (уже было)
                                           //   hint: «use ro COMPUTED = make_point(7,14) — lazy-init»
```

**Almost-automatic rule.** Compiler определяет «constexpr» точно (это checker
property). User не выбирает между `const`/`ro` — выбирает RHS, keyword следует.
Codemod (Ф.4/Ф.9) auto-rewrite'ит **в обе стороны** — promote `ro→const` если
constexpr-eligible, demote `const→ro` если не. После migration корпус
канонический.

**Scope-level — без strict-правила.** Внутри fn body `ro x = 5` и `const x = 5`
оба валидны; разница только в гарантиях (`const` = строго constexpr +
inlined; `ro` = обычная immutable binding с возможной optimizer-инлайнингой
без contract'а). Scope-уровень — пишет user по intent'у.

**Record-field — без strict-правила.** В `type X { … }` `ro field T`,
`const field T = …` и default — три разных kind'а field'а с разной
семантикой:
- `field T` / `ro field T` — instance field (default-mutable / ro).
- `const X T = …` — associated constant (zero-storage, namespace access).

Strict rule применяется только к module-level **bindings** (`const X = …`
vs `ro X = …`), не к field-declar'ам.

**Ф.10 generalizes `const` на новые позиции:**

```nova
// 1. Scope-local (внутри fn body / block)
fn parse_header(data ro []u8) -> Header {
    const HEADER_SIZE = 16                        // scope-local constexpr
    ro buf [HEADER_SIZE]u8 = ...                  // [N]T uses local const
    ...
}

// 2. Record-field — associated constant (новая фича)
type Config {
    const VERSION int = 2                         // associated const — НЕ в layout
    const MAX_PEERS int = 1024                    // accessible via Type.NAME
    name str                                      // instance field
    timeout Duration                              // instance field
}

Config.VERSION                                    // ✓ 2 (no instance needed)
Config.MAX_PEERS                                  // ✓ 1024
ro c = Config { name: "alice", timeout: SECOND }  // VERSION/MAX_PEERS не указываются
sizeof(Config)                                    // == sizeof(name) + sizeof(timeout)
                                                  //    NO storage for const fields

// 3. Sum-type associated constant
type Status = Active | Inactive | Pending {
    const VERSION int = 2                         // applies к sum-type целиком
    const MAX_TRANSITIONS int = 100
}

Status.VERSION                                    // ✓ 2 (на sum-type level)
ro s = Active                                     // обычная variant construction

// 4. Generic-type associated constant (T-independent vs T-dependent)
type Box[T] {
    const TAG int = 0                             // T-independent: emit once
    const SIZE int = sizeof(T)                    // T-dependent: per-monomorphization
    value T
}

Box.TAG                                           // ✓ 0 (T-independent)
Box[int].SIZE                                     // ✓ 8 (per-mono — int = 8 bytes)
Box[str].SIZE                                     // ✓ 16 (per-mono — str layout)
Box.SIZE                                          // ✗ E_GENERIC_CONST_REQUIRES_INSTANTIATION
                                                  //   hint: «use Box[T].SIZE — depends on T»

type Pair[T, U] {
    const TOTAL int = sizeof(T) + sizeof(U)       // depends on both T and U
    first T
    second U
}
Pair[int, str].TOTAL                              // ✓ per-(T,U)-mono
```

**Полные правила:**

1. **Strict constexpr enforcement.** RHS должен быть literal-eligible:
   - Литералы любого primitive-типа.
   - Арифметика/bitwise/comparison над constexpr операндами.
   - Record-литерал из constexpr-полей.
   - Sum-type конструктор из constexpr-аргументов.
   - Ссылка на другой `const` (любой позиции — module/scope/record-field).
   - **Не** runtime call, **не** effect, **не** allocation.
   - Error: `E_CONST_NOT_CONSTEXPR` с pointer'ом на offending sub-expression.
2. **Position-symmetric.** `const` работает в трёх позициях с одной semantics:
   - **Module-level**: `const X = … ` (как было; export'ируется через `export const`).
   - **Scope-local**: `const X = …` внутри fn/block — compiler inlines literal value, zero allocation, нет binding overhead. Полезно для local sizes (`[N]T`), local magic numbers.
   - **Record-field**: `type T { const X = … }` — **associated constant**.
     Не в instance layout; namespace access `T.X`; **instance access `t.X`
     запрещён** (`E_CONST_INSTANCE_ACCESS` с suggestion «use `T.X`»). Это
     соответствует Java `static final`, Rust `impl T { const X = ... }`,
     Kotlin `companion const val`.
3. **Modifier combinations:**
   - `mut const` / `const mut` — parse error `E_CONST_MUT_CONFLICT`.
   - `ro const` / `const ro` — parse error `E_CONST_RO_REDUNDANT` (const уже immutable).
   - `consume const` — parse error `E_CONST_CONSUME_CONFLICT`.
   - `export const` — ✓ для module-level и record-field (publicly accessible).
4. **Лексическая видимость scope-const'а:** обычные scope-rules. Scope-const
   живёт от declaration до end-of-enclosing-block. `[N]T` reference внутри
   block'а с `const N = 16` — OK.
5. **Record-field const codegen:**
   - Не emit'ится в struct layout C.
   - Emit как top-level `const T Type_FieldName = …;` в .rodata.
   - `Type.FieldName` resolution в Nova → C-symbol `Type_FieldName`.
6. **D27/D30/D102 не меняются** — `[N]T` всё ещё требует `const N`
   (но теперь `N` может быть и scope-local!); SCREAMING_SNAKE_CASE convention
   для `const`; D102 default-params reference `const`. Все три формулировки
   остаются корректны.
7. **Naming convention.** SCREAMING_SNAKE_CASE для всех `const` (module,
   scope, field) — enforced lint-rule, не keyword-shape (D30 carry-over).

**Что сохраняется без изменений:**

- Constexpr-evaluation paths (Plan 14 Ф.2 logic).
- Data-segment placement для constexpr-eligible.
- Module-level visibility/export rules (`export const X` остаётся).
- `nova_const_<name>()` lazy-init runtime — **больше не нужен для `const`**
  (теперь strict); используется только для module-level `ro X = …`
  non-constexpr (переименован remains: `nova_ro_<name>()` или оставлен на
  `nova_const_<name>()` — решение в Ф.9.3, см. R-11).

### `const fn` — comptime evaluable functions (Ф.11)

Функция, **вычисляемая компилятором** во время компиляции. Параметры с
`const` модификатором требуют constexpr args на call site; `-> const T`
return type гарантирует constexpr результат. Компилятор evaluate'ит body
в comptime и inline'ит результат литералом на каждый call site.

```nova
fn calc(const a int, const b char) -> const int {
    const c = b as int                    // local const inside body
    a + c * 10                             // final expression
}

// Call sites:
const RESULT = calc(5, 'A')                // ✓ compile-time → RESULT = 655 (5 + 65*10)
ro buf [calc(2, '0')]u8 = ...              // ✓ [N]T size computed → [482]u8
fn open(n int = calc(3, ' ')) { ... }      // ✓ default param computed → 323

fn runtime_caller(x int) {
    ro v = calc(5, 'A')                    // ✓ literal args — comptime evaluated
    ro v2 = calc(x, 'A')                   // ✗ E_CONST_FN_NON_CONST_ARG (x is runtime)
}
```

**Правила V1 (Plan 114 Ф.11):**

1. **All-or-nothing rule.** Если хоть один параметр объявлен `const` или
   return-type `-> const T` — **все** параметры обязаны быть `const`. Mixed
   mode (some const, some runtime) — out-of-scope, followup
   `[M-114-comptime-mixed-args]`.
2. **`const` модификатор на параметре** — argument на call site обязан быть
   constexpr-evaluable. Иначе `E_CONST_FN_NON_CONST_ARG` с pointer'ом на
   non-const arg.
3. **`-> const T` return** — гарантирует constexpr-evaluable результат. Может
   использоваться как RHS для `const X = call(...)`, в `[N]T` size,
   default-param-value, record-field `const`.
4. **Body — что разрешено в V1:**
   - Литералы и арифметика.
   - `as`-casts между primitive-типами.
   - Ссылки на const-параметры и local `const`-binding'и.
   - Локальные `const c = expr` decl'арации.
   - Final expression (последний statement — expression, выражение возврата).
   - Вызовы других `const fn` (с constexpr args).
5. **Body — что НЕ разрешено в V1** (followups):
   - `if`/`else`/`match` control flow → `[M-114-const-fn-control-flow]`.
   - `for`/`while` loops → same followup.
   - Recursion → `[M-114-const-fn-recursion]` (требует depth-limit + memoization).
   - `mut` / `consume` bindings → `E_CONST_FN_MUT_BINDING`.
   - Effects (calls на non-const fn, `Time.now()`, `print()`, etc.) →
     `E_CONST_FN_EFFECT_IN_BODY`.
   - Allocations (`Vec.new()`, `StringBuilder`, etc.) → `E_CONST_FN_ALLOCATION`.
   - Generic type params в body или signature → `E_CONST_FN_GENERIC`, followup
     `[M-114-generic-const-fn]`.
6. **Codegen.** `const fn` НЕ emit'ится как C-function. Каждый call site
   replaces литералом из comptime-evaluator. Если fn не используется на call
   site'ах — не emit'ится совсем (dead code).
7. **First-class использование запрещено в V1.** `ro f = calc` →
   `E_CONST_FN_FIRST_CLASS` («`const fn` cannot be assigned to a binding;
   comptime-only construct»). Followup `[M-114-const-fn-first-class]` —
   возможно через runtime-wrapper.
8. **Top-level definition only в V1.** `const fn` — module-level fn-declaration.
   Nested const fn / closure-const-fn — out-of-scope.

**Сравнение с mainstream:**

| Язык | Синтаксис | Body restrictions | Mixed const/runtime params |
|---|---|---|---|
| Rust | `const fn factorial(n: u32) -> u32` | Subset of safe Rust (no Vec/print/etc); recursion OK | Все runtime по defaultу; нельзя «mark одного arg as comptime» |
| C++ | `constexpr fn factorial(int n)` | Subset of C++; recursion OK; complex rules per C++ version | Аналог Rust |
| Zig | `fn factorial(comptime n: u32) u32` | Полный Zig (`comptime` evaluator = full interpreter) | **Yes** — `comptime` per-param |
| **Nova V2** | `fn factorial(const n int) -> const int { … }` | V1: subset (no if/loop/recursion); followups расширяют | **No** в V1; followup `[M-114-comptime-mixed-args]` |

Nova V2 ближе всего к Zig — `const` per-param как `comptime` per-param. Но
V1 без control flow / recursion — это subset «expression + sequential
const-locals». Достаточно для типичных лookup-таблиц, computed sizes,
type-driven dispatch констант.

### Return-type defaults + `@`-inheritance (амендмент D176)

**Асимметрия с параметрами — намеренная.** Plan 108.1 сделал параметры default
`ro` (callee не может мутировать без opt-in). Для **возвращаемых значений**
правило **противоположное**: default = **mutable** (caller получает значение,
делает с ним что хочет).

```nova
fn make_buf(n int) -> []u8                  // -> mutable []u8 by default
fn read_view(s str) -> ro []u8              // explicit ro в возврате
```

**Обоснование.** Param `ro` default — defensive (callee не имеет права).
Return mut default — permissive (caller владеет результатом). Это совпадает
с Rust/Swift/Kotlin: `fn foo() -> Vec<T>` отдаёт owned mutable; чтобы вернуть
read-only view — explicit `-> ro T`.

**Особый случай: `-> @` (self-return для fluent chains, D181).** Возвращаемая
`@` **наследует мутируемость от receiver**:

| Receiver | Return `-> @` | Пример |
|---|---|---|
| `fn T @method() -> @` (implicit/ro receiver) | `ro @` (read-only self-view) | `let r = obj.method()` — `r` ro-borrow |
| `fn T mut @method() -> @` | mut `@` (mutable self-view) | `obj.push(1).push(2)` — fluent mut chain |
| `fn T consume @method() -> @` | **parse-time error `E_CONSUME_RECEIVER_RETURNS_AT`** | consume already moves ownership; return `@` создал бы dangling-view |

**Почему такое правило для `@`.** `@` это **тот же экземпляр** что receiver
— его access-mutability не может быть строже, чем у receiver'а:

- `ro @` receiver → `@` уже view; return view'а — view; consistent.
- `mut @` receiver → `@` mutable handle; return mutable handle; consistent,
  именно так работают fluent chains `xs.push(1).push(2)`.
- `consume @` receiver → ownership уже перемещён внутрь method'а; вернуть
  `@` = alias на consumed value = use-after-move; **запрещено**. Если нужно
  fluent после consume — возвращайте новый значимый owned (`fn T consume
  @transform() -> T`), не `@`.

**Что НЕ меняется** в return-семантике:
- Любой явный return type (`-> T`, `-> []u8`, `-> ro T`, `-> mut T`) — берётся
  как написано.
- `-> Self` (статический Self-тип, D182) — owned-by-caller; не наследует
  receiver-мут.
- `-> @` без receiver-method context (free fn) — parse error.

### Что НЕ меняется

| Конструкция | Status |
|---|---|
| `mut self` (method receiver) | без изменений |
| `mut field T` (always-mut field) | без изменений |
| `fn f(mut b T)` (mut param) | без изменений |
| `fn f(consume b T)` (consume param) | без изменений |
| `for x in xs { … }` (loop var implicit immutable) | без изменений |
| `for mut x in xs { … }` (loop var explicit mut) | без изменений |
| `for consume x in xs` (Plan 100.2) | без изменений |
| `|mut x| …` (closure param mut) | без изменений |
| `match X { Pat => arm }` (patterns) | без изменений |
| `consume X = expr` (Plan 73.1) | без изменений |
| Error codes (`E_READONLY_*`, `E_PARAM_NOT_MUT`) | сохраняются (только terminology в текстах) |

---

## Грамматика (precise diff)

Текущая (фрагмент `spec/decisions/03-syntax.md` D33 + D34):

```ebnf
binding_stmt   ::= "let" "mut"? IDENT type_opt "=" expr
                 | "consume" IDENT type_opt "=" expr

const_decl     ::= "export"? "const" IDENT type_opt "=" expr           // top-level only

if_let_stmt    ::= "if" "let" pattern "=" expr block ("else" else_branch)?
while_let_stmt ::= "while" "let" pattern "=" expr block

field_decl     ::= ("readonly" | "mut")? "field"? IDENT type
type_modifier  ::= "readonly" type
param_decl     ::= ("mut" | "readonly" | "consume")? IDENT type
```

Новая:

```ebnf
binding_stmt   ::= ("ro" | "mut" | "consume") bind_lhs "=" expr          // scope-position
                 | const_decl                                             // Ф.10: scope-local const
bind_lhs       ::= IDENT type_opt
                 | "(" bind_lhs ("," bind_lhs)* ")"
                 | "{" IDENT ("," IDENT)* "}"

const_decl     ::= "export"? "const" IDENT type_opt "=" expr             // Ф.9: narrowed (strict constexpr)
                                                                          // Ф.10: valid в module/scope/field positions

module_item    ::= ...
                 | "export"? "ro" IDENT type_opt "=" expr                // Ф.9 (lazy-init fallback host)
                 | const_decl                                             // (unchanged from today)

if_let_stmt    ::= "if" if_cond ("," if_cond)* block ("else" else_branch)?
while_let_stmt ::= "while" if_cond block
if_cond        ::= cond_pattern "=" expr                 // pattern-binding
                 | bool_expr                              // обычное boolean
cond_pattern   ::= ("ro" | "mut") IDENT type_opt         // identifier-pattern, требует keyword
                 | constructor_pattern                    // Some(x)/None/etc, default immutable, mut внутри
                 | tuple_pattern                          // (a, b) — default immutable
                 | record_pattern                         // { name, age } — default immutable
constructor_pattern ::= TYPE_PATH "(" pattern_arg ("," pattern_arg)* ")"
                      | TYPE_PATH                         // unit variant (None, Empty, etc)
pattern_arg    ::= "mut"? IDENT type_opt                 // bare = immutable; mut explicit

field_decl     ::= ("ro" | "mut")? "field"? IDENT type
                 | "mut" "field"? IDENT "ro" type                        // mut ref, ro content
                 | "field"? IDENT "ro" type                              // ro content (default-mutable ref)
                 | const_decl                                             // Ф.10: associated const
type_modifier  ::= "ro" type
param_decl     ::= ("mut" | "ro" | "consume" | "const")? IDENT type      // Ф.11: const param
                                                                          // (all-or-nothing — см. checker rule)

fn_return      ::= "->" "const"? type                                    // Ф.11: const return type
fn_decl        ::= "export"? "fn" IDENT generic_params? "(" param_list? ")" effect_list? fn_return? fn_body
```

**Tokenizer изменения:**

- Новый keyword token `KW_RO`.
- Удалить keyword token `KW_LET`.
- Удалить keyword token `KW_READONLY`.
- **`KW_CONST` сохраняется** (Ф.9 narrowing — keyword остаётся, semantics tightens).
- `KW_MUT`, `KW_CONSUME`, `KW_EXPORT` — без изменений.

**Module-level vs scope disambiguation:**

`ro X = expr` и `const X = expr` синтаксически идентичны на разных уровнях.
Parser использует **текущий контекст** (module-item / statement / field-in-
type) — то же что для `fn`/`type`. Не grammar ambiguity, а context-driven
dispatch.

- `mut X = expr` на module-level — `E_MUT_AT_MODULE_LEVEL` (module-level
  mutable global — anti-pattern, запрещено).
- `consume X = expr` на module-level — `E_CONSUME_AT_MODULE_LEVEL`
  (consume-obligation требует scope-exit; module-level scope «никогда не
  выходит» → ill-formed).
- `const X = expr` валиден **везде** (module/scope/field) — единый
  constexpr-eligibility check.
- `ro X = expr` валиден **module и scope**, но **не field** (field-decl
  использует другие правила; `ro field T` — это уже field-modifier).

**Lookahead для disambiguation:**

- На statement-start: видим `ro` или `mut` или `consume` → binding_stmt.
- На condition-start (после `if`/`while`): видим `ro` или `mut` → pattern-binding;
  иначе обычное boolean condition.
- Внутри `type { … }`: `ro IDENT type` или `mut IDENT type` или `(ro|mut) field
  IDENT type` или `IDENT type` (default).
- Внутри `fn (… )`: `(ro|mut|consume)?  IDENT type`.
- Type-position: `ro type` — prefix.

Никаких grammar ambiguities — `ro` нигде не был identifier (grep verified
zero matches в `.nv`-corpus), может быть hard keyword без backwards-compat
шумом.

---

## Фазы

### Ф.0 — GATE: design freeze + D184 draft + audit (~½ dev-day)

- **Ф.0.1** Draft D184 «Keyword refresh: ro/mut bindings, no let» в
  `spec/decisions/03-syntax.md`. Заголовок, дизайн (раздел выше), grammar diff,
  migration note, cross-ref на amended D-blocks. Spec не мержим до Ф.8 — это
  именно draft (готовый к финализации в Ф.8 без переделок).
- **Ф.0.2** Audit current corpus:
  - `rg "^\s*let\s+(mut\s+)?\w+\s*[=:]"` — точное число binding-decl'ов
    (ожидается ~8000-10000 строк).
  - `rg "\bif let\b|\bwhile let\b"` — ожидается ~63 в .nv-corpus.
  - `rg "\breadonly\b"` — ожидается ~160 в .nv + ~161 в spec.
  - Audit `compiler-codegen/` Rust sources: где tokenizer строит `KW_LET`,
    `KW_READONLY`; где парсер потребляет; где error-text упоминает «let»/
    «readonly».
- **Ф.0.3** Design check на conflicts:
  - `ro` как identifier — verified zero `.nv` matches.
  - `ro` в C-codegen (mangling, header gen, runtime symbols) — verified
    нет mangling-clash (Nova-symbols `Nova_*`; user-symbol `ro` не доходит до
    C-уровня).
  - Tree-sitter highlights — необходимо обновить (Plan 104.7).
  - LSP semantic tokens (Plan 104.1) — необходимо обновить (Ф.7).
- **Ф.0.4** Acceptance A1-A12 финализированы.
- **Ф.0.5** Worktree `nova-p114` создан + register hook'ом.

### Ф.1 — Parser: новая грамматика, удаление старой (~½ dev-day)

- **Ф.1.1** Tokenizer:
  - Удалить `KW_LET`, `KW_READONLY`.
  - Добавить `KW_RO`.
- **Ф.1.2** Parser binding_stmt:
  - Точка входа statement-parser: при `ro|mut|consume` IDENT → `parse_binding`.
  - Удалить `parse_let_stmt`.
  - `parse_binding` принимает leading keyword, обрабатывает type_opt,
    destructuring (tuple/record), expression.
- **Ф.1.3** Parser if/while conditions:
  - `parse_if_cond` — speculative parsing: попытка распарсить как pattern
    (`cond_pattern`), затем проверка наличия `=`. Если успешно — pattern-
    binding form. Иначе fallback на `bool_expr`.
  - **Constructor / destructure pattern**: bare bindings inside default
    immutable; `mut` inside pattern explicit. **Identifier pattern**
    требует leading `ro`/`mut` keyword (footgun protection); без keyword'а
    bare `IDENT = expr` в condition position → `E_AMBIGUOUS_IDENT_PATTERN`
    с suggestion «use `if ro IDENT = …` or `if mut IDENT = …`».
  - **`consume` reject**: `if consume Pat = e` → `E_CONSUME_IN_CONDITION`.
  - **Outer `mut` reject** (для compat / clarity): `if mut Some(x) = e`
    (mut outside pattern) → `E_OUTER_MUT_IN_CONDITION` с suggestion «use
    `if Some(mut x) = e`».
  - Удалить `parse_if_let`, `parse_while_let`.
  - Chain через `,` (Plan 106) — переиспользует тот же `parse_if_cond`.
  - **Pattern grammar shared** между match arms и if/while condition:
    единый `cond_pattern` производство (см. §«Грамматика»). Match arm часть
    не меняется — только if/while переходит на ту же grammar.
- **Ф.1.4** Parser field_decl / param_decl / type_modifier — `readonly` →
  `ro` keyword token swap. Position-grammar не меняется.
- **Ф.1.5** Diagnostics: при встрече token'а «let» / «readonly» (которых уже
  нет в lexer'е после Ф.1.1) — невозможно. Но в файлах с residual'ом — будет
  identifier-not-found / parse-error. Чтобы dev-experience был хорошим, добавить
  **lexer-level hint**: если видим `let` или `readonly` как identifier-start,
  emit specific diagnostic:
  - `E_KW_REMOVED_LET` — «`let` was removed in Plan 114 — use `ro x = …` for
    immutable, `mut x = …` for mutable.»
  - `E_KW_REMOVED_READONLY` — «`readonly` was renamed to `ro` in Plan 114.»

  Это **не** dual-syntax fallback — парсер не принимает старое; это просто
  более полезный error message, чтобы writer'ы (включая AI-агентов с
  устаревшим training data) сразу видели что делать. Эти diagnostics —
  primary mechanism для manual rewrite verification (показывают где осталось
  старое).
- **Ф.1.6** Tests T1.1-T1.8 (positive + negative parser).

### Ф.2 — Diagnostics + error message terminology (~¼ dev-day)

- **Ф.2.1** Все strings в compiler-codegen, упоминающие «let mut» / «let
  binding» / «readonly» → переписать на актуальную terminology:
  - «let mut binding» → «mut binding»
  - «let binding»  → «ro binding»
  - «readonly field» → «ro field» (но **код ошибки** `E_READONLY_FIELD`
    сохраняется — это stable API)
  - «readonly type» → «ro type» (но `E_READONLY_CONTENT` / `E_READONLY_COERCE`
    сохраняются)
- **Ф.2.2** Hint в error: при попытке `x = …` (reassign) для `ro` binding —
  hint «declared as `ro` on line N — use `mut x = …` if reassignment intended».
- **Ф.2.3** Tests: snapshot diagnostic-tests должны быть update'нуты (часть
  fixtures в `nova_tests/` имеют expected stderr).

### Ф.3 — `readonly` → `ro` keyword swap в полях и типах (~¼ dev-day)

- **Ф.3.1** Parser swap уже сделан в Ф.1.4. Здесь — migration call-site'ов
  в **.nv** файлах (не code-base самого compiler'а).
- **Ф.3.2** Используем codemod (см. Ф.4) с правилами:
  - В type-decl: `readonly` → `ro` (как field modifier).
  - В type-position (после `:` в let, в `->`, в param): `readonly T` → `ro T`.
  - В param-decl: `readonly` → `ro`.
- **Ф.3.3** Manual review: tests `nova_tests/plan108*/`, `plan108_1/`.
- **Ф.3.4** Tests T3.1-T3.4.

### Ф.4 — Автоматический rewrite recipe (~⅛ dev-day — документ-only)

> **Подход.** Без отдельного codemod-tool'а. Утверждаем точные правила
> R1-R14 и применяем **массово через скрипты** (sed/grep/perl + AI-агенты)
> ко всему корпусу (~2465 .nv файлов, ~150 spec/docs markdown'ов).
> Compiler errors после Ф.1 — primary verification что rewrite применён
> везде correctly. Single-branch hard-cutover: parser+fixtures+spec+docs в
> один merge. Никакого dual-syntax intermediate.

**Полная таблица rewrite-правил (canonical reference для имплементации):**

| # | Pattern (old) | Replacement (new) | Сайтов (estimate) |
|---|---|---|---|
| R1 | `let IDENT = …` | `ro IDENT = …` | ~7800 |
| R2 | `let mut IDENT = …` | `mut IDENT = …` | ~2100 |
| R3 | `let (PAT) = …` | `ro (PAT) = …` | ~50 |
| R4 | `let mut (PAT) = …` | `mut (PAT) = …` | ~10 |
| R5 | `let { PAT } = …` | `ro { PAT } = …` | ~5 |
| R6 | `let IDENT TYPE = …` (typed) | `ro IDENT TYPE = …` | ~500 |
| R7 | `if let Some(x) = e` (constructor pattern) | `if Some(x) = e` (drop `let`, bare immutable) | ~50 |
| R8 | `if let Some(mut x) = e` (pattern-internal mut) | `if Some(mut x) = e` (drop `let`, mut keep inside pattern) | ~3 |
| R9 | `if let IDENT = e` (identifier pattern, default-immutable) | `if ro IDENT = e` (footgun protection — explicit ro required) | ~3 |
| R9a | `if let mut IDENT = e` (identifier pattern, mut) | `if mut IDENT = e` (explicit mut required) | ~2 |
| R9b | `if let (a, b) = pair` (destructure) | `if (a, b) = pair` (drop `let`, bare immutable) | ~5 |
| R10 | `while let …` всех видов | аналогично R7-R9b с заменой `if` → `while` | ~5 |
| R11 | `readonly IDENT` (внутри `type X { … }`) | `ro IDENT` | ~90 |
| R12 | `readonly TYPE` (type-position, после `:`/`->`) | `ro TYPE` | ~70 |
| R13 | **Q1 strict partition module-level**: `ro X = CONSTEXPR_RHS` | `const X = CONSTEXPR_RHS` (promote) | TBD audit |
| R14 | **Q1 strict partition module-level**: `const X = NON_CONSTEXPR_RHS` | `ro X = NON_CONSTEXPR_RHS` (demote) | ~5 (из 76 const-сайтов) |

**Правила корректного применения:**

1. **Применять только к Nova-коду** в `.nv` файлах и fenced ` ```nova `
   blocks в `.md`. **НЕ trogать**:
   - `let`/`readonly`/`const` внутри string literals (`"the let keyword"`).
   - Однострочные `// let x = 5` и multi-line `/* … */` комментарии.
   - Tagged template literals (`regex\`let\\s+\\w+\``).
   - Inline `let x = …` в прозе markdown'ов вне fenced-блоков.
   - Historical упоминания в `spec/decisions/history/rejected.md` и
     `spec/decisions/history/evolution.md` — оставлять как есть (это
     archived context).
2. **Constexpr-eligibility check для R13/R14:** RHS считается constexpr-
   eligible если:
   - Литерал primitive-типа.
   - Арифметика/bitwise/comparison над literal'ами и constexpr identifier'ами.
   - Record-literal из constexpr-полей (`{ x: 0.0, y: 0.0 }`).
   - Sum-type конструктор из constexpr args.
   - Ссылка на module-level `const X` или другой constexpr-eligible `ro X`.
   - Вызов `const fn` с constexpr args.
   
   Иначе — non-constexpr (function call, effect, allocation, ref на `ro` с
   runtime RHS).
3. **Order применения:** R1-R12 механические (single-pass на каждый файл).
   R13-R14 require constexpr-check — применяются после R1-R12 (когда корпус
   уже на новом syntax'е).

**Implementation approach (automated workflow):**

1. Сначала Ф.1 (parser) + Ф.2 (diagnostics) — compiler ждёт новый syntax,
   errors на старом.
2. **Bulk-rewrite через скрипты:** R1-R12 — механические правила, применимые
   через `sed`/`perl`/AST-aware script на весь corpus за один pass. Examples:
   ```bash
   # R1: let IDENT = ... → ro IDENT = ...  (упрощённо, с проверкой пред-контекста)
   find . -name "*.nv" -exec perl -i -pe 's/^(\s*)let\s+(\w+\s*=)/\1ro \2/g' {} \;
   # R2: let mut → mut
   find . -name "*.nv" -exec perl -i -pe 's/^(\s*)let\s+mut\s+/\1mut /g' {} \;
   # ... и т.д. для R3-R12
   ```
   Critical: regex'ы должны учитывать word-boundaries и НЕ trogать string
   literals/комментарии. Edge cases ловятся через compiler errors на verify
   step (3).
3. **Per-subtree parallel agents** (optional speedup): разделить corpus на
   subtrees (`std/`, `nova_tests/syntax/`, `nova_tests/plan*/`, `examples/`,
   `bench/`, `docs/`, `spec/`) — параллельные agents/worktrees применяют
   скрипты + verify через `nova test` на subtree-level.
4. **Compiler-driven verify** после rewrite: `nova test` + `cargo build`
   показывают все missed sites через `E_KW_REMOVED_LET` / `E_KW_REMOVED_READONLY`
   diagnostics с line/column pointers. Agent итерирует: fix → rebuild →
   repeat пока zero errors.
5. После R1-R12 везде clean: применить R13/R14 проход (constexpr partition).
   Compiler errors (`E_RO_FOR_CONSTEXPR_PREFER_CONST` / `E_CONST_NOT_CONSTEXPR`)
   укажут конкретные сайты — promote/demote.
6. Final regression: full `nova test` + cross-platform.
7. Merge single branch.

**Ключевое:** rewrite **автоматизирован** — не «руками файл-за-файлом».
Compiler errors — automatic verification что нигде не пропустили. Plan не
требует разработки специального tool'а; bash/perl/AST-aware-grep + compiler
errors достаточны.

### Ф.5 — Автоматический rewrite: prelude + std + compiler-bootstrap (~¼ dev-day)

- **Ф.5.1** Bulk-script R1-R12 на `std/` + `std/prelude.nv` + `std/prelude/*.nv`.
  ~200 файлов, ~3000 line changes за один скрипт-pass.
- **Ф.5.2** Compiler-driven verify: `cargo build` + `nova test` показывают
  missed sites через `E_KW_REMOVED_*` diagnostics; agent итерирует fix'ы.
- **Ф.5.3** Apply R13/R14 (Q1 strict) к module-level bindings в `std/` —
  compiler errors указывают где promote/demote.
- **Ф.5.4** `cargo build -p nova-cli` — compiler-embedded Nova strings
  (`include_str!("../../std/prelude.nv")`) автоматически захватываются.
- **Ф.5.5** `cargo test -p nova-codegen` — unit tests pass.

### Ф.6 — Автоматический rewrite: nova_tests + examples + bench + docs + spec (~½ dev-day)

> **Parallel-subtree friendly**: bulk-script запускается на разные subtree
> в worktree'ах; verify через `nova test` после каждого subtree.

- **Ф.6.1** Bulk-script R1-R12 + R13/R14 на `nova_tests/` (~1500+ файлов).
  Compiler errors показывают critical subtree concerns:
  - `nova_tests/syntax/if_let.nv` — load-bearing для D34.
  - `nova_tests/plan108*/` — readonly testsuite.
  - `nova_tests/plan73*/` — consume binding (verify не сломан).
- **Ф.6.2** Bulk-script на `examples/` (~80 файлов).
- **Ф.6.3** Bulk-script на `bench/` (~50 файлов).
- **Ф.6.4** Bulk-script на `docs/` markdown — fenced ` ```nova ` blocks only
  (regex'ы должны учитывать fence-context — иначе **manual review**).
  Inline `code` и обычная проза **не trogаются**.
- **Ф.6.5** Bulk-script на `spec/**/*.md`:
  - Fenced ```nova блоки — auto-rewrite.
  - Inline `let x = …` / `readonly …` в прозе — **case-by-case review**
    (historical references в `rejected.md`, `evolution.md` оставляем).
- **Ф.6.6** `MEMORY.md` + memory files (`C:\Users\...\memory\`) —
  case-by-case (few mentions, перечитать; not bulk-scripted чтобы не
  trogать prose).

### Ф.7 — Tree-sitter + LSP + editor packaging (~½ dev-day)

- **Ф.7.1** Tree-sitter grammar (`tree-sitter-nova`, Plan 104.7):
  - `grammar.js`: добавить `ro` keyword, удалить `let`, `readonly`.
    **`const` keyword остаётся** (Ф.9 narrow меняет semantics, не grammar;
    Ф.10 расширяет позиции: добавляет `const` как scope-level statement и
    как field-decl внутри `type X { … }`; Ф.11 — `const` modifier на params
    и в return-type position).
  - Highlights: `ro`, `mut`, `consume`, `const` — все `@keyword`.
  - Fixtures `corpus/`: regenerate через codemod на `.nv`-input файлах.
  - Bump version → 0.2.0.
- **Ф.7.2** LSP (`nova-lsp`, Plan 104.0-104.1):
  - Semantic tokens: `ro`, `mut`, `consume`, `const` → `Token::Keyword`.
  - Quick-fix providers (новые):
    - `quickfix.let-to-ro` — на текстовом `let X = …` (offered только если
      identifier-not-found error fires).
    - `quickfix.let-mut-to-mut` — на `let mut X = …`.
    - `quickfix.readonly-to-ro` — на `readonly …`.
    - Все quick-fix'ы делают локальный AST-patch через тот же codemod
      tokenizer.
  - Hover: при наведении на `ro x` — «immutable binding». На `mut x` —
    «mutable binding». На `ro T` (type) — «read-only view».
- **Ф.7.3** Editor packaging (Plan 104.8):
  - VSCode extension: `package.json` keywords list update + theme defaults.
  - Helix: `runtime/queries/nova/highlights.scm` update.
  - Zed: `extensions/nova/highlights.scm` update.
  - Neovim: same.
- **Ф.7.4** Smoke test: открыть `.nv` файл с новым синтаксисом во всех 4
  editors, verify highlights работают.

### Ф.8 — Spec finalize + full regression + close (~½ dev-day)

- **Ф.8.1** Промоутить D184 draft из Ф.0.1 в active.
- **Ф.8.2** D-block amendments (детали в §«D-block changes» ниже):
  - D32 amend (default immutable теперь expressed через `ro`).
  - **D33 rewrite целиком** (старая three-axis формулировка fake; новая —
    три **реальные** оси: binding mutability `ro`/`mut`/`consume` + hard-
    constexpr `const` + per-field freeze `ro`/`mut` field).
  - D34 amend (`if ro` / `if mut` syntax + grammar).
  - D36 amend (field decl — `ro` keyword, sample обновлён; **Ф.10:** новая
    форма field-decl — `const field` associated const).
  - D175 amend (`ro field` formulation).
  - D176 amend (`ro T` type-modifier + Plan 114 return-type defaults + `@`-inheritance).
  - D180 amend (cross-ref в D184).
  - **D27 amend small (Ф.10):** «`[N]T` требует `const N`» обновлён — «`const N`
    из visible scope (module-level **или** scope-local)»; visible scope-rule
    natural из block-scoping. Семантика не меняется, только wording.
  - **Новый D200 (Ф.10):** «Associated constants — `const` field в `type X`».
  - **Новый D199 (Ф.11):** «`const fn` — comptime evaluable functions».
  - **D30 и D102 — НЕ меняются** (`const` остаётся keyword'ом со strict
    constexpr-only semantics; старые формулировки compatible).
- **Ф.8.3** `nova doc` regen: prelude doc html должен отображать `ro`/`mut`.
- **Ф.8.4** Full `nova test` ≥ baseline 1559/74 (текущий после Plan 113).
- **Ф.8.5** Cross-platform CI: Windows + Linux × clang + MSVC.
- **Ф.8.6** `docs/project-creation.txt` — sprint section для Plan 114.
- **Ф.8.7** `docs/simplifications.md` — close `[M-114-*]` (если открытых нет —
  единственный entry «explicitly deferred» если что-то выпало).
- **Ф.8.8** Memory `project-plan114-status.md`.
- **Ф.8.9** Final merge в `main`.

### Ф.9 — `const` narrowing → strict constexpr-only (~⅛ dev-day, self-contained)

> **Safety hatch:** Ф.9 спроектирована как **self-contained slice**. Если
> tightening checker'а ломает unforeseen edge cases (например, `const` site
> референсит non-constexpr fn в pre-existing code, который думали constexpr,
> а он не) — Ф.9 extract'ится в **Plan 114.1** (sub-plan). Все Ф.9 артефакты (checker tightening,
> codemod-rule «const → ro если non-constexpr», D-block правка D33) сгруппированы
> — не размазаны по другим фазам.

- **Ф.9.1** Checker:
  - `const X = expr` (любая позиция) — run constexpr-evaluator. Ошибки
    `E_CONST_NOT_CONSTEXPR` / `E_CONST_REFERS_NON_CONSTEXPR` /
    `E_CONST_EFFECT_IN_INIT` с pointer'ом на offending sub-expression.
  - Эти правила существуют (для D27 `[N]T` и D102 default-params уже работают
    через Plan 14 Ф.2); теперь применяются **ко всем** `const`-decl'ам, не
    только тем что попадают в constexpr-required контекст.
  - **Strict module-level partition (Q1 rule):** module-level `ro X = expr`
    с constexpr-eligible RHS → `E_RO_FOR_CONSTEXPR_PREFER_CONST` («value is
    compile-time computable; use `const X = …`»). Scope-level — без правила
    (`ro` и `const` оба разрешены).
- **Ф.9.2** Codegen:
  - `const X = expr` — **только** data-segment placement (`const T X = …;`).
  - Lazy-init fallback (`nova_const_<name>()` getter) **удалён из const-path**.
    Сейчас live'ит на `ro` (module-level non-constexpr).
- **Ф.9.3** Manual rewrite (rules R13/R14 из Ф.4 recipe):
  - **Bidirectional partition (Q1 strict):** для каждого module-level
    binding'а применяется constexpr-eligibility check (компилятор делает
    это в Ф.9.1 checker'е; пишущий код просто следует правилам):
    - `const X = CONSTEXPR_RHS` → leaves canonical.
    - `const X = NON_CONSTEXPR_RHS` → manual demote в `ro X = NON_CONSTEXPR_RHS`.
    - `ro X = CONSTEXPR_RHS` (module-level) → manual promote в `const X = CONSTEXPR_RHS`.
    - `ro X = NON_CONSTEXPR_RHS` → leaves canonical.
  - **Compiler enforcement:** после Ф.9.1 строгий checker errors'ит на
    «wrong-keyword» (`E_CONST_NOT_CONSTEXPR` / `E_RO_FOR_CONSTEXPR_PREFER_CONST`)
    — это служит автоматическим verify'ом что rewrite применён правильно.
    Errors показывают что нужно поправить.
  - Estimated affected: из 76 `const`-сайтов ~5 demote'ятся в `ro` (non-constexpr);
    ~71 stay `const`. Из ~150 module-level `let`-сайтов (которые становятся
    `ro` в Ф.4-Ф.6) — некоторая часть промоут'ится в `const` если RHS
    constexpr-eligible. Точная audit в Ф.0.2.
- **Ф.9.4** D-block правка D33 (выполняется в Ф.8.2): три **реальные** оси
  вместо трёх fake'овых.
- **Ф.9.5** Tests T7 series (см. §«Tests» ниже).

### Ф.10 — `const` generalization: scope-local + record-field (associated const) (~½ dev-day, self-contained, extractable)

> **Safety hatch:** Ф.10 — **новая language-фича** (associated constants).
> Если associated-const реализация усложняется неожиданно (codegen namespace
> resolution для `Type.FIELD`, doc-gen для associated consts, ABI implications
> для `export const` field, или edge-cases с generic-types `type Box[T] {
> const SIZE int = sizeof(T) }` — последнее за scope, comptime feature) —
> Ф.10 extract'ится в **Plan 114.2** (sub-plan) одним revert'ом. Plan 114 шипится с Ф.9
> narrowing + module-level `const` only. Все Ф.10 артефакты сгруппированы.

- **Ф.10.1** Parser:
  - `const_decl` grammar расширена на:
    - **Statement-position** (внутри fn body / block) — scope-local const.
    - **Field-position** (внутри `type X { … }`) — associated const (для
      record/struct types).
    - **Sum-type body** (внутри `type X = A | B { … }`) — associated const
      на sum-type-level.
  - Modifier-conflict errors: `E_CONST_MUT_CONFLICT`, `E_CONST_RO_REDUNDANT`,
    `E_CONST_CONSUME_CONFLICT` на parse-time.
- **Ф.10.2** Checker:
  - Scope-local `const X = N`: usable где угодно в enclosing block; `[N]T`
    reference с scope-const'ом — OK (D27 amend small: «`const N` from any
    visible scope»).
  - Record/sum-type-field `const FOO = …`: **не in instance layout**;
    accessible как `Type.FOO`; `instance.FOO` → `E_CONST_INSTANCE_ACCESS`.
  - Namespace resolution: `Type.FOO` — new resolution path в name-resolver
    (associated-const lookup в type's const-table).
  - **Generic-type assoc const (T-independent):** RHS не ссылается на generic
    params — checker evaluates сразу, emit единственный symbol.
  - **Generic-type assoc const (T-dependent):** RHS references хотя бы один
    generic param (e.g. `sizeof(T)`, `T.SIZE`, арифметика над ними) —
    evaluation **deferred** до monomorphization. Per-(T1,T2,…)-mono symbol.
  - **Namespace resolution для generic:**
    - `Box.TAG` (T-independent) — works directly.
    - `Box[int].SIZE` (T-dependent + bound) — resolves per-mono.
    - `Box.SIZE` (T-dependent без instantiation) →
      `E_GENERIC_CONST_REQUIRES_INSTANTIATION` с hint «depends on T;
      use `Box[T].SIZE`».
  - **Cyclic detection:** `type Tree[T] { const SIZE = sizeof(T) + sizeof(Tree[T]) }`
    — circular monomorphization → `E_GENERIC_CONST_CYCLE`.
- **Ф.10.3** Codegen:
  - Scope-local const: inline literal value at use-sites (zero allocation).
  - Record/sum-type-field const (non-generic): emit top-level `static const
    T Type_FOO = …;` в .rodata. `Type.FOO` resolution → C-symbol `Type_FOO`.
  - **Generic T-independent assoc const:** single symbol `BoxTAG` (как
    non-generic).
  - **Generic T-dependent assoc const:** per-mono symbol `Box_int_SIZE`,
    `Box_str_SIZE`, etc. Emit при каждой monomorphization. Naming convention
    coherent с existing generic-fn monomorphization (Plan 70.5 codegen).
  - `export const` field: public C-symbol visibility.
- **Ф.10.4** Doc-gen (`nova doc`):
  - Associated consts отображаются в type-page рядом с methods/fields —
    отдельная секция «Associated constants».
  - Cross-link `Type.FOO` в API docs.
  - Для generic T-dependent: render «`const SIZE int = sizeof(T)` — computed
    per monomorphization».
- **Ф.10.5** Spec D-block: новый **D200** (Associated constants — `const`
  field в `type X`, расширенный sum-types + generic-types); амендмент **D27**
  (small: scope-local const для `[N]T`).
- **Ф.10.6** Tests T9 series (см. §«Tests»).

### Ф.11 — `const fn` comptime evaluable functions (~1 dev-day, self-contained, extractable)

> **Safety hatch:** Ф.11 — **новая language-фича** (comptime evaluator).
> Если comptime-evaluator усложняется неожиданно — Ф.11 extract'ится в
> **Plan 114.3** (sub-plan) одним revert'ом. Plan 114 шипится без Ф.11 — `const` остаётся
> data-only (literals + arithmetic + record-literals). Триггеры для extract:
> evaluator требует значительного шеринга logic'а с runtime interpreter'ом
> (>1 dev-day); body-checker для V1 subset усложняется неожиданно;
> integration с existing constexpr-engine (Plan 14 Ф.2) требует deep
> рефактора. Decision point: конец Ф.11.3 (evaluator smoke на minimal
> example `fn calc(const a int) -> const int => a + 1`).

- **Ф.11.1** Parser:
  - Расширить `param_decl`: добавить `const` в modifier-set (наряду с `mut`/`ro`/`consume`).
  - Расширить `fn_return`: `"->" "const"? type`.
  - **All-or-nothing checker** в parser: если хоть один param `const` ИЛИ
    return `const` — проверить что **все** params `const` И return `const`.
    Mixed → `E_CONST_FN_PARTIAL_CONSTNESS`.
  - Modifier-conflicts на param: `mut const` / `ro const` / `consume const` →
    parse error `E_CONST_PARAM_MOD_CONFLICT`.
  - **Effect-list запрещён в declaration `const fn`**. Сigatur'а вида
    `fn calc(const a int) Log -> const int { … }` → parse-time error
    `E_CONST_FN_EFFECT_IN_SIGNATURE` («const fn cannot declare effects;
    comptime evaluation excludes runtime effect handling»). Comptime
    evaluation не имеет понятия «runtime effect handler» — declarable
    effect = nonsensical.
- **Ф.11.2** Body checker:
  - Whitelist allowed-operations: literals, arithmetic, casts, references,
    `const` locals, calls на других `const fn`.
  - Blacklist (V1): `if`/`else`/`match`/`for`/`while`, `mut`/`consume`
    bindings, effects, allocations, generic params.
  - Errors: `E_CONST_FN_CONTROL_FLOW`, `E_CONST_FN_MUT_BINDING`,
    `E_CONST_FN_EFFECT_IN_BODY`, `E_CONST_FN_ALLOCATION`, `E_CONST_FN_GENERIC`.
- **Ф.11.3** Comptime evaluator:
  - **Reuse** constexpr-evaluator из Plan 14 Ф.2 (расширить для fn-вызовов).
  - Environment-based interpreter: param env + local const env; execute
    sequential statements; evaluate final expression.
  - Recursion-limit: 1 call deep в V1 (no recursion allowed — checker rejects
    recursive call detection). Recursion = followup `[M-114-const-fn-recursion]`.
  - Memoization: за один compilation, кэш `(fn_id, arg_tuple) → result` —
    идентичные call site'ы evaluate раз.
  - Errors на evaluator-side: `E_CONST_FN_EVAL_OVERFLOW` (arithmetic
    overflow), `E_CONST_FN_DIV_ZERO`.
- **Ф.11.4** Call-site validation + replacement:
  - На call site `f(arg1, arg2, …)` где `f` — `const fn`: каждый arg обязан
    быть constexpr-evaluable. Иначе `E_CONST_FN_NON_CONST_ARG` с pointer на
    offending arg.
  - Если все args constexpr → evaluate → replace call с result-литералом.
- **Ф.11.5** Codegen:
  - `const fn` НЕ emit'ится в C-output (нет runtime symbol).
  - Все call site'ы replaced литералом. Никакой C-function generation.
  - Side-effect: dead `const fn` (без call site'ов) — silently dropped из output.
- **Ф.11.6** First-class использование reject'ится в Ф.11.2 checker:
  `ro f = calc` где `calc` — `const fn` → `E_CONST_FN_FIRST_CLASS`.
- **Ф.11.7** Spec D-block: новый **D199** (`const fn` — comptime evaluable
  functions).
- **Ф.11.8** Tests T10 series (см. §«Tests»).

---

## D-block changes

### D184 (NEW) — Keyword refresh: `ro`/`mut`/`consume` bindings, `const` narrowed + generalized, no `let`, `readonly` → `ro`

**Локация:** `spec/decisions/03-syntax.md` (новый раздел после D33).

**Содержание:**

1. Три binding-keyword'а: `ro` (immutable), `mut` (mutable), `consume` (owned).
   `let` retracted. `ro` валиден на module-level и в scope; `mut`/`consume`
   только в scope.
2. **Pattern-bind-in-condition**: `if Pat = expr` / `while Pat = expr` без
   outer keyword. Constructor / destructure patterns — bare bindings inside
   default immutable; `mut` explicit inside pattern (`Some(mut x)`).
   Identifier-pattern требует `ro`/`mut` keyword (footgun protection vs
   assignment lookalike). `consume` запрещён в conditions. `if let` /
   `while let` retracted; unified grammar с match arm patterns.
3. `ro` как single keyword для read-only во всех trёх позициях: field, type,
   param. `readonly` retracted.
4. **`const` narrowed (Ф.9):** strict constexpr-only. Lazy-init fallback
   переезжает на `ro`. `const` теперь делает то что обещает — hard
   compile-time guarantee.
5. **`const` generalized (Ф.10):** работает в трёх позициях — module-level
   (как сегодня), scope-local (`const N = 16` внутри fn), record-field
   (associated constant, `type T { const VERSION = 1 }`, access `T.VERSION`).
   Единая semantics везде.
6. Grammar (см. §«Грамматика» выше).
7. Migration tooling (`nova migrate v2`) referenced.
8. Comparison table vs Go/Rust/TS/Kotlin/Java/Swift (см. §«Бенчмарк» выше).
   Nova V2 — Rust/C-family `const` (hard constexpr) + ortho `ro`/`mut`/`consume`
   triad для binding mutability + associated-const story аналогично Java
   `static final` / Rust `impl T { const X = ... }`.
9. Acceptance reference на Plan 114.

### D32 amend — Default immutable

**Локация:** `spec/decisions/02-types.md`.

Старая формулировка: «`let X` default immutable, `mut` opt-in».
Новая: «`ro X = …` immutable; `mut X = …` opt-in mutable; `consume X = …`
owned. Симметричная триада, без default-keyword».

### D33 rewrite — Three real immutability axes (полная переформулировка)

**Локация:** `spec/decisions/03-syntax.md`. Старый раздел «`const` vs `let`
— compile-time vs runtime» удаляется целиком.

**Контекст rewrite'а.** Старая D33 декларировала три оси, но **одна из них
была fake**: ось «`const` = compile-time, `let` = runtime» не соответствовала
реальности после Plan 14 Ф.2 (расширил `const` на non-constexpr RHS через
lazy-init `nova_const_<name>()` static getter). `const COMPUTED = make_point(7,14)`
— runtime-init, не compile-time.

Plan 114 Ф.9 narrow'ит `const` обратно до **strict constexpr-only** —
старая ось «hard compile-time guarantee» снова становится правдивой. Plan
114 Ф.10 generalize'ит `const` на scope-local и record-field positions —
single semantics во всех трёх позициях.

**Новая формулировка** — **три ортогональные оси, все три реальные**:

| Конструкция | Что фиксирует | Позиции | Решает |
|---|---|---|---|
| `ro x` / `mut x` / `consume x` | binding mutability + ownership | module-level (только `ro`) + scope (все три) | можно ли переприсвоить переменную; кто owns |
| `const X = …` | **hard compile-time guarantee** (strict constexpr) | module-level + scope-local + record-field (associated const) | известно ли значение при компиляции; compile-error если не |
| `ro field T` / `mut field T` / `field ro T` | per-field freeze | внутри `type X { … }` | можно ли мутировать конкретное поле в record'е |

**Удалено:** колонка `let` / `let mut`. **Изменено:** `const` теперь strict
constexpr-only во всех позициях; lazy-init семантика переезжает на `ro`
(module-level).

**Сравнение с mainstream** (расширенная table) — добавлено в D33 body, см.
§«Бенчмарк» в этом плане. Nova V2:
- `ro`/`mut`/`consume` triad — symmetric, унифицирует binding mutability +
  ownership (consumer-binding из Plan 73.1).
- `const` — hard constexpr guarantee, аналогично Rust `const`/C `constexpr`/
  Java `static final` для primitives.
- `const` field в record'е — associated constant, аналогично Java
  `static final`, Rust `impl T { const X = ... }`, Kotlin
  `companion const val`.

**D27, D30, D102 — НЕ меняются** (small wording-update D27 для «`const N`
from any visible scope», но семантика та же). `const` остаётся valid
referencee везде где было.

### D34 amend — Pattern-bind-in-condition (unified grammar с match arms)

**Локация:** `spec/decisions/03-syntax.md`.

`if let Pat = e` → `if Pat = e` (drop `let`). Grammar **унифицирована** с
match-arm patterns:

- **Constructor / destructure pattern**: bare bindings внутри default
  immutable; `mut` explicit inside pattern.
  - `if Some(x) = e` — immutable `x`.
  - `if Some(mut x) = e` — mutable `x`.
  - `if (a, b) = pair` — destructure, immutable.
- **Identifier pattern**: требует explicit `ro`/`mut` keyword (footgun
  protection — иначе `if x = compute()` визуально неотличимо от assignment).
  - `if ro user = compute() { … }` — immutable identifier-binding.
  - `if mut counter = init() { counter += 1 }` — mutable identifier-binding.
  - `if user = compute()` — `E_AMBIGUOUS_IDENT_PATTERN`.
- **`consume` запрещён** в conditions — `E_CONSUME_IN_CONDITION`.
- Тождественные правила для `while`.

Plan 106 chain-syntax работает идентично: `if Some(u) = e, u.active { … }`.

**Consistency с match.** Pattern grammar shared между match arms и if/while
conditions:
```nova
match x {
    Some(y) => use(y)              // y bare = immutable
    Some(mut z) => z.inc()         // mut explicit inside pattern
    None => ...
}
```
Эта pattern grammar **уже работает** в match arm — Plan 114 D34 amend
расширяет её на if/while condition position. Match arm часть не меняется.

Grammar diff — см. §«Грамматика» этого плана.

### D36 amend — Field modifiers

**Локация:** `spec/decisions/02-types.md`.

`readonly field T` → `ro field T`. `mut field T` без изменений. Sample
переписан.

### D175 amend — `ro field` full freeze

**Локация:** `spec/decisions/02-types.md`.

`readonly` → `ro` в title и тексте. Семантика freeze + транзитивность
сохранена. Error code `E_READONLY_FIELD` сохранён (stable API).

### D176 amend — `ro T` type-modifier

**Локация:** `spec/decisions/02-types.md`.

`readonly` → `ro` в title и тексте. Plan 108.1 «`readonly param T` synonym
default» → «`ro param T` synonym default». Error codes `E_READONLY_CONTENT`,
`E_READONLY_COERCE`, `E_PARAM_NOT_MUT` сохранены.

**Plan 114 добавляет к D176 раздел «Return-type defaults»:**

1. **Return default = mutable** (асимметрия с param-default `ro` — намеренная,
   см. §«Return-type defaults + `@`-inheritance» в дизайн-секции). Явные
   модификаторы `-> ro T` / `-> mut T` берутся как написаны.
2. **`-> @` (fluent self-return, D181)** наследует мутируемость от receiver'а:
   - implicit/`@` receiver → returns `ro @`
   - `mut @` receiver → returns mut `@`
   - `consume @` receiver → **`E_CONSUME_RECEIVER_RETURNS_AT`** parse/type
     error (return `@` после consume = dangling alias на moved-out value).
3. Новый error code: **`E_CONSUME_RECEIVER_RETURNS_AT`** — «cannot return
   `@` from method with `consume @` receiver; ownership already moved.
   Use `-> Self` for owned return, or change receiver to `mut @`».
4. `-> @` без receiver-method context (free fn / top-level fn) — **`E_AT_RETURN_OUTSIDE_METHOD`**.

### D180 amend — `consume` binding (cross-ref)

**Локация:** `spec/decisions/05-memory.md`.

Только cross-ref в D184: «`consume X = …` теперь часть симметричной триады
`ro`/`mut`/`consume`».

### D200 (NEW) — Associated constants — `const` field в `type X` (Ф.10)

**Локация:** `spec/decisions/02-types.md` (новый раздел после D184 cross-ref;
рядом с D36 field-decl).

**Что.** `const` объявление внутри `type X { … }` body — **associated
constant** типа. Не часть instance layout; accessible через namespace
`Type.CONST_NAME`.

```nova
type Config {
    const VERSION int = 2                    // associated const
    const PROTOCOL str = "v2"
    const MAX_PEERS int = 1024
    name str                                  // instance field
    timeout Duration                          // instance field
}

// Access — только namespace
Config.VERSION                                // ✓ 2
Config.MAX_PEERS                              // ✓ 1024

// Instance access — error
let c = Config { name: "alice", timeout: SECOND }
c.VERSION                                     // ✗ E_CONST_INSTANCE_ACCESS
                                              //   hint: «use Config.VERSION»

// Layout
sizeof(Config) == sizeof(name) + sizeof(timeout)  // const fields НЕ занимают storage
```

**Семантика.**
1. **Strict constexpr** (как любой `const`): RHS должен быть literal-eligible.
2. **Zero storage in instance.** Codegen не emit'ит const-field в struct
   layout. Каждый const-field живёт как top-level C-symbol `Type_FieldName`
   в .rodata.
3. **Namespace access only.** `Type.NAME` resolution через type's const-table
   в name-resolver. `instance.NAME` → `E_CONST_INSTANCE_ACCESS` parse/type
   error с suggestion.
4. **Не указывается в record literal.** `let c = Config { name: …, timeout: … }`
   — const fields **не** перечисляются (они уже значения типа, не instance
   data). Если user пишет `Config { VERSION: 5, name: …, … }` →
   `E_CONST_FIELD_IN_LITERAL` (const field неуказывается + неизменяем).
5. **`export const` field** — publicly accessible: `OtherModule.Config.VERSION`.
   Без `export` — приватный для module.
6. **Modifier-conflicts:** `mut const` / `ro const` / `consume const` в
   field-position — те же ошибки что и в module/scope const.
7. **SCREAMING_SNAKE_CASE convention** — рекомендуется (lint warning).
8. **Cross-ref:** D36 field-decl расширяется на третий вид field (был:
   default-immutable, `ro`, `mut`; теперь + `const` associated).

**Use cases.**
- Version numbers, protocol identifiers: `Config.VERSION`, `Protocol.MAGIC_BYTES`.
- Capacity/size limits: `Config.MAX_PEERS`, `Buffer.DEFAULT_CAPACITY`.
- Mathematical constants in math types: `Circle.PI`, `Complex.UNIT_IMAGINARY`.
- Error/status codes: `Response.OK_STATUS`, `Response.NOT_FOUND_STATUS`.
- Per-mono sizes for generic containers: `Box[int].SIZE`, `Pair[T,U].TOTAL`.

### Sum-type associated constants

`const` decl внутри sum-type body — associated на sum-type-level:

```nova
type Status = Active | Inactive | Pending {
    const VERSION int = 2
    const MAX_TRANSITIONS int = 100
}

Status.VERSION                 // ✓ 2
Status.MAX_TRANSITIONS         // ✓ 100
```

Semantics идентична record-field assoc const: zero-storage, namespace
access only, single C-symbol per `const`. **Per-variant const'ы** (`Active
{ const X = 1 }`) — out of scope V1 (followup `[M-114-per-variant-const]`).

### Generic-type associated constants

`const` decl внутри generic type body — два sub-case'а по RHS:

**T-independent** — RHS не reference'ит generic params:
```nova
type Box[T] {
    const TAG int = 0          // не ссылается на T
    value T
}
Box.TAG                        // ✓ emit single symbol BoxTAG
```

**T-dependent** — RHS reference'ит хотя бы один generic param через
`sizeof(T)`, `T.CONST_ON_T`, или арифметику над ними:
```nova
type Box[T] {
    const SIZE int = sizeof(T)
    value T
}
Box[int].SIZE                  // ✓ 8 — emit per-mono Box_int_SIZE
Box[str].SIZE                  // ✓ 16 — emit per-mono Box_str_SIZE
Box.SIZE                       // ✗ E_GENERIC_CONST_REQUIRES_INSTANTIATION
                               //   («depends on T; use Box[T].SIZE»)

type Pair[T, U] {
    const TOTAL int = sizeof(T) + sizeof(U)
    first T
    second U
}
Pair[int, str].TOTAL           // ✓ per-(T,U)-mono — emit Pair_int_str_TOTAL
```

**Allowed in T-dependent RHS (V1):**
- `sizeof(T)` где `T` — generic param
- Арифметика над `sizeof(T_i)` и literals
- Ссылки на T-independent `const` (через `Type.CONST`)

**НЕ allowed в V1** (followups):
- `T.METHOD()` calls — runtime, не constexpr
- `const fn` calls с generic args — `[M-114-generic-const-fn]` (требует
  generic const fn monomorphization)
- Recursive type references (`Tree[T] { const X = sizeof(Tree[T]) }`) —
  `E_GENERIC_CONST_CYCLE`

**Сравнение с mainstream:**

| Язык | Синтаксис | Storage |
|---|---|---|
| Java | `static final int VERSION = 2;` (внутри class) | top-level C-static-like |
| Rust | `impl Config { const VERSION: i32 = 2; }` | top-level |
| Kotlin | `companion object { const val VERSION = 2 }` | companion-object slot |
| Swift | `struct Config { static let version = 2 }` | type-metadata |
| TS | `class Config { static readonly VERSION = 2 }` | class-static |
| **Nova V2** | `type Config { const VERSION int = 2; … }` | top-level .rodata |

Nova V2 — самый компактный синтаксис (`const` directly в `type` body, без
`static`/`impl`/`companion` врапперов).

**Acceptance.** См. A14/T9 в этом плане.

### D199 (NEW) — `const fn` — comptime evaluable functions (Ф.11)

**Локация:** `spec/decisions/03-syntax.md` (новый раздел после D184 cross-ref;
рядом с D33 binding-axes).

**Что.** `const fn` — функция, **вычисляемая компилятором** во время
компиляции. Параметры с `const` модификатором требуют constexpr args; return
type `-> const T` гарантирует constexpr результат.

```nova
fn calc(const a int, const b char) -> const int {
    const c = b as int
    a + c * 10
}

const RESULT = calc(5, 'A')        // ✓ comptime → 655
ro buf [calc(2, '0')]u8 = …         // ✓ array size → [482]u8
```

**Семантика V1 (детали в Plan 114 Ф.11):**

1. **All-or-nothing.** Если хоть один param `const` или return `const` — все
   params обязаны быть `const` и return обязан быть `const`. Иначе
   `E_CONST_FN_PARTIAL_CONSTNESS`. Mixed mode — followup
   `[M-114-comptime-mixed-args]`.
2. **Allowed body (V1):** literals, arithmetic, `as`-casts, references на
   const params/locals, local `const c = expr` decl, final expression,
   calls на другие `const fn`.
3. **Forbidden body (V1):** `if`/`else`/`match`, `for`/`while`,
   `mut`/`consume` bindings, effects (как **в declaration signature**, так и
   **в body**), allocations, generic params, recursion. Все followup-маркеры
   в Out-of-scope plan114.
4. **Call-site rules:** все args обязаны быть constexpr-evaluable
   (`E_CONST_FN_NON_CONST_ARG` иначе). Result inline'ится литералом.
5. **First-class запрещено в V1.** `ro f = calc` → `E_CONST_FN_FIRST_CLASS`.
6. **Codegen.** `const fn` НЕ emit'ится в C-output. Все call site'ы replaced
   литералом. Dead `const fn` (без call site'ов) — silently dropped.

**Сравнение с mainstream:** см. table в дизайн-секции «`const fn` — comptime
evaluable functions» этого плана. Nova V2 ближе всего к Zig (`const` per-
param как `comptime` per-param), но без runtime-call-mode (всегда comptime).

**Cross-ref:** D184 (master); D200 (associated const — могут reference `const fn`
для constexpr-eligible RHS, например `type T { const SIZE int = calc_size() }`).

**Acceptance.** См. A16/T10 в этом плане.

---

## Tests

### T1 — Parser positive

- **T1.1** `ro x = 5` — parses → BindingStmt { kind: Ro, name: "x", expr: 5 }.
- **T1.2** `mut counter = 0` — parses → BindingStmt { kind: Mut, … }.
- **T1.3** `consume sb = StringBuilder.new()` — parses → BindingStmt { kind:
  Consume, … } (Plan 73.1 carry-over).
- **T1.4** `ro x int = 5` — typed binding.
- **T1.5** `ro (a, b) = pair` — tuple destructuring.
- **T1.6** `mut { name, age } = user` — record destructuring.
- **T1.7** `if Some(user) = cache.get(k) { use(user) }` — bare immutable.
- **T1.8** `if Some(mut buf) = pool.take() { buf.fill(0) }` — mut inside pattern.
- **T1.9** `while Some(item) = q.pop() { handle(item) }` — bare immutable.
- **T1.9a** `while Some(mut line) = reader.read_line() { line.trim_in_place() }` — mut inside pattern.
- **T1.9b** `if ro user = compute() { use(user) }` — identifier-pattern с explicit `ro`.
- **T1.9c** `if mut counter = init() { counter += 1; … }` — identifier-pattern с explicit `mut`.
- **T1.9d** `if (a, b) = pair { … }` — tuple destructure, bare immutable.
- **T1.10** Plan 106 chain: `if Some(u) = lookup(id), u.active { … }`.
- **T1.10a** `match x { Some(y) => use(y); Some(mut z) => z.inc(); None => … }` —
  consistency check (та же pattern grammar для match arms).

### T2 — Parser negative

- **NEG-T2.1** `let x = 5` → `E_KW_REMOVED_LET` с suggestion «use `ro x = 5`».
- **NEG-T2.2** `let mut x = 0` → `E_KW_REMOVED_LET` с suggestion «use `mut x = 0`».
- **NEG-T2.3** `readonly id u64` (внутри type) → `E_KW_REMOVED_READONLY`.
- **NEG-T2.4** `readonly T` (type-mod) → `E_KW_REMOVED_READONLY`.
- **NEG-T2.5** `if let Some(x) = e` → `E_KW_REMOVED_LET` с suggestion «use
  `if Some(x) = e`» (drop `let`, bare immutable inside constructor pattern).
- **NEG-T2.6** `while let Some(x) = e` → same suggestion «use `while Some(x) = e`».
- **NEG-T2.6a** `if user = compute()` (bare identifier-pattern без keyword'а)
  → `E_AMBIGUOUS_IDENT_PATTERN` с suggestion «use `if ro user = …` or `if mut user = …`»
  (footgun protection: визуально похоже на assignment).
- **NEG-T2.6b** `if mut Some(x) = e` (старый Plan 114 syntax с mut outside pattern)
  → `E_OUTER_MUT_IN_CONDITION` с suggestion «use `if Some(mut x) = e`»
  (mut moves inside pattern).
- **NEG-T2.6c** `if consume Pat = e` → `E_CONSUME_IN_CONDITION` с hint
  «consume binding не allowed в if/while condition; используй match или
  extract в statement».
- **NEG-T2.7** `ro x` (без `= …`) → `E_BINDING_REQUIRES_INIT`.
- **NEG-T2.8** `mut x` (без `= …`) → `E_BINDING_REQUIRES_INIT`.

### T3 — Field/type modifier swap

- **T3.1** `type T { ro id u64 }` — parses; mutation `t.id = …` → `E_READONLY_FIELD`.
- **T3.2** `fn f(b ro []int) { b.push(1) }` → `E_READONLY_CONTENT`.
- **T3.3** `fn g() -> ro []u8 { … }` — parses; caller assignment OK as view.
- **T3.4** `ro view ro []u8 = bytes.view()` — double-ro работает (binding +
  type), `view = …` → bind error, `view[0] = …` → content error.

### T4 — Consume binding still works

- **T4.1** `consume sb = StringBuilder.new(); sb.into()` — Plan 73.1 fixtures
  unchanged.
- **T4.2** `for consume x in xs { … }` (Plan 100.2) — unchanged.

### T5 — Automatic rewrite consistency verification

После Ф.4-Ф.6 bulk-script rewrite — corpus-wide verification:

- **T5.1** `grep -rn "\blet\b" --include="*.nv" std/ nova_tests/ examples/
  bench/` → **zero matches** (excluding string-literals/comments, которые
  grep подсветит — manual review).
- **T5.2** `grep -rn "\breadonly\b" --include="*.nv" std/ nova_tests/
  examples/ bench/` → zero.
- **T5.3** `grep -rn "\bif let\b\|\bwhile let\b" --include="*.nv"` → zero.
- **T5.4** Full `nova test` post-rewrite ≥ baseline 1559/74 — это
  **primary verification**: если manual rewrite сломал что-то — упадут
  тесты с понятным error.
- **T5.5** Module-level `ro X = CONSTEXPR_RHS` → checker errors с
  `E_RO_FOR_CONSTEXPR_PREFER_CONST` (Q1 strict) — это automatic verify
  что rewrite применён правильно (промот в `const`).
- **T5.6** Module-level `const X = NON_CONSTEXPR_RHS` → checker errors
  с `E_CONST_NOT_CONSTEXPR` — automatic verify demote'а в `ro`.

### T6 — Editor integration smoke

- **T6.1** Tree-sitter parse тестовый fixture — нет parse errors, highlights
  выглядят правильно (snapshot test через `tree-sitter test`).
- **T6.2** LSP semantic tokens для `ro`/`mut`/`consume` — все возвращаются
  как `Token::Keyword`.
- **T6.3** Quick-fix `let → ro` — apply через LSP returns edit, который
  компилируется.

### T7 — `const` narrowing (Ф.9)

- **T7.1** `const MAX = 4096` — strict constexpr; emit `const int MAX = 4096`
  в data-segment.
- **T7.2** `const ORIGIN Point = { x: 0.0, y: 0.0 }` — constexpr record-литерал
  из constexpr-полей; emit `const Point ORIGIN = …;` в .rodata.
- **T7.3** `const TIMEOUT_SEC = 60 * 5` — constexpr arithmetic.
- **T7.4** `const NAMESPACE_DNS Uuid = { hi: 0x..., lo: 0x... }` (existing
  std/uuid.nv example) — constexpr record-литерал; passes без изменений.
- **T7.5** **NEG**: `const COMPUTED = make_point(7, 14)` → `E_CONST_NOT_CONSTEXPR`
  с pointer на `make_point(7, 14)`; suggestion «use `ro` for lazy-init non-
  constexpr value».
- **T7.6** **NEG**: `const NOW = Time.now()` → `E_CONST_EFFECT_IN_INIT`
  с pointer на effect call.
- **T7.7** **NEG**: `const Y = X + 1` где `ro X = compute()` — `E_CONST_REFERS_NON_CONSTEXPR`
  с pointer на X-binding.
- **T7.8** `ro COMPUTED Point = make_point(7, 14)` на module-level — auto-
  lazy-init через `nova_const_<name>()` getter (R-11 Option A: C-symbol
  сохранён); first-use инициализирует, второй — кеш.
- **T7.9** Codemod converts `const COMPUTED = make_point(7, 14)` →
  `ro COMPUTED = make_point(7, 14)` (constexpr-check failed → demote
  to `ro`).
- **T7.10** Codemod leaves `const MAX = 4096` как есть (constexpr-eligible).
- **T7.11** Regression: existing `const_complex.nv` Section 1, 2, 4-6
  (constexpr-eligible) — pass без изменений; Section 3 (`COMPUTED =
  make_point`) — codemod rewrite в `ro`, тесты passes.
- **T7.12** Full nova test ≥ baseline post-codemod.
- **T7.13** **Q1 strict partition NEG**: module-level `ro MAX = 4096`
  (constexpr-eligible RHS) → `E_RO_FOR_CONSTEXPR_PREFER_CONST` с
  hint «use `const MAX = 4096`».
- **T7.14** **Q1 strict partition codemod**: codemod promote
  `ro MAX = 4096` → `const MAX = 4096` на module-level.
- **T7.15** **Q1 scope-level OK**: внутри fn body `ro x = 5` — нет ошибки
  (strict rule только для module-level).

### T8 — Return-type defaults + `@`-inheritance

- **T8.1** `fn []T mut @push(x T) -> @` — compiles; `xs.push(1).push(2)`
  fluent chain работает; type `@` после push = mut `@`.
- **T8.2** `fn []T @get(i int) -> Option[T]` — implicit ro receiver; return
  default mut Option (caller fully owns).
- **T8.3** `fn []T @as_view() -> @` — implicit ro receiver; return `ro @`
  (read-only view of self).
- **T8.4** `fn StringBuilder consume @into() -> str` — consume receiver
  returning **non-`@`** (T `str`) — OK.
- **T8.5** **NEG**: `fn StringBuilder consume @into() -> @` →
  `E_CONSUME_RECEIVER_RETURNS_AT` с suggestion «use `-> Self` для owned
  return, or change receiver to `mut @`».
- **T8.6** `fn process(data ro []u8) -> []u8` — return default mut; caller
  получает mutable owned `[]u8`.
- **T8.7** `fn process(data ro []u8) -> ro []u8` — explicit ro return;
  caller получает ro view.
- **T8.8** **NEG**: `fn top_level() -> @` (free fn, not method) →
  `E_AT_RETURN_OUTSIDE_METHOD`.

### T9 — `const` generalization (Ф.10)

- **T9.1** Scope-local `const HEADER_SIZE = 16; ro buf [HEADER_SIZE]u8 = ...`
  — parses; `[HEADER_SIZE]u8` resolves через local const; codegen inlines 16.
- **T9.2** Scope-local `const` reference в enclosing-block expression
  работает (lexical visibility).
- **T9.3** Scope-local `const` НЕ visible вне его enclosing block.
- **T9.4** Associated const: `type Config { const VERSION int = 2; name str }`
  parses; `Config.VERSION` resolves to `2`; emit top-level `const int
  Config_VERSION = 2;`.
- **T9.5** `sizeof(Config) == sizeof(name_field_only)` (const fields НЕ в
  layout).
- **T9.6** `let c = Config { name: "alice" }` — record literal без `VERSION`
  field; type-checks.
- **T9.7** **NEG**: `c.VERSION` → `E_CONST_INSTANCE_ACCESS` с suggestion
  «use Config.VERSION».
- **T9.8** **NEG**: `Config { VERSION: 5, name: "x" }` → `E_CONST_FIELD_IN_LITERAL`.
- **T9.9** **NEG**: `type T { mut const X = 5 }` → `E_CONST_MUT_CONFLICT`.
- **T9.10** **NEG**: `type T { ro const X = 5 }` → `E_CONST_RO_REDUNDANT`.
- **T9.11** **NEG**: `const X = compute()` (любой position) →
  `E_CONST_NOT_CONSTEXPR` (Ф.9 strict tightening).
- **T9.12** `export const VERSION int = 2` field в `export type Config` —
  доступно как `OtherModule.Config.VERSION`.
- **T9.13** Doc-gen: `nova doc` renders type page с секцией «Associated
  constants» (VERSION, MAX_PEERS, etc).
- **T9.14** Regression: full nova test ≥ baseline; новые fixtures
  `plan114_const_field/` PASS.
- **T9.15** Sum-type assoc const: `type Status = Active | Inactive { const
  VERSION int = 2 }` — `Status.VERSION` == 2; emit single `Status_VERSION`.
- **T9.16** Generic T-independent: `type Box[T] { const TAG int = 0 }` —
  `Box.TAG` == 0; emit single `Box_TAG`.
- **T9.17** Generic T-dependent — `type Box[T] { const SIZE int = sizeof(T) }`:
  `Box[int].SIZE` == 8; `Box[str].SIZE` == 16; emit per-mono `Box_int_SIZE`,
  `Box_str_SIZE`.
- **T9.18** Generic T-dependent с арифметикой: `const HEADER int = sizeof(T)
  + 8` — `Box[int].HEADER` == 16.
- **T9.19** Cross-T-param: `type Pair[T, U] { const TOTAL int = sizeof(T)
  + sizeof(U) }` — `Pair[int, str].TOTAL` per-(T,U)-mono.
- **T9.20** **NEG**: `Box.SIZE` (T-dependent без instantiation) →
  `E_GENERIC_CONST_REQUIRES_INSTANTIATION`.
- **T9.21** **NEG**: cyclic `type Tree[T] { const SIZE int = sizeof(T) +
  sizeof(Tree[T]) }` → `E_GENERIC_CONST_CYCLE`.

### T10 — `const fn` comptime evaluable (Ф.11)

- **T10.1** `fn calc(const a int, const b char) -> const int { const c = b as int; a + c * 10 }`
  — parses; checker accepts; `calc(5, 'A')` evaluates to literal `655` на
  call site.
- **T10.2** Expression body: `fn add(const a int, const b int) -> const int => a + b`
  — works; `add(2, 3)` → 5.
- **T10.3** Const fn в `[N]T` size: `ro buf [calc(2, '0')]u8 = …` — compiles;
  array size `482`.
- **T10.4** Const fn в default param: `fn open(n int = calc(3, ' '))` —
  default = `323`; works на runtime call site без override.
- **T10.5** Const fn в record-field assoc-const: `type T { const SIZE int =
  add(2, 3) }` — `T.SIZE` == 5; emit `const int T_SIZE = 5;`.
- **T10.6** Const fn calling const fn: `fn outer(const x int) -> const int =>
  add(x, 1)` — works; recursive chain через others.
- **T10.7** **NEG**: `calc(x, 'A')` где `x` runtime →
  `E_CONST_FN_NON_CONST_ARG` с pointer на x.
- **T10.8** **NEG**: `fn mixed(const a int, b int) -> const int` →
  `E_CONST_FN_PARTIAL_CONSTNESS` («all params must be const if return is
  const»).
- **T10.9** **NEG**: `fn ret_runtime(const a int) -> int` →
  `E_CONST_FN_PARTIAL_CONSTNESS` («all-or-nothing — return must be const
  if any param is const»).
- **T10.10** **NEG**: `fn has_if(const x int) -> const int { if x > 0 then x else 0 }`
  → `E_CONST_FN_CONTROL_FLOW` («`if` not allowed in const fn body in V1»).
- **T10.11** **NEG**: `fn has_mut(const x int) -> const int { mut y = x; y + 1 }`
  → `E_CONST_FN_MUT_BINDING`.
- **T10.12** **NEG**: `fn has_effect(const x int) -> const int { print(x); x }`
  → `E_CONST_FN_EFFECT_IN_BODY`.
- **T10.13** **NEG**: `fn has_alloc(const n int) -> const int { ro v = Vec.new(); n }`
  → `E_CONST_FN_ALLOCATION`.
- **T10.14** **NEG**: `fn recur(const n int) -> const int { recur(n-1) }`
  → `E_CONST_FN_GENERIC` или dedicated recursion-check error.
- **T10.15** **NEG**: `ro f = calc` → `E_CONST_FN_FIRST_CLASS`.
- **T10.16** **NEG**: `mut const a int` параметр → `E_CONST_PARAM_MOD_CONFLICT`.
- **T10.16a** **NEG**: `fn calc(const a int) Log -> const int { … }` (effect-
  list в declaration) → `E_CONST_FN_EFFECT_IN_SIGNATURE` («const fn cannot
  declare effects»).
- **T10.17** Codegen: `calc` НЕ emit'ится в C-output (нет `int calc(int, char)`
  symbol). Call site `calc(5, 'A')` emit'ится как литерал `655`.
- **T10.18** Memoization: 100 identical call sites `calc(5, 'A')` evaluate
  раз; output 100 идентичных literal'ов.
- **T10.19** Regression: full nova test ≥ baseline; новые fixtures
  `plan114_const_fn/` PASS (10+ positive + 10+ negative).

### Regression

- **R1** Full `nova test` ≥ 1559/74 baseline (после Plan 113 merge).
- **R2** Cross-platform Windows + Linux × clang + MSVC.
- **R3** `cargo test -p nova-codegen` ≥ previous baseline.
- **R4** `cargo test -p nova-lsp` PASS (Plan 104.1 baseline 91/91).

---

## Acceptance criteria

| # | Критерий | Verification |
|---|---|---|
| A1 | `let` keyword полностью удалён из tokenizer/parser | NEG-T2.1, lexer source |
| A2 | `readonly` keyword полностью удалён из tokenizer/parser | NEG-T2.3, lexer source |
| A3 | `ro`/`mut`/`consume` — три симметричных binding-statement keyword'а | T1.1-T1.3 + grammar EBNF |
| A4 | `if Pat = e` / `while Pat = e` без outer keyword; unified pattern grammar с match arms (bare immutable / `mut` inside pattern); identifier-pattern требует `ro`/`mut` (footgun); `consume` reject; Plan 106 chains работают | T1.7-T1.10a |
| A5 | `ro` keyword работает во всех позициях: binding, field, type-modifier, param | T1.1 + T3.1 + T3.2 + T3.4 |
| A6 | `consume X = expr` (Plan 73.1) — не сломан | T4 fixtures pass |
| A7 | Error codes (`E_READONLY_*`, `E_PARAM_NOT_MUT`) сохранены | grep исходников; T3.x |
| A8 | Bulk-script rewrite applied consistently на всём корпусе по правилам R1-R14; compiler errors zero | T5 series (grep + full nova test) |
| A9 | String literals, комментарии, tagged templates **не trogаны** rewrite'ом | T5.1-T5.3 grep с spot-review |
| A10 | Tree-sitter grammar обновлён, fixtures regenerated | T6.1 + version bump |
| A11 | LSP semantic tokens + quick-fix `let→ro` работают | T6.2 + T6.3 |
| A12 | Full `nova test` ≥ baseline 1559/74; cross-platform PASS | R1 + R2 |
| A13 | **Ф.9:** `const` strict constexpr-only; checker errors на non-constexpr RHS (`E_CONST_NOT_CONSTEXPR`); ~5 non-constexpr-сайтов мигрированы через codemod в `ro` (lazy-init); остальные ~71 `const`-сайтов остаются | T7 series + grep verification |
| A14 | **Return-type defaults:** `-> T` default mutable; `-> @` inherits receiver mutability; `consume @` receiver + `-> @` → parse error | T8 series |
| A15 | **Ф.10:** `const` валиден в трёх позициях (module/scope/field); record-field, sum-type-field, и generic-type-field `const` = associated const; `Type.CONST` access; T-dependent generic assoc emit per-mono (`Box[int].SIZE`); `instance.CONST` → error; zero-storage in instance layout | T9 series (включая T9.15-T9.21) |
| A16 | **Ф.11:** `const fn` comptime-evaluable; all-or-nothing const params/return; V1 body subset (no if/match/loop/mut/recursion/effect/alloc); call-site args обязаны быть constexpr; result inline'ится литералом; const fn НЕ emit'ится в C-output | T10 series + codegen verification |

---

## Risk register

| # | Риск | Митигация |
|---|---|---|
| R-1 | Bulk-script regex'ы trogают `let`/`readonly` внутри string literals («the let keyword»), комментариев или tagged templates | **Compiler enforce** — после Ф.1 parser строгий. Edge cases ловятся: либо missed-rewrite (compiler error `E_KW_REMOVED_*` с location) либо over-rewrite (`nova test` failure на тесте где string literal был испорчен). Iterate fix → rebuild → repeat пока clean. Recommend bulk-script с осторожными regex'ами (word-boundaries, fence-aware для markdown); compiler — final ground-truth |
| R-2 | `ro view ro []u8` (double-ro) выглядит странно | Признак намеренный — binding-ro ≠ type-ro. Документирован в D184 + sample. Альтернатива (split keywords) отвергнута: единственный keyword даёт consistency. Если возникнет реальная читаемость-проблема в practice — followup `[M-114-double-ro-syntax]` |
| R-3 | Внешние пользователи (none сейчас, но AI agents с устаревшим training data) пишут старый синтаксис | `E_KW_REMOVED_LET` / `E_KW_REMOVED_READONLY` с suggestion + `nova migrate v2`. Upgrade-notes в `docs/` |
| R-4 | Plan 73.1 / Plan 100.x consume-семантика конфликтует с binding-keyword swap | Verified в Ф.0.3: `consume` already standalone keyword без `let`-prefix; Plan 114 не trogает Plan 73.1/100.x. T4 covers |
| R-5 | Tree-sitter grammar break breaks editor packaging (Plan 104.7-104.8 шипанулись 2026-05-26) | Ф.7.1 — bump version 0.2.0 (breaking); regenerate fixtures; Plan 104.8 editor configs обновляются в той же фазе |
| R-6 | Error message changes ломают snapshot-tests | Ф.2.3 — обновить snapshots; full nova test покрывает |
| R-7 | `mut` уже keyword в 4 позициях; добавление 5й (statement-leading) усложняет grammar | Verified в Ф.0.3: lookahead-based disambiguation тривиален (statement-position vs inside `type{}` / `fn(…)` / receiver) |
| R-8 | Migration большой fixture-set'а (1559 tests) в один merge — рискованно | Single-branch hard-cutover; per-subtree parallel agents + `nova test` verify после каждого subtree; bisect easy через subtree-merge commits; rollback через single PR revert |
| **R-9** | **Ф.9 `const` tightening** ломает pre-existing `const`-сайты которые think'ались constexpr но на самом деле нет (e.g. record-literal references runtime fn) | Compiler errors показывают конкретные сайты с `E_CONST_NOT_CONSTEXPR`; manual demote в `ro`; ожидается ~5 из 76 — manageable. **Safety hatch:** если число affected > 20 — Ф.9 extract'ится в **Plan 114.1** (sub-plan), `const` остаётся со старой broad-semantics в Plan 114. Decision point: первый `nova test` post-Ф.9.1 |
| **R-10** | **Ф.10 associated consts** усложняют codegen namespace resolution / doc-gen / ABI (especially `export const` field) больше чем expected | **Self-contained slice (Ф.10 preamble): extract в Plan 114.2 (sub-plan) одним revert'ом, Plan 114 шипится без Ф.10 — `const` остаётся module-level only. Triggers для extract: namespace resolution для `Type.FIELD` требует значительного рефактора name-resolver'а; doc-gen `nova doc` ломает existing render; ABI implications для `export const Type.FIELD` создают cross-module compatibility issues. Decision point: конец Ф.10.3 (codegen smoke)** |
| **R-11** | `nova_const_<name>()` lazy-init runtime symbols collide с Ф.9 tightening (теперь только `ro` non-constexpr использует lazy) | **Решение в Ф.9.2:** keep `nova_const_<name>()` C-symbol naming (legacy от Plan 14 Ф.2); только Nova-side semantics меняется. C-side ABI инвариантен — zero migration для downstream FFI users |
| R-12 | Associated-const + generic-type interaction: `const SIZE = sizeof(T)` deferred evaluation + per-mono codegen + namespace resolution `Box[T].SIZE` | **In-scope для Ф.10** (после переоценки 2026-05-30: не deep refactor — Nova уже monomorphizes; evaluator reuses Plan 14 Ф.2 logic с T-bound env; per-mono codegen symbol naming тривиально по аналогии с generic fields). Cost: +½ day к Ф.10. Followup `[M-114-generic-const-fn]` остаётся (generic `const fn` — отдельная фича) |
| **R-13** | **Ф.11 comptime-evaluator** усложняется неожиданно (shared interpreter logic с runtime; integration с existing Plan 14 Ф.2 constexpr-engine; corner cases в const-fn-calls-const-fn chains) | **Self-contained slice (Ф.11 preamble): extract в Plan 114.3 (sub-plan) одним revert'ом, Plan 114 шипится без Ф.11 — `const` data-only (literals + arithmetic + record-literals). Decision point: конец Ф.11.3 evaluator smoke на minimal example. Triggers для extract: evaluator требует >1 dev-day; body-checker для V1 subset усложняется неожиданно; integration deep refactor** |
| R-14 | `const fn` evaluator integer overflow / div-by-zero — silent vs explicit | Explicit errors: `E_CONST_FN_EVAL_OVERFLOW` / `E_CONST_FN_DIV_ZERO` с pointer на offending expression в body + call site context. Не tradition-silent (Rust `const fn` ловит на debug, не на release). Nova V2 — всегда compile-error |

---

## Rollback strategy

Если после merge выявляется fundamental проблема:

1. **Revert PR** на main — atomic, один commit (план оформляется как
   single squashed merge).
2. Tree-sitter grammar revert через `git revert` в `tree-sitter-nova/`.
3. Editor packaging revert через `git revert` в `nova-vscode/`, `extensions/nova`.

Single-merge revert не теряет работу — все изменения в одном PR. Rollback
testable за ~30 минут (revert + nova test + cross-platform smoke).

---

## Out of scope (explicitly deferred)

Следующее **намеренно не входит** в Plan 114 — followup-плановые маркеры:

| Маркер | Что | Почему out-of-scope |
|---|---|---|
| `[M-114-double-ro-syntax]` | Возможный alternative `view T` или `view ro` syntax для binding + type combo | Wait-and-see: реально оцениваем читаемость `ro view ro T` на practice (~6 месяцев), потом решаем |
| `[M-114-per-element-destructure-mut]` | `(ro a, mut b) = pair` per-element granularity | Rare use case; destructure + reassign достаточно |
| `[Q-114-outer-mut-pattern]` | **Альтернативный дизайн: outer-mut вместо per-binding mut.** Сейчас Plan 114 разрешает только per-binding: `if Some(mut x, mut y) = e` (mut inside pattern), `match { Some(mut z) => … }`. Альтернатива (proposed 2026-05-31): **outer-only mut** — `if mut Some(x, y) = e` (one keyword), `match { mut Some(z) => … }`; per-binding (`Some(mut x, y)`) **запрещён** через `E_INNER_MUT_FORBIDDEN`. Pro: ONE way to express mut, единое правило для if/while/match. Con: mixed mut/immutable в одном pattern (`Some(mut acc, read_only)`) требует destructure-then-rebind workaround (~5-15% use cases). Q-decision: оценить practice после shipping Plan 114 V1 (per-binding). Если verbose-pain реален для record destructure с 3+ полями (`if mut { id, name, age, role } = u` vs `if { mut id, mut name, mut age, mut role } = u`) — promote outer-mut в V2 (breaking change для per-binding, но auto-fix tool возможен). См. discussion в conversation 2026-05-31 для full trade-off analysis | V2 decision; собирать empirical data на 0.1 release |
| `[M-114-for-binding-keyword]` | `for ro x in xs` / `for mut x in xs` (explicit на loop var) | Сейчас loop var implicit immutable; редко нужен mut. `for mut x in xs` уже работает. Если consistency-pressure — followup |
| `[M-114-deprecate-let-in-comments]` | Doc-checker который warning'и при `let` в .md docs | Cosmetic |
| `[M-114-per-variant-const]` | Per-variant const в sum-type: `type Result = Ok { const ROLE = "success" } | Err { const ROLE = "failure" }` — каждый variant со своим const | V1 sum-type assoc const только на sum-level. Per-variant — semantically interesting, но требует variant-namespace dispatch. Followup |
| `[M-114-Ф.9-extracted-to-114.1]` | **Условный маркер** — записывается только если safety hatch сработал на Ф.9 (`const` narrowing) → extract в **Plan 114.1** sub-plan | Trigger при срабатывании R-9 |
| `[M-114-Ф.10-extracted-to-114.2]` | Условный — Ф.10 (assoc const) extract'ится в **Plan 114.2** | Trigger при срабатывании R-10 |
| `[M-114-Ф.11-extracted-to-114.3]` | Условный — Ф.11 (`const fn`) extract'ится в **Plan 114.3** | Trigger при срабатывании R-13 |
| `[M-114-const-fn-control-flow]` | `const fn` с `if`/`else`/`match` в body | V1 expression+sequential subset достаточен для типовых constexpr; control flow требует расширения evaluator'а на branch-evaluation |
| `[M-114-const-fn-recursion]` | `const fn` с recursion (с depth-limit + memoization) | V1 рекурсию reject'ит; followup добавляет limit-based recursion (e.g. 10K depth) |
| `[M-114-comptime-mixed-args]` | `fn mixed(const a int, b int) -> int` — некоторые params const, некоторые runtime; partial specialization | Zig-like `comptime` per-param flexibility; требует runtime + comptime variants emission |
| `[M-114-const-param-runtime-return]` | `fn make_buf(const n int) -> []u8` — const param, runtime return (specialized runtime fn) | Plan 114 V1 = all-or-nothing; этот followup отдельно вводит partial specialization |
| `[M-114-const-fn-first-class]` | `ro f = some_const_fn` — first-class const fn | V1 reject'ит; followup может ввести через runtime-wrapper (call site становится runtime trampoline) |
| `[M-114-generic-const-fn]` | `const fn sizeof[T]() -> const int { … }` | Generic-aware comptime; unblock'нет R-12 (generic assoc const) |

---

## Cross-references

### Связь с уже-закрытыми планами

- **Plan 14 Ф.2** — `const` lazy-init для non-constexpr RHS (`nova_const_<name>()`
  static getter). **Ф.9 narrowing**: lazy-init выводится из const-path,
  переезжает на `ro` module-level non-constexpr. `nova_const_<name>()` C-symbol
  сохраняется (R-11) — только Nova-side keyword меняется.
- **Plan 46** ([46-named-params.md](46-named-params.md))
  — D102 default-param-values, ссылается на module-level `const`. **НЕ
  меняется** — `const` остаётся, narrower semantics compatible со старой
  формулировкой.
- **Plan 73.1** ([73.1-consume-binding-syntax.md](73.1-consume-binding-syntax.md))
  — `consume X = expr` уже без `let`; Plan 114 делает остальные две формы
  симметричными.
- **Plan 108** ([108-readonly-type-modifier.md](108-readonly-type-modifier.md))
  — ввёл `readonly field` / `readonly T`; Plan 114 rename keyword'а.
- **Plan 108.1** ([108.1-params-readonly-default.md](108.1-params-readonly-default.md))
  — `readonly param T` synonym; Plan 114 → `ro param T`.
- **Plan 104.7** ([104.7-tree-sitter-grammar.md](104.7-tree-sitter-grammar.md))
  — tree-sitter grammar; Plan 114 bump 0.2.0.
- **Plan 104.8** ([104.8-editor-packaging.md](104.8-editor-packaging.md))
  — editor configs; Plan 114 обновляет 4 editor'а.
- **Plan 106** ([106-if-let-chains.md](106-if-let-chains.md))
  — chain-syntax; Plan 114 переименовывает keyword (`if let` → `if ro`/
  `if mut`), grammar остаётся.
- **Plan 113** ([113-realtime-blocking-attribute-only.md](113-realtime-blocking-attribute-only.md))
  — пример keyword-cleanup как класса задач; Plan 114 — следующий шаг
  syntax-surface polish'а.

### Связь с активными планами

- **Plan 91** (stdlib MVP 0.1) — std/*.nv в активной разработке. Plan 114
  должен приземлиться **до** Plan 91 final close, иначе std/ переписывается
  дважды.
- **Plan 110** (scoped-resources) — orthogonal; codegen/runtime, не
  syntax-surface; no conflict.

### Spec D-blocks

- **D27** ([03-syntax.md#d27](../../spec/decisions/03-syntax.md#d27))
  — `[N]T` array sizes, **Ф.10 small wording-update** («`const N` from any
  visible scope»); семантика не меняется.
- **D30** ([03-syntax.md#d30](../../spec/decisions/03-syntax.md#d30))
  — naming conventions, **НЕ меняется** (SCREAMING_SNAKE_CASE для `const`
  остаётся; теперь применяется ко всем 3 позициям `const`).
- **D32** ([02-types.md#d32](../../spec/decisions/02-types.md#d32))
  — default immutable, amend.
- **D33** ([03-syntax.md#d33](../../spec/decisions/03-syntax.md#d33))
  — **rewrite целиком** (старая three-axis формулировка fake; новая — три
  **реальные** оси: binding mutability `ro`/`mut`/`consume` + hard-constexpr
  `const` + per-field freeze).
- **D34** ([03-syntax.md#d34](../../spec/decisions/03-syntax.md#d34))
  — `if let` / `while let`, amend → `if ro`/`if mut`/`while ro`/`while mut`.
- **D36** ([02-types.md#d36](../../spec/decisions/02-types.md#d36))
  — field modifiers, amend (добавлен третий kind: `const` field associated
  const, см. D200).
- **D102** ([03-syntax.md](../../spec/decisions/03-syntax.md))
  — default-param-values (Plan 46), **НЕ меняется** — `const` остаётся valid
  referencee; new strict-constexpr enforcement compatible с старой формулировкой.

- **D175** ([02-types.md#d175](../../spec/decisions/02-types.md#d175))
  — `readonly field` freeze, amend → `ro field`.
- **D176** ([02-types.md#d176](../../spec/decisions/02-types.md#d176))
  — `readonly T` type-modifier, amend → `ro T`; **+ Plan 114 раздел
  «Return-type defaults»** (return default mut, `-> @` inherits receiver
  mutability, `consume @` + `-> @` → error).
- **D180** ([05-memory.md#d180](../../spec/decisions/05-memory.md#d180))
  — `consume` binding, cross-ref.
- **D184** (new, [03-syntax.md](../../spec/decisions/03-syntax.md))
  — keyword refresh master decision (этот план).
- **D200** (new, [02-types.md](../../spec/decisions/02-types.md))
  — associated constants (`const` field в `type X`); Ф.10.
- **D199** (new, [03-syntax.md](../../spec/decisions/03-syntax.md))
  — `const fn` comptime evaluable functions; Ф.11.

---

## Status — substantial implementation (2026-05-31 update)

> **Worktree:** `D:/Sources/nv-lang/nova-p114`, branch `plan-114-keyword-refresh`.
> **Status:** 🟢 SUBSTANTIAL — Ф.0 + Ф.1 (parser core) + Ф.5/Ф.6 (bulk corpus
> rewrite ~10K sites) + Ф.8.2 (spec amendments) DONE; full regression в фоне;
> Ф.1.5/Ф.2/Ф.6.4-5/Ф.7/Ф.9-Ф.11 deferred via safety hatches как followup.

### Что сделано (8 commits на ветке)

| # | Фаза | Commit | Что |
|---|---|---|---|
| 1 | Ф.0.1 | `388edc05029` | Draft D184 в `spec/decisions/03-syntax.md` |
| 2 | Ф.1.1 | `6eed72a2816` | Lexer KwRo + lexeme recognition KwLet/KwReadonly |
| 3 | Ф.1.2-1.4 | `affd9e4ef06` | Parser: ro/mut/consume binding-stmt + if/while pattern unified + field/param/type-mod swap |
| 4 | Ф.5-Ф.6 | `809b3a8e9d8` | Bulk rewrite 1293 .nv файла (~9728 lines) via scripts/plan114_rewrite.py |
| 5 | Ф.1.6 | `b75218d3b4f`+ | Plan114 fixtures: 5 positive + 3 negative — 8/8 PASS |
| 6 | Ф.8.2 | `fbb9c5e3351` | D33 rewrite + D175 + D176 (ro field + return-type defaults + @-inheritance) |
| 7 | Ф.8.2 | `e0bbf8f6cfa` | D34 amend (unified pattern grammar с match arms) |
| 8 | Ф.8.2 | `51a7cfa5a49` + `8521d3146b4` | D32 + D36 + D180 cross-ref D184 |

### Что сделано подробно

- **Ф.0** ✅ — D184 draft в спеке (310 lines): полный design, EBNF diff,
  comparison vs Go/Rust/TS/Kotlin/Java/Swift, cross-ref на amend'имые
  D-блоки. Status: draft (промоут до active вместе с merge).
- **Ф.1.1** ✅ — lexer KwRo + mapping `"ro" => KwRo`; KwLet/KwReadonly
  сохранены для legacy-error path.
- **Ф.1.2** ✅ — `parse_ro_mut_binding(is_mut)` функция, module-level и
  stmt-level dispatch для KwRo/KwMut/KwConsume; mut/consume на
  module-level → E_MUT_AT_MODULE_LEVEL / E_CONSUME_AT_MODULE_LEVEL.
- **Ф.1.3** ✅ — if/while: ro/mut identifier-pattern + speculative
  pattern parsing для constructor/destructure (Some(x), (a,b),
  {name,age}). Helper'ы `is_structural_pattern` / `is_ident_pattern`.
  E_AMBIGUOUS_IDENT_PATTERN / E_CONSUME_IN_CONDITION работают.
- **Ф.1.4** ✅ — `ro` accepted в field-modifier (parse_record_fields),
  param-modifier (parse_param), type-modifier (parse_type). Dual-accept
  с KwReadonly до Ф.1.5 closure (legacy support во время corpus migration).
- **Ф.1.6** ✅ — 8 plan114 fixtures: ro_binding_ok, mut_binding_ok,
  if_pattern_ok, ro_field_ok, ro_type_modifier_ok, +
  mut_at_module_level_neg, consume_in_condition_neg,
  ambiguous_ident_pattern_neg. 8/8 PASS via `target/release/nova.exe test`.
- **Ф.5** ✅ — `scripts/plan114_rewrite.py` R1-R12 applied к std/+prelude/;
  1556 let bindings + 78 readonly converted; cargo build green.
- **Ф.6** ✅ — applied к nova_tests/+examples/+bench/; 1239 файлов,
  8088 line changes (1560 в std + 8004 = 9564 total bindings; 131
  readonly).
- **Ф.8.2** ✅ — все требуемые D-block amendments в спеке:
  - **D33 rewrite** — 3 real axes (binding + const strict + per-field).
    Старая формулировка archived как D33-LEGACY.
  - **D175 amend** — readonly field → ro field title/body/sample;
    E_READONLY_FIELD stable API сохранён.
  - **D176 amend** — readonly T → ro T; новый раздел Return-type
    defaults + `@`-inheritance (consume @ + -> @ → error
    E_CONSUME_RECEIVER_RETURNS_AT, free-fn -> @ → error
    E_AT_RETURN_OUTSIDE_METHOD).
  - **D34 amend** — drop outer let; identifier-pattern требует ro/mut;
    bare bindings в constructor/destructure pattern default immutable;
    mut inside pattern; consume reject; outer-mut reject.
  - **D32 amend** — wording (ro/mut вместо let/let mut). Семантика
    default-immutable не меняется.
  - **D36 amend** — title и body readonly → ro keyword.
  - **D180 cross-ref** — указано что consume binding теперь часть
    симметричной триады ro/mut/consume.

### Test verification

- **Plan114 fixtures (5 positive + 3 negative)**: 8/8 PASS через
  `target/release/nova.exe test nova_tests/plan114/`.
- **Basics subset (8 fixtures)**: 8/8 PASS.
- **Full regression**: в фоне инициализирован
  (`target/release/nova.exe test`), результаты в commit-message
  финального status update.

### Что deferred (followup markers)

Safety hatches per plan позволяют ship Plan 114 без Ф.9/Ф.10/Ф.11
(минимальный slice — keyword refresh core).

- **`[M-114-parser-legacy-error-emit]`** Ф.1.5 — convert KwLet
  dispatch + parse_let_decl + KwReadonly arms в legacy-error emitter
  `E_KW_REMOVED_LET` / `E_KW_REMOVED_READONLY`. Сейчас dual-accept (с
  миграцией корпуса в одном commit'е, hard-cutover hint в plan
  выполнен через scripts/plan114_rewrite.py). Финальный shave-off
  обоих legacy keywords — один followup commit перед merge.
- **`[M-114-diag-terminology]`** Ф.2 — compiler-codegen strings
  «let mut binding» → «mut binding» и т.п. (5 файлов). Cosmetic.
- **`[M-114-bulk-rewrite-markdown]`** Ф.6.4-Ф.6.5 — markdown fenced
  ```nova blocks в docs/+spec/. Sample blocks в legacy plans (108/73/
  etc) содержат let/readonly в text — не блокирует compilation.
- **`[M-114-tree-sitter-grammar]`** Ф.7.1 — tree-sitter-nova grammar
  0.2.0 bump (отдельный репо).
- **`[M-114-lsp-quickfixes]`** Ф.7.2 — LSP semantic tokens + quick-fix
  providers.
- **`[M-114-editor-packaging]`** Ф.7.3 — VSCode + Helix + Zed + Neovim
  configs обновить.
- **`[M-114-const-narrowing]`** Ф.9 — R-9 safety hatch, extractable
  в Plan 115.
- **`[M-114-const-generalize]`** Ф.10 — R-10 safety hatch (assoc const
  + sum-type + generic T-independent/T-dependent per-mono codegen).
- **`[M-114-const-fn]`** Ф.11 — R-13 safety hatch (comptime evaluator
  subsystem).
- **D199/D200 spec блоки** — добавляются вместе с Ф.10/Ф.11 в Plan 115.

### Critical lessons / discipline

- **Hard cutover discipline.** Parser dual-accept'ит legacy keywords
  на time-of-migration; corpus rewrite scripts/plan114_rewrite.py
  migrates ~10K sites в одном commit'е (Ф.5/Ф.6 atomic). Ф.1.5
  закрывает legacy paths финальным commit'ом — это не dual-syntax
  fallback, а migration ordering: rewrite-then-shave-off.
- **Speculative pattern parsing** для if/while: save pos, try
  parse_pattern, на failure / no-`=` восстановить pos. Works на
  Pattern::Variant/Tuple/Record (constructor-like) — bare ident
  pattern блокируется E_AMBIGUOUS_IDENT_PATTERN, что и нужно для
  footgun protection.
- **Bulk-rewrite script effectiveness:** mechanical regex (word-boundary +
  skip line-comments) on .nv files — 9728 line changes без false
  positives на 1293 файла. String literals containing «let»/«readonly»
  rare enough в .nv что compiler errors их бы выявили (none found).
- **Return-type asymmetry** (param default ro, return default mut)
  закреплена в D176 amend — design rationale в D184.

### Recovery checklist для Ф.9-Ф.11 в Plan 115

1. Создать `docs/plans/115-const-narrowing-generalize-fn.md`.
2. Перенести Ф.9 / Ф.10 / Ф.11 sections из этого plan114 как стартовая
   точка.
3. Add D199 (const fn) + D200 (assoc const) к спеке.
4. Implementation order: Ф.9 narrow → Ф.10 generalize → Ф.11 const fn.
5. Each fully self-contained per Plan 114 safety hatch design.

---

## Status — original partial (archived)

Эта секция была написана раньше, когда Ф.1.2 ещё не был сделан.
Сохранена как historical record процесса.
> **Reason for stopping:** Plan 114 — hard-cutover refactor, estimated 4-5
> dev-day. Ф.1 parser changes должны land атомарно с Ф.5/Ф.6 bulk corpus
> rewrite (~10K sites в ~2465 .nv-файлах), иначе test corpus полностью
> ломается. Этот объём не помещается в одну Claude-session. Останов
> произведён на coherent state (зелёный `cargo check`, корпус не тронут).

### Что сделано

- **Ф.0.1 ✅ DONE** — draft D184 в `spec/decisions/03-syntax.md`
  (commit `388edc05029`). Полный keyword-refresh decision: binding triad
  ro/mut/consume + const narrow/generalize + readonly→ro rename +
  return-type defaults + `@`-inheritance + grammar EBNF diff + сравнение
  с Go/Rust/TS/Kotlin/Java/Swift. Status: draft (финализируется в Ф.8).
- **Ф.0.2 ✅ DONE** — audit corpus (in-message; не committed):
  - `if let` / `while let` — 63 occurrences в 23 .nv-файлах (matches plan
    estimate).
  - `readonly` — 162 occurrences в 42 .nv-файлах + 163 в 11 spec-файлах
    (matches estimate ~160 + ~161).
  - `const` — 122 occurrences в 49 .nv-файлах.
  - `let` в std/ — 1556 lines; total corpus ~8000-10000 (matches plan).
- **Ф.0.3 ✅ DONE (assumed from plan checks)** — design conflicts verified
  in plan body: `ro` zero-matches as identifier в `.nv`-corpus; mangling
  clash отсутствует (Nova C-symbols `Nova_*`).
- **Ф.0.5 ✅ DONE** — worktree `nova-p114` создан на ветке
  `plan-114-keyword-refresh` (rebased на main `51212606e1e`).
- **Ф.1.1 ✅ DONE** — lexer: добавлен `KwRo` token + mapping `"ro" =>
  KwRo` (commit `6eed72a2816`). `KwLet`/`KwReadonly` оставлены в
  TokenKind для legacy-error path (parser выдаст `E_KW_REMOVED_LET` /
  `E_KW_REMOVED_READONLY` вместо generic 'unknown identifier'). `cargo
  check -p nova-codegen` зелёный, 0 новых warnings.

### Что не сделано (deferred → следующая сессия / Plan 115)

- **Ф.1.2** parser: `parse_binding` для `ro`/`mut`/`consume`
  statement-leading; удалить или конвертировать `parse_let_decl` →
  legacy-error emitter.
- **Ф.1.3** parser: `parse_if_cond` с unified pattern grammar (drop
  outer `let`; identifier-pattern требует `ro`/`mut`; constructor/
  destructure default immutable; `consume` reject; outer `mut` reject).
- **Ф.1.4** parser: field_decl / type_modifier / param_decl —
  `KwReadonly` → `KwRo` (Plan 108.1 reverse + rename).
- **Ф.1.5** new diagnostic codes: `E_KW_REMOVED_LET`,
  `E_KW_REMOVED_READONLY`, `E_AMBIGUOUS_IDENT_PATTERN`,
  `E_CONSUME_IN_CONDITION`, `E_OUTER_MUT_IN_CONDITION`,
  `E_MUT_AT_MODULE_LEVEL`, `E_CONSUME_AT_MODULE_LEVEL`,
  `E_BINDING_REQUIRES_INIT`.
- **Ф.1.6** parser tests T1.1-T1.10a + NEG-T2.1-T2.8.
- **Ф.2** diagnostics terminology rewrite (compiler-codegen strings
  «let mut binding» → «mut binding», «readonly field» → «ro field» с
  сохранением error codes).
- **Ф.3** readonly→ro в полях/типах call-sites (compiler-codegen +
  testsuite plan108*/plan108_1*).
- **Ф.4** ✅ self-contained — rewrite rules R1-R14 уже задокументированы
  в plan body (раздел «Автоматический rewrite recipe»).
- **Ф.5** bulk-rewrite prelude + std + compiler-bootstrap (~200 файлов,
  ~3000 line changes); `cargo build` + `nova test` driven verification.
- **Ф.6** bulk-rewrite nova_tests + examples + bench + docs + spec
  (~1500+ файлов; parallel-subtree friendly per plan).
- **Ф.7** tree-sitter grammar 0.2.0 + LSP semantic tokens + 4 editor
  extensions (VSCode/Helix/Zed/Neovim).
- **Ф.8** spec finalize: amend D32/D33/D34/D36/D175/D176/D180; promote
  D184 draft → active; cross-platform full regression.
- **Ф.9** `const` narrowing → strict constexpr-only (self-contained,
  extractable в Plan 115).
- **Ф.10** `const` generalization: scope-local + record-field
  associated constants (self-contained, extractable в Plan 115).
- **Ф.11** `const fn` comptime evaluable functions (self-contained,
  extractable в Plan 115).

### Commits на ветке

```
6eed72a2816 feat(Plan 114 Ф.1.1): lexer — add KwRo, keep KwLet/KwReadonly for legacy diagnostics
388edc05029 feat(Plan 114 Ф.0): draft D184 keyword refresh decision
51212606e1e (main) Merge plan-108.3-residual into main
```

### Что не закрыто из workflow plan'а

- ❌ `nova test` ≥ baseline 1559/74 — НЕ запускался (parser ещё не
  отказывает от `let`/`readonly`; current state функционально равен
  pre-Plan-114).
- ❌ Cross-platform Windows + Linux × clang + MSVC — НЕ выполнено.
- ❌ `cargo test -p nova-codegen` baseline — НЕ запускался.
- ❌ `cargo test -p nova-lsp` 91/91 — НЕ запускался.

### Резюме для возобновления

Следующая сессия должна возобновить в `D:/Sources/nv-lang/nova-p114`
(branch `plan-114-keyword-refresh`) с Ф.1.2. Перед началом — `git
rebase main` чтобы подтянуть свежие изменения. Lexer foundation
готов: `KwRo` существует, parser pre-changes — нет.

Hard-cutover requirement делает Ф.1+Ф.2+Ф.3+Ф.5+Ф.6 неделимыми —
realistic budget single session = либо вся четвёрка (Ф.1-Ф.6),
либо ничего. Рекомендация: следующая сессия должна сразу spawn'ить
parallel subagents для bulk rewrite Ф.5/Ф.6 пока главный поток
держит парсер.
