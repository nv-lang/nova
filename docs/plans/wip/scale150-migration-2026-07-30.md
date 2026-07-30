# №150 — масштаб миграции под энфорс правила (2)+(3)

Только чтение/подсчёт. Модель: **sonnet**. Суб-агенты не спавнились.
Правило из реестра (docs/plans/221.1-bug-sweep.md, №150, редакция 2026-07-29):
(2) замыкание/значение с mut-захватом = линейный ресурс ОДНОГО файбера;
пересечение границы файбера (spawn/detach/parallel for/send в канал/
handler вокруг spawn-содержащего кода) = ошибка. (3) белый список:
share-типы (D415), Atomic*, Mutex, концы каналов (Sender/Receiver) —
разделение разрешено, транзитивно по составу.

## 1. Таблица находок

Классы: 1 = closure создано вне spawn/detach/parfor, передано параметром/
полем/элементом коллекции; 2 = closure отправлено в канал; 3 = handler-
литерал с mut-захватом, with-тело содержит spawn/detach/parallel-for;
4 = mut-захват белого списка (легален).

| Репо | Файл:строка | Класс | Что захвачено | Вердикт |
|---|---|---|---|---|
| nova | std/src/concurrency/supervisor_test.nv:39 | 3 | `mut caught` (Fail[int] handler) вокруг `Supervisor.escalate()+spawn` | **НАРУШИТЕЛЬ** |
| nova | std/src/concurrency/supervised_deadline_test.nv:61,84,107,130,150,168,191 (7 тест-блоков) | 3 | `mut caught_typed/caught/timed_out` (Fail[TimeoutError] handler) вокруг `supervised(timeout/deadline/cancel)+spawn` | **НАРУШИТЕЛЬ** ×7, один повторяющийся идиом |
| nova | spec_tests/conformance/standalone/supervisor_escalate_test.nv:24,52,112 | 3 | `mut caught`+`mut got` (Fail[int]) вокруг `supervised+spawn`(×2 for-loop) | **НАРУШИТЕЛЬ** ×3 |
| nova | spec_tests/conformance/standalone/supervisor_escalate_test.nv:80 | 3 | `mut stopped`+`mut seen_int` (Supervisor.on_child_fail EFFECT-ЛИТЕРАЛ, не builtin) вокруг `supervised+spawn` | **НАРУШИТЕЛЬ** (но см. §2 — тот же класс, что D416§2-safe в serve.nv/background.nv) |
| nova | spec_tests/conformance/triple_nested_throw.nv:9 | 3 | `mut caught` (Fail[int]) вокруг 3-уровневого nested `supervised+spawn` | **НАРУШИТЕЛЬ** |
| nova | spec_tests/conformance/triple_nested_throw.nv:31 | 4 | `mut depth = AtomicInt.new(0)` — тот же сценарий, УЖЕ мигрирован | **ЛЕГАЛЕН** (образец правильной миграции) |
| nova | spec_tests/conformance/child_error_retention_test.nv:31,68 | 3 | `mut caught`+`mut got` (Fail[int]) вокруг `supervised{for{spawn}}` | **НАРУШИТЕЛЬ** ×2 |
| nova | spec_tests/conformance/standalone/supervisor_parfor_test.nv:29 | 3 | `mut caught`+`mut got` (Fail[int]) вокруг `with Supervisor.escalate(){parallel for}` | **НАРУШИТЕЛЬ** |
| nova | spec_tests/conformance/application_cross_fiber_t8_7.nv:41-46 | 3 | `mut child_overrun` (ResourceTrace-handler-литерал) вокруг `supervised{spawn{consume...}}` — ЭТО САМ ТЕСТ НА CROSS-FIBER Application-propagation (D80) | **НАРУШИТЕЛЬ** (самый прямой пример реальной гонки из формулировки №150) |
| nova | spec_tests/conformance/standalone/m2211_38_sequential_supervised_accept_stale_deadline.nv:39 (`read_attempt` fn, оба теста файла зовут её через `handle_one`) | 3 | `mut timed_out` (Fail[TimeoutError]) вокруг `supervised(deadline)+spawn consume share` | **НАРУШИТЕЛЬ** |
| nova | examples/tour/concurrency.nv:12 (`probe`) | 3 | `mut timed_out` — идентичный идиом, ДЕМО ИЗ ЯЗЫКОВОГО ТУРА (docs) | **НАРУШИТЕЛЬ** |
| nova | examples/mini_aggregator.nv:27 (`probe`) | 3 | `mut timed_out` — файл сам себя называет «суть флагманского демо» | **НАРУШИТЕЛЬ** |
| nova | examples/flagship/aggregator/regressions/spawn_capture_value_struct/…nv:15 (`guarded`) | 3 | `mut timed_out` | **НАРУШИТЕЛЬ** |
| nova | examples/flagship/aggregator/regressions/spawn_throw_multifield_payload/…nv:23 (`m187_guarded`) | 3 | `mut caught` | **НАРУШИТЕЛЬ** |
| nova | examples/flagship/aggregator/src/app/aggregate.nv:87 (`fetch_guarded`) | 3 | `mut timed_out` — **ЖИВОЙ КОД ФЛАГМАНА**, вызывается `parallel for` по N источникам | **НАРУШИТЕЛЬ** |
| nova | examples/flagship/aggregator/src/app/live.nv:118 (`live_lane_tcp`), :150 (`live_lane_weather`) | 3 | `mut timed_out` ×2 — **ЖИВОЙ КОД ФЛАГМАНА** (live-режим) | **НАРУШИТЕЛЬ** ×2 |
| nova-polaris | src/net/policy.nv:58 (`read_attempt`) | 3 | `mut timed_out` — **ЖИВОЙ ПРОД-КОД**, вызывается на каждый header/body read HTTP-сервера | **НАРУШИТЕЛЬ** |
| nova-polaris | src/net/serve.nv:145 (`run_request`) | 3 | `mut failed`+`mut reason` (Supervisor.on_child_fail) вокруг `supervised{spawn consume share}` — **владелец САМ обосновал безопасность цитатой D416§2** ("on_child_fail исполняется на drive-потоке scope'а, сериализованно") прямо в коде | **НАРУШИТЕЛЬ по букве правила / заявлен безопасным по духу** — см. §2 |
| nova-polaris | src/background.nv:119 (`BackgroundTasks.@drain`) | 3 | `mut failed`+`mut reason` (Supervisor.on_child_fail), идентичный паттерн | то же, что выше |
| nova | 8 файлов (m2217_15b, m2217_15, m222_7_cancel_loop_accept_swallowed, m_net2stream_close_refcount_uaf, m_net2stream_split_close_refcount_uaf, m222_7_spawn_ctx_capture_mut_param, register_finalizer_lifo_v1_1, timeout_application_level2_t3_8, app_effect_basic_t8_1) | 3 (кандидаты) | handler `\|_e\| {}` ПУСТОЙ либо без mut-мутации, либо with-тело БЕЗ spawn | **ЛЕГАЛЕН** — нет реального mut-захвата/нет пересечения границы |
| nova-polaris | rt/recover500_smoke.nv, multipart_policy_smoke.nv, serve_policy_smoke.nv, _repro_serve_408_codegen.nv(коммент) | 3 (кандидаты) | тот же пустой `\|_e\| {}` | **ЛЕГАЛЕН** |
| nova-polaris | rt/recover500_smoke.nv:90 | 4 | `logtx` (Sender) захвачен в Log-handler-литерал внутри spawn | **ЛЕГАЛЕН** (канал — белый список) |
| nova | m2217_16_detach_ctx_capture_value_ptr_mismatch.nv | — | `ro policy Widget` (не mut) захвачен в detach | не относится к правилу (2) — ro, не mut |
| nova | m2217_22_defer_captured_var_in_detach_undeclared.nv | 4 | `mut c Ctr{n AtomicInt}` — запись целиком из белого списка (AtomicInt) | **ЛЕГАЛЕН**, хороший пример транзитивной композиции |
| nova | std/src/concurrency/cancellation.nv `race2[T](a fn()->T, b fn()->T)` | 1 (структурный) | closures-параметры вызываются внутри `spawn` | **0 живых вызовов** `race2(` в .nv-коде — риск есть, нарушений не найдено (мёртвый путь) |
| nova | std/src/prelude/effects.nv `Application.register_finalizer(f fn()->())` | 1 (структурный) | fn-параметр складывается в LIFO, исполняется на exit из `with Application` | конкретных mut-захватов на живых call-sites не найдено (`finalizer_noop`/`\|\| ()` — без захватов) |
| nova-polaris | src/background.nv `BackgroundTasks.tasks []fn()->()` (поле-коллекция замыканий) | 1 (структурный) | `@add(task)`, дренится `spawn`-ом по одному | конкретных живых mut-захватывающих задач в .add() не найдено (сам `@drain` — класс 3, см. выше) |
| nova-http | src/**/*.nv (28 файлов) | — | — | **0 spawn/detach/parallel-for/with-Fail-handler во всём пакете** — protocol-only библиотека, к правилу (2) не относится |
| nova-bigint | src/**/*.nv | — | — | **0 spawn/detach/parallel-for** — конкурентности нет вовсе |
| nova-compress | src/**/*.nv | — | — | **0 spawn/detach/parallel-for** — конкурентности нет вовсе |

