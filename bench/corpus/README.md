# Bench corpus — frozen reference programs

> Plan 57 §3 L8 — canonical corpus для compiler performance measurement.

Эти файлы — **frozen reference programs** для measuring compile-time
(parse + type-check + mono-pass + codegen + C-compile). Изменения требуют
explicit acknowledgement (commit msg `corpus: update X — reason`).

Не trivially-correlated с runtime benches в `../micro/` — те measure
behaviour generated code, а эти — performance компилятора самого.

## Files

| File | LOC | Что measures |
|---|---|---|
| `01_hello.nv` | ~5 | Минимум compile-time (parse + emit overhead) |
| `02_arithmetic_loop.nv` | ~30 | Tight loop codegen (int + float) |
| `03_generic_heavy.nv` | ~150 | Mono-pass stress (10 generic types) |
| `04_effects_handlers.nv` | ~100 | Handler dispatch codegen (5 effects) |
| `05_channels_select.nv` | ~80 | Channel runtime calls + select |
| `06_contracts.nv` | ~60 | Type-check + (optional) SMT cost |
| `07_collection.nv` | ~250 | Realistic medium module |
| `10_massive_match.nv` | ~100 | Match codegen on 30-variant sum |

## Usage

Эти файлы вызываются compiler-bench harness (Plan 57.A — TBD) либо
manually через `nova check <file>` / `nova build <file>` с PerfTimer.

Per-pass breakdown (parse / type-check / mono-pass / codegen / c-compile /
link) — Plan 57.A через PerfTimer hooks.
