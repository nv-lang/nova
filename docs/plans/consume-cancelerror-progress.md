# [M-consume-block-cancelerror-bare-cu] — прогресс (worktree `nova-ccancel`, ветка `fix-consume-cancelerror-bare`)

**Статус:** РЕШЕНО (codegen-фикс, Path B). Не запушено — ждёт гейта оркестратора.

## Репро (ДО фикса)

Минимальный bare-CU (`nova build`, БЕЗ `with Net`), партиал-прелюдия
`#prelude(core, runtime)` (core даёт `ScopeOutcome`, но НЕ `errors` →
НЕ даёт `CancelError`) + любой `consume @cleanup(outcome ScopeOutcome)`
(и escape-, и statement-форма D188 — ОБЕ репродуцируют, вопреки
первоначальной гипотезе про "только escape"):

```nova
#prelude(core, runtime)
module reprocc

type Res consume value { id int }
fn Res consume @cleanup(_outcome ScopeOutcome) -> () { }
fn Res consume @dispose() -> () { }
fn make(v int) -> Res => { id: v }
export fn Res @share() -> Res => { @id }

fn main() -> () {
    consume s = make(5)
    consume out = consume s { s.share() }   // escape-форма (D188-амендмент)
    print(out.id)
    out.dispose()
}
```

CC-FAIL:
```
error: use of undeclared identifier 'Nova_CancelError'
    Nova_CancelError* _nv_tmp_4 = (Nova_CancelError*)nova_alloc(sizeof(Nova_CancelError));
```

## Корень (проверено, НЕ совпадает с изначальной гипотезой заявки)

- НЕ tree-shaking по usage-reachability модуля (я проверил: обычный
  `#no_prelude`-независимый bare CU с ПОЛНОЙ default prelude — typedef
  эмитится нормально, даже когда `CancelError` нигде текстуально не
  упомянут пользователем).
- Настоящий root cause: **Plan 62.F splittable-prelude** —
  `ScopeOutcome` (нужен для сигнатуры `@cleanup`) объявлен в
  `std/prelude/core.nv`, а `CancelError` — в ОТДЕЛЬНОМ
  `std/prelude/errors.nv`. `#prelude(core, ...)` без `errors` в списке
  делает `ScopeOutcome` доступным, но НЕ мёржит `.nv`-декларацию
  `CancelError` вообще (не «tree-shaken после мёржа», а «никогда не
  распаршена/не смёржена»). `assign_scope_outcome_from_frame`
  (`emit_c.rs:25758` в исходном main, безусловно эмитит
  `Nova_CancelError` на КАЖДОМ FAIL/INTERRUPT run-site consume-cleanup
  (`emit_consume_entry_cleanup`, все 4 exit-path: FAIL/LEAVE/EARLY/
  INTERRUPT) — не только когда юзер пишет `err is CancelError`.
- **Уточнение к исходной заявке:** escape- И statement-форма (d188)
  ОБЕ репродуцируют бажный CC-FAIL под partial-prelude — различие
  escape/statement НЕ структурный гейт (обе формы одинаково зовут
  `emit_consume_entry_cleanup`/`assign_scope_outcome_from_frame`).
  Изначальный текст заявки, вероятно, наблюдал разницу из-за того, что
  сравнивались ДВЕ разных переменных сразу (partial-prelude vs
  effect-context), а не escape vs statement как таковой.

## Path A (испробован, ОТКЛОНЁН эмпирически)

Форс-инжект `import std.prelude.errors` в `compute_prelude_imports`
(`imports.rs`) когда модуль объявляет `consume @cleanup` — **работает
для CancelError typedef**, но тянет ВЕСЬ `errors.nv`, включая
`MultiError.@find_first_panic()`, который использует `str.starts_with`
— метод, объявленный в `std/runtime/string/search.nv`, НЕ мёржащийся
под `#prelude(core, runtime, errors)` (только default-facade
`prelude.nv`'s `import std.runtime.string.{...}` его подтягивает).
Это **отдельный, ПРЕДСУЩЕСТВУЮЩИЙ баг** — воспроизведён БЕЗ
consume/cleanup вообще, только `#prelude(core, runtime, errors)` +
`RuntimeError.DivByZero` (см. `errsonly.nv` в отчёте). `errors.nv`
заявляет «ZERO imports / self-contained на primitives», но фактически
не самодостаточен. Задокументирован новым маркером
`[M-prelude-errors-startswith-not-selfcontained]` в
`backlog-followups.md` — ВНЕ периметра этой волны.

## Path B (ВЫБРАН, реализован)

`CancelError` добавлен в `RUNTIME_DEFINED_TYPES`
(`compiler-codegen/src/codegen/emit_c.rs:27-56`, список уже содержит
`Error`/`RuntimeError`/etc с тем же паттерном) + hand-written C struct
в `compiler-codegen/nova_rt/array.h` (рядом с `Nova_Error`):

```c
typedef struct Nova_CancelError {
    nova_str reason;
} Nova_CancelError;
```

`emit_type_decl` (emit_c.rs:14360) уже безусловно skip'ает
emission для имён из `RUNTIME_DEFINED_TYPES` — так что когда `.nv`
декларация `CancelError` ТОЖЕ смёржена (default full prelude), codegen
не дублирует struct (no redefinition) — типаж `Error`/`RuntimeError`.
`err is CancelError` narrowing/`NOVA_TID_USER_CancelError` type-id
регистрация — НЕ зависят от факта мёржа `.nv`-декларации (уже были
безусловны ДО фикса, проверено — присутствовали в C даже в баговом
репро); фикс их не трогает.

Обновлён вводящий в заблуждение комментарий в
`assign_scope_outcome_from_frame` (было: «force-emitted prelude-тип,
всегда доступен» — было ЛОЖЬЮ до этого фикса; теперь — правда, via
RUNTIME_DEFINED_TYPES).

## Гейт (ПОСЛЕ фикса)

1. Минимальный repro (escape-форма, `#prelude(core, runtime)`) —
   компилится + линкуется + рантайм-прогон корректен (`5`, exit 0).
2. Statement-форма (d188, `#prelude(core, runtime)`) — аналогично
   зелено (`5`, exit 0) — подтверждает уточнение выше.
3. Full default-prelude сценарий с явным `err is CancelError`
   narrowing внутри `@cleanup` — компилится БЕЗ redefinition, `.c`
   содержит РОВНО одно (header-only) определение struct, narrowing
   (`*(Nova_CancelError**)nova_any_data(err)`) корректно эмитится.
4. `nova test std/src/net/tcp_share_test.nv` — PASS (эффект-контекст
   не регрессировал).
5. `nova test std/src/net` (весь net-модуль) — PASS.
6. spec_tests/conformance НЕ гонялся (мега-CU, запрещено CPU-
   дисциплиной этой волны) — точечная замена: собственные repro выше +
   tcp_share_test покрывают escape/statement/narrowing/effect-context.

## Незакрытые смежные находки (НЕ фикшены, вне периметра)

- `[M-prelude-errors-startswith-not-selfcontained]` (НОВЫЙ маркер,
  добавлен в backlog-followups.md) — `errors.nv` не самодостаточен под
  `#prelude(errors)`/`#prelude(core, ..., errors)`: `MultiError.
  @find_first_panic()` использует `.starts_with`, не мёржащийся без
  полной default-facade. Найден КАК ПОБОЧНЫЙ ЭФФЕКТ разведки Path A
  (отклонённого); не путать с текущим фиксом (Path B его не касается
  и им не затронут).
