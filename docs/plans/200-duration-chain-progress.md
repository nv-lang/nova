# Plan 200 — цепочка duration (Шаг 0 → П10 → П12 → П13) — прогресс (ФИНАЛ этой волны)

Worktree: `d:/Sources/nv-lang/nova-200dur`, ветка `p200-duration-chain`, база main `49c4e8297`.
Модель: sonnet. Сессия прерывалась дважды (рестарт хоста + два watchdog-таймаута на CPU-contention
от параллельного гейта интегратора) — работа велась через checkpoint-коммиты. **С определённого
момента действует жёсткий запрет на запуск `nova.exe`** (CPU занят авторитетным гейтом интегратора):
дальнейшие правки — чистым Read/Grep/Edit без верификации компиляцией. Это явно помечено ниже per-site.

## Шаг 0 — clamp-бланкет — ✅ СДЕЛАНО

- `std/src/prelude/protocols.nv`: `fn[T Ints] T @clamp(lo T, hi T) -> T` (после `saturating_mul`, стиль
  206-семьи).
- `std/src/runtime/defaults.nv`: retract конкретного `int @clamp` (коллизия с бланкетом при T=int);
  `f64 @clamp` не тронут.
- `spec/decisions/08-runtime.md` (D74) — амендмент про бланкет + ретракт `int @clamp`.
- Тесты: `spec_tests/conformance/d74_clamp_ints_blanket_dispatch.nv` (dispatch-коллизия с конкретным
  `D74ClampFoo.clamp` в том же CU + width-boundary i64/u32/i8); 2 inline-теста в
  `std/src/math/overflow_policy_test.nv`.
- **Верификация:** `nova check`/`nova test` на эту фикстуру запускался (внутри `spec_tests/conformance` —
  единый mega-CU, ~975 файлов, компиляция заняла много wall-clock времени под contention и была прервана
  до финального результата) — **прямого PASS/FAIL по самой фикстуре не получено**. Косвенно уверенность
  высокая: паттерн идентичен уже смёрженному `primitive_bounded_blanket_dispatch.nv` (196.8/196.9), а
  `nova check std/src/time`/`std/src/concurrency` (см. Шаг 2) зелёные и используют тот же `@clamp`-путь
  транзитивно через `sat_add_i64`/`clamp`-семью. `[M-200-duration-chain-verify]`: сама d74-фикстура — точно
  подтвердить в авторитетном гейте.

## Шаг 1 (Пункт 10) — i64-хелперы → встроенные — ✅ СДЕЛАНО, ПРОВЕРЕНО `nova check`

`std/src/time/duration.nv`: `i64_max()`/`i64_min()`/`clamp_i64()` удалены → `i64.MAX`/`i64.MIN`/`@clamp`;
`checked_add_i64`/`checked_sub_i64`/`checked_mul_i64` wrapper-функции удалены, call-сайты →
`a.checked_add(b)` напрямую. `checked_neg_i64`/`checked_div_i64` оставлены (нет бланкета neg/div).
`sat_add/sub/mul_i64` оставлены (кастомный `[lo,hi]` ≠ `saturating_add`).

**Верификация:** `nova check std/src/time` — PASS (до Step 3 split; после split код физически переехал в
timestamp.nv/monotonic.nv без изменения тел — см. риск в Шаге 3).

## Шаг 2 (Пункт 12, вбирает 11) — getter=голое / конструктор=`to_` — ✅ СДЕЛАНО (call-сайты по всему repo)

**Getters** (Duration/Timestamp/Monotonic): `@as_nanos/micros/millis/secs/mins/hours/days()` → голые
`@nanos/micros/millis/seconds/minutes/hours/days()`; `@as_secs_f64/millis_f64` → `@seconds_f64/millis_f64`;
Timestamp `@as_unix_secs/millis/nanos` → `@unix_seconds/unix_millis/unix_nanos`; Monotonic `@as_nanos` →
`@nanos`.

**Конструкторы:** `Duration.from_*` (nanos/micros/millis/secs/mins/hours/days/weeks/secs_f64) ПОЛНОСТЬЮ
retracted → `fn[T Ints] T @to_nanos/to_micros/to_millis/to_seconds/to_minutes/to_hours/to_days/to_weeks()
-> Duration`; `f64 @to_seconds()` (единственный float-конструктор — «только секунды»; f64
@nanos/micros/millis/minutes/hours/days RETRACTED без замены, repo-wide grep на использование = 0).
`Timestamp.from_unix_*` retracted → `fn[T Ints] T @to_unix_seconds/to_unix_millis/to_unix_nanos() ->
Timestamp`. `Duration.try_from_secs_f64` ОСТАВЛЕН (fallible, не `from_*`-паттерн) — тело переключено на
`n.to_nanos()`. Singular-алиасы (`int @second/minute/hour/day/week()`) УБРАНЫ без замены (DRY).

