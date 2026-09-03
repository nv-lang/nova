# Охота emit_c x К4 — чекпоинт (финальный)

## Находки (расхождение доказано командой)

Класс А — «это float-арифметика?» считается в двух местах по разным правилам:
- **p1_compound_mixed_float** — `b += 2` при `b:f64`: оракул `3.5`, novac `3`.
- **p20_compound_mul_named_int** — `b *= k` (k int): оракул `3`, novac `2`.

Класс Б — «даёт ли этот хвостовой оператор ЗНАЧЕНИЕ функции?» решается позицией,
а не объявленным возвратом:
- **p2_tail_call_in_void_fn** — ICE «callee_of on a node with nothing recorded».
- **p4_tail_userfn_in_void_fn** — clang: returning 'nova_int' from 'nova_unit'.
- **p22_tail_if_stmt_in_void_fn** — ICE «no type recorded for this node».
- контроль **p5_tail_call_not_last_control** — зелёный (снят ОДИН рычаг).

Класс В — «эту конструкцию строит хойст» решают две двери, и вторая не знает
позиций первой:
- **p12_assign_interp_str** (присваивание), **p13_return_interp_str** (return),
  **p14_if_cond_interp_str** (условие if), **p18_match_arm_interp** (тело руки
  match, файл emit_match.nv) — ICE «an interpolated string the hoist never built».
- **p15_assign_coalesce** — ВТОРАЯ конструкция того же свойства: ICE «a `??` the
  hoist never built».

## Побочное (НЕ emit_c)

- **p8_dup_variant_name_across_sums** — novac check ложно отказывает, оракул
  печатает 1/2.

## Ходил, расхождения нет

p3 (int += float), p5 (контроль), p6 (свой println), p7 (метод у newtype — check
отказывает), p9/p10/p11 (позиционные ограничения check), p16/p17 (метод на
value-записи — check отказывает), p19 (кортеж в поле — check отказывает),
p21 (скобочная база доступа к полю — тип записан, зелёный).
