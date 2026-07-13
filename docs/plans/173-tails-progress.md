# 173 — хвосты пост-закрытия семейства: прогресс (ветка tails-173)

> Чекпойнт-файл волны «173 хвосты» (2026-07-13, база main dfe47ca16).
> Задание: верификация гейтов → propagation-trace → MultiError-агрегация scope /
> panics-клаузула (по гейтам) → semaphore-cap (остаток бюджета).

## Пункт 0 — верификация гейтов (выполнено, код не трогался)

| Гейт | Статус | Факт |
|---|---|---|
| (а) Plan 174.3 (any/is-downcast) — гейтит п.1 | ✅ ЗАКРЫТ в нужном объёме | Ф.1+Ф.2 выполнены 2026-07-04 (any fat-pointer + `is`/`try_as` + narrowing + `E_IS_NON_ANY`); Ф.3 (гетерогенные `[]any`-коллекции) вынесена в Plan 188 и Ф.4-механике НЕ нужна — `suppressed() -> []any` уже работает (Ф.4 закрыта на этой инфре) |
| (б) 173 Ф.5 `nova_runtime_reset` — гейтит п.3 | ✅ ЗАКРЫТ | Ф.5 закрыта 2026-07-10 (ветка err-173-f56); п.6 `nova_runtime_reset()` — коммит `cdd23a5b2` (fibers.h; из user-кода недоступен, neg-тест есть) |
| (в1) объём Ф.4 (перечитано) | ✅ закрыта 2026-07-06 — но ТОЛЬКО defer/cleanup-ось | Сделано: D158 модель Б (primary как есть + карман `suppressed() -> []any`), typed `ScopeOutcome.Failure(any)`, catchability-инвариант, anti-hijack `is_cleanup`. **НЕ сделано: scope-агрегация N детских ошибок** — re-throw-хвост `nova_supervised_run_impl` (fibers.h ~2960) кидает только primary; `nova_scope_collect_child_errors` (fibers.h:1172) не имеет ни одного вызывающего; теста «3 ребёнка падают → primary + 2 в suppressed» (Ф.3-остаток §Тесты) не существует. D414 §1 в спеке УЖЕ обещает «Не-primary ошибки уходят в suppressed-карман» — спека впереди реализации. → п.1 задания реально открыт И разгейчен |
| (в2) объём Ф.6 (перечитано) | ✅ закрыта ПОЛНОСТЬЮ 2026-07-10 | D348 в спеке (09-tooling.md) + amend D89; `TestDecl.panics` + codegen-инверсия + `nova_runtime_reset()` в эпилоге; миграция 67 тестов / −52 CU; маркер `[M-173-panics-clause]` ЗАКРЫТ (ветка err-173-f56, слита в main) → **п.3 задания уже сделан в main, работы нет** |

## Таблица пунктов

| Пункт | Статус | Коммит(ы) |
|---|---|---|
| 0. Верификация гейтов | ✅ сделано | b6d5ea091 |
| 2. Полный propagation-trace `[M-173-error-return-trace]` | ✅ сделано (ring-buffer 16 + `?`/`!!`-стемпы + сбросы origin/catch/reset; тест rt/f5_propagation_trace_full; маркер CLOSED; попутный `[M-cli-build-source-file-name-unknown]` P3) | 1b3b87aa2 |
| 1. MultiError: scope-агрегация детских ошибок в suppressed | ✅ сделано (staging `_nova_pending_suppressed` + escalated-флаг + skip-primary по payload/tid/kind; ABI-фикс `nova_any_from_boxed` value-примитивов; D414 §1-амендмент; тест err173_2/scope_multierror_test 3 сценария) | 7bbd523d7 |
| 3. Panics-клаузула (Ф.6) | уже закрыта в main — работы нет | — |
| 4. Semaphore-cap живых детей parallel for (P3) | НЕ начат (решение по бюджету): объём = заход в parfor-десугар (spawn-троттлинг Semaphore Plan 103.4, парковка родителя × cancel/deadline-семантика) — полноценная волна, не остаток; опция остаётся зафиксированной в 173.1 §Ф.3 | — |

## Plan 201 (ветка suppressed-explicit): рефактор suppressed-цепочки + оценка trace-буфера

Владелец: «требовать следовать комментарию — неверная практика» — про
ИНВАРИАНТ M:N в fibers.h над блоком throw'ов (D414 §1 scope-агрегация).

**201.1 (сделано):** `_nova_pending_suppressed` (ambient TLS staging-слот)
удалён. Suppressed-цепочка теперь явный параметр: `nova_last_error_set_ex`
(effects.h) + `nova_rethrow_scope(err, kind, payload, tid, suppressed)`
(fibers.h, единая scope re-throw точка) + explicit-suppressed сиблинги
`nova_throw_ex`/`nova_throw_typed_ex`/`nv_panic_ex` (старые имена — обёртки
с `suppressed=NULL`, все прочие call-сайты не тронуты). `nova_supervised_run_impl`
строит цепочку в локальную переменную и передаёт параметром — механика
вместо инварианта-в-комментарии. Проверено: `scope_multierror_test`
(err173_2) 3/3 без изменений, в `--mode dev` и `--mode release`.

