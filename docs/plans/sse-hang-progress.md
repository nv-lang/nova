# [M-187-sse-live-tls-server-hang] — ЗАКРЫТ (worktree nova-ssehang, fix-sse-live-tls-hang)

## Итог
Корень — НЕ SSE-механизм и НЕ фундаментальная M:N-архитектура remote-park.
Корень: `nova_runtime_cancel_worker_fibers` (`compiler-codegen/nova_rt/runtime.c`)
безусловно no-op'ила ЦЕЛИКОМ, как только стартовал driver-поток (это происходит
почти сразу после старта сервера — `_materialize_pool` вызывает `nova_driver_init()`
сразу после материализации worker-пула, т.е. на первом же `spawn`). Комментарий
там объяснял это как "driver уже владеет отменой sleep через armed_sleeps_head",
что верно для `Time.sleep`, но блок отсекал ЛЮБУЮ отмену для parked-на-
`pending_stop_cb` фиброа — а это ВСЕ сетевые операции (`nova_sched_register_pending`
в `net.c`: TCP connect/read/write, TLS handshake/read/write, UDP, DNS). Когда
`supervised(deadline:)` реально нужно было прервать лейн, зависший в РЕАЛЬНОМ
сетевом парке (TLS-фетч open-meteo в live.nv), `nova_scope_deliver_cancel` →
`nova_runtime_cancel_worker_fibers` → мгновенный no-op → async-handle никогда не
закрывается → `pending_remote` никогда не декрементируется →
`nova_supervised_run_impl`'s wait-loop крутится вечно (100% CPU на worker-потоке).
Это НЕ SSE-специфично — воспроизводится через ЛЮБОЙ `supervised(deadline:)` над
живым сетевым фетчем, где дедлайн реально наступает во время сетевой операции;
SSE лишь первым столкнулся с этим при повторных запросах (soak-тест).

## Репро ДО фикса
- Обычный SSE-live weather (budget=4000мс, реальная задержка ~300-500мс —
  отмена почти никогда не наступает): цикл `curl .../api/events?...mode=live`
  зависал ВЕРОЯТНОСТНО — прогон 1: зависло на 17-й итерации; прогон 2: 80/80
  чисто (совпадает с описанием оркестратора "usually 2nd, sometimes 3rd-7th,
  occasionally 10+ clean" — тот же класс, что и уже закрытый
  [M-187-supervised-nested-fiber-slot-race], но ДРУГОЙ механизм).
- Форс-репро (диагностика, временно `LIVE_BUDGET_MS=15` — гарантированная
  отмена на КАЖДОМ лейне): зависло ДЕТЕРМИНИРОВАННО на 2-й итерации.
- Контроль-изоляция: SSE demo/mock (`mode=demo`) — 5/5 чисто, НЕ виснет
  (ожидаемо: демо-путь использует `Time.sleep` для симуляции задержки,
  отмена там идёт через driver'ный `armed_sleeps_head`-путь, который РАБОТАЕТ;
  единственный настоящий сетевой TCP-коннект в demo — к loopback-моку, быстрый,
  почти никогда не попадает под дедлайн).
- /api/run weather-live x5: 4/4 done во всех прогонах и ДО, и ПОСЛЕ фикса —
  подтверждает, что баг вероятностный (нужно реально попасть отменой в сетевой
  парк), не завязан именно на SSE-путь как таковой.

