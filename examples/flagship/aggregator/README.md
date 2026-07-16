<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Nova flagship: concurrent aggregator

Реальное Nova-приложение (не тьюториал-сниппет): параллельный fan-out по N
источникам с общим дедлайном, настоящей отменой опоздавших ланов
(`supervised(deadline:)`), веб-UI (waterfall-визуализация) и HTTP-сервером
на чистом `nova build` → нативный бинарь (без VM) — **весь HTTP-слой идёт
через пакет `http`** (`ServeMux`/`http.servernet.handle_connection`), в
`src/main.nv` нет ни одной ручной парсинг/роутинг/response-serialization
строки.

```nova
fn aggregate(sources []Source, budget Duration) Net Time Emit -> Report
```

Один код, три мира: Demo (фиксированная таблица), Chaos (та же таблица,
перетасована сидом), Live (реальная сеть) — единственное, что меняется
между ними, это какой хендлер эффекта `Net` установлен вокруг вызова
(`mock_net()` vs `real_net()`) и какой список `Source` подан на вход.
Компилятор статически проверяет эффект-строку (`Net Time Emit`) — бек не
может тихо полезть в файловую систему или сеть в обход неё (см. «Эффекты =
supply-chain» ниже).

## Сборка и запуск (локально)

Требуется собранный компилятор `nova` (см. корневой README проекта) и
переменные окружения для линковки GC (те же, что для любого `nova build` в
этом репозитории):

```sh
# Windows (PowerShell) / Unix — путь только меняет разделители
export NOVA_GC_LIB_DIR=<путь-к-nova>/compiler-codegen/vcpkg_installed/x64-windows-static/lib
export NOVA_INCLUDE_DIR=<путь-к-nova>/compiler-codegen/vcpkg_installed/x64-windows-static/include
export NOVA_GC_INCLUDE_DIR=$NOVA_INCLUDE_DIR

cd examples
nova build flagship/aggregator/src/main.nv -o aggregator
./aggregator            # слушает 127.0.0.1:8187
```

Порт — константа `DEFAULT_PORT = 8187` (`src/main.nv`), переопределяется
переменной окружения `AGGREGATOR_PORT`:

```sh
AGGREGATOR_PORT=9000 ./aggregator
```

Открой `http://127.0.0.1:8187/` в браузере — страница сама запускает
демо-прогон в первые секунды (без кликов).

## Эндпоинты

| Метод + путь | Что отдаёт |
|---|---|
| `GET /` | UI (`frontend/index.html`, встроен в бинарь через `embed()`) |
| `GET /api/snapshot` | fallback: фиксированный прогон `legend=weather&mode=demo&seed=42` — JSON |
| `GET /api/run?legend=&mode=&seed=` | свежий прогон, JSON-снапшот (см. контракт ниже) |
| `GET /api/events?legend=&mode=&seed=` | тот же прогон, SSE-replay `Emit`-таймлайна (`event: replay_info` первым, затем `lane_started`/`lane_done`/`lane_failed`/`lane_cancelled`/`run_summary`, каждое с `t_ms`) |

`legend` = `weather` \| `health` (по умолчанию `weather`);
`mode` = `demo` \| `chaos` \| `live` (по умолчанию `demo`);
`seed` — игнорируется при `mode=live` (сеть — сама себе случайность).

Каждый роут — обычная `ServeMux`-запись (`http.server`); `/api/run`/
`/api/events` — handler-closure, которая ВНУТРИ своего тела открывает
`with Net = mock_net()`/`with Net = real_net()` по `mode` и вызывает
`aggregate()`/`aggregate_live_*` — эффекты дисчаржатся ДО возврата
`ServerResponse`, так что сам handler остаётся значением подходящего
(пустого) типа `fn(ServerRequest) -> ServerResponse` (`http.server`'s
`Handler`). Accept-loop (`TcpListener.bind`/`.accept()` + цикл) — единственная
легитимно ручная часть: у `http.servernet` пока нет переиспользуемой
accept-loop функции, только одноразовый `handle_connection`.

