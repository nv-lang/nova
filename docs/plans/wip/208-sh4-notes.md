# Plan 208 Ф.4R Ш4 — прогресс (worktree `nova-sh4`, ветка `p208-sh4-teardown`)

Карта: docs/plans/208-unified-formatter.md §10R «Ф.4R» + §10R-Д. Модель: sonnet.
База: main `18c751b0a` (Д3 `b3ae85b05` уже влит).

## Задача

1. Снос kill-switch `NOVA_FMT_LEGACY` + старой nova_fmt_*-эмиссии в emit_c.rs — ТОЛЬКО
   `*_display_spec`-путь остаётся.
2. Закрыть Ш3-хвост: str/char/bool rich-spec + int/float Debug rich — перевести на
   `*_display_spec`.
3. Снести из `conv.h` то, что больше никем не зовётся; доложить остаток.
4. D422-амендмент в spec/decisions/02-types.md + переписка примеров §3/§4/§6/§9 плана на
   канон §10R-Д.

## Шаг 1 — emit_c.rs

- Убрал `fn fmt_legacy_enabled()` целиком + оба `if !Self::fmt_legacy_enabled() { … }` /
  `if !legacy { … } else { legacy-nova_fmt_* }` разветвления в `emit_interpolated_str` и
  `emit_format_spec_value`.
- `emit_interpolated_str`: единый блок для ВСЕХ primitives (int/f64/f32/char/bool/str,
  включая `CharLit`-литерал special-case) — резолвит `*_display_spec`/`*_debug_display_spec`
  по имени (`free_fn_c_name`, компилятор-конвенция §3) и пишет ПРЯМО в реальный interp `sb`
  (bare-путь как и раньше — zero-copy).
- `emit_format_spec_value`: убрал `let legacy = …`; radix/int/float ветки объединены
  (Debug==Display для чисел — раньше Debug падал в общий "core"-хвост с ДРУГИМ default-align,
  артефакт недоделанности, не Rust-паритет — Rust `{:?}` числа выравнивает так же, как `{}`);
  добавлены `is_char`/`is_bool`/`is_str` ранние return'ы через новые хелперы
  `emit_char_display_spec_call`/`emit_bool_display_spec_call`/`emit_str_display_spec_call`
  (тот же temp-`StringBuilder`+steal паттерн, что уже был у `emit_int_display_spec_call`/
  `emit_f64_display_spec_call` — НЕ новая архитектура, тот же класс). Composite/user-type
  "хвост" (dispatch `@display(f)`/`@debug(f)` в FRESH builder + внешний `nova_fmt_pad`) —
  НЕ тронут (вне scope Ш4, §10R предписывает только примитивную семью).
- Убрал мёртвый `primitive_to_str_fn`-closure-table целиком (все его `Some(_)`-ветки стали
  недостижимы после переноса int/f64/f32/char/bool/str в ранний блок; выжила только `None`-
  ветка — user-type D410 str.from/@to_str fallback, вынесена как безусловный код).
- `core_after_prec`/`precision_consumed` в хвосте убраны — этот путь ВСЕГДА `precision_consumed
  = true` (D419→D422 решение), так что ветка `nova_fmt_str_precision(...)` была МЁРТВОЙ (никогда
  не исполнялась) — упростил до прямого `core`, убрал текстовый след `nova_fmt_str_precision`.

## Шаг 2 — conv.h

Снесено (полностью, функция+тело):
- `nova_bool_to_str`, `nova_f64_to_str`, `nova_f32_to_str`, `nova_char_to_str`
- `nova_bool_to_debug_str`, `nova_int_to_debug_str`, `nova_f64_to_debug_str`, `nova_f32_to_debug_str`
- `nova_str_to_debug_str`, `nova_char_to_debug_str`
- `nova_fmt_int_body`, `nova_fmt_int_radix_body`, `nova_fmt_int_prefix`, `nova_fmt_radix_prefix`
- `nova_fmt_f64_body`, `nova_fmt_f64_prefix`
- `nova_fmt_str_precision`, `nova_fmt_bytes_for_chars` (только звалась из `nova_fmt_str_precision`)

Остаток (грепнуто по ВСЕМУ compiler-codegen — только emit_c.rs/nova_rt.h/conv.h упоминают эти
имена; живые вызовы подтверждены):
- `nova_fmt_pad` + `nova_fmt_encode_fill` + `nova_fmt_char_count` — единственный оставшийся
  внешний pad-путь: composite/user-type rich-spec tail (`emit_format_spec_value`) не имеет
  своего `*_display_spec`-аналога (произвольные типы), рендерит во FRESH builder и пэдит через
  этот C-хелпер. ВНЕ scope Ш4 (§10R — только примитивы).
- `nova_ptr_to_debug_str` — pointer `${p:?}` (hex-адрес), нет `.nv`-порта, не примитив-семья.
- str↔numeric parsers (`nova_str_to_i64/_u64/_f64/_bool`, `str_parse_f64`), str↔char
  (`nova_str_to_char`/`nova_int_to_char`) — ДРУГАЯ подсистема (Plan 08 conv), не тронуты.

