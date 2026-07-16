# Plan 200 — цепочка duration (Шаг 0 → П10 → П12 → П13) — прогресс

Worktree: `d:/Sources/nv-lang/nova-200dur`, ветка `p200-duration-chain`, база main `49c4e8297`.
Модель: sonnet. **Хост перезагружался в середине волны** — сессия оборвалась, фоновые verify-сборки
потеряны (файлы на диске уцелели). Этот файл — чекпоинт восстановления + текущий статус.

## Шаг 0 — clamp-бланкет — ✅ РЕАЛИЗОВАНО, НЕ ВЕРИФИЦИРОВАНО (сборка/тест ещё не прогнаны после рестарта)

- `std/src/prelude/protocols.nv`: добавлен `fn[T Ints] T @clamp(lo T, hi T) -> T` (после `saturating_mul`,
  тот же стиль doc/`#stable`, что 206-семья).
- `std/src/runtime/defaults.nv`: retract конкретного `int @clamp` (коллизия с новым бланкетом для T=int);
  `f64 @clamp` оставлен нетронутым.
- `spec/decisions/08-runtime.md` (D74): амендмент — `@clamp` теперь бланкет над `Ints`, ретракт
  конкретного `int @clamp`.
- Тесты: `spec_tests/conformance/d74_clamp_ints_blanket_dispatch.nv` (НОВЫЙ, untracked) — dispatch-коллизия
  (конкретный `D74ClampFoo.clamp` в том же CU) + width-boundary фикстура (i64/u32/i8). Inline-тест —
  `std/src/math/overflow_policy_test.nv` (2 новых `test` блока, clamp multi-width + lo>hi edge).
- **НЕ СДЕЛАНО ДО РЕСТАРТА:** ни один `nova test`/сборка компилятора не прогнаны с начала сессии
  (машина была под тяжёлым CPU-contention от параллельных агентов — один test-run завис на 90+ сек
  CPU-time за 40+ минут wall-clock). Гейт шага 0 (фикстура зелёная + `nova test std/src/time` из
  СТАРОГО worktree p200dur читает clamp) — ОТКРЫТ, делать после рестарта сессии.

## Шаг 1 (Пункт 10) — i64-хелперы → встроенные — ✅ РЕАЛИЗОВАНО, НЕ ВЕРИФИЦИРОВАНО

`std/src/time/duration.nv`: `i64_max()`/`i64_min()`/`clamp_i64()` удалены (→ `i64.MAX`/`i64.MIN`/`@clamp`);
`checked_add_i64`/`checked_sub_i64`/`checked_mul_i64` wrapper-функции удалены (call-сайты → `a.checked_add(b)`
напрямую). `checked_neg_i64`/`checked_div_i64` оставлены (нет бланкета neg/div). `sat_add/sub/mul_i64`
оставлены (кастомный `[lo,hi]`). Гейт: `nova test std/src/time` — ЕЩЁ НЕ ПРОГНАН.

## Шаг 2 (Пункт 12, вбирает 11) — единицы времени getter=голое/конструктор=to_ — В ПРОЦЕССЕ

**Сделано** (в `std/src/time/duration.nv`, самодостаточно):
- Getters: `@as_nanos/micros/millis/secs/mins/hours/days()` → голые `@nanos/micros/millis/seconds/
  minutes/hours/days()`; `@as_secs_f64/millis_f64` → `@seconds_f64/millis_f64`; Timestamp `@as_unix_
  secs/millis/nanos` → `@unix_seconds/unix_millis/unix_nanos`; Monotonic `@as_nanos` → `@nanos`.
- Конструкторы: `Duration.from_*` (nanos/micros/millis/secs/mins/hours/days/weeks/secs_f64) ПОЛНОСТЬЮ
  retracted → бланкет `fn[T Ints] T @to_nanos/to_micros/to_millis/to_seconds/to_minutes/to_hours/
  to_days/to_weeks() -> Duration`; `f64 @to_seconds()` (единственный float-конструктор — «только
  секунды», f64 @nanos/micros/millis/minutes/hours/days RETRACTED без замены, repo-wide grep на
  использование = 0). `Timestamp.from_unix_*` retracted → `fn[T Ints] T @to_unix_seconds/to_unix_millis/
  to_unix_nanos() -> Timestamp`. `Duration.try_from_secs_f64` ОСТАВЛЕН (fallible, не `from_*`-паттерн,
  не в scope ретракта) — внутреннее тело переключено на `n.to_nanos()`.
  Singular-алиасы `int @second/minute/hour/day/week()` УБРАНЫ без замены (DRY, `1.to_seconds()` и т.п.).