`mode=live`: `health` — реальный TCP-замер (`std.net.resolve` +
`TcpStream.connect`, порт 443) против 5 настоящих доменов; `weather` —
**честно заблокирован** (не подделывает данные) маркером
`[M-187-weather-live-tls-diamond-blocked]` — diamond-зависимость `tls`
(этот пакет тянет её путём, `nova-http` — через `git`+version, две разные
физические копии) — подробности в `src/app/live.nv`.

## Как это тестируется

Ни одного мок-фреймворка — эффект-система подменяет МИР, не код
(`src/app/aggregate_test.nv`):

```nova
test "aggregate: done/failed/cancelled settle correctly on a mixed fan-out" {
    with Net = mock_net(), Emit = null_emit() {
        ro report = aggregate(tiny_sources(), Duration.from_millis(120))
        assert(report.done == 2)
    }
}
```

Комбинированный `with Net = ..., Emit = ...` (`spec/decisions/04-effects.md`,
строка 2321) устанавливает оба хендлера ОДНИМ блоком. Та же функция
`aggregate`, что уходит в прод под `real_net()` (Live-режим), здесь гоняется
под `mock_net()` — интеграционный тест превращается в юнит без единого
сокета.

Полный прогон тестов пакета:

```sh
nova test flagship/aggregator --strict-effects
```