## Дамп watchdog
Существующий main-thread watchdog (`fibers.h:2869` `nova_supervised_run_impl`)
СТРУКТУРНО не мог сработать для этого зависания: гейт `_watchdog_enabled`
требует `!_nova_on_worker_thread()`, а вся цепочка (accept-loop spawn →
per-connection supervised{spawn{}} → aggregate()'s parallel-for →
fetch_guarded/live_lane_weather's supervised(deadline:)) выполняется целиком
на worker-потоках. Добавлен ДИАГНОСТИЧЕСКИЙ (opt-in, `NOVA_WATCHDOG_WORKER=1`,
default OFF — нулевое поведенческое изменение) worker-thread watchdog в
`nova_supervised_run_impl` — реальной находке не помог (сам факт зависания —
не "SUSPENDED-not-parked стак", это генуинно ещё живой, но никогда-не-
разбуженный сетевой парк, `nova_runtime_has_stuck_fibers()` его не видит),
но методологически подтвердил структурный пробел (main-thread-only watchdog
не покрывает основной путь этого флагмана) — оставлен как полезная,
безопасная (по умолчанию выключена) диагностика на будущее.
Настоящая локализация получена ЧТЕНИЕМ КОДА (runtime.c:2044
`nova_runtime_cancel_worker_fibers`, комментарий "Legacy cancel path is
BYPASSED when driver started") + форс-репро с урезанным бюджетом для
подтверждения гипотезы (100% детерминированное зависание на 2-й итерации,
CPU процесса рос — busy-spin, не блокировка на syscall).

## Зона фикса
(а) runtime `compiler-codegen/nova_rt/runtime.c`,
`nova_runtime_cancel_worker_fibers` — единственное место фикса. Раньше:
`if (nova_driver_is_started()) return;` (безусловный no-op). Теперь: driver-mode
только гейтит bare-park fallback-ветку (`else if (parked_at)`, нужна ТОЛЬКО
чтобы не задвоить wake с driver-путём Time.sleep — тот тоже bare-park, без
stop_cb); ветка `if (cb && hdl)` (сетевые операции с зарегистрированным
stop_cb — TCP/TLS/UDP/DNS) выполняется ВСЕГДА, независимо от driver-режима.
Безопасно: `_nova_sleep_via_driver` НИКОГДА не регистрирует stop_cb (паркуется
через `armed_sleeps_head` + голый `parked[slot]`), так что множества
"stop_cb-зарегистрированный парк" и "driver-routed sleep" не пересекаются —
гонки удвоенного wake, которую боялся исходный комментарий, для cb-ветки
не существует.
(б) `compiler-codegen/nova_rt/fibers.h`,
`nova_supervised_run_impl` — добавлен ДИАГНОСТИЧЕСКИЙ opt-in worker-thread
watchdog (`NOVA_WATCHDOG_WORKER=1`, default off, нулевое изменение поведения
по умолчанию). Не обязателен для фикса, оставлен как полезное расширение
существующей диагностики (закрывает структурный пробел — main-thread-only
watchdog не видит эту и любую другую нагрузку, живущую целиком на workers).
(в) `examples/flagship/aggregator/src/main.nv` — НЕ менялся (только временный
диагностический `LIVE_BUDGET_MS=15` для форс-репро, откачен обратно на 4000
перед финальным гейтом и коммитом — `git diff` пуст).
(г) `http`/`nova-http` — НЕ трогался, не является зоной фикса (see ниже).

## Гейт ПОСЛЕ фикса
- 40/60 + 40/40 итераций форс-репро (`LIVE_BUDGET_MS=15`, гарантированная
  отмена на каждом лейне) — 0 зависаний (было: 100% зависание на 2-й
  итерации до фикса).
- Официальный гейт (budget вернули на 4000): 10x SSE-live weather подряд —
  10/10 `replay_info`+`run_summary`, сервер жив (200) между каждым; 15с
  простоя; ещё один SSE-live после простоя — жив, полный ответ.
- Регрессии: `/api/run` weather-live x5 — done: 4,4,4,3,4 (варьируется по
  реальной сетевой погоде, ожидаемо — НЕ зависание, сервер жив после каждого);
  demo/chaos/health-live/health-demo/SSE-demo x5/`/`/`/api/snapshot` — все 200,
  сервер жив на всех проверках; health-live retry с расширенным таймаутом ДАЖЕ
  дал реальную отмену одного лейна (`npmjs.org` → `cancelled`, `wall_ms=4010`)
  — сервер остался жив (доп. подтверждение фикса вживую, не только в форс-тесте).
- `nova test` точечно: `std/src/concurrency/supervised_deadline_test.nv` PASS,
  `supervisor_test.nv` PASS, `rate_limiter_test.nv` PASS. `retry_test.nv`
  CC-FAIL — ПРЕДСУЩЕСТВУЮЩИЙ, не связан с фиксом (падает в изоляции ТОЖЕ,
  без наших файлов рядом; generic-мono codegen баг emit_c.rs, наш фикс
  трогает только runtime.c C-тело функции, emit_c.rs не касался — git diff
  подтверждает: изменены только `fibers.h`+`runtime.c`).
- nova-http юнит-тесты (`server_test.nv`/`streaming_test.nv`) не удалось
  запустить кросс-репо напрямую (package-root резолвинг `std.encoding.compress`
  ломается при вызове файла из другого репо не-от-своего-корня — предсуществующее
  ограничение тулинга, не регрессия). Компенсировано: реальный SSE/http.server/
  servernet путь уже проверен E2E 100+ раз через сам флагман-гейт (включая
  форс-отмену) — это тот же самый `write_stream_chunks`/`handle_connection`
  код, что и в юнит-тестах.

## Окружение / важная находка про параллельную работу
Корневой `d:/Sources/nv-lang/nova` во время этой сессии был НЕ на `main`, а на
ветке `integ-206-v3` (Plan 206 интеграция) с незакоммиченными правками в
`examples/flagship/aggregator/src/app/live.nv` — параллельная сессия ведёт
работу прямо в общем корне. `main` в корне указывал точно на 7ae47d737 (та же
база, что и у нашего worktree) — так что сборка приложения делалась НЕ из
корня (как предполагала изначальная инструкция), а из ИЗОЛИРОВАННОГО
`nova-ssehang` (свой полный чекаут той же базы; sibling-пути
`../../nova-tls`/`../../nova-http` от `nova-ssehang/examples` резолвятся
идентично корневым). Создан локальный (gitignored) `nova-ssehang/examples/
nova.local.toml`.

## Спека/язык
Рантайм-баг (cancel-delivery gap для сетевых parks под driver-режимом),
НЕ языковая семантика. D-амендмент НЕ нужен — эффект-семантика отмены
(`supervised(deadline:)`, D408/D349) не менялась, только внутренний C-механизм
доставки отмены до async-хэндла.
