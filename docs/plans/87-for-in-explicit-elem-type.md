# Plan 87 — for-in с явным типом элемента (`for x TYPE in iter`)

> **Статус:** ✅ ЗАКРЫТ 2026-05-22 (Ф.1-Ф.5; ветка `plan-87`)
> **Приоритет:** P3 (фича опциональна, но это spec-compliance +
> design-consistency fix, не «сахар на потом»)
> **Оценка:** ~0.75 dev-day
> **Зависимости:** Plan 79 (typecheck hardening — assignability-проверки
> E73xx; переиспользуются в Ф.3) ✅
> **Источник:** Plan 85.5 — маркер `[M-for-in-explicit-elem-type]` в
> `docs/simplifications.md`

## Зачем

`spec/syntax.md` документирует явную аннотацию типа loop-переменной в
for-in как валидный синтаксис (в двух местах):

```nova
for id u64 in ids { ... }       // §«Аннотации типа — без двоеточия»
for x int in nums { ... }       // §«Циклы for / while / loop»
```

Bootstrap-парсер этого **не поддерживает** — после `for <ident>` сразу
ожидает `in` и падает «expected `in`, got identifier» (перепроверено
2026-05-22). Spec/impl drift: спека обещает фичу, которой нет.

Глубже это **дыра в дизайн-консистентности Nova**: язык построен на
универсальном правиле «name type» — аннотация типа доступна везде
(`let x int`, `fn(x int)`, `type { x int }`, `[T Bound]`,
`for id u64 in` по спеке). for-in — **единственное** место, где этого
правила нет в реализации. Plan 87 закрывает исключение.

## Сравнение с Go / Rust / TS

| Язык | Аннотация типа loop-переменной for-each |
|---|---|
| **Go** | ❌ нельзя. `for i, v := range s` — тип всегда выводится. |
| **Rust** | ❌ нельзя. `for x in iter` — тип выводится; `for x: T in` — не синтаксис Rust. |
| **TS** | ❌ **запрещено явно** — «The left-hand side of a 'for...of' statement cannot use a type annotation». |
| **Nova (цель)** | ✅ **опционально, и проверяется** — `for x T in iter`; если `T` ≠ тип элемента → compile error. |

Вывод: ни один из трёх не даёт аннотацию на loop-переменной. Nova
получает её как **строгий superset** — но это «лучше», а не «другое»,
**только при обязательной проверке** аннотации против фактического
типа элемента. Аннотация без проверки = тихий рассинхрон (можно
написать `for x str in [1,2,3]` и ввести в заблуждение читателя/AI) —
это было бы **хуже** индустрии. Поэтому type-check аннотации — **ядро
плана** (Ф.3), а не опциональный довесок.

Чем полезна проверяемая аннотация (то, чего нет у Go/Rust/TS):
- consistency с универсальным «name type» Nova;
- документация в местах, где тип элемента не очевиден (сложный
  итератор, generic-источник) — особенно ценно для AI-ревью (D10);
- checked assertion — фиксирует ожидание; смена типа источника →
  loud compile error, а не молчаливое протекание нового типа.

## Scope

- **Парсер:** после loop-pattern, если следующий токен не `in` —
  распарсить `TypeRef` по правилу «name type», затем обязательный `in`.
  Для `for` и `parallel for`.
- **Type-checker:** если аннотация задана — она **обязана** быть
  assignability-совместима с типом элемента итератора; иначе compile
  error. Биндинг loop-переменной получает аннотированный тип.
- **Codegen:** аннотированный тип используется как C-тип элемента.

**В scope:**
- `for x TYPE in iter`, `for mut x TYPE in iter`.
- Типы: примитивы, generic (`[]T`, `Option[T]`), user record/sum,
  protocol-типы.
- Полная проверка аннотация ↔ элемент (Ф.3).

## Декомпозиция (фазы и шаги)

> **Привязка к коду** (сверено 2026-05-22): for-in парсится
> `parse_for` (`compiler-codegen/src/parser/mod.rs:5272`) —
> `KwFor` → `parse_pattern()` → `expect(KwIn)` → `parse_expr()` → body.
> Узел `ExprKind::For { pattern, iter, body, invariants, decreases }`
> поля типа элемента **не имеет**. `parse_parallel_for` (:5297) —
> та же структура. Падение `for x TYPE in`: после `parse_pattern()`
> (съедает `x`) `expect(KwIn)` видит `TYPE` → «expected `in`».

### Ф.1 — AST + парсер (~0.15 д)

- **Ф.1.1** AST (`ast/mod.rs`): добавить поле
  `elem_type: Option<TypeRef>` в `ExprKind::For` и (для
  консистентности) `ExprKind::ParallelFor`.
- **Ф.1.2** `parse_for` (:5272): между `parse_pattern()` и
  `expect(KwIn)` — `if !self.check(&TokenKind::KwIn) { elem_type =
  Some(parse_type) }`. Тот же блок в `parse_parallel_for` (:5297).
- **Ф.1.3** `for mut x TYPE in` — проверить, что `mut` уже съедается
  `parse_pattern()`; если да — `mut x TYPE` работает без доп. правок.