**D-амендменты:** D410 (`spec/decisions/03-syntax.md`, хвост `[M-d410-as-to-migration]` для
`std/time/duration.nv` закрыт) + D317 (`spec/decisions/04-effects.md`, `from_*` → `to_*`-бланкет контракт,
плюс попутный фикс stale-текста про `i64_max()`/`i64_min()` из Шага 1).

**Call-site миграция по репо (ВСЕ подтверждены грепом = 0 на момент финального коммита, для
`Duration.from_`/`Timestamp.from_unix_`/`@as_*`-getters/singular-алиасов, scope = std + examples +
spec_tests):**
- `std/src/time/duration.nv` (самодостаточно, все внутренние call-сайты + все inline-тесты)
- `std/src/time/civil/{zoned,zoned_test,datetime,parse,parse_test,time_of_day,tz_test,civil_test}.nv`,
  `neg/period_not_duration.nv`
- `std/src/time/{overflow_safe_test,units_test,value_typed_surface_test,timer_metrics_test}.nv`,
  `std/src/time/rt/{dur_add_overflow_traps,dur_div_zero_traps,dur_f64_nan_traps}.nv`
- `std/src/concurrency/{retry,retry_test,rate_limiter,supervised_deadline_test}.nv`
- `std/src/fs/effect.nv` (3 сайта `Metadata.@modified/@accessed/@created`, реальная регрессия — retracted
  `Timestamp.from_unix_nanos` использовался тут)
