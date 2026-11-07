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

## 2026-11-06, ночь (4) — пункт 6 (item 17): доказательства закрытия собраны

Item 17 (atomic_in_record_copy: Atomic*-хендлер в record копируется, store
через переменную не виден) закрыт D447 `#no_copy` — аффинность на уровне типа
(spec/decisions/02-types.md:19469, Plan 248; Atomic* = value-inside record,
копирование типа запрещено компилятором).

Доказательства (все на новом компиляторе, 5/5 PASS):
- spec_tests/conformance/p248w3_atomic_value_inside_spawn.nv — ключевой:
  mut-захват Atomic-счётчика в spawn/supervised/parallel-for, все fibers
  пишут В ОДНО хранилище (escape-promotion D228 маршрутизирует по указателю);
- spec_tests/conformance/d447_no_copy_second_name.nv — аффинность;
- neg/n_d447_arg_escape.nv, neg/n_d447_tuple_literal_embed.nv,
  neg/n_d447_invalid_kind.nv — отрицательные формы отказа компиляции.

Пункт 6 состава: DONE (доказательства в записи релиза).

## 2026-11-06, ночь (5) — пункт 5 (item 16): естественный repro НЕ воспроизводится; пин собран

`b = b.bump(5)` (assign-back fleeting-ref в value-record) и цепочки
`b.bump(1).bump(2).n` на текущем тулчейне компилируются и мутируют ЕДИНОЕ
хранилище переменной (повторный assign-back после цепочки — 8+5=13). Отказ
«a pointer assigned to a value» ушёл (вероятно, Plan 184 fleeting-refs).
По правилу владельца пункт НЕ исключается: собран пин
spec_tests/conformance/standalone/fluent_assign_back.nv (3 EXPECT_STDOUT) —
доказательство снимаемости метки ORACLE-WORKAROUND(16) в canopus
(src/acp/link.nv:198) против этого релиза; снятие — через их acceptance.
Пункт 5 состава: DONE (пин + запись).

## 2026-11-06, ночь (6) — пункт 7 (item 18): ВОСПРОИЗВЕДЁН, корень локализован

Минимальное репро (локально, /tmp/probe/n18.nv): consume-биндинг `tx`
(`consume tx = mk(11)`, тип consume value) + record с полем `tx` + тело
`spawn consume tx { ... m.tx ... }` → CC-FAIL: `no member named
'_consume_tx_0' in 'struct Nova_Marks'`.

Корень: эмиссия consume-биндинга = C-препроцессорный макрос:
- NovaValue_Pipe _consume_tx_0 = _c->tx; #define tx _consume_tx_0
  (emit_c.rs:35376, consume-scope);
- слой fiber-capture: #define {cap} (_c->{cap}) (emit_c.rs:14131).
Макрос переписывает КАЖДЫЙ токен `tx` после #define — включая поле `m.tx`
(лексер C не знает про `.`-члены). Репро-фикстура пока НЕ закоммичена (красный
пин ломает гейт): фикс компилятора и пин идут одним коммитом.

