// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 100: linear `must-consume` — D133

> **Создан 2026-05-23.** Выделен из outline'а [Plan 77 §«Потенциал «лучше
> Rust»»](77-fluent-return.md#L105-L109) (D132 `-> @` fluent-return) как
> «отдельный возможный план».
>
> **Статус:** 📋 proposed, не начат, **P3** (polish — не блокер релиза
> 0.1, ожидает stdlib MVP). ~4–6 dev-day.
>
> **Зависимости:**
> - [Plan 73](73-consume-qualifier.md) ✅ (D131 affine `consume` —
>   foundation; flow-sensitive `check_consume` pass, `VarState`,
>   alias-tracking).
> - [Plan 77](77-fluent-return.md) ✅ (D132 `-> @` fluent-return —
>   sound builder-chain alias через `check_fluent_return`; без него
>   linear был бы дырявым).
> - [Plan 91](91-stdlib-mvp-for-0.1.md) — pilot-candidates (`File`,
>   `Connection`, lock-guard) появляются после stdlib MVP.
>
> **Цель:** ввести в Nova **linear «must-consume»** квалификатор на
> type-decl: instance такого типа **обязан** быть consumed до выхода из
> scope'а на **каждом** пути выполнения. Compile error если на каком-то
> exit-point значение всё ещё `Live`. Канонический use-case — `Transaction`
> обязан `.commit()` или `.rollback()`; `File` обязан `.close()`;
> lock-guard обязан `.release()`. Rust такое не enforce'ит
> (`#[must_use]` — только warning, подавляется). Это направление, где
> Nova **строже Rust** без borrow-checker.

---

## Контекст

D131 (Plan 73) ввёл `consume` — **affine** квалификатор: значение
можно потребить **≤1 раз**. Использовать после consume — compile
error. **Забыть** значение (не потребить вообще) — OK, GC соберёт.

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
запись в БД, file descriptor leak, deadlock на mutex'е. Это **не
memory safety** (GC справится), а **логическая надёжность** —
дополнение к D131 с противоположной стороны:

| Свойство | D131 affine `consume` | Plan 100 linear `must-consume` |
|---|---|---|
| Потребить ≤1 раз | ✅ enforce | ✅ enforce (наследуется) |
| Потребить ≥1 раз (обязательно) | ❌ забыть OK | ✅ enforce — must consume |
| Тип помечается на | receiver / param метода | **type-decl** |
| Bootstrap-аналог | `StringBuilder.@into()` | `Transaction.@commit()/.@rollback()` |

---

## Сравнение с другими языками

| Язык | Механизм | Точность | Suppressable |
|---|---|---|---|
| **Rust** `#[must_use]` | attribute на return-type / тип | warning | да (`let _ = v`) |
| **Rust** ownership + `Drop` | RAII auto-cleanup | guaranteed | нет, но Drop не различает «commit» от «rollback» |
| **Linear Haskell** `=>` arrow | linear arrow в системе типов | guaranteed | нет, type-level |
| **Idris** linear-types | quantitative type theory | guaranteed | нет |
| **Go** | нет; convention + linter | manual | n/a |
| **TS** | нет | n/a | n/a |
| **Nova** D131 (текущий) | affine `consume` | guaranteed «≤1» | n/a |
| **Nova** Plan 100 linear | `linear type` на type-decl | guaranteed «обязан» | **нет** (consume-метод — единственный способ) |

**Что Nova может лучше:** Rust `#[must_use]` подавляется через `let _
= v` — gateway-bug. `Drop` гарантирует cleanup, но не различает
«commit» (успех) от «rollback» (откат) — оба auto, выбор теряется.
Linear Haskell / Idris дают точность, но ценой type-system complexity
(quantitative arrows). Nova получает Rust-точность **без borrow** и
строже Rust по non-suppressability — поверх GC, без move-семантики.

---

## Что НЕ входит (явно отвергнуто)

- **Universal affine/linear для всех `let`.** Отвергнуто в [spec D75
  «Compile-time token-scope enforcement»](../../spec/decisions/06-concurrency.md#d75)
  и [Plan 47:143](47-supervised-cancel.md): «affine/linear-типы
  несоразмерны — это Rust borrow checker ради одной фичи, несоразмерно
  для GC-языка». Linear — **opt-in per type**, не default.
- **Borrow checker / lifetimes / move-семантика в Rust-смысле.** Память
  по-прежнему GC (D6). Linear работает поверх существующего
  `check_consume` pass'а как dataflow-расширение.
- **Suppress-механизм `let _ = v`.** Намеренно отсутствует (anti-Rust
  `#[must_use]` gateway). Единственный способ удовлетворить linear —
  consume-метод. Если в коде встречается «иногда хочу забыть» — это
  знак, что тип неправильно помечен linear.
- **Drop-method auto-cleanup.** Linear требует **явный** consume —
  выбор `.commit()` vs `.rollback()` важен. Auto-drop размывает выбор.
- **Linear bound на generic-параметре (`[T linear]`).** Откладывается:
  bootstrap-scope — generic T неизвестен linear ли, linear-маркер
  пропадает (sound в сторону permissive).

---

## Архитектурные решения (Ф.0 GATE — фиксировать до старта)

### D1: место маркера

A. **На type-decl:** `linear type Transaction { ... }` —
   рекомендуется.
B. На return-type: `fn begin() -> linear Transaction`.
C. На конструкторе: `fn Transaction must_consume @new() -> Self`.

→ **Рекомендация A.** Type-уровень — единственное место, где маркер
наследуется на **все** instance'ы консистентно. B/C создают
inconsistency (один и тот же тип linear в одной точке, не linear в
другой), что взрывает flow-tracking.

### D2: что считается consume для linear

- **consume-метод (D131)** — ✅ обязательно. Главный канал.
- **передача в consume-параметр** — ✅ обязательство переходит callee.
- **`return v`** — ✅ обязательство передаётся caller'у (linear-тип в
  return-type — caller обязан consume).
- **передача в обычный (non-consume) fn-параметр** — ❌ compile error.
  Linear-значение **не может** просто «передаться» — либо callee
  объявлен `consume`, либо вызов в receiver-position consume-метода.

### D3: точки проверки (scope-exit'ы)

На каждом из этих exit-point'ов проверяем — нет ли live linear-var:
- конец function body (последний statement);
- `return expr` — все live linear-vars кроме возвращаемой → error;
- `panic` / `expr!!` / `??!!` — пути с выходом без consume → error;
- `loop break` — exit из цикла с live linear → error;
- `defer` / `errdefer` тело может consume (учитываем при join).

### D4: linear как поле record'а — transitivity

A. Запрещено (linear только top-level binding).
B. **Record, содержащий linear-поле, сам становится linear** (transitive).

→ **Рекомендация B.** Иначе `struct App { tx: Transaction }` — тривиальный
escape. Partial consume через `app.tx.commit()` помечает `app.tx =
Consumed` (требует field-aware tracking в `ConsumeCtx`).

### D5: linear в generics

- `Result[linear T, E]` / `Option[linear T]` — wrapper-типы. Linear
  наследуется внутрь Ok/Some variants: «если Ok — значит linear T внутри,
  обязан быть consumed дальше». Если Err/None — обязательства нет.
- `fn foo[T](v T)` — T неизвестно linear ли. **Bootstrap:** в generic
  context'е linear-маркер пропадает (sound в сторону permissive —
  false-negative, не false-positive). Linear bound `[T linear]` — будущее
  расширение, не в scope Plan 100.

