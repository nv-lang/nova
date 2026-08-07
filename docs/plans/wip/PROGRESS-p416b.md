# p416b — nova lint --deny spec_tests: 83 → 0

Реестр 221.1 №416 (хвост). Прецедент: p416 вычистил `std` (11 находок: 7 фикса + 4
`nova:allow`). Здесь — `spec_tests` (корпус проверки языка, не обычный код).

## Шаг 0 — измерение (ДО правок)

Команда:
```
d:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe lint --deny spec_tests
```
Итог: `lint: 1388 file(s), 83 finding(s), 83 denied (--deny, exit 1)`.
Полный вывод сохранён в `docs/plans/wip/lint_spec_tests_raw.log` (86 строк).

### Разбивка по правилам

| Код правила | Кол-во | Смысл (кратко) |
|---|---|---|
| `W_COERCE_EXPLICIT_REDUNDANT` | 34 | явный `.bytes()`/`.into_str()` там, где `#coerce` (D429 R6/R9) уже даёт тот же результат |
| `W_MANUAL_COALESCE` | 11 | ручной `match Some/Ok(v)=>v, None/Err(_)=>D` вместо канона `X ?? D` (D86) |
| `W_REDUNDANT_CONST_TYPE_ANNOTATION` | 10 | аннотация типа у константы избыточна (тип и так выводится из литерала) |
| `W_STR_CONCAT_METHOD` | 7 | `.concat(...)` на `str` вместо интерполяции `"${a}${b}"` |
| `W_REDUNDANT_CONSUME_REBIND` | 7 | `consume x = y` при том, что `y` — уже `consume`-биндинг из паттерна той же ветки |
| `W_CONSUME_NAKED_NAME` | 5 | consume-конверсия в другой тип названа голым именем вместо `@into_*()` |
| `W_WHILE_COUNTER_FOR_RANGE` | 2 | счётчик-based `while i < n` вместо канона `for i in a..n` |
| `W_NON_COMPOUND_ASSIGN` | 2 | `@n = @n + …` вместо составной формы `@n += …` |
| `W_MANUAL_COLLECT` | 2 | ручной `mut v = <empty>` + `for … { v.push(x) }` вместо `.collect()` |
| `W_RESULT_DISCARDED` | 1 | swallow-match `Err(_) => ()` без обработки |
| `W_REDUNDANT_OF` | 1 | `Vec[int].of(...)` там, где литерал `[...]` дал бы тот же тип |
| `W_MANUAL_SLICE_TO_END` | 1 | `recv[0..recv.len()]` вместо открытого диапазона `recv[..]` |
| **Итого** | **83** | |

### Разбивка по файлам (41 файл затронут)

Полный список файл→строка→правило — в `lint_spec_tests_raw.log`. Файлы с >1 находкой:
`d374_write_sink_decouple.nv`(9), `d229_debug_format_spec.nv`(6),
`m248_named_tuple_interp_display_dispatch_pos.nv`(6), `d179_stringbuilder_cross_fn_consume.nv`(3),
`d200_associated_const.nv`(3), `d422_unified_display_dispatch.nv`(3), `gc_forced_collect.nv`(3),
`m2211_38_sequential_supervised_accept_stale_deadline.nv`(3),
`fixtures/known_red/t4_sqlite_e2e_ok.nv`(3), `standalone/hunt_str_concat_operator.nv`(4, все на
ОДНОЙ строке 10 — вероятно 4 повтора `.concat` в одном выражении),
`d129_match_arm_width_widen.nv`, `d186_interp_no_display_pos.nv`, `d209_protocol_method_at_recv_mut.nv`,
`effect_default_handler_no_with.nv`, `generic_match_scope_gap.nv`,
`standalone/m2211_108_main_fiber_accept.nv`, `standalone/m222_7_spawn_ctx_capture_mut_param.nv`,
`standalone/m240_detach_box_while_loop_read_after.nv`, `to_str_facade_collision.nv` (по 2). Остальные 22
файла — по 1 находке.

### Предварительная стратегия по группам

Все находки — в `conformance/` (проверка результата поведения) или `fixtures/known_red/` (заведомо
красная фикстура на другой дефект). Гипотеза ДО чтения файлов (проверяется по каждому индивидуально):

- `W_COERCE_EXPLICIT_REDUNDANT` (34, `.bytes()`/`.into_str()`) — подозрение: часть фикстур ИМЕННО
  проверяет явный вызов coerce-цели (conformance на D429). Нужно читать каждую: если тест про
  «работает и с явным вызовом», это может быть предметом теста → `nova:allow`; если явный вызов
  случаен (просто так писали) → канонизировать.