## 2. Итоговые числа

**Класс 3 (handler с mut-захватом вокруг spawn/parallel-for) — доминирующий класс, единственный с живыми находками:**

- nova (std/src + spec_tests/conformance): **18 конкретных occurrences** в 8 файлах
  (supervisor_test.nv×1, supervised_deadline_test.nv×7, supervisor_escalate_test.nv×4,
  triple_nested_throw.nv×1, child_error_retention_test.nv×2, supervisor_parfor_test.nv×1,
  application_cross_fiber_t8_7.nv×1, m2211_38×1).
- nova examples: **7 occurrences** в 6 файлах, из них **3 — живой код флагманской демки**
  (aggregate.nv×1 + live.nv×2), 4 — regression-фикстуры/тур-демо/mini_aggregator (последний
  сам себя называет «сутью» флагмана).
- nova-polaris: **1 явный нарушитель** (net/policy.nv `read_attempt`, реальный HTTP-сервер) +
  **2 «заявленных безопасными владельцем»** (net/serve.nv `run_request`, background.nv
  `@drain` — оба Supervisor.on_child_fail, с цитатой D416§2 прямо в коде).

**Итого живых сайтов класса 3: 26 бесспорных + 2 спорных (Supervisor.on_child_fail) = 28.**
Все 26 бесспорных — это ОДИН повторяющийся идиом: `mut <flag> = false; with Fail[T] =
|_e| { <flag> = true } { supervised(...)/parallel-for { spawn {...} } }`. Он же —
идиом, показанный в `docs/quickstart.md`/`docs/language-tour.md` и являющийся сутью
флагманской демки (`mini_aggregator.nv`'s own header).

**Класс 1/2 (closure как параметр/поле/канал-элемент):** ни одного КОНКРЕТНОГО живого
нарушения не найдено — но найдено 3 СТРУКТУРНЫХ вектора без текущих виновных вызовов
(`race2[T]` — 0 вызовов в .nv-коде; `Application.register_finalizer` — только
no-capture callbacks на живых сайтах; `BackgroundTasks.tasks` — коллекция замыканий,
но `.add()`-сайты с mut-захватом не найдены). Эти три остаются РИСКОМ (нужен API-граничный
чек), а не подтверждённой миграцией.

**Класс 4 (белый список, легально):** Channel tx/rx — доминирующий БЕЗОПАСНЫЙ идиом,
используется в буквально каждом файле с spawn (десятки сайтов) — это и есть «случайно
безопасный» паттерн, о котором писал владелец в записи №150. Плюс явные примеры
AtomicInt (×2) и композиция record из белого списка (×1).

**Как мигрировать 26 бесспорных нарушителей (одна и та же группа приёмов):**
1. **Канал вместо mut-флага** (уже присутствует в каждом сайте как `tx`/`rx`!) — заменить
   отдельный `mut timed_out`/`mut caught` boolean на ЗНАЧЕНИЕ, отправленное в СУЩЕСТВУЮЩИЙ
   канал (sentinel-вариант outcome), либо завести отдельный `Channel[bool]` под сам факт
   timeout — самый дешёвый и однородный фикс, т.к. канал уже в 100% сайтов есть.
2. **Atomic\*** вместо mut-bool/int там, где нужен именно счётчик/флаг без семантики
   Result (пример уже есть в `triple_nested_throw.nv` test2 и `supervisor_escalate_test.nv`
   test3 — оба ЛЕГАЛЬНЫ сегодня).
2. **Возврат значения из `with`-выражения** вместо побочной mut-мутации хендлера —
   часть сайтов уже частично это делают (`ro _ = with Fail[int] = |e| {...; 0} {...; 99}`) —
   расширить: хендлер возвращает sentinel, `with`-выражение целиком становится
   единственным источником результата, mut-переменная снаружи не нужна вовсе.
4. **Supervisor.on_child_fail — ОТДЕЛЬНОЕ РЕШЕНИЕ ВЛАДЕЛЬЦА needed**: 3 сайта
   (serve.nv, background.nv, supervisor_escalate_test.nv test3) используют именно этот
   хендлер и по D416§2 он исполняется СЕРИАЛИЗОВАННО на drive-fiber самого scope'а —
   т.е. НЕ конкурентно со spawn-детьми. Если энфорс не сделает явное исключение для
   `Supervisor.on_child_fail` конкретно, эти 3 сайта потребуют такой же миграции (канал/
   Atomic), хотя владелец уже аргументированно считает их безопасными — это ЛОЖНО-
   ПОЛОЖИТЕЛЬНЫЙ риск конкретно для консервативного синтаксического чека
   «with-тело содержит spawn ⇒ handler under check».

## 3. spec_tests/conformance — что покраснеет

8 файлов / ~13 test-блоков непосредственно в conformance (плюс 2 в std/src, которые
гейт компилирует в составе того же CU):
- `spec_tests/conformance/standalone/supervisor_escalate_test.nv` (4 теста)
- `spec_tests/conformance/triple_nested_throw.nv` (1 из 2 тестов — второй уже мигрирован)
- `spec_tests/conformance/child_error_retention_test.nv` (2 теста)
- `spec_tests/conformance/standalone/supervisor_parfor_test.nv` (1 из 2 тестов)
- `spec_tests/conformance/application_cross_fiber_t8_7.nv` (1 тест — САМ ПРО cross-fiber)
- `spec_tests/conformance/standalone/m2211_38_sequential_supervised_accept_stale_deadline.nv`
  (оба теста файла зовут `read_attempt` транзитивно через `handle_one`)
- `std/src/concurrency/supervisor_test.nv` (1 тест)
- `std/src/concurrency/supervised_deadline_test.nv` (7 тестов — весь файл про deadline/timeout)

Эти тесты либо потребуют миграции на канал/Atomic (совместимо с их же ассертами — они
проверяют исход, а не механизм), либо явной пометки `neg`/known-red до миграции.
`application_cross_fiber_t8_7.nv` — особый случай: он СПЕЦИАЛЬНО тестирует cross-fiber
handler-propagation (D80 snapshot) — миграция на Atomic тут прямая (замени
`mut child_overrun = false` на `AtomicInt`/`AtomicBool`-эквивалент), тест не теряет смысл.

## 4. Честные ограничения методики

- Не читал ПОЛНОСТЬЮ все 31 файла std/src со `spawn` и все 71 файл spec_tests/conformance
  со `spawn` — сначала пересёк со списком файлов, содержащих closure-литералы (`= |...|`)
  и `with Fail`/`with Supervisor`, и разбирал глазами именно пересечение. Файлы со `spawn`,
  но БЕЗ closure/handler-литерала в том же файле (примерно 2/3 списка — net/fs
  smoke-тесты типа udp_test.nv, dns_test.nv, tcp_test.nv, pingpong_test.nv,
  concurrent_stat_test.nv, d323_real_fs_test.nv, split_test.nv, mock_test.nv,
  byte_surface_test.nv, stress_test.nv, write_all_test.nv, tcp_share_test.nv,
  timer_metrics_test.nv, d302_neterror_iokind_test.nv) НЕ читал построчно — по беглому
  контексту (grep -C6) там `supervised{spawn{...}}` без внешнего mut-захватывающего
  хендлера, но полной построчной проверки на класс 1 (closure как параметр/поле) в них
  не делал.
- Класс 1 (closure создано вне spawn, передано как параметр функции/поле структуры/элемент
  коллекции) методологически самый сложный для grep — не существует надёжного
  синтаксического паттерна, кроме ручного разбора каждого fn-типизированного параметра/
  поля. Проверил только явные примеры (`race2`, `Application.register_finalizer`,
  `BackgroundTasks.tasks`, `ServerResponse.upgrade`/`.background`) — НЕ гарантирую, что
  прошёл все struct-поля типа `fn(...)`/`Handler` во всём nova-polaris (`Router`,
  middleware-цепочки, `StreamBody.f`) на предмет живого mut-захвата внутри.
- Не проверял `nova_tests/**` (плейновая директория с legacy-тестами, которую конвенция
  предписывает удалять/не использовать как гейт — сознательно исключил из охвата по
  заданию: задание перечисляет std/src, examples, spec_tests/conformance явно).
- Не запускал компилятор/тесты (задание запретило) — все вердикты «легален/нарушитель»
  основаны на ЧТЕНИИ кода и текста правила (2)+(3), не на эмпирической гонке. В частности,
  вопрос «Fail[TimeoutError]-хендлер реально исполняется НА том же fiber, что spawn-дети,
  или строго ПОСЛЕ их join'а (следовательно, не гонка вовсе)» — НЕ верифицирован
  экспериментально для доминирующего идиома (только для Supervisor.on_child_fail есть
  явная нормативная цитата D416§2 в самом коде serve.nv). Если окажется, что
  `with Fail[TimeoutError]` тоже диспатчится строго после отмены/join'а детей (не
  конкурентно) — все 26 «нарушителей» этого идиома окажутся ложно-положительными для
  консервативного синтаксического чека, и потребуется та же оговорка, что и для
  Supervisor. Рекомендую перед включением энфорса померить это ОДНИМ репро (как §150 уже
  мерил для параметра-замыкания) именно для `with Fail[T] = |e| {mut=...} {supervised{spawn}}`.
- Отдельно НЕ проверял `docs/quickstart.md`/`docs/language-tour.md` (прозу) на предмет
  показанного там же идиома — упомянул по факту grep-совпадения, не читал сам текст доков.