- Все внутренние call-сайты (arithmetic operators, checked_*, saturating_*, abs, Timestamp/Monotonic
  operators, ВСЕ inline-тесты внутри duration.nv) мигрированы на новые формы. Файл **самодостаточен**
  (repo grep внутри файла на `Duration.from_`/`Timestamp.from_unix`/`@as_`/singular = 0, кроме
  explanatory-комментариев).
- D-амендмент D410/D317 — упомянут в комментариях duration.nv, но **ЕЩЁ НЕ ВНЕСЁН в spec/decisions/
  03-syntax.md (D410) / 04-effects.md (D317)** формально — TODO перед коммитом шага 2.

**Call-site миграция ВНЕ duration.nv — ЧАСТИЧНО:**
- ✅ `std/src/time/civil/zoned.nv` (Duration.from_secs→to_seconds, @as_unix_nanos→@unix_nanos,
  Timestamp.from_unix_nanos→to_unix_nanos)
- ✅ Комментарии-only (без функционального кода): `std/src/concurrency/timer.nv`, `std/src/runtime/
  sync.nv`, `std/src/time/civil/period.nv`, `examples/flagship/aggregator/src/app/aggregate.nv`,
  `spec_tests/conformance/neg/f1_close_after_int_neg.nv`
- ✅ Компилятор: `compiler-codegen/src/codegen/emit_c.rs` (E5101 fix-it текст + "expected Duration
  argument" диагностика — 2×2 места) и `nova-cli/src/bin/migrate_plan65.rs` (весь генератор +
  тесты) — ОБА обновлены на `N.to_millis()`/`N.to_seconds()` вместо `Duration.from_millis/from_secs_f64`.

**❌ ЕЩЁ НЕ МИГРИРОВАНО (реальный код, не только комменты) — TODO следующим проходом:**
- `std/src/time/civil/civil_test.nv`, `datetime.nv`, `time_of_day.nv` (singular `1.hour()` и
  `Duration.from_nanos(0)`/`Duration.from_days`/`Timestamp.from_unix_nanos` внутри арифметики)
- `std/src/time/overflow_safe_test.nv`, `std/src/time/rt/dur_add_overflow_traps.nv`,
  `std/src/time/rt/dur_div_zero_traps.nv`, `std/src/time/timer_metrics_test.nv`,
  `std/src/time/units_test.nv`, `std/src/time/value_typed_surface_test.nv`
- `std/src/concurrency/retry.nv`, `std/src/concurrency/supervised_deadline_test.nv`
  (`Duration.from_nanos`/`Duration.from_secs`/`Duration.from_millis` в реальном коде, не комментах)
- `examples/flagship/aggregator/src/app/aggregate_test.nv`, `live_test.nv`, `src/main.nv`,
  `regressions/monotonic_now_bare_binding/*.nv`, `regressions/spawn_throw_multifield_payload/*.nv`
  (все — `Duration.from_millis(N)`/`Duration.from_secs(N)` с литералами — механическая замена на
  `N.to_millis()`/`N.to_seconds()`)
- `spec_tests/conformance/d317_duration_overflow_policy.nv`,
  `spec_tests/conformance/standalone/d316_time_effect_typed_surface.nv`,
  `spec_tests/conformance/v2_condvar_tuple_newtype_ok.nv`,
  `spec_tests/soundness/plan206_d317_duration_checked_saturating.nv`

**Гейт шага 2 (ЕЩЁ НЕ ПРОЙДЕН):** `nova test std/src/time` зелёный; грепы `@as_`/`Duration.from_`/
`Timestamp.from_unix`/bare-fluent (`int @second()` и т.п.) = 0 ПО ВСЕМУ РЕПО (std/examples/spec_tests).

## Шаг 3 (Пункт 13) — разбить duration.nv на Timestamp/Monotonic файлы — НЕ НАЧАТО

## Дисциплина восстановления
Все правки Шага 0/1/2(частично) лежат ВПЕРЕМЕШКУ в одних и тех же файлах (duration.nv центральный,
редактировался непрерывно поверх Step 0→1→2 без промежуточных коммитов из-за CPU-contention +
рестарта хоста). **Решение:** один WIP-checkpoint коммит на всё текущее состояние (честно
промаркирован как незавершённый шаг 2), затем — довести Шаг 2 (call-site миграция) отдельным
коммитом, гейт → коммит; далее Шаг 3 отдельно.