- `W_MANUAL_COALESCE` (11) — часть фикстур может проверять именно ручной `match` (например,
  d129_match_arm_width_widen — «arm width widen», сам ручной match может быть предметом). Читать
  индивидуально.
- `W_REDUNDANT_CONST_TYPE_ANNOTATION` (10) — вероятно небрежность (аннотация на константе), но
  `t4_sqlite_e2e_ok.nv` — это known_red (уже не PASS), нужно проверить не изменит ли правка её
  текущий (красный) статус по другой причине.
- `W_REDUNDANT_CONSUME_REBIND` (7) — вероятно небрежность, безопасно править.
- `W_CONSUME_NAKED_NAME` (5) — конвенция именования `@into_*`; ИЛИ фикстура нарочно проверяет
  «голое имя» (naming conformance на нарушение) → тогда allow. Читать каждую.
- `W_STR_CONCAT_METHOD` (7) — вероятно небрежность, но `hunt_str_concat_operator.nv` подозрительное
  имя — возможно ИМЕННО тестирует `.concat()`. Читать.
- Остальные (по 1-2) — читать индивидуально, решение по месту.

Известные ловушки (из брифа): `?? throw` в позиции обязательного покрытия ошибок ломает сборку —
дефект №417 (`expr_has_throw` не спускается в `Coalesce`); если наткнусь — `nova:allow` со ссылкой,
воевать не буду. `target/.nova-cache` чистить перед сравнением build с/без флага — ключ кэша не
учитывает флаги кодогена (№415).

---

## Итог — 83 → 0

Чекпоинты (порядок исполнения, каждый — отдельный коммит):
1. `674bd8cff` — измерение (этот файл).
2. `353c0276f` — W_REDUNDANT_CONSUME_REBIND (7) + W_RESULT_DISCARDED (1). 83→76.
3. `21a8ed7ae` — W_CONSUME_NAKED_NAME (5). 76→71.
4. `0173bb7c7` — W_COERCE_EXPLICIT_REDUNDANT (34). 71→37.
5. `bbd8ff0a2` — доловлен пропущенный W_REDUNDANT_CONSUME_REBIND (instance B,
   `replace_all` не матчил из-за другого отступа во втором тестовом блоке). 37→36.
6. `bb24fda5c` — W_MANUAL_COALESCE (11); по пути поймал и починил 2 сломанных
   `nova:allow` (маркер-строка стояла ПЕРВОЙ в многострочном комментарии, а не
   последней перед находкой — то же семейство дефекта, что нашло окно p416 на std).
   36→25.
7. `9f00b04ba` — W_REDUNDANT_CONST_TYPE_ANNOTATION (10). 25→15.
8. `e14149bd7` — W_STR_CONCAT_METHOD (7). 15→8.
9. `76c21a66d` — хвост: W_MANUAL_COLLECT (2), W_NON_COMPOUND_ASSIGN (2),
   W_REDUNDANT_OF (1), W_MANUAL_SLICE_TO_END (1), W_WHILE_COUNTER_FOR_RANGE (2). 8→0.
10. `57f2e25fe` — шаг в `scripts/gate.sh` (по образцу `nova lint --deny std/src`).

### Приёмка 1 — `nova lint --deny spec_tests` → 0 находок (дословно)

```
lint: 1388 file(s), 0 finding(s), 0 denied (--deny, exit 1)
EXIT=0
```

### Приёмка 2 — вердикты фикстур не изменились

Для каждого из 41 затронутого файла прогнан `nova check <файл>` ДО (оригинал
подставлен на то же место в дереве — `git show <base>:<путь> > <путь>`, где
`<base>` = `8dea33297` для файлов, существовавших на момент старта окна p416b,
или `d4f4915a6` — фактический базовый коммит этого worktree — для 3 файлов,
появившихся между двумя коммитами: `m248_named_tuple_interp_display_dispatch_pos.nv`,
`fiber_safety_seed_pos.nv`, `standalone/m240_detach_box_while_loop_read_after.nv`)
и ПОСЛЕ правки. Во всех 41 случаях: `PASS`/`FAIL`/`WARN`-счётчики совпали (типично
`PASS: 1 FAIL: 0 WARN: 67` для conformance-CU или `PASS: 1 FAIL: 0` для standalone-
файлов; единственное исключение — `fixtures/known_red/t4_sqlite_e2e_ok.nv`, где
`FAIL: 1` сохранился с ТЕМ ЖЕ кодом ошибки `E_D78_MODULE_PATH_MISMATCH`,
не связанным с правкой). Полный лог сравнений — в истории коммитов
(сообщения чекпоинтов 2–9 выше).