### D6: spec D-блок

**Зарезервирован D133** (после D131 affine / D132 fluent-return).
Раздел spec — `02-types.md` (linear — свойство type-decl), с cross-ref
в `05-memory.md` (родственный D131).

### D7: миграционная политика

Linear — **opt-in**, ни один существующий тип в bootstrap-stdlib не
становится linear автоматически. Pilot — **mock `Transaction`** в
`nova_tests/plan100/`, без реального DB. Реальные миграции
(`File.close` / `Lock.release` / `Connection.close`) — после Plan 18
stdlib MVP.

---

## Фазы

### Ф.0 — GATE: design + decision points (~0.5 dev-day)

- Зафиксировать D1–D7 выше; внести правки если probe выявит
  inconsistency.
- Probe: hand-написать желаемое поведение для 2 use-case'ов
  (mock `Transaction.@commit()`/`.@rollback()` + linear field в
  record'е). Убедиться, что fixture-grammar self-consistent.
- Decision point: **A B C** (D1) + transitivity B (D4).
- Acceptance: документ Ф.0 в `docs/plans/100-artifacts/gate.md`
  фиксирует выбранные варианты.

### Ф.1 — Спек D133 в `spec/decisions/02-types.md` (~0.3 dev-day)

- Раздел `## D133. linear type — обязательная consume-семантика`.
- Что / Синтаксис / Правило (must-consume на каждом exit-point) /
  Transitivity (D4) / Границы bootstrap (D5) / Сравнение Rust/
  Haskell/Go/TS.
- Cross-ref D131 (affine sibling), D132 (sound alias), D75 (почему
  не universal).
- Зеркало в `05-memory.md` §«Связь» — приписать D133.

### Ф.2 — AST + lexer + parser (~0.5 dev-day)

- **Лексер:** `KwLinear` (после `KwConsume`).
- **AST:** `TypeDecl { linear: bool, ... }` — единое поле в `TypeDeclKind::Record`
  / `TypeDeclKind::Sum` (Plan 100 пилот — record; sum после Ф.4).
- **Парсер:** `linear type Foo { ... }` — `linear` qualifier стоит
  **перед** `type` (как `export`/`pub` качества). Position: `linear`
  и `consume` на одном decl не совмещаются (parse error — `consume`
  — receiver/param qualifier, `linear` — type qualifier).
- **Acceptance:** parser принимает `linear type Foo { ... }`,
  отвергает `consume linear type` / `linear consume type`.

### Ф.3 — Linearity registry + transitivity (~0.5 dev-day)

- `LinearityRegistry` в `types/mod.rs`: `HashSet<String>` linear-type
  names + helper `type_is_linear(TypeRef)`:
  - сам тип в registry → true;
  - Record с ≥1 linear-полем → true (transitive, D4);
  - generic-wrap (`Option[T]`/`Result[T,E]`/user-generic): true если
    хотя бы один type-arg linear (D5);
  - generic-param T (без bound): false (bootstrap).
- Pre-pass до `check_consume`: собираем registry из всех модулей.
- Acceptance: `type_is_linear` корректен на pilot-typah (mock
  Transaction, Option[Transaction], record{tx: Transaction}).

### Ф.4 — `check_consume` extension: must-consume на exit-point'ах (~1.5 dev-day)

Расширяем существующий `check_consume` pass (Plan 73 Ф.4):

- **На каждом scope exit** — проход по `ConsumeCtx.states`: для каждой
  var, чей тип `type_is_linear == true` и состояние `Live` или
  `MaybeConsumed` — emit error E (D133):
  ```
  error: linear value `tx` must be consumed before scope exit
    note: type `Transaction` is declared `linear`
    note: declared at <span>
    note: consume via `.commit()` or `.rollback()`
  ```
- **`return expr`:** возвращаемая var с linear-типом передаёт
  обязательство caller'у (НЕ ошибка — переход в `Returned` state);
  остальные live linear — error.
- **`panic` / `expr!!` / `??!!` / unwinding-paths:** все live linear
  → error. Это main rationale (вне Rust, где `Drop` срабатывает на
  unwind — но различить commit/rollback нельзя, см. Сравнение).
- **`loop break`:** на пути break'а — live linear → error.
- **Branch join (`if`/`match`):** linear var, ставшая `MaybeConsumed`,
  при exit-point расценивается как error (а не warning, как в
  affine D131 — здесь жёстче).
- **Передача в non-consume non-receiver-position fn-параметр:**
  compile error E (D133-move) — linear не может «просто передаться».
- **Field-aware partial consume (D4):** при `r.f.into()` где
  `type_is_linear(r) && type_is_linear(r.f)` — состояние `r.f` →
  `Consumed`; record exit check'ает per-field.

Acceptance: 4 probe-фикстуры (forget / branch-forget / loop-forget /
record-partial) дают expected compile errors.

### Ф.5 — Pilot stdlib + диагностика (~0.5 dev-day)

- **Не мигрировать существующие типы.** `StringBuilder` остаётся
  affine (consume-метод `@into()` — opcional cosume, забывать OK):
  миграция в linear ломала бы 100+ call-sites без выгоды (если
  забыл — буфер просто GC).
- **Mock `Transaction`** в `nova_tests/plan100/`:
  ```nova
  linear type Transaction { ... }
  fn Transaction consume @commit() -> ()
  fn Transaction consume @rollback() -> ()
  fn begin() -> Transaction => Transaction { ... }
  ```
- AI-first диагностика: при пропущенном consume —
  «`Transaction` requires `.commit()` or `.rollback()` before scope
  exit» (явное перечисление consume-методов из `runtime_registry` /
  type-decl).

Acceptance: pilot-фикстура с двумя положительными (commit / rollback)
и тремя отрицательными (forget / partial / branch) проходит как
expected.

### Ф.6 — Тесты pos/neg (~0.5 dev-day)

`nova_tests/plan100/` (~12 фикстур):

**Позитивные:**
- `linear_ok_commit.nv` — `let tx = begin(); tx.commit()` — OK.
- `linear_ok_rollback.nv` — оба consume-метода.
- `linear_ok_both_branches.nv` — `if cond { tx.commit() } else { tx.rollback() }`.
- `linear_ok_return.nv` — `fn factory() -> Transaction => begin()`;
  caller обязан consume.
- `linear_ok_pass_consume.nv` — `fn finish(consume tx Transaction) -> ()`;
  обязательство переходит callee.
- `linear_ok_record_field.nv` — record с linear-полем, partial consume
  через field.

**Негативные (EXPECT_COMPILE_ERROR D133):**
- `linear_err_forget.nv` — `let tx = begin()` без consume.
- `linear_err_branch_forget.nv` — `if cond { tx.commit() }` (only one
  branch).
- `linear_err_loop_forget.nv` — `loop { let tx = begin() }` (на 2-й
  итерации `tx` предыдущей итерации live).
- `linear_err_pass_non_consume.nv` — `fn log(tx Transaction)` без
  `consume` qualifier — error на call-site.
- `linear_err_record_partial.nv` — record с linear-полем, exit без
  consume этого поля.
- `linear_err_panic_path.nv` — `let tx = begin(); panic("x")` — путь
  через panic без consume.

Acceptance: 6 pos PASS, 6 neg дают expected E (D133); полный
`nova test` → 0 регрессий с baseline.

### Ф.7 — Docs + close (~0.3 dev-day)

- `docs/idiom/linear-types.md` — «когда писать `linear type`» (heuristic:
  «забыть → leak / inconsistency»; примеры: Transaction, File, Lock-guard,
  Pending-error).
- `docs/plans/README.md` — добавить Plan 100 row.
- `docs/project-creation.txt` — closure entry.
- `nova-private/discussion-log.md` — обсуждение (опциональное).
- Snять `[M-must-consume-linear-outline]` маркер если будет заведён
  (или просто закрыть outline в Plan 77 ссылкой на Plan 100).

---

## Acceptance criteria

- [ ] `linear type Foo { ... }` парсится; `linear` + `consume` на
      одном decl → parse error.
- [ ] Linear instance, не consumed до scope exit'а → compile error
      E (D133) с указанием консьюм-методов.
- [ ] Branch-forget / loop-forget / panic-path-forget → compile error.
- [ ] Передача linear в non-consume non-receiver fn-param → compile
      error (D133-move).
- [ ] Linear как record-поле — transitive (record сам linear), partial
      consume через field соответствует D4.
- [ ] D131 affine остаётся доступным; `consume` без `linear` работает
      как раньше (forget OK).
- [ ] Pilot `nova_tests/plan100/` — 6 pos + 6 neg PASS как expected.
- [ ] Полный `nova test` — 0 регрессий vs baseline (предположительно
      ~1130 PASS на момент старта).
- [ ] Spec D133 опубликован, cross-ref'ы D131/D132/D75 ✅.

---

## Границы (bootstrap)

- **Generic linear bound** (`[T linear]`) — отложено. Bootstrap:
  generic T неизвестен → linear-маркер пропадает (sound permissive).
- **Closure capture linear-var** — bootstrap-permissive: closure
  считается «может consume», что закрывает обязательство caller'а.
  Sound в сторону false-negative (closure без consume в теле даёт
  silent forget). Уточнение в Plan 73 followup-style.
- **`defer` / `errdefer` cleanup-блоки** — body может consume; учёт
  bootstrap: `defer { tx.rollback() }` после `let tx = begin()`
  засчитывается как consume на всех exit-path'ах (включая panic).
  Edge case (defer не выполнился из-за panic в `defer` body) —
  honest defer.
- **Linear в `[]T`** — массивы из linear-элементов. Bootstrap:
  collection-API linear-aware не делается (`vec.push(tx)` — это
  передача в non-consume `push` → error). Закрытие — отдельный
  followup после Plan 96 array-slices.
- **Linear через FFI / external fn** — external без `consume` на
  linear-параметре → compile error. Bootstrap: linear не пересекает
  FFI границу (consistent с D131 правилом «consume помечается на
  Nova-стороне»).

---

## Risks

1. **Combinatorial explosion с alias-tracking (Plan 73 followup).**
   Linear требует, чтобы alias-class linear-var тоже отслеживался
   как linear obligation: consume через любой alias закрывает
   обязательство. Митигация: переиспользовать `dissolve_alias_class`
   из Plan 73 followup; добавить linear-флаг в alias-class.

2. **Generic-distribution.** `Result[linear Transaction, E]` — Ok
   несёт linear, Err не несёт. Pattern-matching `match r { Ok(tx)
   => tx.commit(), Err(_) => () }` корректно? Ok-arm: tx linear,
   обязан consume — `.commit()` закрывает. Err-arm: нет linear var
   вообще — OK. Митигация: pattern-binding linear T → emit
   `VarState::Live` для tx в Ok-arm, обычная флоу-проверка.

3. **Миграция / migration cost.** Реальные типы (`File`,
   `Connection`, `Lock-guard`) появятся в Plan 18 stdlib. Pilot
   Plan 100 — mock, чтобы инфра была готова к моменту stdlib MVP.

4. **`return` через `expr!!` / `??!!`.** Error-paths — наиболее
   тонкое место (Plan 49 lineage). Митигация: переиспользовать
   handler-flow и cancel-throw routing из Plan 49.

5. **AI-first диагностика.** Сообщение «must be consumed» легко
   read'ается LLM'ом? Митигация: explicit list of consume-методов
   (через `runtime_registry` `is_consume` lookup) + structured
   suggestion как в Plan 50 D102 named-only.

6. **Suppress-pressure (anti-pattern).** Если кто-то добавит «forget»
   escape hatch — gateway-bug. Митигация: явная позиция в spec D133
   «suppress не существует by design».

---

## Связь

- [Plan 73 / D131](73-consume-qualifier.md) — affine `consume`;
  Plan 100 reuses `check_consume` pass и `VarState` flow-tracker.
- [Plan 77 / D132](77-fluent-return.md) — `-> @` fluent-return;
  builder-chain alias через `-> @` критичен для sound linear через
  builder API. Plan 77 §«Потенциал лучше Rust» — outline-источник.
- [Plan 47 / D75](47-supervised-cancel.md) — почему universal
  affine/linear отвергнуто; Plan 100 — opt-in per-type ровно в этом
  scope'е.
- [Plan 18 stdlib](18-stdlib-roadmap.md) — реальные кандидаты на
  миграцию (`File`, `Connection`, lock-guard); зависимость в сторону
  Plan 18 — pilot Plan 100 не блокирует Plan 18.
- [Plan 49 / D85](49-cancel-throw-routing.md) — kinded throws и
  cancel-routing; источник для error-path handling в Ф.4.
- [Plan 91 stdlib MVP](91-stdlib-mvp-for-0.1.md) — после релиза 0.1
  появятся типы, для которых linear имеет смысл.
- [D75 §«Compile-time token-scope enforcement»](../../spec/decisions/06-concurrency.md#d75)
  — формулировка отказа от universal linear для GC-языка.
