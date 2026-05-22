# Plan 87 — for-in с явным типом элемента (`for x TYPE in iter`)

> **Статус:** 📋 proposed 2026-05-22, не начат
> **Приоритет:** P3 (L — тип элемента всегда выводится; аннотация —
> документирующий сахар, не функциональная необходимость)
> **Оценка:** ~0.5 dev-day
> **Зависимости:** нет (standalone)
> **Источник:** Plan 85.5 — маркер `[M-for-in-explicit-elem-type]` в
> `docs/simplifications.md`

## Зачем

`spec/syntax.md` документирует явную аннотацию типа loop-переменной в
for-in как валидный синтаксис:

```nova
for id u64 in ids { ... }       // syntax.md §«Аннотации типа»
for x int in nums { ... }       // syntax.md §«Циклы for / while / loop»
```

Bootstrap-парсер этого **не поддерживает** — после `for <ident>` сразу
ожидает `in` и падает «expected `in`, got identifier». Это spec/impl
drift: спека обещает фичу, которой в реализации нет. Все существующие
тесты (`nova_tests/syntax/for_iter*.nv`) используют inferred-форму
`for x in iter`, поэтому drift не всплывал до Plan 85.5.

Цель: реализовать `for x TYPE in iter` в парсере — привести
реализацию в соответствие со спекой.

## Scope

Парсер for-in: после loop-pattern, если следующий токен **не** `in` —
распарсить `TypeRef` по правилу «name type» (D-без двоеточия), затем
обязательный `in`.

**В scope:**
- `for x TYPE in iter` — простой ident + тип элемента.
- `for mut x TYPE in iter` — с `mut`-биндингом.
- Типы: примитивы (`int`, `u64`, `char`, ...), generic (`[]T`,
  `Option[T]`), user-типы.

**НЕ в scope** (см. Non-scope):
- `for (a, b) TYPE in` — tuple-паттерн с типом.
- Семантическая проверка, что аннотированный тип совпадает с типом
  элемента итератора.

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
  `parse_pattern()` (binding-mutability на стороне паттерна); если да
  — `mut x TYPE` работает без доп. правок.
- **Ф.1.4** Прокинуть `elem_type` в оба конструктора `ExprKind::For{}`
  / `ParallelFor{}`.

### Ф.2 — Обновление match-сайтов (~0.1 д)

- **Ф.2.1** Все `ExprKind::For { .. }` / `ParallelFor { .. }` —
  добавить `elem_type` (или `..`). Известные сайты (grep):
  `types/mod.rs` (~6: walk/infer/check-проходы),
  `codegen/emit_c.rs` (`emit_for`), `ast/pretty.rs`.
- **Ф.2.2** Type-checker: если `elem_type` задан — биндить
  loop-переменную с этим типом в scope (а не inferred);
  если `None` — текущий inference без изменений.
- **Ф.2.3** Codegen (`emit_for`): если `elem_type` задан — взять
  C-тип из него; иначе — текущий путь.

### Ф.3 — Тесты (~0.15 д)

- **Ф.3.1** `nova_tests/plan87/explicit_primitive.nv` —
  `for x int in [...]`, `for b u8 in`, `for c char in`.
- **Ф.3.2** `nova_tests/plan87/explicit_generic.nv` —
  `for o Option[int] in`, элемент-generic.
- **Ф.3.3** `nova_tests/plan87/explicit_mut.nv` — `for mut x int in`.
- **Ф.3.4** `nova_tests/plan87/explicit_custom_iter.nv` —
  `for x int in <user Iter>`.
- **Ф.3.5** `nova_tests/plan87/neg_bad_for_type.nv` — негатив
  (`EXPECT_COMPILE_ERROR`): мусор между loop-pattern и `in`.
- **Ф.3.6** Регресс: полный `nova test` — inferred `for x in` цел,
  0 новых FAIL.

### Ф.4 — Spec / docs (~0.1 д)

- **Ф.4.1** `spec/syntax.md` — сверить примеры `for id u64 in ids` /
  `for x int in nums`, при необходимости уточнить.
- **Ф.4.2** `docs/simplifications.md` —
  `[M-for-in-explicit-elem-type]` → ✅ ЗАКРЫТО.
- **Ф.4.3** `docs/plans/README.md` — Plan 87 → ✅ ЗАКРЫТ.
- **Ф.4.4** `docs/project-creation.txt` +
  `nova-private/discussion-log.md` — записи.

## Acceptance criteria

- [ ] `for x TYPE in iter` парсится и работает — примитивы, generic,
      user-типы.
- [ ] `for mut x TYPE in iter` работает.
- [ ] Inferred-форма `for x in iter` не сломана (регресс 0 FAIL).
- [ ] Полный `nova test` — 0 новых FAIL относительно baseline.
- [ ] `[M-for-in-explicit-elem-type]` закрыт в simplifications.md.

## Non-scope

- **Tuple-паттерн с типом** — `for (a, b) TYPE in`. Tuple-
  деструктуризация в for-in остаётся без поэлементных типов.
- **Type-checker enforcement** — проверка, что аннотированный тип
  равен фактическому типу элемента итератора. Если реализуется
  тривиально в Ф.1 — добавить; иначе вынести в отдельную задачу
  (аннотация без проверки = всё равно документирующий сахар, как
  было до плана для inferred-формы).