Ограничение: полный `nova test`/`nova build` (реальный runtime-прогон assert'ов)
не запускался для затронутых файлов — попытка через env-переменные
(`NOVA_STD_PATH`/`NOVA_RT_DIR`/…, см. `project-worktree-nova-test-setup`) зависала
на build без вывода (>1-3 мин, несколько попыток) даже для одного файла; дальше не
эскалировал — CPU-дисциплина брифа явно запрещает мега-CU, а зависшая точечная
сборка того же типа риска не оправдывает. Верификация — только `nova check`
(type-check) parity + текстовое обоснование по каждой находке (что именно
проверяет фикстура и почему замена/nova:allow не меняет её поведение).

### Приёмка 3 — проба «подсунь заведомо негодное»

Временно вернул снятую аннотацию `const RAW = 300` → `const RAW int = 300`
(W_REDUNDANT_CONST_TYPE_ANNOTATION) в `standalone/hunt_const_to_const.nv`,
прогнал новый шаг гейта:

```
lint spec_tests :: lint: 1388 file(s), 1 finding(s), 1 denied (--deny, exit 1)
GATE FAIL: nova lint spec_tests: находки > 0, ожидался канон 0 (см. ...): 'lint: 1388 file(s), 1 finding(s), 1 denied (--deny, exit 1)'
GATE FAIL: nova lint --deny spec_tests: exit=1 (см. ...)
```

Шаг корректно покраснел. Файл возвращён обратно (`git diff` на него пуст),
`nova lint --deny spec_tests` снова 0 находок.

### Приёмка 4 — шаг в `scripts/gate.sh`

Добавлен после существующего шага `nova lint --deny std/src` (симметричная
структура: ассерт строки `N finding(s)` присутствует, ассерт `0 finding(s)`,
ассерт кода возврата — три отдельные проверки, не одна голая `exit code`).
Шапка состава гейта (комментарий в начале файла) дополнена пунктами 4–5
(было 4 пункта без явного упоминания линта, стало 6).

### Приёмка 5 — таблица находок

| Файл | Правило (кол-во) | Что сделано |
|---|---|---|
| `m_boxcap_detach_cancel_token_pos.nv` | W_REDUNDANT_CONSUME_REBIND (1) | правка: bind напрямую в `Ok(consume X)` |
| `standalone/m2211_108_main_fiber_accept.nv` | W_REDUNDANT_CONSUME_REBIND (2) | правка |
| `standalone/m2211_38_sequential_supervised_accept_stale_deadline.nv` | W_REDUNDANT_CONSUME_REBIND (3), W_RESULT_DISCARDED (1) | правка ×3 (вкл. пропущенный instance-B); `nova:allow` для намеренного swallow (read-error≡no-data, предмет теста — deadline, не error-handling) |
| `standalone/m222_7_spawn_ctx_capture_mut_param.nv` | W_REDUNDANT_CONSUME_REBIND (2) | правка |
| `d131_consume_qualifier.nv` | W_CONSUME_NAKED_NAME (1) | правка: `finish` → `into_finish` |
| `d209_protocol_method_at_recv_mut.nv` | W_CONSUME_NAKED_NAME (1), W_MANUAL_COLLECT (1) | правка: `close` → `into_close`; `nova:allow` для for-in (предмет теста D209 коллекция (1)) |
| `d326_mode_overload_axis.nv` | W_CONSUME_NAKED_NAME (1) | правка: `sink` → `into_sink` |
| `samename_extension_recv_dispatch.nv` | W_CONSUME_NAKED_NAME (1) | `nova:allow` — одноимённый метод на 3 ресиверах ЕСТЬ предмет теста (Plan 92) |
| `standalone/m176_method_return_turbofish.nv` | W_CONSUME_NAKED_NAME (1) | `nova:allow` — имя `into` дословно из подтверждённого репро в шапке файла |
| `d179_stringbuilder_cross_fn_consume.nv` | W_COERCE_EXPLICIT_REDUNDANT (3) | правка: убран `.into_str()` на tail-return |
| `d186_interp_no_display_pos.nv` | W_COERCE_EXPLICIT_REDUNDANT (2) | правка: убран `.bytes()` на строковых литералах |
| `d229_debug_format_spec.nv` | W_COERCE_EXPLICIT_REDUNDANT (6) | правка |
| `d268_opt_in_conformance_impl.nv` | W_COERCE_EXPLICIT_REDUNDANT (1) | правка: `.into_str()` на tail-return |
| `d374_write_sink_decouple.nv` | W_COERCE_EXPLICIT_REDUNDANT (9) | правка (+ поправлен устаревший комментарий, ошибочно утверждавший необходимость `.bytes()`) |
| `d422_unified_display_dispatch.nv` | W_COERCE_EXPLICIT_REDUNDANT (3) | правка |
| `effect_default_handler_no_with.nv` | W_COERCE_EXPLICIT_REDUNDANT (2) | правка |
| `m248_named_tuple_interp_display_dispatch_pos.nv` | W_COERCE_EXPLICIT_REDUNDANT (6) | правка |
| `p176repro_generic_wrapper_valuerecord_err.nv` | W_COERCE_EXPLICIT_REDUNDANT (1) | правка |
| `p176repro_result_valuerecord_err.nv` | W_COERCE_EXPLICIT_REDUNDANT (1) | правка |
| `c_keyword_ident_mangling.nv` | W_MANUAL_COALESCE (1) | `nova:allow` — match-arm binding буквально названа C-keyword'ом (сам предмет теста) |
| `d129_match_arm_width_widen.nv` | W_MANUAL_COALESCE (2) | `nova:allow` — унификация ширины match-арм ЕСТЬ предмет теста, `??` не трогает тот код-путь |
| `d162_consume_defer_cover.nv` | W_MANUAL_COALESCE (1) | `nova:allow` — throw внутри match-ветки предмет теста + открытый дефект №417 |
| `d55_bytes_lit_type_directed.nv` | W_MANUAL_COALESCE (1) | правка |
| `d55_const_bytes_lit.nv` | W_MANUAL_COALESCE (1) | правка |
| `fnarg_option_elem.nv` | W_MANUAL_COALESCE (1) | правка |
| `generic_match_scope_gap.nv` | W_MANUAL_COALESCE (1), W_NON_COMPOUND_ASSIGN (1) | `nova:allow` для match на generic-bound `@next()` (предмет теста — scope-gap); правка `@i += 1` |
| `m_ice_channel_reader_try_recv_binding_pos.nv` | W_MANUAL_COALESCE (1) | правка |
| `to_str_facade_collision.nv` | W_MANUAL_COALESCE (2) | правка |
| `d200_associated_const.nv` | W_REDUNDANT_CONST_TYPE_ANNOTATION (3) | правка (скалярные assoc-const'ы; составные НЕ трогал — не были находкой) |
| `standalone/f3_generic_body_const_kept.nv` | W_REDUNDANT_CONST_TYPE_ANNOTATION (1) | правка |
| `standalone/hunt_const_in_method.nv` | W_REDUNDANT_CONST_TYPE_ANNOTATION (1) | правка |
| `standalone/hunt_const_to_const.nv` | W_REDUNDANT_CONST_TYPE_ANNOTATION (1) | правка |
| `standalone/hunt_const_via_callchain.nv` | W_REDUNDANT_CONST_TYPE_ANNOTATION (1) | правка |
| `fixtures/known_red/t4_sqlite_e2e_ok.nv` | W_REDUNDANT_CONST_TYPE_ANNOTATION (3) | правка (FAIL остаётся, причина не связана — module-path) |
| `gc_forced_collect.nv` | W_STR_CONCAT_METHOD (3) | правка: `.concat()` → интерполяция (тоже строит динамически, GC-тест валиден) |
| `standalone/hunt_str_concat_operator.nv` | W_STR_CONCAT_METHOD (4, одна строка) | `nova:allow` — явный chained `.concat()` ЕСТЬ предмет DCE-охоты (Plan 159 Ф.1) |
| `fiber_safety_seed_pos.nv` | W_NON_COMPOUND_ASSIGN (1) | правка: `@n += 1` |
| `followup_slice_elision_pos.nv` | W_MANUAL_SLICE_TO_END (1) | `nova:allow` — именно форма `v[0..v.len()]` (не `v[..]`) есть предмет теста (bounds-elision) |
| `p106_launder_pattern_bind_ok.nv` | W_REDUNDANT_OF (1) | правка: `Vec[int].of(1,2,3)` → `[1,2,3]` |
| `standalone/m240_detach_box_while_loop_read_after.nv` | W_WHILE_COUNTER_FOR_RANGE (2) | `nova:allow` — counted `while` есть точная форма репро №240 (уже объяснено в коде) |
| `d251_str_surface.nv` | W_MANUAL_COLLECT (1) | `nova:allow` — задокументированный codegen-пробел на CharsIter (D410), маркер добавлен к готовому обоснованию |

Итого: 41 файл, 83 находки. Правкой закрыто ~55, `nova:allow`-ом (с обоснованием
в комментарии, единичной строкой над находкой) — ~28 (в основном крупные группы:
5 W_CONSUME_NAKED_NAME частично, 5 W_MANUAL_COALESCE находок, 4 W_STR_CONCAT_METHOD
на одной строке, 2 W_WHILE_COUNTER_FOR_RANGE, 1 W_MANUAL_COLLECT ×2, 1
W_MANUAL_SLICE_TO_END, 1 W_RESULT_DISCARDED).
