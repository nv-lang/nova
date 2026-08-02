# [M-d55-str-literal-coercion-name-gated] — рабочие заметки

Ветка: `p-fix-d55-type-directed`, worktree `d:/Sources/nv-lang/nova-d55coerce` (база main 764d0319d).
Задача: backlog-followups.md строка ~97, P2. Модель sonnet, по карте.

## Суть

D55-амендмент «str-литерал → `[]u8`» специфицирован ТИПО-направленно (любая
`[]u8`-позиция), но был реализован ИМЯ-направленно (`emit_c.rs::
synthesize_write_str_lit_bytes_coercion`, гейт на метод буквально `write` +
`all_methods`-safety-gate). Спека уже правильная → D-амендмент не нужен;
чинится реализация.

## Выбранное окно (по коду, не гадание)

Два раздельных, но взаимодополняющих окна (оба нужны для полной коррекции):

1. **Чекер, ACCEPT-side** — `compiler-codegen/src/types/mod.rs::assignable_direct`,
   арм `ExprKind::StrLit(_)` (~14073). БЫЛО: принимает ТОЛЬКО `exp_rt ==
   ResolvedType::Str`. СТАЛО: доп. принимает `is_bytes_slice_rt(&exp_rt)` —
   новый module-level helper (~17243, рядом с `array_elem_type`), структурно
   проверяющий `ResolvedType::Array(Scalar{width:8,signed:false})` (канонич.
   форма `[]u8`/`Vec[u8]` после `resolved_cat_of`, D239). `assignable_direct`
   — ЕДИНЫЙ choke-point, через который проходят call-arg (`overload_
   applicability`/`f1_check_call`/instance-method narrowing loop) И let/const-
   аннотация (`f1_check_assign_let`) И array-element (ArrayLit-арм в
   `assignable_direct` сам рекурсирует `assignable` per-элемент) — ПОЭТОМУ
   один фикс здесь закрывает ACCEPT для позиций 1, 2, 4 разом. `InterpolatedStr`
   намеренно ОТДЕЛЕНА от `StrLit` в отдельный арм — НЕ получает коэрсию
   (только литерал, не произвольное str-выражение).

   Важно: это ЗАКРЫВАЕТ асимметрию, задокументированную в spec (02-types.md
   «Статус реализации» до этого фикса) — protocol-типизированный приёмник
   (`w Fmt`) раньше тихо проходил (permissive `overload_applicability`
   skip), а КОНКРЕТНЫЙ приёмник (`sb StringBuilder`) падал `[E_NO_MATCHING_
   OVERLOAD]`. Теперь оба принимают одинаково — тип решает, не форма
   приёмника.

   Return-позиция ЧЕРЕЗ `assignable` НЕ проверяется вообще (ни до, ни после
   этого фикса — задокументированный, не связанный с этой волной, пробел:
   «No `assignable` runs here (return-type compat is checked elsewhere)»,
   комментарий в f1 около строки 6843). Значит нет риска регресса для
   return — там раньше не было diagnostics и сейчас нет; ЗАТО codegen (окно
   2 ниже) теперь эту позицию корректно КОМПИЛИРУЕТ.

2. **Codegen, resolved-C-type-string choke point** —
   `compiler-codegen/src/codegen/emit_c.rs`:
   - `emit_expr_with_target_type` (~28408) — добавлен арм в начале функции:
     bare `ExprKind::StrLit` + `target_ty_c` резолвится в `[]u8`
     (`is_bytes_slice_c_ty` helper — сравнение с канонич. `Nova_Vec____
     nova_byte*`, const-qualifier-agnostic) → rewrite-and-recurse в
     `<lit>.bytes()` (переиспользует существующий рабочий `.bytes()`-путь).
     Эта функция УЖЕ является тем самым «одно окно» для let (`ty_c`, вызов
     на ~26958/44620), return (`ret_ty`, ~27819), assign (`lhs_ty`, ~27720),
     array-literal-element (`elem_c`, ~44443 внутри `try_emit_typed_vec_
     literal`-подобной ф-ции) — закрывает позиции 2, 3, 4 ОДНИМ edit'ом (сама
     функция уже была общим choke-point'ом для целой семьи «literal
     coercion в target-typed позиции», я просто добавил туда ещё один арм).
   - `synthesize_write_str_lit_bytes_coercion` ПЕРЕИМЕНОВАН/обобщён в
     `synthesize_bytes_lit_call_args` (call-arg pre-pass в `emit_call`,
     позиция 1 — единственная позиция, которую `emit_expr_with_target_type`
     НЕ покрывает, т.к. обычный call-arg emission не вызывает эту ф-цию для
     не-ArrayLit аргументов). Ключ — `method_overloads` registry (тот же,
     что читает `call_consume_arg_idxs`), НЕ имя метода: `(recv_type_name,
     method_name)` → `Vec<MethodSig>` → `param_c_types[i]`. Populated
     ТОЧНО в том же коде, что и старый `all_methods` (тот же insert-сайт) —
     значит покрытие СТРОГО совпадает со старым гейтом (никакого регресса
     для `write`-кейсов, включая protocol-erased `Fmt`/`Write`), но БЕЗ
     привязки к конкретному имени метода. `resolved_callees[call_id]`
     дизамбигуирует multi-overload; single-overload — сразу берём. Variadic
     tail-слот явно пропускается (param_c_types[last] там — тип
     КОЛЛЕКТОРА, не элемента).

