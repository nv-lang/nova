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

## Прогресс по группам

(заполняется по мере разбора)
