# Журнал задачи 144 (reliz-oracle-013)

## 2026-11-06, вечер — Этап 0 + вскрытие по пункту 3 (126-остаток)

Этап 0:
- Директивы владельца 5-6 записаны в задание (800d223b0): каждый пункт БЕЗ
  УПРОЩЕНИЙ, как для прода; малый маршрут.
- `docs/release-plan-oracle-013` запушена (ce030259b — план убран из docs/dev/).
- main не пушился: страж требует CI-доказательство. Кандидат выложен на
  `integrate/144-reliz-oracle-013` (800d223b0); 8/10 workflow зелёные
  (включая crate-tests), nova-gate и carina-gate ещё шли на момент записи.
- Компилятор пересобран чисто (cargo clean -p nova, 20:54).

ВСКРЫТИЕ (пункт 3, 126-остаток):
- Пин `spec_tests/conformance/standalone/if_none_option_str_infer.nv`
  КРАСНЫЙ на пересобранном main: CC-FAIL NovaOpt_nova_int vs NovaOpt_nova_str
  (генерируемый C, строки ~7804/7808).
- `nova check` на пине — PASS: дверь чекера (a0bb8ab6b) работает.
- Значит дверь эмиттера (4b257806a, emit_c.rs emit_if_expr) НЕ ПОКРЫВАЕТ
  реальный путь эмитирования этой формы: `ro x = if flag { None } else {
  Some("s") }` в теле функции — голая None-ветка материализуется
  NovaOpt_nova_int в C.
- Источники идентичны во всей цепочке (фикстура f200bc60 и diff .rs пустые:
  a0bb8ab6b/4b257806a = release/0.1.3 = main), чистая пересборка не меняет
  результат.
- Вывод: журнал 142 «Item 126 закрыт» не подтверждается end-to-end пином —
  зелёный прогон пина при закрытии не выполнялся (батарея была только unit).
  Это и есть содержание «126-остатка»: дверь эмиттера нужно доводить
  ПОЛНОЦЕННО (без упрощений), пин должен прогоняться зелёным.

Дефект-индекс (canopus spikes/env/workarounds.md): 29 меток в 22 файлах,
items с метками: 19. Определения пунктов волны: 16 fluent_assign_back
(src/acp/link.nv), 17 atomic_in_record_copy (там же, закрыт D447),
18 consume_capture_vs_field (там же), 20 output_over_4k (src/acp/README.md),
25 nested_module (README x2), 49 fn_name_two_modules (src/server/hub.nv).

## 2026-11-06, поздний вечер — пункт 3 (126-остаток): корень + полный фикс

Корень (вскрытие): форма `ro x = if c { None } else { Some(s) }` эмитируется
ДВУМЯ независимыми выводами C-типа: value-сторона (emit_if_expr, несла
item-126 реконсиляцию с 4b257806a) и storage-сторона (infer_expr_c_type
Channel 6j — probe типа Let-инициализатора, НЕ несла). Копии расходились уже
дважды (публикация 2026-07-04 это фиксировала комментарием). Итог:
`NovaOpt_nova_int request = <NovaOpt_nova_str>` → CC-FAIL. Пин при закрытии
142 зелёным end-to-end не прогонялся (только nova check + unit).

Фикс (как для прода): общий helper `if_branch_chosen_c_type` в emit_c.rs —
единый решающий конвейер для ОБОИХ потребителей: Plan 125 divergence-aware →
D275 unit-доминирование → item 126 bare-None реконсиляция (симметрично) →
d174 empty-fallback. Probe веток остался на вызовах (branch_trailing_c_type_
with_locals vs block_tail_c_type — разные overlay-карты, слияние отдельно);
Result-комбинация у вызывающих (6j inline, emit_if_expr через U.4.4).

Верификация: пин if_none_option_str_infer.nv — GREEN end-to-end (nova test);
io-mock GREEN; conformance 498 PASS (1 флаки-красный m2217_22 под нагрузкой
чужой батареи — соло PASS на том же бинаре, дважды). Коммит на ветке
fix/item126-emitter-infer (c02019bae + фикс), юнит-батарея запущена.

Этап 0 попутно: carina-gate круга 2 починен (baseline rows task 145 для
дрейфа read_line; сид перебазируется после релиза). CI независимо подтвердил
находку: nova-gate круга 2 красный ровно на этом пине на кандидате c02019bae.

## 2026-11-06, ночь — пункт 4 (print/println → Io): спек-решение владельца

Канон (проверен): spec/effects.md:201 (Io = stdin/stdout/stderr); syntax.md:1681
и effects.md:348 [D431] — ОБА примера спеки ставят Io в сигнатуру функции,
вызывающей println; эргономика — D431 #default_handler (real_io() уже в
std/src/io/console.nv — канонический дефолт готов). Research-док
docs/dev/research/2026-08-01-println-on-nv.md (ЗАМОРОЖЕН → GO).

РЕШЕНИЕ ВЛАДЕЛЬЦА (06.11.2026, этот чат): вариант (б) — канон-точно:
print/println несут Io в сигнатуре; эргономика через #default_handler;
мокабельность with Io = mock_io(...). Спрошено через выбор (а/б/в), выбран (б).