План фикса (как для прода): убрать текстово-макросный механизм привязки имени
биндинга к C-локалу. Эмитить локал с исходным именем (C-блочная областность
легально тенит вложенные скоупы; -Wno-everything), сохранить save/restore
var_types (уже есть, [M-detach-consume-match-arm-capture-lost]); для
fiber-захватов — тот же перевод с макросов на прямые ссылки через ctx-структуру.
Обязателен A/B: батарея consume-фикстур (conformance/consume_fixtures/*,
detach-consume формы из волны 0.1.2).

## 2026-11-06, ночь (7) — пункт 8 (item 49): ВОСПРОИЗВЕДЁН, корень локализован

Пин-репро (локально, не закоммичен — красный): spec_tests/conformance/
fn_name_two_modules/{driver,def_mod}.nv (раскладка как const_peer_group:
каталог = программа, ./def_mod.{count_of} относительный импорт). Точно по
спайку task 114: вызов `begin(2)` driver-модуля резолвится в ПРИВАТНУЮ
generic `begin[T]` чужого модуля → «cannot infer type argument T (returned
only — turbofish required)». Имена функций глобальны программе даже для
приватных чужих (семья items 9/14/21/39/43).

План фикса (как для прода): резолюция свободных вызовов в чекере —
модуль-локальные объявления выигрывают у чужих; приватные чужие имена
невидимы вовсе (импортируется только экспортированное). Место: free-fn
call resolution (types/mod.rs; novac-сторона синхронно при самосборке).
После фикса — пин fn_name_two_modules коммитится зелёным, метка
ORACLE-WORKAROUND(49) в canopus (src/server/hub.nv:230) снимается против
релиза через их acceptance.

## 2026-11-06, ночь (8) — пункт 9 (item 25): половина жива

Пин (закоммичен, зелёный): spec_tests/conformance/nested_module_two_levels/ —
модуль в ДВУХ уровнях под корнем (nested_module/inner/inner.nv, в первой
папке нет .nv) — РАБОТАЕТ в форме ОТНОСИТЕЛЬНОГО импорта `./nested_module.inner`
(импорт, вызов, вывод корректны). Но АБСОЛЮТНАЯ форма
`import nested_module_two_levels.nested_module.inner` того же модуля — НЕ
резолвится (cannot find module). Итого item 25 жив наполовину: дефект в
абсолютной резолюции путей вложенных модулей (семья item 49 — резолюция имён/
путей программы). Относительная форма — канонический маршрут (const_peer_group
прецедент). Фикс абсолютной резолюции — вместе с item 49 (одна семья).

## 2026-11-06, ночь (9) — пункт 10 (item 20): не воспроизводится на уровне раннера

Раннер `nova test` сейчас ДРЕНИРУЕТ piped stdout ребёнка: тест с 201 строкой
(~9 KB > 4096) проходит до конца, EXPECT_STDOUT-хвост совпадает. Дефект
«раннер не читал stdout → TIMEOUT 70s» ушел; пин
output_over_4k_drain.nv (закоммичен) — доказательство снимаемости заметки
ORACLE-WORKAROUND(20) в canopus (src/acp/README.md: «Two units, short test
names»). Случай «бинарь запущен вручную с piped-и-непрочитанным stdout» —
семантика OS-пайпа (любой язык), не дефект Nova. Пункт 10 состава: DONE.

## 2026-11-07 — пункт 12 (item 19-к3): дизайн-проход завершён, решение выбрано

Механизм: G1 = слепой SEQ_CST store `park_state=WAIT` в nova_gopark
(nova_sched.h:311) затирает латч R3 (`NIL->READY`, :253-259), выигранный
waker'ом в окне «gopark-enter .. G1» (трасса b7bf05ee8: pred-recheck
промахнулся по CLOSED, вытеснение, goready латчит READY на NIL, G1 затирает,
G3 CAS READY->DISPATCHED фейлится на собственном WAIT → вечный mco_yield).
Slot-Dekker (mark_slot/CLOSED) замыкается честно — дыра на стыке slot-протокола
и co-латча.

Отклонённый минимальный фикс (b7bf05ee8): G1=xchg(WAIT) с потреблением READY
как fastpath — hang ушёл, но 5 segfaults/18 loaded runs: STALE READY прошлой
транзакции (опоздавший waker латчит NIL->READY на УЖЕ бегущем fiber) превращал
одноразовый park в skip → double-run → UAF.

РЕШЕНИЕ (рекомендация дизайн-прохода): 64-бит слово `park_state|epoch` —
пер-fiber монотонный epoch, инкремент на каждый gopark-вход:
- G1 = xchg(WAIT|epoch): READY|same-epoch → валидный fastpath (потребить);
  READY|stale-epoch → отбросить (лечит segfault-причину);
- R3 (waker) = CAS NIL|e -> READY|e — в чужой эпохе CAS fail, безопасно;
- конец ожидания = store(NIL|epoch+1, RELEASE).
Стоимость: ноль доп. атомиков (G1 и так xchg), изменения в 3 точках
nova_sched.h + хелперы fibers.h (:2091 полe до 64 бит — ЗЕРКАЛО в codegen
SpawnCtx_N обязателен, fibers.h:2087-2090 дисциплина) + диагностические
load-сайты (:2602/:3100/:3130/:877) на unpack-хелпер. R2/R4, каналы,
sync_*, net — бит-в-бит. 32-бит цели: двухсловный CAS-цикл. Страховка:
fs.c:115 bare-park перевести на park_until с предикатом.

Верификация: A/B (A=blind store, B=epoch) ≥30 loaded runs каждая под 6-way
нагрузкой, 400-раундовый spike item19_child_pipes_exit (база 6d88dc484),
NOVA_DIAG_ITEM19_R3=1; инварианты accounting: gopark-enter ==
gopark-returned + G3-fastpath; каждый goready-R3-latch потребляется той же
эпохой. ЗАКРЕПИТЬ: скрипт 6-way нагрузки (в истории не зафиксирован — только
числа в коммитах). Отсутствует tmp_f2_design.json (ссылка nova_sched.h:169).

## 2026-11-07 (2) — пункт 11 (items 14/21/22): дизайн-проход завершён

Карта пространств C: single-CU; value-record юзера = NovaValue_<Name> из
голого имени (emit_value_record_type :19063; fwd-decl :7287 тоже голый);
heap/sum — Nova_<Name>; эффекты NovaVtable_<Name>/_nova_handler_ из голого
имени (:18729+, коллизии не проверяются вовсе); free-fn — модульно-квалифицированы;
методы Nova_<T>_method_<m> из голого recv (латентный гэп дизамбигуации символов).

Механизм D381 ([M-sync-crossmodule-samename-type-collision], :6263-6460):
type_def_modules -> colliding_type_names -> def_type_base/ref_type_base
(Nova_std_io_ErrorKind). Свойство: CU без коллизий = байт-идентичный C.
record_def (:6305-6347) ЗАКРЫВАЕТ оси value-record/NamedTuple/Effect/Protocol/
генерики («distinct axis — followup») и RUNTIME_DEFINED_TYPES — потому
юзер Output (value-record) + std.os Output (proc.nv:140 value-record) оба
невидимы → тихий дубль typedef NovaValue_Output → incomplete type (item 14).

РЕШЕНИЕ (вариант A): расширить record_def.qualifiable на value-records и
NamedTuple; механизм D381 переиспользуется целиком (def_base уже протянут
в emit_value_record_type :18519/:18535). Точки: record_def kinds, fwd-decl
:7286-7296 через def_type_base, value_struct_field_tys/type_aliases ключи
остаются голыми (конвенция), mono-путь NovaValue_{short}____args — qualified
base. RUNTIME_DEFINED_TYPES/BUILTIN/A4_RUNTIME_ABI_SUMS — frozen (не трогать).
Blast radius точечный; бесколлизионные CU байт-идентичны.
Отклонены: B (сменить префикс всем — ломает ABI нейминг всех программ),
C (реестр занятых + суффиксы — недетерминизм порядка эмиссии).

СОПУТСТВУЮЩИЕ АТОМЫ: (1) эффекты/протоколы — квалификация NovaVtable_/
_nova_handler_/NovaBox_ по модулю для коллидирующих (Fail/Time в nova_rt
frozen); (2) ЧЕКЕР: program-global таблица типов → module-scoped — без него
items 21/22/30/37-40 не проходятся даже с уникальным C (падают ДО C:
E_NO_STATIC_METHOD, E_MISSING_FIELD_IN_LITERAL, E7301) — ЭТА ЖЕ семья, что
item 49 (модуль-локальная резолюция имён); (3) методные символы коллидирующих
типов через ref_type_base.

Пины (7, перечислены в карте): user_output_vs_std_os (14), std_os_output
(21, two-module), pg_event (22), pg_polaris_message (30), user_type_child
(40), same_value_record_two_modules (новая ось A), ErrorKind-регрессия +
байт-дифф бесколлизионного дерева. Спайк repro_output_c_collision (9b6ccd3f)
на диске/в истории canopus НЕ найден — формы восстановлены по эквивалентным
repro (repro_acp/user_output_vs_std_os, repro_engine/std_os_output,
repro_pg_event/pg_event).

## 2026-11-07 (3) — пункт 7 (item 18): ФИКС ЗАВЕРШЁН (f486cac72)

Локал consume-биндинга несёт исходное (мангленное) имя; пара #define/#undef
удалена. Две тонкости, найденные A/B:
1. Символы машинерии (_defer_<name>_active и пр.) уже были shadow-based по
   дизайну (Plan 253.4, решение владельца 2026-08-08) — не тронуты.
2. Plan 217 function-scope хойст одноимённого биндинга давал
   само-инициализацию (T s = s;) в re-consume форме (m548) — init теперь
   двухшаговый (temp-first).

Верификация: repro-пин GREEN (captured =11 field =7); m548 (re-consume
disarm) PASS; a_q3 (multivar LIFO cleanup) PASS; conformance 500 PASS
(красные = 2 пре-существующих + красный-по-дизайну пин item 49 + TIMEOUT
pos_fibers_per_worker_concurrent — флак под нагрузкой, соло PASS).
Попутно: helper-подкаталог item 25 оформлен folder-module (2 peers) по
конвенции энумератора корпуса.

Хвост item 18: fiber-capture слой (#define {cap} (_c->{cap}) в
handler/fiber ctx) — ОТДЕЛЬНЫЙ слой, в repro не участвует; перевод его на
прямые ссылки — отдельно (не блокирует item 18).

## 2026-11-07 (4) — toml-коллизия: прелюдийный импорт вскрыл корень; решение — чекер-хирургия

toml_test красный при прелюдийном `import std.io`: голое имя
`UnexpectedEof` (toml.nv:221 throw + match-паттерн) резолвится
программно-глобально в io.ErrorKind.UnexpectedEof ВМЕСТО локального
ParseTomlError.UnexpectedEof — та же семья, что item 49/29 (имена глобальны
программе). Попытка сплита console -> std.io.console вскрыла упаковочный
клубок (смешанные модули в каталоге -> D78, двойная регистрация методов ->
E_METHOD_REDEFINITION); ОТКАТ (git checkout, сплит не в истории).

РЕШЕНИЕ: чекер-хирургия «модуль-локальные имена выигрывают» (variant→owner
поиск уже существует — прецедент №599 в базовом комментарии; free-fn
резолюция — тот же принцип). Один фикс закрывает: item 49, чекер-ось
items 21/22/29/30, toml-коллизию, и легализует прелюдийный import std.io
как есть. Прудь: bare-имена, НЕ объявленные локально, резолвятся в
импортированное/глобальное (поведение программ без локальных деклараций
не меняется).

Состояние кандидата: f486cac72 (item 18 фикс) + журналы выложены в
integrate (круг 7). Прелюдийный import std.io остаётся в HEAD (495a2fb84) —
после чекер-хирургии toml_test должен позеленеть без откатов.

## 2026-11-07 (5) — пункт 8 (item 49): фикс резолюции готов; вскрыт латентный дефект-эхо

ФИКС (в дереве): generic_overload_mono.rs + гейт emit_call (~48412) —
`resolved_callee_is_concrete(name, call_id)`: если канал чекера
(resolved_callees) указывает на КОНКРЕТНУЮ декларацию имени, у которого есть
и generic, mono-арм не перехватывает вызов (голый fallback
mono_fn_decls — last-registration-wins — монотипизировал ЧУЖУЮ generic).
Турбофиш-ошибка ушла, своя begin выбирается.

ВСкрылось эхо (раньше маскировалось ранним абортом кодгена): в turbofish-
вызове `begin[int](n)` внутри count_of эмитится МЁРТВЫЙ erased-generic
thunk (NovaClos_ii, C-строки ~2408-2415 артефакта: _nv_tmp_1 нигде не
читается), тело танка зовёт БАЗОВЫЙ символ
nova_fn_..def_mod5begin, который никогда не определён (у generic только
mono-инстансы) → link error. Две линии для добивки: (а) кто эмитит
func-ident как value в turbofish-вызове (подозреваемые: fn_ret_-кэш в
var_types + is_user_fn-арм :36577), (б) танк для generic должен таргетить
mono-инстанс, а не базу (emit_free_fn_value, decl_module_symbol.rs:153).
ПИН fn_name_two_modules НЕ коммичится до добивки (красный линк).

## 2026-11-07 (6) — кейстоун: модуль-локальный выбор владельца внесён; toml НЕ закрыт им

Круг 9 (6cb39951e, с кейстоуном variant-owner preference): toml на Linux
ВСЁ ЕЩЁ красный — гипотеза «утечка → двусмысленность владельца варианта»
не подтвердилась как ПОЛНАЯ причина. Кейстоун ОСТАЁТСЯ (корректен по
дизайну, 0 регрессий: conformance 501 PASS/3 известных, byte-identical
вне двусмысленностей) — он нужен для семьи 49/29 в любом случае.

Диагностичность: страж check-std-test-baseline теперь печатает строку
исхода НОВОГО отказа (343126c3b) — Linux-текст отказа toml будет виден
из лога CI круга 10 без локального прогона. Windows-toml красный
пре-существующе (чистая пара бинарь+дерево до волны — тоже красный),
Linux-зелёный до волны: триггер Linux-красного — в волне, но какой
именно элемент (утечка имён? сигнатуры? emit-путь?) — определит текст
из круга 10.

## 2026-11-07 (7) — toml: Linux-текст отказа получен (круг 10); состояние развилки

Круг 10 (343126c3b): Linux-toml красный с текстом, ИДЕНТИЧНЫМ Windows-
пре-существенному (оба ассерта: :81 незакрытая string + :121 repeated
Fail-effect calls). Факты:
- Windows: красный на ЧИСТОЙ паре (бинарь+дерево до волны) — пре-существующий.
- Linux: зелёный в кругах 2-3 (800d223b0/459f00b4e), красный с круга 5
  (print-Io) — волна сделала Linux похожим на Windows.
- База стража (4 имени) toml не содержала — Linux-красный = подлинный
  регресс волны; Windows-красный — тот же СИМПТОМ по своей причине.
- Кейстоун (variant-owner preference) Linux-toml не закрыл.

Гипотеза-кандидат: Wave-изменение сделало Linux-путь Windows-подобным —
наиболее общий кандидат: print-Io эмиссия (fflush-per-write через
io_write_fd vs nova_print_end) ЛИБО утечка имён, меняющая резолюцию
Fail-сцены toml; проверка требует либо Linux-бисекта по коммитам волны
(3 кандидата: print-Io, item-18 consume, item-49 gate), либо локальной
минимализации Fail-сцены toml на Windows (пре-существующий корень —
тот же дефект!). Путь минимальализации: редуцировать toml_test:81 до
минимального repro (match "k = \"unclosed".to_toml() { Err(...) ... })
и бисектить Windows-историю — найденный корень закроет ОБЕ платформы.

СЛЕДСТВИЕ ДЛЯ РЕЛИЗА: пуш main заблокирован этим красным (гейт работает
как задуман). Кандидат 343126c3b содержит: item 31, 126+остаток, print-Io
ядро, item 18, item 49-резолюцию, кейстоун, волны, пины 16/17/20/25.

## 2026-11-07 (8) — toml: план CI-бисекта (Windows-дискриминатор исчерпан)

Эксперименты: старый бинарь+новое дерево (утечка) — probe всё ещё CC-FAIL
(Windows-форма красна ВСЕГДА — дискриминатора нет). Дальше только CI-бисект:
кандидаты-откаты по одному элементу волны (каждый — ветка от
fix/println-io-effect с односрочным revert, круг CI):
1. revert прелюдийного import std.io (гипотеза утечки);
2. revert сигнатур print/println (гипотеза Fail-сцены);
3. revert print-эмиссии (гипотеза flush-per-write).
Каждый круг отвечает да/нет; найденный элемент чинится по существу
(не откатом) — откат только для локализации.

ПРИМЕЧАНИЕ: probe (leak.nv, Fail-вызов в match без with) — сам по себе
Windows-красный пре-волны: прямой Fail-вызов в match-позиции типится
nova_unit на Windows — отдельный старый дефект прямой Fail-формы
(D65), toml:81 использует ровно её. Это МОЖЕТ быть и корнем Linux-флипа
(волна изменила контекст резолюции этой формы), и независимым старым
дефектом, который волна обнажила. Бисект ответит.

## 2026-11-07 (9) — бисект-1: гипотеза утечки НЕ проверяема так (импорт несущий)

Откат прелюдийного импорта ломает nova build hello-world РАНЬШЕ toml
(smoke-страж и crate-тесты ident_x: «Io effect is not in this CU») —
импорт несущий для print-пути, сигнала по toml нет. Ветка
bisect/toml-no-prelude-import остаётся как доказательство несущести.

Бисект-2 (bef2f4623, откат СИГНАТУР print/println — импорт на месте,
схема Io в CU, hello собирается) выложен кандидатом; в toml.nv/toml_test.nv
println НЕТ вовсе — так что гипотеза 2, если подтвердится, будет означать
«сигнатуры Io меняют Fail-сцену глобально», и чиниться по существу в
механике effect-строк. Бисект-3 (эмиссия) — если 2 не подтвердит.

## 2026-11-07 (10) — toml: КОРЕНЬ НАЙДЕН (эмит-сторона, first-wins владелец варианта)

Эксперименты на Windows (точка-проба по плейбуку §2.5/№17, наконец-то):
- Минимальная форма match над Fail-вызовом — ЗЕЛЕНАЯ на текущем дереве.
- toml_test:81 ВСТАВКА ВЕТОК меняет исход: 2 ветки (arm1 + `_`) — arm1 НЕ
  срабатывает → assert(false); 4 ветки — arm1 срабатывает верно.
- ЧИСТАЯ ПАРА (бинарь+дерево до волны) + вариант B (4 ветки): :81 ЗЕЛЕЕТ —
  баг формы СТАРЫЙ (пре-волна); Windows-красный томl — он же.
- Волна (прелюдийный import std.io) добавила io.ErrorKind.UnexpectedEof в
  CU → порядок регистрации в sum_schema_registry.variant_to_sum сменился →
  first-wins (find_variant_v2/find_variant_compat) на Linux отдал ЧУЖОГО
  владельца → тег arm1 сравнивается не с тем → fall to `_`.

КОРЕНЬ: эмит-сторона резолвит владельца варианта паттерна first-wins по
глобальному реестру, вместо КОНТЕКСТА (expected sum из scrutinee
Result[_, ParseTomlError]). Комментарий D381 в sum_schema_registry.rs:264
сам говорит: «len() >= 2 → ambiguous → disambiguated by call-site context
(the expected/return sum) rather than find_variant_v2's first-wins» — но
путь match-arm этим правилом не пользуется.

ФИКС (следующий шаг): в резолюции payload-варианта match-arms предпочесть
expected sum (канал/scrutinee) до first-wins; кандидаты-сайты: emit_c.rs
:5026 (types_get-дверь), :24058, :35982 (комментарий уже признаёт
«may pick the WRONG sum»). Зеркало кейстона на эмит-стороне. После фикса:
пины fn_name_two_modules + toml-восстановление в базу стража НЕ требуется
(toml зелёный = страж зелёный), пуш main.

## 2026-11-07 (11) — бисект-2 опровергнут; канал №279 расширен; круг 11 арбитраж

Бисект-2 (откат сигнатур): toml всё ещё красный на Linux — гипотеза 2
опровергнута. По плейбуку §2.5 («проверь посылку инструментированием»)
перешёл на точечные пробы Windows-репро:
- ВСТАВКА ВЕТОК в match (:81) меняет исход: 2 ветки — сломано, 4 — верно;
- ЧИСТАЯ ПАРА + вариант B: :81 зелеет — баг формы СТАРЫЙ;
- ⇒ Linux-красный = эмит-сторона: тег arm1 резолвится first-wins
  (find_variant_compat) вместо скрутини-канала; утечка (import std.io)
  переупорядочила variant_to_sum → ErrorKind перехватил тег
  ParseTomlError.UnexpectedEof → RUN-FAIL.
- Недостающее звено: канал №279 (pattern_variant_types_buf) проверял
  сумму через types_get_here (только свой файл) — toml_test видит
  ParseTomlError ЧЕРЕЗ ИМПОРТ → канал молчал → first-wins.

ФИКС (0c733f124): канал видит импортированную сумму (fallback на
глобальный слот; sound — scrutinee-тип из объявленной сигнатуры,
D381-коллизии квалифицируются на эмит-стороне независимо). Conformance
501 PASS / 3 известных, 0 новых. Круг 11 арбитрирует toml на Linux.

## 2026-11-07 (12) — toml: канал-фикс не закрыл Linux; состояние расследования

Круг 11 (0c733f124, канал №279 видит импортированные суммы): toml на Linux
ВСЁ ЕЩЁ красный, текст тот же. Гипотезы опровергнуты точечными пробами:
- утечка→двусмысленность владельца варианта (кейстоун) — не полностью;
- сигнатуры Io (бисект-2) — нет;
- `err != None` (Option-сравнение с sum-payload) — РАБОТАЕТ (probe is-some=true).

Установленный твёрдый факт: Windows и Linux падают ИДЕНТИЧНЫМ текстом
(оба ассерта) — ОДИН дефект; Windows-репро детерминировано и локально.

Следующий диагностический ход (по §5.1 — не итерировать вслепую):
скопировать toml.nv + мини-тест в /tmp/probe/tomldiag/ как standalone
(там println ДОСТУПЕН — в std-модулях его нет!), инструментировать
парсер свободно (что возвращает parse_document на "k = \"unclosed",
какая ветка err срабатывает, что в Result), локально бисектить поведение.
Потом переносить корень в std.

Кандидат 0c733f124 остаётся на integrate с известным красным; пуш main
честно заблокирован гейтом до зелёного toml.

## 2026-11-07 (13) — томl: standalone-копия заведена; нюанс для продолжения

Копия toml.nv в /tmp/probe/tomldiag/ (module standalone.zz_probe) + mini.nv
peer: НЮАНС — extension `str @to_toml()` не виден peer-файлу после
переименования (E7320) — при продолжении проверить, что тянет
регистрацию extension-метода (export-форма на месте; возможно импорты
toml.nv отвалились при переименовании модуля). Инструментировать после
починки видимости; println в /tmp-копии доступен.

## 2026-11-07 (14) — toml: цепочка проб сузила корень до хендлера interrupt

Плэйбук-пробы (standalone-копия, panic-дискриминаторы):
1. parse_basic_string ВЫЗЫВАЕТСЯ (DIAG-pbs-enter).
2. Её C-код КОРРЕКТЕН: nova_make_ParseTomlError_UnexpectedEof(nova_self->line),
   err-проверка (opt_eq) и _nova_throw_typed — всё правильное.
3. Match-сторона: FELL-THROUGH (arm1 не срабатывает); `e is UnexpectedEof`
   → IS-NOT-EOF — пойманный Err НЕ UnexpectedEof.
4. Значит искажение МЕЖДУ throw и match: to_toml = `with Fail[ParseTomlError] =
   |e| interrupt Err(e) { ... }` — путь |e| → interrupt → Err(e) → Result —
   ПОДПОЗРЕВАЕМЫЙ: payload e в interrupt Err(e) (D65-механика interrupt из
   Fail-хендлера). Следующая проба: дискриминировать payload хендлера
   (interrupt Err(...) с маркерным значением? или is-проба НА хендлерном e
   внутри to_toml до interrupt).

Примечание: probe с `Some(UnexpectedEof { line: @line })` дал D52-отказ
(шортхенд обязателен) — канон подтверждён. Проба @-шортхенда в
изолированном методе дала CC-FAIL nova_self (отдельный находка —
@-шортхенд в variant-литерале внутри метода эмитит несуществующий
nova_self в НЕ-томл контексте; в toml-контексте self-символ другой,
компилируется, но семантика под вопросом) — задокументировано как
отдельный кандидат-дефект.

## 2026-11-07 (15) — toml КОРЕНЬ СУЖЕН: payload Fail-хендлера чужого типа

Цепочка проб (standalone-копия, panic-дискриминаторы):
1. parse_basic_string вызывается, его C корректен (ctor/eq/throw — проверены
   по C-артефакту), throw_typed(ParseTomlError*) выполняется.
2. Match: FELL-THROUGH; `e is UnexpectedEof` → IS-NOT-EOF.
3. Хендлер `with Fail[ParseTomlError] = |e| ...`: payload e не соответствует
   НИ ОДНОМУ варианту ParseTomlError (H-UNKNOWN) — payload ЧУЖОГО ТИПА.

⇒ КОРЕНЬ: механизм доставки payload Fail-хендлеру (D65/D13 fail-frame →
handler-literal fail-op) доставляет значение НЕ того типа: throw_typed
кладёт ParseTomlError*, хендлер читает не то (боксинг/маршаллинг
рассинхронизированы: сигнатура fail-op vs содержимое fail-frame).
Repro: standalone (toml.nv-копия + mini.nv) на Windows, ДЕТЕРМИНИРОВАННО,
вне toml-контекста воспроизводится shape'ом throw-в-with → match.

Это ОБЪЯСНЯЕТ всё: оба ассерта toml (парсер возвращает Err с чужим
payload → оба match'а падают в `_`), и Windows-пре-существенность
(дефект в fail-машинерии, не в toml). Linux-флип вторичен (CU-порядок
мог маскировать/раскрывать — неважно, корень один).

Следующий шаг: C-артефакт mini — inspect handler-literal fail-op
сигнатуру + throw_typed store (маршаллинг), найти рассинхрон; это
компиляторный дефект D65-машинерии, кандидат в состав 0.1.3 как
самостоятельный фикс (Fail-хендлеры — ядро языка).

## 2026-11-07 (16) — toml КОРЕНЬ В C: fail-op хендлера типизирован nova_str

Артефакт mini (probe_tomldiag_mini.c):
- `_nova_handler_lit_2_impl_Fail_fail(void* _ctx, nova_str e)` — параметр
  payload хендлер-литерала `|e|` ЭМИТИТСЯ КАК nova_str (строка!);
- `throw_typed_ParseTomlError` кладёт `Nova_ParseTomlError*`.

⇒ Хендлер-литерал `|e|` в `with Fail[E] = |e| ...` получил параметр
ДЕФОЛТНОЙ догадки (nova_str — семья ClosureLight 6h «params=nova_int
дефолт без контекста»), вместо payload типа E из with-контекста. Throw
кладёт ParseTomlError*, хендлер читает как str → тег мусорный →
H-UNKNOWN → match падает в `_` → оба ассерта toml.

КОРЕНЬ (компиляторный, D65-машина): типирование параметра хендлер-
литерала из expected Fail[E] payload — канал должен течь из with-
заголовка в closure-параметры литерала. Фикс-локус: эмиссия/чекер
хендлер-литералов (:13347 handler_id; ClosureLight-каналы). Кандидат
в состав 0.1.3 как самостоятельный фикс (Fail-хендлеры — ядро).
Repro: /tmp/probe/tomldiag (standalone, детерминированный).

## 2026-11-07 (17) — toml: третье звено локализовано ТОЧНО (свежий C)

Mini's match (C, строки ~10400): Result тип ВЕРНЫЙ
(NovaRes_Nova_TomlValue_p_Nova_ParseTomlError_p), внешний тег Err верный,
но ВНУТРЕННИЙ тег: `_nv_scr_1112->payload.Err._0->tag ==
NOVA_TAG_ErrorKind_UnexpectedEof` — ЧУЖАЯ константа.

⇒ Паттерн-цепь pattern_cond (scr_ty → канал №279 → first-wins) промахнулась
двумя ПЕРВЫМИ ступенями: scr_ty темпа payload Err зарегистрирован
ПЕРВО-wins'ом при ПРИВЯЗКЕ (UnexpectedEof → ErrorKind после утечки — у
ErrorKind ЕСТЬ одноимённый вариант, поэтому scr_has_variant=TRUE и
авторитетная цепь честно вернула отравленный scr_ty). Канал №279 не
спасает (не вызывается — scr_ty «сработал»).

ФИКС (следующий шаг, по существу): привязка payload результата в
pattern_bind_typed / match-binding обязана брать тип Err-payload из
ОБЪЯВЛЕННОГО типа скрутини (Result[_, ParseTomlError] известен!), а не
first-wins по имени варианта. Это то же семейство «модуль-локальное/объявленное
побеждает first-wins». После фикса: mini зелёный → toml_test зелёный →
круг CI → пуш main.