- `std/src/prelude/effects.nv`, `std/src/prelude/errors.nv` (doc-comment примеры, включая
  ```nova-fenced блок в errors.nv), `std/src/identifiers/snowflake.nv` (комментарий)
- `examples/flagship/aggregator/{src/main.nv, src/app/aggregate.nv, src/app/aggregate_test.nv,
  src/app/live_test.nv, regressions/monotonic_now_bare_binding/*.nv,
  regressions/spawn_throw_multifield_payload/*.nv}`
- `spec_tests/conformance/{d317_duration_overflow_policy,d318_monotonic_non_regression,
  v2_condvar_tuple_newtype_ok}.nv`, `neg/{d124_monotonic_as_unix_secs_neg,d316_realtime_sleep_neg,
  d316_sleep_bare_int_neg}.nv`, `standalone/{d124_wall_monotonic_separation,
  d316_time_effect_typed_surface}.nv`
- `spec_tests/soundness/plan206_d317_duration_checked_saturating.nv`
- Компилятор (текст диагностик, не язык-семантика): `compiler-codegen/src/codegen/emit_c.rs` (E5101
  fix-it + «expected Duration argument» — 4 места) и `nova-cli/src/bin/migrate_plan65.rs` (генератор +
  тесты) — переключены на `N.to_millis()`/`N.to_seconds()`.

**НЕ трогалось (сознательно, вне scope):** `examples/real_world/orm_demo.nv:369,407` —
`Timestamp.from_unix(ts)` — этот метод НЕ существовал никогда (ни `from_unix_secs`, ни `_millis`, ни
`_nanos` — просто `from_unix`), пред-существующий баг, не связанный с Plan 200. `nova_tests/**` — вне
scope задания (только std/examples/spec_tests).

**Верификация:** `nova check` (targeted, без codegen) — зелёный на `std/src/time` (весь модуль, кроме
пред-существующего `civil_arith_test.nv` str.len()-гэпа — НЕ мой, и двух `EXPECT_COMPILE_ERROR` neg-тестов,
которые корректно показывают ожидаемую ошибку типов), `std/src/time/civil`, `std/src/concurrency`. НЕ
прогнан `nova check`/`test` на `examples/flagship/aggregator` и `std/src/fs` в финальной фазе (после
запрета на `nova.exe`) — миграция там чисто механическая (тот же паттерн `Duration.from_millis(N)` →
`N.to_millis()`, уже многократно verified в других файлах), но формально `[M-200-duration-chain-verify]`.
Полный `nova test`/conformance — НЕ прогнан (авторитетный гейт — оркестратор, машина занята интегратором).

## Шаг 3 (Пункт 13) — разбить duration.nv на Timestamp/Monotonic файлы — ✅ СДЕЛАНО, НЕ ВЕРИФИЦИРОВАНО КОМПИЛЯЦИЕЙ

`Timestamp` → `std/src/time/timestamp.nv`, `Monotonic` → `std/src/time/monotonic.nv` — co-equal файлы
модуля `time.duration` (оба объявляют `module time.duration`, зеркалит `std/src/time/civil/*.nv`, где ВСЕ
18 файлов объявляют один `module time.civil`). `Duration` + module-private overflow-safe хелперы
(`sat_add_i64`/`sat_sub_i64`/`sat_mul_i64`/`checked_neg_i64`/`checked_div_i64`/`add_or_trap`/`sub_or_trap`/
`neg_or_trap`/`mul_or_trap`/`div_or_trap`/`f64_nanos_checked`/`f64_nanos_or_trap`) — общие для всех трёх
типов — остались в `duration.nv`. Import-путь `std.time.duration` не меняется. Ни один метод-тело НЕ
изменён — чистая текстовая экстракция по проверенным построчным границам (head/tail, дважды
перепроверенные до/после разреза).

**⚠ `[M-200-duration-chain-verify]` — НЕ ВЕРИФИЦИРОВАНО КОМПИЛЯЦИЕЙ** (запрет на `nova.exe` наступил до
того, как дошли до Шага 3). Основание уверенности:
1. Архитектурный precedent module-private-функций, видимых межфайлово в ОДНОМ модуле, подтверждён на
   `std/src/net/ffi.nv` ↔ `std/src/net/addr.nv` (`net_addr_parse` объявлена non-export в `ffi.nv`, вызывается
   из `addr.nv`, оба `module std.net`) — то же самое нужно для `sat_add_i64` и др.
2. Границы среза (`duration.nv:706` — начало `Timestamp` банера, `:877` — конец `@elapsed()` перед
   секцией `Time effect`; `:1322` — начало `Monotonic` банера до EOF) перечитаны и сверены построчно ДО
   и ПОСЛЕ разреза — совпадение подтверждено.
3. Оба новых файла (`timestamp.nv`/`monotonic.nv`) НЕ требуют `import` (не вызывают `th.*` в реальном
   коде — только в doctest-примерах внутри `///`-комментариев; `Ints`/`Time`/`Write`/`Option` — глобальный
   prelude).

**Первое действие авторитетного гейта:** `nova test std/src/time` (или хотя бы `nova check
std/src/time/timestamp.nv std/src/time/monotonic.nv std/src/time/duration.nv`) — убедиться, что co-equal
резолв реально работает для этой тройки, и что `Duration`/`Timestamp`/`Monotonic` взаимные ссылки
(`measure`/`deadline_in`/тесты в duration.nv, использующие `Monotonic.now()`/`Timestamp.now()`) резолвятся
без ошибок.

**Приёмка (авторитет — оркестратор):** `nova test std/time` зелёный; import-пути неизменны; conformance δ0.

## Итоговый список сайтов с `[M-200-duration-chain-verify]` (не подтверждено компиляцией)

1. **`spec_tests/conformance/d74_clamp_ints_blanket_dispatch.nv`** (Шаг 0) — mega-CU compile прерван до
   результата.
2. **`std/src/time/timestamp.nv` + `std/src/time/monotonic.nv` + пересечения с `duration.nv`** (Шаг 3) —
   file-split, не собран.
3. **`examples/flagship/aggregator/**`** (Шаг 2, поздняя часть) — механическая замена, не прогнана через
   `nova check`/`test` после запрета на `nova.exe`.
4. **`std/src/fs/effect.nv`** (Шаг 2) — 3 сайта `Metadata`, механическая замена, не прогнана.

Всё остальное (Шаги 0/1, большая часть Шага 2 включая `std/src/time`, `std/src/time/civil`,
`std/src/concurrency`) — подтверждено `nova check` до запрета.

## Коммиты этой волны (ветка `p200-duration-chain`)
1. `2e0b4e3c1` — wip checkpoint после рестарта хоста (Шаг 0 + Шаг 1 готовы, Шаг 2 частично).
2. `0db38b5a4` — wip checkpoint Шаг 2 частично (call-site миграция: aggregator/spec/conformance/std).
3. `5a97d185a` — wip checkpoint Шаг 2/3 продолжение (retry.nv defaults, civil neg/tz_test/zoned_test,
   dur_f64_nan_traps.nv — найдены через `nova check`).
4. (следующий) — финал: fs/effect.nv, prelude/{effects,errors}.nv, identifiers/snowflake.nv comment fixes
   + Шаг 3 file-split + обновление progress/200-std-improvements.md статусов.
