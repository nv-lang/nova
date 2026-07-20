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

## Дополнительная находка — E_UNKNOWN_METHOD гейт для FixedArray (не было в
исходной карте, добавлено по факту разведки)

Проба показала: bare `arr.len` (без скобок) НЕ даёт чистую диагностику —
`nova check` пропускает молча (permissive gap до codegen), `nova test`
падает CC-FAIL («no member named 'len' in struct») — это D117-путь
НЕ применим (не чистая диагностика). Проба с опечаткой `arr.lenx()`
оказалась ХУЖЕ — `nova test` падал внутренним ICE `[P67-LEGACY] method
call return type unknown` (panic, "This is a bug in nova"), ОБА пути
pre-existing (не regression — `check_instance_overload`'s E_UNKNOWN_METHOD
гейт исторически скипался для Array/FixedArray-ресиверов целиком, т.к.
"Vec" не входит в `is_primitive_recv_name`). Решение (в scope этой волны,
т.к. Пункт 19 — единственный, кто вводит РЕАЛЬНУЮ FixedArray-метод-
поверхность): добавлен FixedArray-специфичный гейт в `check_instance_overload`
(types/mod.rs, перед primitive-гейтом) — любой метод НЕ `len`/`ptr` на
`TypeRef::FixedArray`-ресивере → чистый `[E_UNKNOWN_METHOD]`. `Array`/`[]T`
(реально `Vec[T]`) НЕ затронут — гейт матчит буквально `FixedArray`, не
peeled-"Vec"-имя. Неg-фикстура использует typo-ветку (`arr.lenx()`),
подтверждено: было ICE → стало чистый compile error.

## Статус на чекпоинте — ГОТОВО

- [x] Ш0-проба
- [x] Checker synthesis (2 producer hooks + E_UNKNOWN_METHOD гейт)
- [x] Codegen synthesis (emit_call Member arm)
- [x] Rebuild green (checker+codegen, затем повторно после E_UNKNOWN_METHOD гейта)
- [x] D431 spec-блок — полный decision-блок в `spec/decisions/03-syntax.md`
      (amendment-заметка в D27 + отдельный `## D431.` перед `## D30`)
- [x] Фикстуры: pos `spec_tests/conformance/d431_fixarr_len_ptr.nv` (3 test-блока:
      len на 3 разных N, RawMem.copy round-trip, mut-перегрузка запись);
      neg `spec_tests/conformance/neg/d431_fixarr_unknown_method_neg.nv`
      (typo → E_UNKNOWN_METHOD, EXPECT_COMPILE_ERROR)
- [x] Верификация — 5 прогонов, все зелёные (см. финальный отчёт)
- [x] Байт-паритет — 2 нетронутых фикстуры, SHA-256 идентичны против
      базовой ветки `f0eba7b5f` (throwaway reference worktree, удалён)

Миграция трёх мотив-сайтов (pad fill_bytes/display-fallback) — НЕ в этой
волне (текст Пункта 19, отдельный шаг).
