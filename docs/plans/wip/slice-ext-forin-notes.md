# [M-slice-ext-receiver-for-in-elem-type] — рабочие заметки

Worktree: `d:/Sources/nv-lang/nova-sliceext`, ветка `p-fix-slice-ext-forin`, от `main`@2afcb3f3d.
Модель: sonnet.

## Баг

`fn []T @method(...) { ... for r in @ { r.field } ... }` — итерация ГОЛОГО `@` в
slice-расширении роняет тип элемента цикла в nova_int-fallback (codegen гадает C-тип
`r`), из-за чего match по `r.status` резолвит теги ЧУЖОЙ суммы.

Живой репро (uncommitted WIP в main-репо на момент находки интегратором, НЕ трогаю
main-репо): `examples/flagship/aggregator/src/domain/domain.nv` — миграция
`Report.from(results, wall_ms)` → `fn []TaskResult @to_report(wall_ms int) -> Report`
(nv-coding-style §1а W_STATIC_CONVERSION) с прямым `for r in @` — CC-FAIL; обход через
`ro results []TaskResult = @; for r in results` — работает.

## Корень (найдено)

`compiler-codegen/src/types/mod.rs`, `f1_check_fn` (~строка 6773, ДО фикса):

```rust
if let Some(recv) = &fd.receiver {
    if matches!(recv.kind, ReceiverKind::Instance) {
        scope.insert("@".to_string(), TypeRef::Named {
            path: vec![recv.type_name.clone()],
            generics: recv.generics.clone(),
            span: recv.span,
        });
    }
}
```

Для slice-расширения `fn []TaskResult @to_report` парсер (parser/mod.rs ~3006)
СИНТЕЗИРУЕТ `recv.type_name` как ФЛАТ-строку `"[]TaskResult"` (один identifier,
`"[]".repeat(depth) + elem_name`) — но при этом кладёт ПОЛНУЮ структурную форму
(`Array(Named("TaskResult"))`) в `recv.receiver_ty` (Plan 153.5 / D263, для монoморфизации
на любой глубине вложенности). `f1_check_fn` игнорировал `receiver_ty` и ВСЕГДА строил
`Named{path:["[]TaskResult"]}` — Named с ЛИТЕРАЛЬНЫМ путём-строкой, синтетическая
"slice-sugar spelling", не настоящий структурный `Array`.

`infer_iter_elem_type` (~10047-10078) для НЕ-Range/НЕ-ArrayLit iter вызывает
`infer_expr_type(iter, scope)` и матчит СТРУКТУРНО:
```rust
_ => match self.infer_expr_type(iter, scope)? {
    TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) => Some(*inner),
    TypeRef::Readonly(inner, _) => match *inner { Array(..)|FixedArray(..) => ..., _ => None },
    _ => None,
}
```
`infer_expr_type(SelfAccess, scope)` = `scope.get("@").cloned()` (D176, ~14671/~19989) —
возвращает `Named{path:["[]TaskResult"]}`, которое НЕ матчит `TypeRef::Array(..)` →
`None` → `elem_ty` не материализуется → codegen без канала гадает `nova_int` для `r`.

Отдельные консьюмеры scope["@"] УЖЕ умеют распознавать флат-"[]T"-Named (метод-резолв,
`resolve_return_channel` и др., ~15537/~15557/~16350/~16383 — все со спец-веткой
`path[0].starts_with("[]")` → нормализация к "Vec"), но `infer_iter_elem_type` — нет:
она смотрит на СТРУКТУРНУЮ форму TypeRef, а не строковую.

Родня: тот же класс "второе окно" (канал пуст → codegen гадает), что и
[M-vec-ext-method-untyped-let-breaks-chain-dispatch] (docs/plans/backlog-followups.md,
fixed 2026-07-17 в `f3_check_member_ctx` третьим гейтом `prefix_generic_slice_method`) —
там тоже флат-"[]T"-строка не была известна ОДНОМУ из консьюмеров, хотя другие её уже
понимали.

## Фикс

Использовать `recv.receiver_ty` (уже правильно построенную ПОЛНУЮ структурную форму —
`Array(Named(T))` для slice-receiver'а, depth-aware для `[][]T`), когда он есть, вместо
синтетического `Named{path:[type_name]}` — ТОТ ЖЕ идиом уже применяется в
`resolve_return_channel`'s `recv_pattern_tr` (~10343: "prefer the FULL structured form
(`receiver_ty`) ... else the flat `Named{type_name, generics}`") — не изобретение,
воспроизведение существующего канона.

```rust
let self_ty = recv.receiver_ty.clone().unwrap_or_else(|| TypeRef::Named {
    path: vec![recv.type_name.clone()],
    generics: recv.generics.clone(),
    span: recv.span,
});
scope.insert("@".to_string(), self_ty);
```

Для НЕ-slice / не-carrier ресиверов (`fn Type @m`) `receiver_ty` — `None` (парсер строит
его только для `[]T` и `Type[T]`-carrier веток) → fallback идентичен старому поведению,
без регрессии.

Побочный эффект (шире for-in): `SelfAccess`-канал (`f1_expr_inner`, ~7353) пишет
`ResolvedType::from_type_ref(scope["@"])` в `resolved_types_buf` БЕЗ гейта — раньше это
был `R::Named{name:"[]TaskResult"}` (мусор, не резолвится ни во что), теперь —
канонический `R::Named{name:"Vec", args:[TaskResult]}` (D239). Значит `@[i]`
(Index-проекция через `Constraint::Project`) на named-generic slice-ресивере тоже может
начать резолвиться корректно там, где раньше молчаливо не резолвилось (не проверено само
по себе как отдельный баг — эмпирически проверяю тем же заходом).

## Соседние формы — план проверки

`@len()` (НЕ `@.len()` — дот-форма `@.field`/`@.method()` синтаксически ЗАПРЕЩЕНА,
E_SELF_DOT_INVALID, `spec_tests/conformance/neg/neg_self_dot_invalid.nv`; задание,
видимо, имело в виду no-dot sugar) и `@[i]` — есть прецедент в `std/src/sort.nv`
(`@len()`, `@[0]`, `@[i]` в `min()`/`max()`) на КОНКРЕТНОМ `[]int`-ресивере, уже
работает (часть std-гейта). Для NAMED-generic-ресивера (наш кейс, `[]TaskResult`) —
проверяю фикстурой ниже.

## Файлы

- Фикс: `compiler-codegen/src/types/mod.rs` (`f1_check_fn`, инъекция scope["@"]).
- Фикстура (red→green): `spec_tests/conformance/slice_ext_receiver_for_in_elem_ok.nv`.
- Гейт-носитель: `examples/flagship/aggregator/src/domain/domain.nv` +
  call sites (`aggregate.nv`, `live.nv`, `report_json_test.nv`) — миграция
  `Report.from` → `[]TaskResult @to_report`, ПРЯМОЙ `for r in @` (без обхода).

## Статус

В процессе — см. TodoWrite сессии.