`nova_int_to_str` (nova_rt.h, НЕ conv.h) — НЕ трогал: живёт в другом файле, другие вызыватели
(TaggedTemplate bootstrap ~32119, D410 numeric-cast fallback ~41787/42134-эквивалент) — вне
scope (не primitive-family conv.h teardown).

## Шаг 3 — DCE-seed баг (найден и починен в РАМКАХ этой же волны, zero-tolerance)

Folder-CU гейт поймал линк-ошибки: `undefined symbol: nova_fn_...bool_display_spec` /
`..._char_display_spec` на 3 standalone-фикстурах (`f14_legacy_workaround_still_works`,
`f2_static_method_str_from_bool`, `f7_char_var`) — эти free fn имена не были seed'нуты в
DCE-reachability seed-лист (`compiler-codegen/src/lints.rs` ~1313, `int_display_spec`/
`f64_display_spec`/`f32_display_spec` уже там были с Ш3, а `bool_display_spec`/
`char_display_spec`/`char_debug_display_spec`/`str_display_spec`/`str_debug_display_spec` —
новые в Ш4 — забыл добавить). Починено в том же слиянии: добавил все 5 имён. Пере-собрал,
пере-прогнал — 0 FAIL.

## Гейты (дословно)

- `nova build --release` (compiler-codegen, nova-cli) — чисто, только pre-existing warnings.
- `nova test std/src/runtime/fmt_buf` → `PASS: 1 FAIL: 0`.
- `nova test std/src/runtime/string_builder_test.nv` → `PASS: 1 FAIL: 0`.
- `nova test std/src/checksums` → `PASS: 3 FAIL: 0 SKIP: 3`.
- Folder-CU `spec_tests/conformance` (146 путей, ВРЕМЕННО без `d216_ptr_methods_174_5.nv` —
  см. находку ниже, файл возвращён на место после гейта, `git status` на него чист) →
  **`PASS: 130 FAIL: 0 SKIP: 18`** — включает все 3 `d422_f4r_baseline_*.nv` (эталоны Ш0, byte-
  parity подтверждён — тесты используют `assert(...)` на точных литералах, зелёные).
- `nova build examples/flagship/aggregator/src/main.nv --strict-effects` → `built` (29.37s),
  только pre-existing warnings (unused-import/W_DEP_PATH_NO_RELEASE/W_PARAM_TYPE_POS_MUT).
- Мега-CU (весь `spec_tests/conformance` С `d216_ptr_methods_174_5.nv`) — НЕ гоняла как
  авторитетный гейт (за оркестратором/интегратором, §10R-инструкция «Мега-CU НЕ гонять»); ОДИН
  прогон сделан для диагностики находки ниже (крашится ДО и НЕЗАВИСИМО от Ф.4R Ш4).

## Находка вне scope (P1, задокументирована, НЕ чинилась)

`[M-d216-write-at-return-type-unknown-cc-panic]` (`docs/plans/backlog-followups.md`) —
`spec_tests/conformance/d216_ptr_methods_174_5.nv:18` (`q.write_at(1, 99)`) роняет компилятор
internal-error (`[P67-LEGACY] method call .write_at return type unknown`, `emit_c.rs`) при
codegen ЛЮБОГО подмножества путей внутри `spec_tests/conformance` (module=folder, весь CU
мёржится). Подтверждено НЕ регрессией Ф.4R: репродюсируется байт-в-байт на НЕТРОНУТОМ main-
бинаре (`d:/Sources/nv-lang/nova`, HEAD `c190de41e` на момент проверки) — тот же craш вне
зависимости от Ф.4R-правок (pointer-method-dispatch код не пересекается с fmt-районом этой
волны). Блокирует мега-CU гейт независимо от Ш4. Не чинилось (вне ЗОНЫ волны).

## Файлы

- `compiler-codegen/src/codegen/emit_c.rs` — снос kill-switch/legacy + новые примитивные
  ветки + новые хелперы `emit_char_display_spec_call`/`emit_bool_display_spec_call`/
  `emit_str_display_spec_call`.
- `compiler-codegen/src/lints.rs` — DCE-seed фикс (5 новых имён).
- `compiler-codegen/nova_rt/conv.h` — снос функций (см. Шаг 2), новый scope-коммент в шапке.
- `compiler-codegen/nova_rt/nova_rt.h` — 2 stale-комментария поправлены (ссылки на снесённые
  `nova_f64_to_str`/`nova_fmt_f64_body`).
- `docs/plans/208-unified-formatter.md` — статус-шапка (Ш4 закрыта) + примеры §3/§4/§6/§9
  переписаны на канон §10R-Д (без `_into`, value-first).
- `spec/decisions/02-types.md` — D422 «Статус реализации»: Ф.4R-строка обновлена, V1-упрощение
  #3 частично закрыто (аннотации в подсекции #1/#2/#3), §5 code-пример переписан на канон.
- `docs/plans/backlog-followups.md` — новый маркер `[M-d216-write-at-return-type-unknown-cc-panic]`.
