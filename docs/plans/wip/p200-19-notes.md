<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 200 Пункт 19 — рабочий чекпоинт

Worktree: `nova-p19` (ветка `p200-19-fixarr`). Модель: sonnet.

## Ш0-вердикт (первым делом, определяет дизайн)

Проба `fn [4]u8 @probe() -> int => 4` + `a.probe()` — **НЕ парсится**:
```
error: expected identifier, got int literal
fn [4]u8 @probe() -> int => 4
    ^ (на "4" внутри "[4]")
```
Путь = **КОМПИЛЯТОР-СИНТЕЗ** (как ожидалось картой) — const-generic `N` в
языке нет, `[N]T`-ресивер вообще не входит в грамматику `fn`. Файл пробы
был временным, удалён после проверки (не фикстура).

## Механизм — "одно окно" с @index (D238-семьёй)

Найдено структурное распознавание FixedArray, которое `arr[i]` уже
использует (checker `ExprKind::Index` арм, `compiler-codegen/src/types/mod.rs`
~8991: `TypeRef::FixedArray(_, inner, _) => Some(inner.as_ref())`; codegen
`compiler-codegen/src/codegen/emit_c.rs` ~32415: `Self::parse_mono_fixed_array_name(bare)`
+ `.data`/`->data`). `@len()`/`@ptr()` добавлены ТЕМ ЖЕ структурным способом
(не через `method_table`/name-keyed ветку сбоку):

**Checker (`compiler-codegen/src/types/mod.rs`):**
- `peel_fixed_array(ty) -> Option<(usize, &TypeRef)>` — новый маленький
  helper (peel `ro`/`mut` → `FixedArray(N, inner)`).
- `fixed_array_accessor_return(n, inner, method, is_mut, span) -> Option<TypeRef>`
  — ОДИН источник синтеза (`"len"` → `int`, `"ptr"` → `*T`/`*mut T`).
- Вызывается из ДВУХ существующих продюсеров (оба уже были "одно окно" для
  всех прочих instance-call форм):
  1. `infer_expr_type`'s `ExprKind::Call` арм (~15103) — inline-инференс
     (нужен для `E_POINTER_RO_ASSIGN`, чтобы `unsafe { p.write(...) }` на
     mut-ресивере типизировался как `*mut T`).
  2. `infer_method_call_channel_type` (~16648) — codegen-канал
     (`resolved_types_buf`, читает `infer_expr_c_type`).
- `is_mut` — `!self.is_through_ro_binding(obj)` (тот же predicate, что уже
  использует D175/D326 checks — НЕ новый mutability-tracking).

**Codegen (`compiler-codegen/src/codegen/emit_c.rs`):**
- В `emit_call`'s `ExprKind::Member { obj, name: method }` арм (~34393),
  ПЕРВЫМ действием: если `method ∈ {"len","ptr"}` и `args.is_empty()` —
  `self.infer_expr_c_type(obj)` → `Self::parse_mono_fixed_array_name(bare)`
  (ТА ЖЕ функция, что и Index-арм) → синтез C напрямую
  (`((nova_int)N LL)` для len; `((const T*)(data))` / `((T*)(data))` для
  ptr, `const` по `self.is_place_mutable(obj)` — тот же predicate, что уже
  используют Vec-overload'ы Plan 135/138.4). Bypass'ит name-keyed
  `method_receivers`-диспетч ниже целиком (который иначе попытался бы
  вызвать Vec-тело `@len`/`@ptr` на не-Vec-структуре — CC-FAIL).
- Любое ДРУГОЕ имя метода на FixedArray-ресивере падает НЕТРОНУТЫМ в
  существующий (pre-existing, вне объёма) диспетч — zero interference.

## Собрано (первый билд после чекера+кодгена)

`cargo build --release --manifest-path nova-cli/Cargo.toml` — **OK** (exit 0).

## Статус на чекпоинте

- [x] Ш0-проба
- [x] Checker synthesis (2 producer hooks)
- [x] Codegen synthesis (emit_call Member arm)
- [x] Rebuild green
- [ ] D431 spec-блок (в процессе — amendment-заметка в D27 уже вставлена,
      полный блок "## D431." ещё не вставлен)
- [ ] Фикстуры pos/neg
- [ ] Верификация (targeted nova test + conformance-фикстуры + байт-паритет)

## Следующие шаги

1. Вставить `## D431.` decision-блок в `spec/decisions/03-syntax.md` перед
   `## D30` границей (номер D431 подтверждён свободным — highest existing
   `D430`, README индекс отдельной нумерованной таблицы не держит).
2. Фикстуры: pos (`arr.len()`, `RawMem.copy(arr.ptr(),...)` round-trip,
   mut-перегрузка запись), neg (bare `arr.len` без скобок — D117-класс,
   ожидание: обычная диагностика, НЕ наша новая форма).
3. Верификация по брифу: nova test std/collections/vec 1/0, string_builder_test
   1/0, checksums 3/0, спот-байт-паритет на 2 нетронутых фикстурах.
