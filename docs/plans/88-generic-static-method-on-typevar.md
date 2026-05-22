# Plan 88 — generic static-method dispatch на type-параметре

> **Статус:** 📋 proposed 2026-05-22, не начат
> **Приоритет:** P2 (M — idiom `T.from(v)` в generic-коде; пока
> обходится конверсией на call-site)
> **Оценка:** ~1–1.5 dev-day (включая обязательный аудит Ф.0)
> **Зависимости:** Plan 48 (мономорфизация) ✅ partial; Plan 54 / 63
> (mono followups) ✅ — Plan 88 продолжает эту линию
> **Источник:** Plan 85.3 — маркер `[M-generic-static-method-on-typevar]`
> в `docs/simplifications.md`

## Зачем

Вызов **static-метода на type-параметре** внутри тела generic-функции
не мономорфизируется:

```nova
fn wrap[T From[str]](s str) -> T => T.from(s)
let n = wrap[Name]("alice")     // ← undefined symbol nova_fn_T_from
```

Codegen эмитит литеральный `nova_fn_T_from` — `T` не подставляется в
obj-позиции static-вызова → undefined symbol на линковке. Аналогично
`Result[T, E]` с type-параметром в return-позиции generic-функции даёт
unsubstituted `Nova_Result_...` (CC-FAIL).

**Контраст:** instance-метод на type-параметре (`it.next()` для
`[T Iter[U]]`) мономорфизируется корректно (Plan 62
`protocol_param_generic_bound.nv`). Дыра — именно static-вызовы
`T.method(...)`.

Линия mono-followup'ов: **Plan 48** (mono core) → **Plan 54**
(return-path) → **Plan 63** (cross-module dispatch) → **Plan 88**
(static-dispatch на typevar).

Уже отмечалось как bootstrap-ограничение в Plan 62.E (`nova_tests/
plan62/tryfrom_tryinto_from_prelude.nv` — bound-функции «не
вызываются»).

## Scope

- `emit_c.rs` static-call resolution: при `obj = Ident(n)`, где `n` —
  type-параметр в активном mono-контексте (`current_type_subst`) —
  резолвить `n` в concrete Nova-тип (через `nova_type_name_from_c` или
  registry) и далее обычный static-dispatch.
- mono `Result[T, E]` и прочих generic-типов с type-параметром в
  return-позиции инстанцируемой generic-функции.

## Декомпозиция (фазы и шаги)

> **Привязка к коду** (из работы Plan 85.3, сверить при старте):
> static-call резолвится в `emit_c.rs` ~line 14987 — `recv_type_name`
> из `obj = Ident(n)`: ветка `method_overloads.keys().any(t==n)` →
> static, иначе → instance через `obj_ty`. Для type-параметра `n` ни
> то ни другое не срабатывает → fallback `nova_fn_<n>_<method>`.
> Mono-контекст: `current_type_subst: HashMap<typeparam → C-type>`.

### Ф.0 — Аудит кластера mono-static-dispatch (~0.25 д) — GATE

Static-метод на typevar — почти наверняка не единственная дыра. План
обязан закрыть **семейство**, а не один симптом.

- **Ф.0.1** Probe static-методов на type-параметре в generic-теле:
  `T.from`, `T.try_from`, `T.new`, `T.with_capacity`, user `T.make`
  — временные фикстуры, зафиксировать симптом каждого
  (CC-FAIL / undefined-symbol / silent-wrong).
- **Ф.0.2** Probe generic return-типов: `Result[T,E]`, `Option[T]`,
  `[]T`, generic record в return-позиции generic-функции с unresolved
  `T`.
- **Ф.0.3** Свести симптомы в таблицу в разделе «Кластер по итогам
  Ф.0» (дописывается в этот план).
- **Ф.0.4** **Decision point:** что закрывает Plan 88, что выносится
  в Plan 54-lineage. Финализировать объём Ф.1/Ф.2.