`src/main.nv` сам по себе тестов не содержит (это реальный сервер,
`fn main()` никогда не возвращается — `EXPECT_TIMEOUT_MS`-маркер в его
заголовке выводит файл из дефолтного прогона `nova test`, тот же приём, что
`nova-http`'s собственные live-socket smoke-тесты). Его сквозная проверка —
`nova build` + запуск + `curl` (см. ниже); бизнес-логика, которую его
handler'ы вызывают (`build_snapshot`, JSON-форма), покрыта
`src/api/report_json_test.nv`.

### Детерминизм

`chaos_variant` (чистая функция, `src/app/scenarios.nv`) байт-в-байт
детерминирована по сиду — покрыто тестом
(`src/app/aggregate_test.nv`, «same seed -> identical source list»).

**Честная оговорка:** полный HTTP JSON-снапшот (`/api/run`) НЕ гарантированно
байт-в-байт идентичен между прогонами с ОДНИМ и тем же `seed`, даже для
`mode=demo`: `elapsed_ms`/`wall_ms`/`sequential_ms` — РЕАЛЬНЫЕ измеренные
`Time.sleep`+fiber-scheduling длительности (весь смысл этого пакета — реальная
конкурентность, не симуляция времени), а порядок `results[]` — порядок
COMPLETION, тоже зависящий от реального шедулинга около границы дедлайна.
Детерминировано: `fanout`, какие source'ы вообще есть, какие из них
seed-помечены на провал, `budget_ms`, `legend`/`mode`/`seed` — то есть всё,
что выше настоящих часов. Проверено вручную в этой сессии (два `curl` подряд
к `/api/run?legend=weather&mode=demo&seed=42`): одинаковые `fanout`/состав
провалившихся, РАЗНЫЕ `elapsed_ms` и порядок `results[]`.

## Эффекты = supply-chain защита

Компилятор статически агрегирует эффект-поверхность публичного API пакета —
`nova info` показывает РОВНО то, что бек может тронуть снаружи (сеть/время/
прогресс-события), и ничего больше: ни `Fs`, ни `Os` — реальный вывод на
этой сессии (`nova info flagship/aggregator`, дословно):

```
Пакет: aggregator
Публичный API: 20 функц., 3 с эффектами

Effect-surface: Emit, Net, Time
  Emit  ← src.app.aggregate, src.app.aggregate_live_health, src.app.aggregate_live_weather
  Net   ← src.app.aggregate, src.app.aggregate_live_health, src.app.aggregate_live_weather
  Time  ← src.app.aggregate, src.app.aggregate_live_health, src.app.aggregate_live_weather
```

Заметь: `Fail` в поверхности НЕТ — `supervised(deadline:)`'s `TimeoutError`
ловится и обрабатывается ВНУТРИ пакета (`with Fail[TimeoutError] = ...`,
см. `src/app/aggregate.nv`), наружу он никогда не течёт. `src.main`
(`main.nv`, `Os`/`Net`-эффекты через `real_os()`/`real_net()`) не публичный
API пакета — это точка входа, `nova info` про неё не отчитывается тем же
образом. Если бы кто-то в будущем добавил, скажем, чтение файла в
`fetch_one`, `nova info --diff <baseline> --fail-on-new` упал бы в CI ДО
ревью — supply-chain-дыра ловится статически, не в проде.

## Структура

```
examples/flagship/aggregator/
  src/
    main.nv         — точка входа: accept-loop + ServeMux-роутинг (module src.main)
    domain/
      domain.nv      — Source/SourceData/AggError/TaskStatus/TaskResult/Report,
                        чистые данные, БЕЗ эффектов (module src.domain,
                        отдельный от src.app — см. известные ограничения)
    app/              — домен-логика (эффектная): aggregate, emit, scenarios,
                        live (module src.app, D78 folder-module)
    api/              — HTTP-слой: report_json (typed DTO + serde,
                        `build_snapshot`/`snapshot_to_json`) (module src.api)
  frontend/
    index.html        — UI (self-contained: стили+JS внутри), встроен в
                        бинарь через embed()
```

## Известные ограничения этого прогона

- **Live-погода заблокирована** (`[M-187-weather-live-tls-diamond-blocked]`) —
  diamond tls-зависимость между этим пакетом и `nova-http`; каждый лан
  честно проваливается с текстом-подсказкой, не подделывает данные.
- **`/api/events` — replay, не живой стрим.** Первая запись содержит полный
  таймлайн уже завершённого прогона (с реальными `t_ms`-метками), не
  push по мере выполнения (`[M-187-sse-live-stream]`, решение владельца —
  план 187 §9.3 п.3).
- **Сервер обслуживает подключения ограниченно-конкурентно** (обновлено
  2026-07-16, bounded-accept mitigation) — до `MAX_INFLIGHT_CONNS = 2`
  соединений одновременно через `detach { }` (D50 fire-and-forget,
  реальный worker-pool dispatch под armed M:N этого бинаря), сверх лимита
  соединение честно закрывается сразу (без чтения/ответа, без очереди).
  `[M-187-nested-spawn-scope-var-cc-fail]` (компилятор-баг: `spawn`,
  вложенный в `spawn` под одним `supervised`, CC-FAIL'ит на сгенерированном
  C) по-прежнему в силе для `spawn`, но `detach` — другой codegen-путь, не
  задет. Значение `2` — не формула, а эмпирический обрыв: `1`/`2`
  переживают массированные `xargs -P80`/`-P200` без permanent-wedge, `3`/
  `4` воспроизводят ТОТ ЖЕ permanent-wedge, что и без bound'а вообще —
  выше связной concurrency этого демо-сервера не поднимать без повторного
  прогона гейта (см. main.nv, док-комментарий `MAX_INFLIGHT_CONNS`, и
  ограничение ниже).
- **Стабильность сервера (обновлено 2026-07-16 — прежние блокеры ЗАКРЫТЫ):**
  ранние заходы фиксировали, что сервер виснет на 2-3-м запросе и умирает в
  простое. ВСЁ ЭТО ПОЧИНЕНО и влито в main:
  `[M-187-supervised-nested-fiber-slot-race]` ✅ (83.4.5.12 — drain yielded-FIFO
  в pump), `[M-187-watchdog-idle-server-kill]` ✅ и
  `[M-187-sse-live-tls-server-hang]` ✅ (a59800994 — `cancel_worker_fibers`
  больше не no-оп'ит отмену сетевого парка при driver). Сервер держит
  последовательную нагрузку и простой БЕЗ env-костылей (никакого
  `NOVA_WATCHDOG_DUMP_SECS=0` не нужно — тот обход устарел). Проверено
  нагрузочным тестом (`loadtest.ps1`): 50× SSE weather-live подряд + 10 раундов
  по всем 12 legend×mode комбо + 12с простоя — 0 сбоев.
- **Высокая ОДНОВРЕМЕННАЯ параллельность соединений** —
  `[M-187-high-concurrency-connection-wedge]`, архитектурный баг M:N-планировщика
  (нет work-stealing/преемпции для nested `supervised` под пиком) — сам баг
  OPEN, эскалирован, НЕ чинился этой волной (настоящий фикс — scheduler-level
  park/join rework, отложен в research: вносил memory corruption в отдельной
  ветке). **Bounded-accept mitigation влита (2026-07-16):** admission control
  в `main.nv` перед выдачей соединения хендлеру — до `MAX_INFLIGHT_CONNS = 2`
  соединений одновременно (`detach { }`, реальный worker-pool dispatch),
  сверх лимита — честный close без очереди. С mitigation'ом сервер ВЫЖИВАЕТ
  под тем же `loadtest.ps1 -Concurrency 80` (BLOCK 5 — жив после, часть
  запросов честно отбита) и под прямым `xargs -P80`/`-P200` (~80: 2-8/80
  проходит, реджект — не permanent-000; ~200: аналогично, ~4-8% проходит) —
  раньше на этих нагрузках был permanent-000 (сервер НЕ восстанавливался).
  Без mitigation'а (или с `MAX_INFLIGHT_CONNS` выше 2, эмпирически — вплоть
  до 16) баг воспроизводится ТАК ЖЕ, как раньше — сам scheduler-баг НЕ
  устранён, только окружён admission control'ом снаружи. Для одного
  браузер-таба неактуально; воспроизводится только массовой параллельной
  атакой. Нагрузочный тест — `loadtest.ps1` (самодостаточный: сам собирает/
  поднимает/глушит сервер, 7 блоков); BLOCK 5's строгий `$ok -eq $Concurrency`
  теперь ожидаемо не проходит (часть 80 честно отбита по design) — критерий
  живучести (сервер отвечает 200 ПОСЛЕ BLOCK 5/6) выполняется.
- **JSON рендерится вручную в `main.nv`**, не через
  `std.encoding.serde.json_encode` (хотя `report_json.nv` ПОЛНОСТЬЮ typed-
  serde, деливерабл 2 не откачен) — `[M-187-http-serde-setcookie-serialize-
  collision]`: компилятор линкует `json_encode[T]` НЕПРАВИЛЬНО (undefined
  symbol) стоит `http`-пакету (тянет `SetCookie.serialize()`,
  RFC 6265bis-метод, случайно тёзка) и любому `#impl(Serialize)`-типу
  оказаться в ОДНОМ compile unit. `build_snapshot` (typed DTO + вся
  бизнес-логика — id-lookup, structural fiber-count, `TaskStatus`-маппинг)
  ПОЛНОСТЬЮ переиспользуется; рендерится в текст вручную (`json_escape` +
  конкатенация) только сам ФИНАЛЬНЫЙ шаг — не «ручной HTTP» (ни статус-строк,
  ни роутинга, ни wire-framing — всё это по-прежнему `http.server`/`ServeMux`/
  `sse_event`).
- **HTTP JSON-снапшот не байт-в-байт детерминирован** между прогонами с
  одним `seed` — см. «Как это тестируется → Детерминизм» выше (реальные
  часы, не симуляция).
- **Docker — не в этом прогоне** (волна 2 плана 187).

## См. также

- `docs/plans/187-flagship-concurrency-demo.md` — план, декомпозиция,
  контракт данных.
- `examples/net/echo_server.nv` — образец accept-loop, на котором построен
  `src/main.nv`.
- `std/src/concurrency/supervised_deadline_test.nv` — образец
  `supervised(deadline:)`, на котором построена настоящая отмена.
- `nova-http/src/servernet/rt/handle_connection_smoke.nv` — образец
  `handle_connection` + `ServeMux` за живым сокетом, на котором построен
  роутинг этого пакета.