- **Ф.1.4** Прокинуть `elem_type` в оба конструктора `ExprKind::For{}`
  / `ParallelFor{}`.

### Ф.2 — Обновление match-сайтов + codegen (~0.15 д)

- **Ф.2.1** Все `ExprKind::For { .. }` / `ParallelFor { .. }` —
  добавить `elem_type` (или `..`). Известные сайты (grep):
  `types/mod.rs` (~6: walk/infer/check-проходы),
  `codegen/emit_c.rs` (`emit_for`), `ast/pretty.rs`.
- **Ф.2.2** Type-checker: если `elem_type` задан — биндить
  loop-переменную с этим типом в scope; если `None` — текущий
  inference без изменений.
- **Ф.2.3** Codegen (`emit_for`): если `elem_type` задан — взять
  C-тип из него; иначе текущий путь. Аннотация **не** должна менять
  поведение для совпадающего типа (только Ф.3 добавляет проверку).

### Ф.3 — Type-check аннотации (ядро, ~0.2 д)

- **Ф.3.1** В type-checker (там, где выводится тип элемента for-in —
  `types/mod.rs`, обработка `ExprKind::For`): если `elem_type` задан,
  сравнить его с фактическим типом элемента итератора через
  assignability-проверку Plan 79 (E73xx-семейство).
- **Ф.3.2** Несовпадение → compile error с понятным текстом:
  «for-in: loop-переменная аннотирована `T`, но итератор даёт `U`»
  (новый код E73xx — следующий свободный в семействе Plan 79; или
  переиспользовать E7301 assignability с for-in-контекстом).
- **Ф.3.3** Согласовать с inferred-формой: `None` → проверки нет,
  поведение 1:1 как до плана.

### Ф.4 — Тесты (~0.15 д)

- **Ф.4.1** `nova_tests/plan87/explicit_primitive.nv` —
  `for x int in [...]`, `for b u8 in`, `for c char in`.
- **Ф.4.2** `nova_tests/plan87/explicit_generic.nv` —
  `for o Option[int] in`, элемент-generic; `for s str in`.
- **Ф.4.3** `nova_tests/plan87/explicit_mut.nv` — `for mut x int in`.
- **Ф.4.4** `nova_tests/plan87/explicit_custom_iter.nv` —
  `for x int in <user Iter>` + `parallel for`.
- **Ф.4.5** `nova_tests/plan87/neg_type_mismatch.nv` — **негатив**
  (`EXPECT_COMPILE_ERROR`): `for x str in [1,2,3]` — аннотация ≠ тип
  элемента → compile error (ключевой production-тест).
- **Ф.4.6** `nova_tests/plan87/neg_bad_syntax.nv` — негатив: мусор
  между loop-pattern и `in`.
- **Ф.4.7** Регресс: полный `nova test` — inferred `for x in` цел,
  0 новых FAIL.

### Ф.5 — Spec / docs (~0.1 д)

- **Ф.5.1** `spec/syntax.md` — сверить/уточнить примеры
  `for id u64 in ids` / `for x int in nums`; добавить явное
  предложение «аннотация проверяется компилятором».
- **Ф.5.2** `docs/simplifications.md` —
  `[M-for-in-explicit-elem-type]` → ✅ ЗАКРЫТО.
- **Ф.5.3** `docs/plans/README.md` — Plan 87 → ✅ ЗАКРЫТ.
- **Ф.5.4** `docs/project-creation.txt` +
  `nova-private/discussion-log.md` — записи.

## Acceptance criteria

- [x] `for x TYPE in iter` парсится и работает — примитивы, generic
      (`[]T`, `Option[T]`), user Iter. _(user record/sum, protocol-типы:
      деструктуризация боксированного sum-элемента в теле упирается в
      pre-existing codegen-дефект `[M-iflet-match-boxed-sum-ptr]` —
      аннотация при этом парсится/проверяется/итерация работает.)_
- [x] `for mut x TYPE in iter` работает (`mut` съедается в `parse_for`).
- [x] `parallel for x TYPE in iter` работает (консистентно с `for`).
- [x] **Аннотация проверяется:** `for x WrongType in iter` →
      compile error `E7340` (Ф.4.5 — `neg_type_mismatch.nv`).
- [x] Inferred-форма `for x in iter` не сломана; `elem_type = None`
      — поведение 1:1 как до плана (codegen аннотацию не потребляет).
- [x] Полный `nova test` — 0 новых FAIL относительно baseline.
- [x] `[M-for-in-explicit-elem-type]` закрыт; spec/impl drift устранён.

## Non-scope

- **Tuple-паттерн с поэлементным типом** — `for (k str, v int) in m`.
  Tuple-деструктуризация в for-in остаётся без поэлементных типов.
  Это **не делает Nova хуже индустрии**: Go (`for k, v := range m`),
  Rust (`for (k, v) in &m`), TS — также не дают типы внутри
  destructuring-паттерна for-each. Деление осознанное, не упрощение.
- Existential / dynamic итераторы, изменения протокола `Iter[T]` —
  Plan 87 трогает только синтаксис аннотации, не семантику итерации.