### Ф.1 — Static-call на typevar в mono-контексте (~0.4 д)

- **Ф.1.1** `emit_c.rs` static-call resolution (~14987): при
  `obj = Ident(n)`, если `n ∈ current_type_subst` — резолвить `n` →
  concrete Nova-тип (`nova_type_name_from_c(current_type_subst[n])`),
  далее обычный static-dispatch вместо fallback `nova_fn_<n>_<m>`.
- **Ф.1.2** Синхронно поправить `want_instance` (тот же блок): для
  резолвленного typevar это static-вызов (`want_instance = false`).
- **Ф.1.3** Покрыть **все** static-методы (`from`/`try_from`/`new`/
  `with_capacity`/user), не только `from` — обобщённый путь, не
  спец-кейс.
- **Ф.1.4** Targeted-verify: probe-фикстуры из Ф.0.1 → PASS.

### Ф.2 — Generic return-тип с type-параметром (~0.4 д, объём из Ф.0)

- **Ф.2.1** mono `Result[T, E]` / `Option[T]` в return-позиции
  инстанцируемой generic-функции (T подставляется в return-тип).
- **Ф.2.2** Если Ф.0 показал, что generic-record / `[]T` return —
  отдельное большое семейство → defer в Plan 54-lineage,
  зафиксировать в Non-scope этого плана.
- **Ф.2.3** Targeted-verify: probe-фикстуры из Ф.0.2 → PASS.

### Ф.3 — Тесты (~0.2 д)

- **Ф.3.1** `nova_tests/plan88/` позитив — generic-функция с
  настоящим `T.from(...)` / `T.try_from(...)` внутри тела, реально
  вызывается с turbofish и работает.
- **Ф.3.2** `nova_tests/plan88/` негатив (EXPECT_COMPILE_ERROR).
- **Ф.3.3** Переписать `nova_tests/protocols/conversion/
  generic_from_bound.nv` + `generic_try_bound.nv` — с probe-формы
  (T как value-тип, конверсия на call-site) на прямой `T.from` /
  `T.try_from` внутри generic-тела; снять комментарии-ограничения.
- **Ф.3.4** Полный `nova test` — 0 новых FAIL.

### Ф.4 — Spec / docs (~0.1 д)

- **Ф.4.1** `docs/simplifications.md` —
  `[M-generic-static-method-on-typevar]` → ✅ ЗАКРЫТО; родственные
  маркеры из Ф.0 — обновить.
- **Ф.4.2** D72/D73 — аменд только если меняется наблюдаемая
  семантика; вероятно не требуется (фикс приводит реализацию в
  соответствие с уже существующими D72/D73).
- **Ф.4.3** `docs/plans/README.md` — Plan 88 → статус-апдейт.
- **Ф.4.4** `docs/project-creation.txt` +
  `nova-private/discussion-log.md` — записи.

## Кластер по итогам Ф.0

> Заполняется по результатам аудита Ф.0 — таблица «симптом → стадия
> отказа → закрывает Plan 88 / вынесено». До аудита раздел пуст.

## Acceptance criteria

- [ ] `fn wrap[T From[str]](s str) -> T => T.from(s)` + вызов
      `wrap[X]("...")` компилируется, линкуется, корректно работает.
- [ ] generic-функция с `Result[T, E]` return-типом мономорфизируется
      (либо обоснованный defer в Plan 54-lineage по итогам Ф.0).
- [ ] `generic_from_bound.nv` / `generic_try_bound.nv` переписаны на
      прямой `T.from` / `T.try_from` внутри generic-тела.
- [ ] Полный `nova test` — 0 новых FAIL.
- [ ] `[M-generic-static-method-on-typevar]` закрыт.

## Non-scope

- Полная type-erasure → vtable fat-pointer dispatch (Plan 72 P3-B
  territory).
- Если Ф.0 покажет, что generic record-`[]T` return-path — отдельное
  большое семейство, оно остаётся в Plan 54-lineage, не в Plan 88.
