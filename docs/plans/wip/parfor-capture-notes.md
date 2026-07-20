# [M-parfor-capture-callee-name-collides-std-local] — заметки волны

## Корень
`compiler-codegen/src/codegen/emit_c.rs`, `emit_spawn` (~строка 11542-11567):
capture-цикл `for name in refs { ... if let Some(ty) = self.var_types.get(&name) ... }`.
`refs` собирается `collect_idents_expr` (строка ~13927), который для `Call{func,..}`
кладёт имя callee-Ident в `refs` НАРАВНЕ с обычными переменными — без различия
call-position vs value-position.
`self.var_types: HashMap<String,String>` (поле ~650) — ПЛОСКАЯ, НЕ per-функция:
`emit_fn_scoped_inner` (~24241-24919) вставляет params/`nova_self` в var_types,
но НИКОГДА не восстанавливает/не чистит их в конце функции (в отличие от
`var_mutable`, которое restore'ится на 24888). Поэтому локал `probe` из одной
функции CU (напр. std float-format engine, `uint64_t probe`) оставляет
паразитную запись, которую подхватывает capture-скан СОВЕРШЕННО другой функции,
вызывающей module-fn `probe()`.

## Фикс
Добавлены (рядом с `collect_idents_expr`/`_block`/`_stmt`, ~14101):
- `collect_resolved_call_target_names_expr/_block/_stmt(&self, ...)` — зеркало
  `collect_idents_expr` (та же полная рекурсия по всем позициям), но
  ЦЕЛЕНАПРАВЛЕННО собирает ТОЛЬКО имена callee, для которых `Call`-узел (по
  `expr.id`) есть в `self.resolved_callees` (канал U.3.4 checker'а —
  `types/mod.rs` `f1_check_call`, populate'ится ТОЛЬКО для однозначно
  резолвнутых free-fn/method вызовов, НИКОГДА для вызова локальной
  closure-переменной).
- В `emit_spawn`: после сбора `refs`/`bound` — `resolved_fn_call_names` через
  новый walker; в capture-цикле `if resolved_fn_call_names.contains(&name) { continue; }`
  ПЕРЕД `var_types.get`. Резолв-гейт, не блэклист по имени: захваченная closure-
  переменная, вызванная как `f(x)`, НЕ попадает в `resolved_fn_call_names`
  (checker не резолвит динамический вызов в `resolved_callees`), поэтому
  капчур closures не сломан.

## Верификация
- `self.resolved_callees` реально прокинут в CEmitter: `main.rs:554`
  `emitter.set_resolved_callees(&module_env.resolved_callees)`.
- `Call` — поле `Expr.id: ExprId` (НЕ внутри `ExprKind::Call`), ключ в
  `resolved_callees` = `e.id` самого Call-узла (`types/mod.rs:7966`
  `self.f1_check_call(func, args, trailing.is_some(), gs, scope, errors, e.id)`).