Итог: 2 checker-edits + 2 codegen-edits (1 helper + 1 арм в
emit_expr_with_target_type + переписанный pre-pass с 2 helper'ами).
Safety-gate «приёмник с зарегистрированным write» СНЯТ полностью (не
портирован) — тип уже даёт позицию однозначно, ложной коэрсии на
неродственном типе быть не может (это и была задумка марке — «gate
становится ненужным»).

## Покрытые позиции спеки (все 4)

1. Call-arg на ЛЮБОЙ `[]u8`-параметр (не только `write`) — free-fn, instance-
   method, static-ctor. ✅ (emit_c.rs::synthesize_bytes_lit_call_args +
   checker assignable_direct accept)
2. `let`/`const` с явной `[]u8`-аннотацией — ✅ для `let` (ro/mut). `const`
   (scope-local `Stmt::Const` / module-level `Item::Const`) — ОТДЕЛЬНАЯ
   архитектура: `emit_const_expr_typed`/`emit_const_expr` (constexpr-only
   emission), НЕ проходит через `emit_expr_with_target_type` вообще —
   `[]u8`-типизированный const, независимо от str-литерал-гейта, скорее
   всего вообще не representable как C-constexpr (Vec — heap-struct, не
   scalar). Честный под-маркер добавлен в backlog-followups.md (см. ниже) —
   не тихий пропуск.
3. Return-позиция — ✅ (emit_expr_with_target_type, обе формы: arrow-body
   `=> "lit"` И explicit `return "lit"` внутри block-body).
4. Element-позиция `[][]u8` — ✅ (emit_expr_with_target_type →
   try_emit_typed_vec_literal-путь, элемент-loop уже сам вызывает
   emit_expr_with_target_type(elem, elem_c) per-элемент).

## Тесты

- Изолированный dev-модуль (test-conventions.md workflow — «новый D-тест
  сначала в isolated module → PASS → мерж в spec_tests.conformance»):
  `docs/plans/wip/d55-scratch/scratch.nv` (`module wip.d55scratch`) —
  ВРЕМЕННЫЙ, удаляется перед финальным коммитом. Покрывает все 4 позиции +
  2 non-regression case (str-литерал в НЕ-[]u8 позиции остаётся str;
  str-ПЕРЕМЕННАЯ всё ещё требует явный `.bytes()`, D176 не сломан).
  Использует РЕАЛЬНЫЙ std (`WriteBuffer.from`/`WriteBuffer.write_bytes` —
  оба НЕ названы `write`) вместо выдуманных типов там, где возможно.
- ПОСЛЕ зелёного isolated-прогона — переносится в
  `spec_tests/conformance/d55_bytes_lit_type_directed.nv` (`module
  spec_tests.conformance`, root peer-файл) + neg-фикстура в
  `spec_tests/conformance/neg/`.
- Мега-CU (988 файлов, `module spec_tests.conformance`) СОЗНАТЕЛЬНО НЕ
  гоняется в этой волне (владелец: «мега-CU НЕ гонять» в тексте задачи) —
  compile+link всего дерева — дорого/долго (per project memory ~60-90 мин);
  авторитетная проверка — за оркестратором на слиянии. `nova test
  <path-to-folder>` НЕ агрегирует root-файлы бare-модуля в один репортируемый
  ряд по непонятной причине (эмпирика: `nova test spec_tests/conformance
  --jobs 4` вернул только 123 PASS/1 FAIL/14 SKIP — это подмножество
  подпапок+standalone, БЕЗ root d55_*/d374_* файлов вообще; передача
  единственного root-файла `nova test spec_tests/conformance/d55_literal_
  coercion.nv` тоже стягивает ВСЕ 988 peer-файлов в один прогон, но
  RUN-FAIL-строка почему-то показывает assertions из ДРУГОГО файла
  (app_effect_basic_t8_1.nv) — известный существующий P1 [M-vec-ext-
  method-untyped-let] (см. git log), НЕ регресс этой волны). Из-за этого
  «существующие D55/write-фикстуры зелёные» (accept-item 2) верифицируется
  копией в isolated-модуле, НЕ прогоном самого mega-CU.

## Приёмка (план, дословно из задания)

1. Новые фикстуры pos+neg — зелёные (isolated-модуль → потом merge).
2. Существующие D55/write-фикстуры (`d55_literal_coercion.nv`,
   `d374_write_sink_decouple.nv`) — зелёные (копия в isolated-модуль).
3. `spec_tests/conformance/standalone` отдельным CU `--jobs 4` — PASS 69/0
   как на main.
4. `nova check std/src/runtime` (write_buffer/read_buffer) чист.
5. Флагман: `examples/flagship/aggregator/src/main.nv --strict-effects` —
   зелёный.

## Статус — ЗАКРЫТО (2026-07-18)

Все 5 пунктов приёмки зелёные (подробный вердикт + числа — `docs/dev/simplifications.md`
запись `[M-d55-str-literal-coercion-name-gated] ЗАКРЫТ`). Изолированный scratch
доведён до PASS (2 доп. бага найдены и исправлены по пути — return-position
target-typed-гейт не знал про `[]u8`, `[][]u8`-array-literal elem_c type-punning),
фикстуры перенесены в `spec_tests/conformance/d55_bytes_lit_type_directed.nv` +
`spec_tests/conformance/neg/d55_bytes_lit_var_not_coerced_neg.nv`, scratch-
директория удалена. Честный под-маркер `[M-d55-const-bytes-lit-not-constexpr]`
(P3) заведён в `backlog-followups.md` за `const []u8`-позицию. Эта заметка
оставлена как рабочий журнал расследования (не требуется для дальнейшей работы).
