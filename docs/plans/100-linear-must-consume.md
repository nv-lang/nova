// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 100: `consume`-тип (must-be-consumed obligation) — D133

> **Создан 2026-05-23. Дизайн-decisions финализированы в обсуждении
> 2026-05-23 (D1–D10 ниже).** Выделен из outline'а
> [Plan 77 §«Потенциал «лучше Rust»»](77-fluent-return.md#L105-L109)
> (D132 `-> @` fluent-return) как «отдельный возможный план».
>
> **Статус:** 📋 proposed, не начат, **P3** (polish — не блокер релиза
> 0.1, ожидает stdlib MVP). ~4–6 dev-day.
>
> **Зависимости:**
> - [Plan 73](73-consume-qualifier.md) ✅ (D131 affine `consume` —
>   foundation: flow-sensitive `check_consume` pass, `VarState`,
>   alias-tracking).
> - [Plan 77](77-fluent-return.md) ✅ (D132 `-> @` fluent-return —
>   sound builder-chain alias через `check_fluent_return`; без него
>   field-aware tracking был бы дырявым на builder API).
> - [Plan 95](95-builtin-sum-method-mono.md) ✅ (Option/Result как
>   generic-method-able типы; pattern-match через `MethodRouting::DeclaredBody`).
> - [Plan 91](91-stdlib-mvp-for-0.1.md) — реальные кандидаты на
>   миграцию (`File`, `Connection`, `Transaction`, lock-guard)
>   появляются после stdlib MVP.
>
> **Цель:** ввести квалификатор `consume` на **type-decl**, делающий
> инстансы такого типа **обязательными к закрытию** до выхода из
> scope'а на каждом пути выполнения. Compile error, если live consume-
> переменная остаётся на exit-point'е. Канонический use-case —
> `Transaction.@commit()` / `.@rollback()`, `File.@close()`, lock-guard
> `.@release()`. Строже Rust `#[must_use]` (warning, suppressable через
> `let _ = v`) — suppress намеренно отсутствует, единственный канал —
> consume-метод. Не universal (D75 «несоразмерно для GC»), а **opt-in
> per-type**; transitivity по полям и generic-args.
>
> **Терминология:** в type-theory это называется *linear types* (Linear
> Logic, Girard 1987; Linear Haskell, Idris). В Nova используем
> существующий keyword `consume` везде вместо нового `linear` — единое
> понятие.

---

## Контекст

D131 (Plan 73) ввёл `consume` — **affine** квалификатор на receiver/
параметре: значение можно потребить **≤1 раз**. Использовать после
consume — compile error. **Забыть** значение (не потребить вообще) — OK,
GC соберёт.

Этого недостаточно для целого класса типов, где «забыть» — bug:

```nova
fn process(db Db) -> () {
    let tx = db.begin()
    write_stuff(tx)
    // ❌ забыли tx.commit() / tx.rollback() — D131 не ловит,
    //    GC собирает Transaction в неопределённом состоянии.
}
```

Цена пропущенного `.commit()`/`.close()`/`.release()` — потерянная
запись в БД, file descriptor leak, deadlock на mutex'е. Это **не memory
safety** (GC справится), а **логическая надёжность** — дополнение к
D131 с противоположной стороны:

| Свойство | D131 affine `consume` | Plan 100 type-level `consume` |
|---|---|---|
| Потребить ≤1 раз | ✅ enforce | ✅ enforce (наследуется) |
| Потребить ≥1 раз (обязательно) | ❌ забыть OK | ✅ enforce — must consume |
| Тип помечается на | receiver / param метода | **type-decl** + поле / binding |
| Bootstrap-аналог | `StringBuilder.@into()` | `Transaction.@commit()/.@rollback()` |

---

## Сравнение с другими языками

| Язык | Механизм | Точность | Suppressable |
|---|---|---|---|
| **Rust** `#[must_use]` | attribute | warning | да (`let _ = v`) |
| **Rust** ownership + `Drop` | RAII auto-cleanup | guaranteed | нет, но Drop не различает «commit» / «rollback» |
| **Linear Haskell** `=>` arrow | linear arrow | guaranteed | нет, type-level |
| **Idris** linear-types | quantitative type theory | guaranteed | нет |
| **Go** | нет; convention + linter | manual | n/a |
| **TS** | нет | n/a | n/a |
| **Nova** D131 (текущий) | affine `consume` | guaranteed «≤1» | n/a |
| **Nova** Plan 100 | `consume` на type-decl | guaranteed «обязан» | **нет** (consume-метод — единственный путь) |

**Что Nova может лучше:** Rust `#[must_use]` подавляется через `let _ =
v` — gateway-bug. `Drop` гарантирует cleanup, но не различает «commit»
(успех) от «rollback» (откат). Linear Haskell / Idris дают точность
ценой type-system complexity (quantitative arrows). Nova получает
Rust-точность **без borrow** и строже Rust по non-suppressability —
поверх GC, без move-семантики.

---

## Что НЕ входит (явно отвергнуто)

- **Universal affine/linear для всех `let`.** Отвергнуто в [D75
  «Compile-time token-scope enforcement»](../../spec/decisions/06-concurrency.md#d75)
  и [Plan 47](47-supervised-cancel.md): «affine/linear-типы
  несоразмерны — Rust borrow checker ради одной фичи, несоразмерно
  для GC-языка». Plan 100 — **opt-in per-type**, не default.
- **Borrow checker / lifetimes / move-семантика в Rust-смысле.** Память
  по-прежнему GC (D6). Plan 100 работает поверх существующего
  `check_consume` pass'а как dataflow-расширение.
- **Suppress-механизм `let _ = v`.** Намеренно отсутствует (anti-Rust
  `#[must_use]` gateway). Единственный способ удовлетворить consume-
  обязательство — consume-метод. Если в коде встречается «иногда хочу
  забыть» — это знак, что тип неправильно помечен `consume`.
- **Drop-method auto-cleanup.** Plan 100 требует **явный** consume:
  выбор `.commit()` vs `.rollback()` важен. Auto-drop размывает выбор.
- **Strict-mode generic bound `[T consume]`.** Откладывается; bootstrap
  использует silent-ignore с документированной дырой
  (`[M-generic-consume-leak]`). Future plan.
- **Destructure consume-типа** (`let { tx } = state`). Запрещено —
  это сломает encapsulation (consume-поле уехало бы в независимый
  linear-binding мимо parent-методов). Если нужно вынести содержимое —
  явный consume-метод record'а возвращает tuple.
- **`consume` на return type функции.** Не вводится. Тип уже несёт
  consume-семантику через свою декларацию (`type X consume`);
  заразность через generic-args покрывает `Box[T]`/`Option[T]` авто-
  матически. Дублирующий маркер на return — шум.
- **`consume` + `-> @` на одном методе.** Parse error — семантически
  противоречиво: `consume` забирает целиком, `-> @` возвращает тот же
  объект.
- **Pattern-match для runtime-проверки «consumed ли»** (`match tx { None
  => ... }`). Compile-time `check_consume` — единственная семантика;
  runtime-представление (zero-fields / NULL-pointer) — defense-in-depth,
  не часть user-facing model.

---

## Финальные дизайн-решения (D1–D10, locked 2026-05-23)

### D1 — место маркера

**`consume` на type-decl:** `type Transaction consume { ... }`. Type-
уровень — единственное место, где обязательство наследуется на все
instance'ы консистентно.

### D2 — что считается consume

- **consume-метод (D131):** `tx.commit()` — главный канал.
- **передача в consume-параметр:** `fn finish(consume tx Transaction)`
  — обязательство переходит callee.
- **`return tx`** где `tx` — consume-тип: обязательство передаётся
  caller'у через return-value.
- **передача в обычный (non-consume) fn-параметр:** ❌ compile error.
  Linear не может «просто передаться» в функцию, которая её не объявила
  как consume.

### D3 — точки проверки (scope-exit'ы)

Live consume-переменная на любом из этих exit-point'ов → compile error
E (D133):
- конец function body (последний statement);
- `return expr` — все live consume-vars кроме возвращаемой;
- `panic` / `expr!!` / `expr?` / unwinding-paths;
- `loop break`;
- branch join `if`/`match` — если на разных ветках разное состояние
  (Live ⊔ Consumed = MaybeConsumed → error).

`defer` / `errdefer` тело может consume; учитывается при join.

### D4 — заразность через поля (transitive, **explicit double-marker**)

Record/sum, имеющий поле consume-типа, **обязан** быть объявлен
`consume`:

```nova
type TxState consume {          // ← consume на type-decl ОБЯЗАТЕЛЕН
    consume tx Transaction,     // ← consume на поле ОБЯЗАТЕЛЕН (тип = consume)
    writes []Write,             // не consume — обычное поле
}
```

Compiler проверяет согласованность:
- `consume`-поле без `consume` на type-decl → error.
- `consume` на type-decl без хотя бы одного `consume`-поля → error
  («consume-тип должен иметь consume-обязательство»).
- Поле consume-типа без маркера `consume` → error («не маркировал
  obligation»).

### D5 — consume-поле в методах record'а: field-aware flow analysis

`@field` отслеживается как локальная переменная VarState внутри тел
методов record'а. На exit-point'е метода:

| Метод | consume-поля должны быть |
|---|---|
| `fn X consume @method(...)` | Consumed (record closes) |
| `fn X mut @method(...)` | **Live** (invariant preserved) |
| `fn X @method(...)` (regular) | **Live** (invariant preserved) |

Это позволяет реальные паттерны (rotate / reopen / replace):

```nova
type Service consume {
    consume file File,
}

fn Service mut @reopen() -> Result[(), OpenErr] {
    let new_file = File.open()?     // сначала получить замену
    @file.close()                    // только теперь закрыть старое
    @file = new_file                 // rebind — @file опять Live
}                                    // exit mut-метода: @file Live ✅
```

Compiler ловит реальные баги:
- забытый rebind на ветке: `if cond { @file.close() }` → exit
  MaybeConsumed → error.
- early return без rebind: `if cond { @file.close(); return }` → error.
- наивный close-then-open с error-path: `@file.close(); @file =
  File.open()?` → error если open Err (@file Consumed, не rebinded).

#### D5.1 — assignment в Live consume-поле запрещён

Прямое присваивание `@field = expr` в consume-поле разрешено **только**
когда `@field` в состоянии `Consumed`. Если поле `Live` — compile error
E (D133-assign-live-field) с suggestion «consume the existing value
first via `<consume-method>()`». Защита от silent overwrite старого
consume-значения (gateway к утечке — поле было Live, его перетёрли, GC
собрал в неопределённом состоянии).

```nova
fn Service mut @overwrite_naive() {
    @file = File.open()?              // ❌ E (D133-assign-live-field):
                                      //    @file Live, перезапись затёрла бы
                                      //    старый File без close.
}

fn Service mut @overwrite_correct() {
    @file.close()                     // сначала consume старое
    @file = File.open()?              // ✅ теперь @file Consumed → assign OK
}
```

Та же логика применима и к **локальным переменным** consume-типа:
повторный `consume tx = expr` в том же scope'е (в форме reassignment)
без предшествующего consume старой `tx` — ошибка. Re-binding через
shadow (`let tx = ...; let tx = ...`) — отдельный случай (см. D2 +
[Plan 73 alias-tracking](73-consume-qualifier.md)): первый `tx` должен
быть consumed до shadow.

### D6 — generic-заразность

`type_is_consume(TypeRef)` — рекурсивная функция:
- тип в LinearityRegistry (объявлен `consume`)?
- record/sum с ≥1 consume-полем?
- generic-wrap `G[T1, ..., Tn]` — хотя бы один Ti consume?
- generic-param T (без bound): false (bootstrap silent-ignore).

`Option[Transaction]` / `Result[Transaction, E]` / `Box[Transaction]`
/ user `Wrapper[Transaction]` — все автоматически consume через wrap.
**Никакого Option-специфичного хардкода** — общее правило для любого
generic-wrapper'а.

### D7 — read-only access (non-consume параметр)

```nova
fn print_id(tx Transaction) {           // без `consume` — read-only
    println(tx.id)                       // ✅ чтение поля
    tx.commit()                          // ❌ consume-метод — error
    finish(tx)                           // ❌ передача в consume-param — error
    storage.last = tx                    // ❌ store-в-поле — error
    return tx                            // ❌ возврат — error (не владеешь)
    let f = || tx.commit()               // ❌ closure capture с consume — error
    let local = tx                       // ✅ alias (Plan 73 alias-tracking)
}
```

Правило: read-only param = только чтение полей + alias. Никаких передач,
сохранений, возвратов наружу. Identity-функция требует явного
`consume tx`.

### D8 — `consume` + `-> @` несовместимы

`fn Tx consume @prepare() -> @ { ... }` — parse error. Противоречие
между «забираю целиком» и «возвращаю тот же объект».

### D9 — binding-level `consume tx = ...`

Две формы привязки value к binding'у (для consume-типов):

```nova
consume tx = begin()    // strict: ОБЯЗАН закрыть в этом scope'е
                        //          (вернуть наверх — error)

let tx = begin()        // strict: ОБЯЗАН передать дальше (return /
                        //          consume-param / в record, который сам
                        //          пойдёт наверх). Закрывать локально — error.
```

Каждая форма — одно поведение, без «или-или». Декларация намерения с
compiler-checked гарантией.

### D10 — runtime-модель (mental, не ABI)

Концептуально consume-тип проецируется в `Option[T]`-space:
- `Live` ≡ `Some(t)`.
- `Consumed` ≡ `None`.
- `MaybeConsumed` ≡ ветка-зависимо.

Это **mental model для spec/docs** — помогает объяснить flow-семантику
через знакомый Option. **Реализация остаётся pragmatic** (D131-style):
- pointer-based consume-типы: NULL = None (zero overhead).
- value consume-типы: zero-out fields после consume.
- Compile-time `check_consume` — основной механизм; runtime-defence
  через NULL-deref panic.

User-facing pattern-match `match tx { Some(t) => ... }` для runtime-
проверки **не вводится** — ослабило бы compile-time гарантии.

---

## Фазы

### Ф.0 — GATE: probe пилотных use-case'ов (~0.3 dev-day)

Design-decisions D1–D10 уже locked (см. выше). Ф.0 — sanity-probe:
hand-написать желаемое поведение для 3 use-case'ов (mock Transaction
с commit/rollback; Service с consume File + reopen pattern; Option-
поле). Убедиться, что fixture-grammar self-consistent с D1–D10. При
несоответствии — внести корректировки до старта Ф.2.

Acceptance: документ `docs/plans/100-artifacts/gate.md` фиксирует probe
со ссылками на конкретные фикстуры Ф.6.

### Ф.1 — Spec D133 в `spec/decisions/02-types.md` (~0.4 dev-day)

Раздел `## D133. type-level consume — обязательная consume-семантика`:
- что / синтаксис (D1) / правило must-consume на exit-point'ах (D3) /
  transitivity (D4) / generic-заразность (D6) / границы bootstrap (D5
  generic).
- Cross-ref D131 (affine sibling), D132 (sound alias), D75 (почему не
  universal).
- Mental Option-projection (D10) — как объяснение flow-семантики.
- Сравнение Rust/Haskell/Go/TS.

Зеркало в `05-memory.md` §«Связь» — приписать D133.

### Ф.2 — Lexer + parser (~0.6 dev-day)

- **Лексер:** existing `KwConsume` (Plan 73) расширяем на новые
  позиции: type-decl, поле record'а, binding `consume tx = ...`.
- **AST расширения:**
  - `TypeDecl { consume: bool, ... }`.
  - `RecordField { consume: bool, ... }`.
  - `Stmt::Let { consume: bool, ... }`.
- **Парсер:**
  - `type X consume { ... }` — `consume` ПОСЛЕ имени типа, перед `{`.
  - `consume field T` внутри record body.
  - `consume tx = expr` — alternative к `let tx = expr`.
- **Парсер-проверки (parse-time):**
  - `consume` + `mut` на одном receiver / параметре — parse error
    (D131, остаётся).
  - `consume` + `-> @` на одном методе — parse error (D8).

### Ф.3 — Linearity registry + transitivity + согласованность (~0.6 dev-day)

- `LinearityRegistry` в `types/mod.rs`: `HashSet<TypeName>` + helper
  `type_is_consume(TypeRef)` — рекурсивный (D6).
- Pre-pass до `check_consume`: собирает registry из всех модулей.
- **Согласованность маркеров (D4):**
  - field с consume-типом без `consume`-маркера → error E (D133-field-marker).
  - `consume`-field без `consume` на type-decl → error E (D133-type-marker).
  - `consume` на type-decl без consume-полей → error E (D133-empty-consume).

### Ф.4 — `check_consume` extension: must-consume на exit-point'ах (~1.5 dev-day)

Расширяем existing `check_consume` pass (Plan 73 Ф.4):

- **На каждом scope-exit'е** — проход по `ConsumeCtx.states`: для
  каждой var, чей тип `type_is_consume == true` и состояние Live /
  MaybeConsumed → emit error E (D133-not-consumed) с указанием консьюм-
  методов:
  ```
  error: consume value `tx` must be consumed before scope exit
    note: type `Transaction` is declared `consume`
    note: consume via `.commit()` or `.rollback()`
  ```
- **`return expr`** возвращаемая consume-var передаёт обязательство
  caller'у; остальные live consume-vars → error.
- **`panic` / `expr!!` / `expr?`** — live consume на этом пути → error.
- **`loop break`** — live consume → error.
- **Передача в non-consume non-receiver fn-param** (D2) → error
  E (D133-move-to-non-consume).
- **Field-aware flow** (D5):
  - inside record method, `@field` — отдельный track в `ConsumeCtx`.
  - exit non-consume метода → consume-fields должны быть Live →
    error E (D133-field-not-restored).
  - exit consume-метода → consume-fields могут быть Consumed.
- **Assign в Live consume-поле / locals** (D5.1) → error E (D133-
  assign-live-field) с suggestion «consume the existing value first».
  Покрывает: `@field = expr` в record-методе при `@field` Live; ре-
  ассайн локальной consume-var (`consume tx = ...; consume tx = ...`)
  без consume старой.
- **Match на consume-value** — считается consume-операцией; pattern-
  bindings внутри arm'ов — независимые linear-binding'и, обязаны
  consumed в arm'е (D6 для Option[Transaction]).

Acceptance: 4 probe-фикстуры (forget / branch-forget / loop-forget /
field-not-restored) дают expected compile errors.

### Ф.5 — Pilot stdlib + диагностика (~0.5 dev-day)

- **Не мигрировать существующие типы.** `StringBuilder` остаётся
  affine (consume-метод `@into()` — опционально, забывать OK).
  Миграция в `consume` ломала бы 100+ call-sites без выгоды.
- **Mock `Transaction`** в `nova_tests/plan100/`:
  ```nova
  type Transaction consume { id int }
  fn Transaction consume @commit() -> ()
  fn Transaction consume @rollback() -> ()
  fn begin() -> Transaction => Transaction { id: 42 }
  ```
- **Mock `Service` с consume File** для проверки field-aware flow
  (D5) — закрытие / reopen / error-path.
- AI-first диагностика: при пропущенном consume — explicit list
  консьюм-методов из `runtime_registry` / type-decl.

Acceptance: pilot-фикстуры (positive: commit, rollback, branch-both,
reopen; negative: forget, partial, branch-forget, field-not-restored)
проходят как expected.

### Ф.6 — Тесты pos/neg (~0.5 dev-day)

`nova_tests/plan100/` (~17 фикстур):

**Позитивные (8):**
- `consume_ok_commit.nv` — `consume tx = begin(); tx.commit()`.
- `consume_ok_rollback.nv` — оба consume-метода работают.
- `consume_ok_both_branches.nv` — `if cond { tx.commit() } else { tx.rollback() }`.
- `consume_ok_factory.nv` — `fn factory() -> Transaction => begin()` (let-form,
  передача наверх).
- `consume_ok_pass_consume.nv` — `fn finish(consume tx Transaction)`.
- `consume_ok_record_field.nv` — record с consume-полем; consume-метод
  потребляет поле.
- `consume_ok_reopen.nv` — D5 reopen pattern (mut-метод, consume + rebind).
- `consume_ok_option_field.nv` — `consume file Option[File]`, match-
  based replacement.

**Негативные (9) (EXPECT_COMPILE_ERROR D133):**
- `consume_err_forget.nv` — `consume tx = begin()` без consume-метода.
- `consume_err_branch_forget.nv` — `if cond { tx.commit() }`.
- `consume_err_loop_forget.nv` — `loop { consume tx = begin() }`.
- `consume_err_pass_non_consume.nv` — `fn log(tx Transaction)` без
  `consume` qualifier — error на call-site.
- `consume_err_record_partial.nv` — record с consume-полем, exit non-
  consume-метода с Consumed-полем.
- `consume_err_field_marker.nv` — `tx Transaction` поле без `consume`-
  маркера при consume-типе.
- `consume_err_type_marker.nv` — record с consume-полем, но без
  `consume` на type-decl.
- `consume_err_panic_path.nv` — `consume tx = begin(); panic("x")`.
- `consume_err_assign_to_live_field.nv` — D5.1: `mut @overwrite() {
  @file = File.open()? }` без предшествующего `@file.close()`; покрывает
  silent-overwrite gateway.

Acceptance: 8 pos PASS, 9 neg дают expected E (D133); полный `nova
test` → 0 регрессий с baseline.

### Ф.7 — Docs + close (~0.3 dev-day)

- `docs/idiom/consume-types.md` — «когда писать `type X consume`»
  (heuristic: «забыть → leak / inconsistency»; примеры: Transaction,
  File, Lock-guard, pending-error).
- `docs/plans/README.md` — обновить Plan 100 row (терминология +
  ✅ ЗАКРЫТ).
- `docs/project-creation.txt` — closure entry.
- `nova-private/discussion-log.md` — обсуждение (опционально).
- Снять `[M-generic-consume-leak]` как «known hole, fixed by future
  strict-mode plan» (если будет заведён в Ф.4).
- Merge `plan-100` → `main`.

---

## Acceptance criteria

- [ ] `type X consume { ... }` парсится; согласованность маркеров (D4)
      enforced (field-marker / type-marker / non-empty).
- [ ] `consume tx = ...` (binding) парсится с strict-семантикой (D9).
- [ ] Consume-instance, не consumed до scope exit'а → compile error
      E (D133) с указанием консьюм-методов.
- [ ] Branch-forget / loop-forget / panic-path-forget → compile error.
- [ ] Передача consume в non-consume non-receiver fn-param → compile
      error (D2 / D7).
- [ ] Read-only param (без `consume`) разрешает только чтение + alias;
      consume-метод / store / return / closure-capture-с-consume → error
      (D7).
- [ ] Consume-поле в record'е — transitive (record сам consume), field-
      aware flow внутри методов (D5): mut-метод reopen pattern работает.
- [ ] Assign в Live consume-поле / повторный `consume tx = ...` без
      предшествующего consume → compile error E (D133-assign-live-field)
      с suggestion (D5.1).
- [ ] `Option[Transaction]` / `Box[Transaction]` / user-generic с
      consume-arg — автоматически consume через generic-заразность
      (D6); никакого Option-специфичного кода.
- [ ] `consume` + `-> @` на одном методе → parse error (D8).
- [ ] D131 affine остаётся доступным; `consume` без type-level
      `consume` работает как раньше (forget OK).
- [ ] Pilot `nova_tests/plan100/` — 8 pos + 9 neg PASS как expected.
- [ ] Полный `nova test` → 0 регрессий vs baseline (~1130 PASS на
      момент старта).
- [ ] Spec D133 опубликован; cross-ref'ы D131/D132/D75 ✅.

---

## Границы (bootstrap)

- **Generic strict-mode bound `[T consume]`** — отложено в future plan.
  Bootstrap: внутри generic-функции T неизвестен consume ли, обязательство
  не отслеживается → silent leak возможен (пример: `fn first[T](pair (T,
  T)) -> T => pair.0` теряет `pair.1`). Документируется маркером
  `[M-generic-consume-leak]`.
- **Deep peek внутрь `Option[ConsumeType]`** — невозможен без borrow-
  концепта. Тривиальный peek через `is_some()` / `is_none()` (Plan 95
  ✅) работает. Глубокий peek требует view/borrow — future plan.
- **Closure capture consume-var** — bootstrap-permissive: closure body
  считается «может consume» (закрывает обязательство caller'а). False-
  negative: closure без consume в теле → silent forget. Уточнение —
  Plan 73 followup-style в будущем.
- **`defer` / `errdefer` cleanup-блоки** — body может consume; учёт
  bootstrap: только unconditional consume в defer-body засчитывается.
- **Linear в `[]T`** — массивы из consume-элементов: bootstrap не
  делает collection-API consume-aware (`vec.push(tx)` где push без
  `consume` qualifier → error). Закрытие — followup.
- **Linear через FFI / external fn** — external без `consume` на
  consume-параметре → compile error. Bootstrap: consume не пересекает
  FFI границу.

---

## Risks

1. **Combinatorial alias-tracking** — Plan 73 followup уже сложный;
   field-aware расширение усугубляет. **Митигация:** переиспользовать
   `dissolve_alias_class` + добавить field-paths (`@.tx`, `@.file`)
   как алиас-class members.
2. **Match на consume-Option / Result** — pattern-binding должен быть
   linear; Plan 95 mono-pipeline уже знает Option/Result, расширение
   `check_consume` на match-arms — точечно. **Митигация:** Ф.4 probe
   с `consume_ok_option_field` + `consume_err_match_arm_forget` (если
   forgot consume f в Some-arm).
3. **Field-aware flow analysis может пропустить assignments через
   deep paths** (`@.state.tx.commit()`). **Митигация:** bootstrap
   ограничиваем direct-fields (`@.field`); nested-paths — followup.
4. **Migration cost** реальных типов (`File` / `Connection` / lock-
   guard). **Митигация:** pilot Plan 100 — только mock; реальная
   миграция в Plan 18 stdlib MVP.
5. **`return` через `expr?` / `expr!!`** error-paths — наиболее тонкое
   место (Plan 49 lineage). **Митигация:** переиспользовать handler-
   flow и cancel-throw routing из Plan 49.
6. **AI-first диагностика** для типов с N consume-методами — текст
   может разрастаться. **Митигация:** truncate с suggestion «see
   `nova doc Transaction`», explicit list только если ≤4 методов.
7. **Suppress-pressure (anti-pattern)** — если кто-то предложит
   «forget» escape hatch, это gateway-bug. **Митигация:** явная
   позиция в spec D133 «suppress не существует by design».

---

## Связь

- [Plan 73 / D131](73-consume-qualifier.md) — affine `consume`;
  Plan 100 reuses `check_consume` pass и `VarState` flow-tracker.
- [Plan 77 / D132](77-fluent-return.md) — `-> @` fluent-return;
  builder-chain alias через `-> @` критичен для sound consume через
  builder API. Plan 77 §«Потенциал лучше Rust» — outline-источник.
- [Plan 95](95-builtin-sum-method-mono.md) — Option/Result как
  generic-method-able; обеспечивает базис для match consume-Option.
- [Plan 47 / D75](47-supervised-cancel.md) — почему universal
  consume отвергнут; Plan 100 — opt-in per-type ровно в этом scope'е.
- [Plan 18 stdlib](18-stdlib-roadmap.md) — реальные кандидаты на
  миграцию (`File`, `Connection`, lock-guard).
- [Plan 49 / D85](49-cancel-throw-routing.md) — kinded throws и
  cancel-routing; источник для error-path handling в Ф.4.
- [Plan 91 stdlib MVP](91-stdlib-mvp-for-0.1.md) — после 0.1 появятся
  типы, для которых consume имеет смысл.
- [D75 §«Compile-time token-scope enforcement»](../../spec/decisions/06-concurrency.md#d75)
  — формулировка отказа от universal consume для GC-языка.
