# [M-slice-ext-receiver-for-in-elem-type] — рабочие заметки

Worktree: `d:/Sources/nv-lang/nova-sliceext`.
Модель: sonnet.

## ВОЛНА 2 (P2, ветка `p-fix-slice-ext-forin2` от свежей main, после мержа P1)

P1 (чекер, коммит 71938307a, влит в main мерж-коммитом e0d03c6f9) закрыл ПОЛОВИНУ —
`nova check` зелёный, но ЭМИССИЯ (codegen) на слитом main осталась CC-FAIL: та же
фикстура `slice_ext_receiver_for_in_elem_ok.nv` — `Nova_NovaArray_nova_int_method_sef_tally(Nova_Vec____nova_int* nova_self)`
— ВЕСЬ ресивер (`nova_self`) mono'т как `Vec[int]`, а не `Vec[SliceExtForinLane]`.
Причина: `for r in @` ЛОУЭРИТСЯ через `var_types["nova_self"]` (не через чекер-канал
P1 починил) — корень в codegen, `compiler-codegen/src/codegen/emit_c.rs`,
`receiver_c_type()` (~16853), ветка `[]<elem>` (~16935): для КОНКРЕТНОГО НЕ-примитивного
elem-типа (`elem_ty` не в списке str/bool/f64/f32/u8/char/i32/…/uint) код (Plan 101.1,
~16953) БЕЗУСЛОВНО пробовал `self.subst_c(elem_ty)` (лукап в
generic-substitution-карте `current_type_subst` — предназначен для РЕАЛЬНЫХ typevar'ов
`fn[T] []T @m`), и при промахе ВСЕГДА дефолтил на `"nova_int" // erased T fallback` —
не различая «действительно нерезолвнутый generic typevar» (T/U/K, короткое
all-uppercase имя — см. параллельную проверку чуть выше по файлу, ~16902, для
non-array receiver'а) от «конкретный именованный record/sum, которого просто НЕТ в
current_type_subst, потому что он и не должен там быть» (`SliceExtForinLane`/
`TaskResult`). Нигде в корпусе (std/examples/nova_tests) до этой правки НЕ было
slice-расширения с конкретным ИМЕНОВАННЫМ (не примитив, не generic) элементом —
ветка была «недостижима, но неправильна» до `[]TaskResult @to_report`.

Фикс (та же функция, тот же match arm): добавлена ветка МЕЖДУ subst_c-успехом и
nova_int-эрейзом — если `elem_ty` НЕ похож на typevar (не ≤2-символьное
all-uppercase имя, тот же гейт что уже использует non-array-ветка чуть выше), резолвим
его через ЕДИНЫЙ канонический лоуэринг `resolved_type_to_c(&ResolvedType::Named{...})`
— ТОТ ЖЕ путь, которым `resolved_array_to_c` (~3744, РАБОЧИЙ путь для `[]TaskResult`
как обычного поля/параметра) уже строит elem-арг для Vec-mono — так что смангленное
имя инстанса (`Nova_Vec____Nova_SliceExtForinLane_p`) БАЙТ-В-БАЙТ совпадает с тем, что
уже зарегистрировал call-site (`lanes []SliceExtForinLane`), а не расходится во
ВТОРОЙ, отдельный mangle. Никакого name-guessing НЕ добавлено — переиспользован
существующий канал (`resolved_type_to_c`), просто РАСШИРЕН гейт «когда его вызывать».

Изолированный прогон (`spec_tests/_iso_slice_ext_forin/`, копия фикстуры в отдельном
module) red→green ПОДТВЕРЖДЁН эмпирически на пересобранном бинаре (обе волны):
- main ДО codegen-фикса (P1-фикс уже внутри): CC-FAIL, `member reference base type
  'nova_int'`, сигнатура `Nova_NovaArray_nova_int_method_sef_tally(Nova_Vec____nova_int* nova_self)`.
- main + codegen-фикс: `PASS: 1  FAIL: 0`.

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

- Фикс P1 (чекер): `compiler-codegen/src/types/mod.rs` (`f1_check_fn`, инъекция
  scope["@"]) — коммит `71938307a` (влит в main мержем `e0d03c6f9`).
- Фикс P2 (codegen): `compiler-codegen/src/codegen/emit_c.rs` (`receiver_c_type`,
  ветка `[]<elem>`, Plan 101.1 arm) — коммит `cd114d0d5` (ветка
  `p-fix-slice-ext-forin2`).
- Фикстура (red→green): `spec_tests/conformance/slice_ext_receiver_for_in_elem_ok.nv`
  (влита в main вместе с P1; изолированная копия-прогон подтвердила P1-only —
  всё ещё CC-FAIL, P1+P2 — PASS).
- Гейт-носитель: `examples/flagship/aggregator/src/domain/domain.nv` +
  call sites (`aggregate.nv`, `live.nv`, `report_json_test.nv`) — миграция
  `Report.from` → `[]TaskResult @to_report`, ПРЯМОЙ `for r in @` (без обхода) —
  уже в main (влито вместе с P1).

## Приёмка (P2, бинарь `p-fix-slice-ext-forin2`@cd114d0d5, оба фикса)

- Изолированная копия фикстуры (`spec_tests/_iso_slice_ext_forin/`, отдельный
  module, не коммичена): `PASS: 1 FAIL: 0` (было `CC-FAIL` на P1-only).
- `nova build examples/flagship/aggregator/src/main.nv --strict-effects`:
  `built: main.exe` (только pre-existing warnings — unused imports, W_PARAM_TYPE_POS_MUT).
- `nova test examples/flagship/aggregator`: `PASS: 7 FAIL: 1 SKIP: 1`. Единственный
  FAIL — `src/app/aggregate` (`report.done == 2`, `wall_ms < sequential_ms`) —
  ИЗВЕСТНЫЙ таймингово-чувствительный concurrency-тест (реальный `supervised(deadline:)`
  fan-out), предупреждён заданием как "не твой", не относится к slice-ext.
- `nova test spec_tests/conformance/standalone --jobs 4`: `PASS: 68 FAIL: 0`
  (эквивалент "standalone-CU 69/0" из задания — счёт по документированному
  прецеденту плавает 68↔69 в зависимости от состава ветки, 0 failures — критерий
  выполнен).
- `nova test std/src/collections` (vec_seq `@map`/`@filter` slice-расширения):
  `PASS: 13 FAIL: 0 SKIP: 6` — без регрессии.
- Полный мега-CU (`spec_tests/conformance`, 988 файлов) НЕ гонялся повторно на
  P2-бинаре по указанию координатора (авторитет — CI); один прогон на P1-only
  бинаре (до кодогена-фикса) дал `PASS: 123 FAIL: 1` — единственный FAIL
  (`app_effect_basic_t8_1`, файл БЕЗ единого `@`-ресивера/slice-extension) —
  похоже на pre-existing/несвязанный шум, НЕ доисследован дальше (не гонять
  мега-CU повторно — прямое указание координатора); флаг для CI.

## Статус

P1+P2 закрыты (коммиты влиты в ветку `p-fix-slice-ext-forin2`, main НЕ
затронут — по дисциплине задания). Не язык-меняющее — D-амендмент не требуется.
