# P0-фикс регрессии d45_inferred_return_type (2026-07-17)

## Регрессия
Коммит `ba9a8a2f3` (продюсер Q1 «bare free-fn declared return», уже в main
`726e734af`) сломал `spec_tests/conformance/d45_inferred_return_type.nv`:
CODEGEN-FAIL `E_NO_MATCHING_OVERLOAD` на `assert(d45_is_positive(1))`
(строка 66). Красило весь mega-CU `spec_tests/conformance` на CI.

## Корень
`compiler-codegen/src/types/mod.rs`, `f1_expr`, продюсер под гейтом
`if let [callee] = arity_matches.as_slice()` (bare free-fn call, ровно один
arity-matching non-generic overload). Ветка на отсутствие `-> T`:

```rust
None => Some(ResolvedType::Unit),   // БАГ
```

конфлировала «нет аннотации» с «возвращает Unit». Для expression-body
(`=> expr`, D45) возврат **инферится из тела** — в AST `return_type == None`,
но реальный тип НЕ Unit (например `x > 0` → bool). Продюсер канализировал
ЛОЖЬ `Unit` в `resolved_types_buf`, что рвало резолв `assert(bool)` на
overload-подборе (E_NO_MATCHING_OVERLOAD), а для `d45_double`/`d45_negate`/
`d45_greet` (int/int/str) тихо подменяло реальный тип на Unit ниже по
каналу (не проявлялось как ошибка компиляции, но было бы тихим багом —
см. верификацию ниже).

## Верификация семантики (ДО фикса кода)
Спека `spec/decisions/03-syntax.md` D45 (строки 2511-2585):

> В **expression-body** (`=> expr`) тип возврата `-> T` **опционален** —
> выводится из тела. В **block-body** (`{ ... }`) `-> T` обязателен,
> если тип не unit.

И явно про реализацию (Plan 55 Ф.3):

> Bootstrap-codegen (`emit_c.rs::return_type_c`) реализует **только**
> Expr-body inference (`FnBody::Expr`) — Block-body без аннотации →
> `nova_unit` (как раньше; см. «Что отвергнуто» выше).

Т.е. `None => Unit` для **block/external**-тела — гарантированное,
задокументированное поведение (inference в block-body осознанно отвергнут
дизайном — «теряется явный контракт»). Баг — ТОЛЬКО в ветке `FnBody::Expr`.

## Фикс
```rust
None => match &callee.body {
    FnBody::Expr(_) => None,   // D45: реальный возврат инферится из
                                // выражения — этот продюсер не умеет
                                // это инферить, должен промолчать.
    _ => Some(ResolvedType::Unit),   // block/external — nova_unit
                                       // гарантирован спекой.
},
```
`FnBody` уже был в скоупе файла (используется в десятке других мест
`types/mod.rs`), доп. импорт не понадобился.

Не язык-меняющее (фикс продюсера под существующий D45) — амендмент в
спеку не требуется.

## Репро ДО фикса (дословно)
```
CODEGEN-FAIL   .../spec_tests/conformance/d45_inferred_return_type  # d45_inferred_return_type.nv:66:5: error: [E_NO_MATCHING_OVERLOAD] no overload of `assert` matches the given argument types
66 |     assert(d45_is_positive(1))
  |     ^^^^^^
... (аналогично строки 67, 68)
PASS: 0  FAIL: 1
```

## Вердикт ПОСЛЕ фикса (5 пунктов таргет-приёмки)

1. **Целевой файл восстановлен**: CODEGEN-FAIL/E_NO_MATCHING_OVERLOAD
   исчез. Файл теперь компилируется (RUN-FAIL остался, но по ДРУГОЙ,
   заранее известной причине — см. п.2).
2. **RUN-FAIL — только известный паттерн**: сырой запуск сохранённого
   test-exe (`--keep-artifacts`, т.к. CLI-сводка `detail` берёт только
   последние 4 FAIL-строки через `.rev().take(4)` — не все) даёт
   **2737 PASS / 5 FAIL**. Все 5 FAIL — `Vec[f32]`/`Vec[int]` chained
   `.debug`/`.display` `into_str()`-ассерты (`vec_f32_chained_debug.nv`,
   известный `[M-154.1-chained-vec-f32-method-misdispatch]`, в задании
   упомянут как `[M-208-vec-chained-debug]`). File:line в детали
   ошибочно указывают на `d45_inferred_return_type.nv` — косметическая
   особенность атрибуции диагностики в merged-CU (folder=module тянет
   `vec_f32_chained_debug.nv` в тот же CU); НЕ предмет этого фикса.
3. **Тихий-баг-угол закрыт**: все 9 `(D45)`-тестов в файле —
   `PASS`, включая `d45_double`/`d45_negate`/`d45_greet`/
   `d45_is_positive` (int/int/str/bool больше не подменяются Unit).
4. **Соседи-продюсеры**: `test --positive --compile-error
   spec_tests/conformance/standalone --timeout 300 --jobs 4` →
   `PASS: 69  FAIL: 0`. Полностью зелено на Windows (никаких
   supervisor-гонок не проявилось — они Linux-only по знанию).
5. **Смоук флагман-агрегатора**: `build
   examples/flagship/aggregator/src/main.nv --strict-effects` →
   успешно (только pre-existing warnings: unused-import,
   W_PARAM_TYPE_POS_MUT, W_DEP_PATH_NO_RELEASE — не связаны с фиксом).

## Модель
sonnet (исполнение по готовой карте интегратора).