**201.2 (сделано):** `NOVA_ASSERT_NO_AMBIENT_ERROR_STAGING()` (effects.h,
`#if !defined(NDEBUG)`) — debug-tripwire класса «ambient TLS-слот с
инвариантом "нет точки планирования между постановкой и потреблением"»;
вызывается из `nova_gopark` (nova_sched.h) — единственной настоящей точки
планирования (`mco_yield`). Реестр проверок пуст: других живых слотов
ЭТОГО класса не найдено. `_nova_fail_top`/`_nova_interrupt_top`/handler-
vtable-слоты — другой класс: намеренно переживают scheduling-точки как
per-fiber контекст и явно save/restore'ятся вокруг `mco_resume`
(`runtime.c`, Plan 44.5 Layer 5, оба сайта — `_worker_main` и
`_worker_run_one_fiber`).

**201.3 (оценка, БЕЗ рефактора) — trace ring-buffer (`[M-173-error-return-trace]`,
effects.h `_nova_throw_trace`/`_nova_throw_site`, п.2 таблицы выше):**

Вопрос: может ли fiber мигрировать на другой OS-поток МЕЖДУ штампами одной
`?`-цепочки проброса?

Факт из кода (`runtime.c`, оба сайта `_worker_main`/строки ~941-1078 и
`_worker_run_one_fiber`/строки ~1844-1913): вокруг каждого `mco_resume`
runtime явно save/restore'ит per-fiber TLS-набор — `_nova_active_scope`,
`_nova_active_slot`, `_nova_fail_top`, `_nova_interrupt_top`, effect-handler
snapshot (`NovaEffectSnapshot`). Этот набор ИСЧЕРПЫВАЮЩИЙ по коду: `_nova_last_error`,
`_nova_throw_site`, `_nova_throw_trace` в него НЕ входят — они остаются
чистым OS-thread-local без per-fiber save/restore.

Работа-стилинг подтверждена кодом (`nova_runq_steal`, `runtime.c`): fiber,
запаркованный через `nova_sched_park_until` (park-точки есть в реальных
cleanup-путях — напр. `net.c` `nova_sched_park_until(scope, slot,
_nn2_stream_op_ready, s)` при закрытии/операциях сокета — "Net-cleanup
класс" из задания), может быть подобран ЛЮБЫМ worker'ом при следующем
resume, не обязательно тем же OS-потоком.

Вердикт: **ДА, трасса рвётся/смешивается.** `?`-размотка сама по себе
синхронна (обычные C-возвраты/longjmp, без yield) — стемпы внутри одной
`?`-цепочки, не пересекающей cleanup/defer с блокирующей операцией, идут
на одном потоке штатно. Но если ПО ПУТИ размотки defer/errdefer/consume-
cleanup body делает блокирующую операцию (Net close/write, sleep, channel)
— fiber паркуется; runtime НЕ сохраняет `_nova_throw_trace`/`_nova_throw_site`
в per-fiber снапшот; после resume (тем же или ДРУГИМ worker'ом) TLS
принадлежит уже ДРУГОМУ логическому состоянию — тем стемпам, что накопил
на этом OS-потоке кто-то ещё (другой fiber) за время парковки, либо
пустому буферу после `nova_runtime_reset`. Следующий `nova_throw_trace_push`
после resume допишется поверх ЧУЖИХ записей → смешанная/оборванная трасса
в диагностическом выводе. Тот же класс дефекта затрагивает `_nova_last_error`
(используется `interrupt`-consume для восстановления typed-ошибки после
unwind) и `_nova_throw_site` — они тоже не в save/restore наборе, но вне
объёма этой оценки (задание — только trace-буфер).

Важная оговорка: это баг ТОЛЬКО diagnostic/debug-путей (propagation-trace
для uncaught-abort вывода, `Zig error-return-trace` парность) — сама
механика проброса ошибки (`_nova_fail_top` longjmp-цепочка) корректна и
per-fiber (входит в save/restore). Функциональность/корректность catch
не страдает, страдает ТОЛЬКО читаемость трассы в uncaught-выводе — и
только в сценарии "блокирующая операция внутри cleanup/defer, ИДУЩЕГО
во время активной пропагации ошибки" (относительно редкий, но реальный
путь: errdefer с Net-cleanup).

Рекомендация (не реализовано — отдельное решение по объёму): перенести
`NovaThrowTrace`/`NovaThrowSite` (и, шире, `NovaLastError`) из чистого
`__thread` в per-fiber storage — по образцу уже существующего save/restore
`_nova_fail_top`/`_nova_interrupt_top` (добавить поля в `NovaSpawnCtxBase`
+ save/restore в обоих сайтах `runtime.c`), либо (дешевле, но менее чисто)
переиспользовать существующий save/restore hook и добавить туда эти три
поля один раз. Объём: 2 функции (`_worker_main`, `_worker_run_one_fiber`)
× save + restore + поля в `NovaSpawnCtxBase` — небольшой, но требует
отдельного гейта (затрагивает диагностический вывод во всех uncaught-тестах
с `propagation trace`, нужен прогон conformance + rt-лейна на предмет
byte-identical trace output в существующих тестах). НЕ входит в объём
Plan 201 (задание — только оценка).
