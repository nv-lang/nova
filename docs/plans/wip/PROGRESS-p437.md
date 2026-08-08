# №437 — авторитетный гейт не завершается: где на самом деле уходит время

Окно `p437-checker-perf`, ветка `p437-checker-perf`, worktree `d:/Sources/nv-lang/nova-p437`.
Модель: **opus** (claude-opus-5).

## Шаг 1. Гипотеза о `fail_reach` (№428) — ОПРОВЕРГНУТА замером

### Как мерил

Выборка фиксированная и воспроизводимая:

```
ls spec_tests/conformance/*.nv | sort | head -N > /tmp/fN.txt
NOVA_PERF=1 ./nova-cli/target/release/nova.exe check $(cat /tmp/fN.txt | tr '\n' ' ')
```

`NOVA_PERF` — новый env-флаг (этим окном): пофазные таймеры внутри
`check_module_impl` (`compiler-codegen/src/types/mod.rs`, `struct PerfPhase`).
Выключен по умолчанию, стоимость при выключенном — один `env::var` на compile-unit.

Важное наблюдение о форме прогона: `nova check f1 … fN` — это **N отдельных
compile-unit'ов**, и в каждом `module.items` уже плоский (prelude + std влиты):
**8878 items, 4149 функций** на один самый простой conformance-файл. То есть
стоимость «на файл» почти целиком — это стоимость прохода по слитому prelude+std,
а не по самому файлу.

### Замеры общего времени (бинарь HEAD = 70dca84ef, до правок)

| файлов | wall | на файл |
|---|---|---|
| 10 | 90.6 с | 9.1 с |
| 20 | 287.6 с | 14.4 с |
| 30 | не уложился в остаток 10-мин окна | — |
| 100 | снят через 30 мин (не завершился; RSS 6.4 ГБ, 4527 CPU-с) | — |

### Пофазная разбивка, 10 файлов = 10 compile-unit'ов (сумма по всем CU)

| фаза | сумма, мс | доля |
|---|---|---|
| **`MapLitCtx::check_module` (Plan 52 Ф.2, D108)** | **484 310** | **~85 %** |
| `verify_module` (SMT) + хвост post-check | 48 262 | 8 % |
| `NameResCtx::build` | 11 149 | 2 % |
| `TypeCheckCtx::check_module` (основной вывод типов) | 10 596 | 2 % |
| pre-passes (скан деклараций) | 4 189 | <1 % |
| `check_consume` (D131) | 2 956 | <1 % |
| `check_no_copy_second_name` | 1 262 | <1 % |
| `fiber_safety::run` (238 Ф.1) | 1 032 | <1 % |
| `fiber_safety::check_seed_points` | 615 | <1 % |
| `fiber_safety::check_param_passing` | 532 | <1 % |
| `check_unsafe_context_in_module` | 461 | <1 % |
| **`fail_reach::run` (№428, главный подозреваемый)** | **361** | **0,06 %** |
| `MapLitCtx::build` | 402 | <1 % |
| остальные 15 фаз | < 350 каждая | — |

**Вердикт: гипотеза опровергнута.** `fail_reach::run` — 36 мс на compile-unit,
0,06 % времени. Он не может быть причиной ни таймаута гейта, ни 400 секунд на
100 файлах. Виновник — `MapLitCtx::check_module`, **49 секунд на один
compile-unit**.

### Корень внутри `MapLitCtx::check_module` — замерен, не предположен

Точечный зонд (`items=`/`per-fn-ctx-clone=` в том же `NOVA_PERF`-выводе):

```
[perf]   MapLitCtx: items=8878 fns=4149 per-fn-ctx-clone=31545.3ms |
         maps: type_methods=729 fn_param_types=1292 method_param_types=2435
               unique_method_param_types=713 method_receiver_generic_names=2598
               record_field_types=912 wrap_types=1270 coerce_pairs=3
[perf]      48917.4ms  MapLitCtx::check_module
```

`check_module` на **каждой** функции модуля строит новый `MapLitCtx`, **глубоко
копируя все 13 разделяемых карт** (`type_methods`, `method_param_types`,
`record_field_types`, `wrap_types`, `method_receiver_generic_names`, …) — только
ради одного поля `fn_generics`, которое единственное и меняется per-fn.

* 4149 функций × ~10 000 записей карт (каждая — `String`/`TypeRef` с кучей) —
  **31,5 с из 49 с чистого копирования**, остальное — сам обход, тормозящий на
  аллокациях и промахах кэша.
* Сложность: **O(F · M)**, где `F` — число функций compile-unit'а, `M` — суммарный
  размер разделяемых карт. Обе величины растут с размером слитого CU, то есть по
  сути **квадратично по размеру compile-unit'а**. На мега-CU (1155 файлов одним
  юнитом) обе стороны произведения кратно больше — отсюда «не завершается».

## Шаг 2 — атрибуция по слияниям (в работе)

## Шаг 3 — фикс (в работе)
