<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Чекпоинт окна `p599-codegen-typing`

Статус: ОБЕ причины закрыты, коммиты сделаны, отчёт готов.

## Причина 2 (cron) — ЗАКРЫТА
- Не была закрыта фиксом p576 (проверено: `cron_test` всё ещё CC-FAIL на
  чистом слиянии p576 в main).
- Корень: `compiler-codegen/src/types/mod.rs`, `infer_expr_type`'s
  `ExprKind::RecordLit` arm (~L20001) — bare RECORD-shaped sum-variant
  construction (`InvalidValue { field, value }` в теле untyped closure
  `|_| ...`, аргумент `map_err`) резолвился как САМОСТОЯТЕЛЬНЫЙ тип по имени
  варианта, а не как owning sum type. Тот же класс, что №576.
- Фикс: добавлен variant→owner поиск (single-unambiguous-owner), зеркало
  того, что `f1_expr_inner` уже делает для этого же случая (~L9256-9281) и
  что сосед-арм для bare UNIT-variant у `Ident` уже делает (~L19981-19998).
- Фикстура: `spec_tests/conformance/standalone/map_err_bare_variant_owner_type.nv`

## Причина 3 (serde) — ЗАКРЫТА
- Корень: `compiler-codegen/src/codegen/emit_c.rs` — существующий фикс
  `[M-slice-static-deserialize-garbage-len]` (закрывал ДРУГОЙ случай) был
  гейтован на `!is_result_like(&inner_ty)` — пропускал случай, когда более
  общий диспетчер УЖЕ дал Result-ФОРМЫ ответ, но с НЕВЕРНЫМ элементом
  (`Vec[int]` вместо `Vec[str]`, canonical default-T stub). Плюс тот же
  пересчёт жил ТОЛЬКО в мутирующем `emit_expr`, а параллельный
  query-only `infer_expr_c_type`'s `ExprKind::Try` (нужен для типизации
  `Some(<try-expr>)`-обёртки) пересчитывал НЕЗАВИСИМО и ошибался так же.
- Фикс: вынесен общий `&self`-хелпер `slice_static_generic_call_ret_c`
  (через `const_fn_trampoline::subst_type_ref_pub`, без мутации
  `current_type_subst`), гейт убран (пересчёт безусловный для этой узкой
  формы), обе точки используют один хелпер.
- Фикстура: `spec_tests/conformance/standalone/slice_static_generic_turbofish_option_wrap.nv`

## Побочная находка (НЕ в этом окне)
После фикса причины 3 `decode_errors_test`'s CC-FAIL ушёл, но
`nova test std/src/encoding/serde` вскрыл RUN-FAIL в `sum_autoderive_test.nv`
(та же CU/модуль) — 4 теста, ранее НЕВИДИМЫЕ за тем же CC-FAIL (модуль
никогда не собирался). Это рантайм-баг сериализации sum-варианта с record-/
Option-/Vec-payload, класс не тот (не типизация кодогена) — записан в
`scripts/guards/std-test-fail.baseline` с №TBD, не чинился.

## Коммиты
1. fix(types/mod.rs) — причина 2 + фикстура.
2. fix(emit_c.rs) — причина 3 + фикстура.
3. gate(baseline) — снятие cron_test, обновление decode_errors_test.

## Верификация
- `nova test std/src/time` — PASS:5 FAIL:1 (только №601, известный).
- `nova test std/src/encoding/serde` — RUN-FAIL (новая находка, задокументирована).
- `nova check std/src` — ДО и ПОСЛЕ байт-в-байт одинаковы: PASS:154 FAIL:26 WARN:62.
- Саботаж: откат причины 2 → фикстура 1 красная (та же СИ-ошибка класса);
  откат причины 3 → фикстура 2 красная (та же СИ-ошибка класса); оба фикса
  вместе → обе фикстуры зелёные.