Ф.0-пины 4-5 (spread в println; голый println()): закрываю СОХРАНЕНИЕМ
статус-кво (spread остаётся Err, println() = голый newline) — это фиксация
текущего задокументированного поведения, не новое спек-решение.

План реализации (Ф.1-Ф.2 по доку + (б)):
1. std/src/prelude/runtime.nv: сигнатуры print/println + Io (extern "nova";
   EffectItem — ребёнок FnDecl, парсер принимает; энфорс чекера проверить).
2. emit_c.rs: intercept (40993) сохраняется, emit меняется: синтез
   InterpStrPart-списка → emit_interpolated_str (честная E_INTERP_NO_DISPLAY
   вместо %lld-мусора) → ОДИН dispatch Io.write_out(s.bytes()).
   Peephole §6: единственный str-арг — без StringBuilder. Канал f1_expr
   (61538) синхронизировать. Spread — оставить Err.
3. interp/stdlib.rs: print/println через Io-механику интерпа (capture).
4. Энфорс-волна: сигнатуры с println получают Io (canopus 8 мест, примеры,
   тесты репо); expect_output-фикстуры (effects: [] → Io).
5. Пины: behavior-changes таблицы §5 (юзер-тип → честная ошибка, char →
   UTF-8, мульти-арг → один write), f64/f32 байт-паритет Ф.0-3.

## 2026-11-06, ночь (2) — пункт 4: шаг 1 (сигнатуры) верифицирован; карта emit-машинерии

Шаг 1 (сигнатуры print/println + Io в prelude/runtime.nv) — в рабочем дереве
fix/println-io-effect, НЕ закоммичен до перевода emit-пути. Верификация:
`--strict-effects` ловит `fn main() { println("x") }` как
E_UNDECLARED_TRANSITIVE_EFFECT (без флага — D62 правило 1, не ошибка);
энфорс-механика чекера (check_callee_effects читает callee.effects) — ноль
правок. /tmp-копии используют ДРУГУЮ std — проверять только из репо.

Карта emit-машинерии (для Ф.1-Ф.2):
- Сносимое: intercept emit_c.rs:40993 -> emit_println/make_print_call/
  infer_print_helper (:48638-48743, ~190 LOC) -> 6 rt-хелперов nova_print_*.
- Замена: emit_interpolated_str(&[InterpStrPart]) -> Result<String,String>
  (:49710) возвращает nova_str; parts = аргументы (spec None/Display) + Lit
  "\n" для println; пины честной E_INTERP_NO_DISPLAY вместо %lld-мусора.
- Диспетчер эффекта: оп-вызов = vtable-слот `_nova_handler_Io` ->
  `NovaVtable_Io*`->op; ленивый D431-дефолт — inline
  `if (!_nova_handler_X) { _nova_handler_X = ctor(); }` перед vtable-вызовом
  (генерит emit_effect_type per-op wrapper; default_handler_fns :2396).
  B11ac (:62777) — рет-инференс vtable-вызова по effect_schemas.
- Вывод ret у Io.write_out: Result[int, IoError] — в синтезе println
  результат игнорируется (statement-position), но должен корректно
  материализоваться (mirror Stdout @write).
- Дизайн-инвариант (урок item 126): НЕ дублировать per-op wrapper руками в
  emit_println — либо синтез AST-вызова (ExprKind::Path(["Io","write_out"]),
  прецедент CODEGEN-SYNTHESIZED узлов :62228), либо вынести общий хелпер
  диспетчера оп-вызова. Копии расходятся.
- interp-канал: interp/stdlib.rs:52-79 print/println Native через
  OUTPUT_CAPTURE (:13-24) — направить через Io-механику интерпа.
- Канал f1_expr (:61538): println|print|assert -> "nova_unit" — синхронизировать.

## 2026-11-06, ночь (3) — пункт 4: ЯДРО ЗАВЕРШЕНО (495a2fb84), 0 регрессий

Сделано: синтез println через emit_interpolated_str + диспетчер Nova_Io_write_out
(schema-keyed, не хардкод); снос make_print_call/infer_print_helper (~77 LOC);
prelude импортирует std.io (прецедент char/defaults/math); сигнатуры print/
println + Io; пин print_io_lowering.nv (EXPECT_STDOUT x3, все примитивы,
no-separator семантика документации подтверждена).

Верификация: conformance 498 PASS (2 пре-существующих красных, бazель идентичен),
nova_tests 1 пре-существующий, std/src 82 PASS (2 пре-существующих, bazель
идентичен). f64-паритет — интерполяционный движок (Plan 208), пин подтверждает.

Решения в рамках пункта (инженерные, не спек):
- Interp-канал: print/println в интерпретаторе остаётся на OUTPUT_CAPTURE
  (capture = честный мок doc-тестов); полное Io-диспетчерство в интерпретаторе —
  отдельная подсистема (машинерии эффектов в интерпе нет), зафиксировано как
  хвост пункта 4. Поведение doc-фикстур не меняется.
- expect_output-фикстуры (effects: []) не затронуты (interp-канал не менялся).

Хвосты пункта 4: Io-сигнатурная волна по корпусу (примеры/тесты репо под
--strict-effects; canopus 8 мест — через их acceptance); interp-Io подсистема.
