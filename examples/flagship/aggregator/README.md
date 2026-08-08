<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Nova flagship: concurrent aggregator

Реальное Nova-приложение (не тьюториал-сниппет): параллельный fan-out по N
источникам с общим дедлайном, настоящей отменой опоздавших ланов
(`supervised(deadline:)`), веб-UI (waterfall-визуализация) и HTTP-сервером
на чистом `nova build` → нативный бинарь (без VM) — **весь HTTP-слой идёт
через пакет `http`** (`Router`/`http.servernet.handle_connection`), в
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

## Что здесь делает язык (а не фреймворк)

Пять вещей, которые стоит увидеть, даже не читая исходники:

1. **Эффекты в сигнатуре.** `fn aggregate(…) Net Time Emit -> Report` — всё,
   что функция может делать с внешним миром, видно в типе и проверяется
   компилятором (`--strict-effects` в CI).
2. **Тест подменяет мир, не код.** `with Net = mock_net() { aggregate(…) }` —
   тот же код под другим хендлером; ни одного мок-фреймворка.
3. **Отмена настоящая.** Опоздавший источник обрывается по общему дедлайну
   (`supervised(deadline:)`), а не доживает в фоне с выброшенным результатом.
4. **«0 leaks» — проверяемое свойство, не лозунг.** `/api/snapshot` отвечает
   `fibers_spawned/closed: 12/12` — структурная конкуррентность видна прямо
   в API работающего сервера.
5. **Один нативный бинарь.** `nova build` → C → машинный код, без VM; тот же
   бинарь уезжает в Docker-образ на 126 МБ.

Хотите тот же паттерн без HTTP и UI — 30-строчный
[`examples/mini_aggregator.nv`](../../mini_aggregator.nv): `parallel for` +
общий дедлайн + честные судьбы (done/cancelled), компилируется и печатает
результат за секунды. Лестница для читателя новости: мини-файл → эта
витрина → исходники `src/`.

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

UI двуязычный: дефолт — английский (автодетект языка браузера: ru → русский),
переключатель RU/EN в шапке, прямые ссылки — `?lang=ru` / `?lang=en`
(запоминается в localStorage).

## Docker

Живое демо «скачал/собрал и запустил одной командой» (Plan 187, волна 2,
§9.4 п.7 / Ред.6 п.3) — multi-stage `Dockerfile` рядом с этим README:
стадия 1 (`builder`) компилирует `nova-cli` (release) и собирает бинарь
`aggregator` через `nova build`; стадия 2 (`runtime`) — минимальный
`ubuntu:22.04` + сам бинарь (без Rust/clang-тулчейна в финальном образе).

```sh
# один раз — submodule (build context собирается из git-checkout'а,
# submodule НЕ чекаутится автоматически):
git submodule update --init compiler-codegen/nova_rt/libuv

# сборка (контекст = корень репозитория nova; nova-http — сиблинг-каталог
# на диске, см. "Sibling-зависимости" ниже) + запуск:
docker build -f examples/flagship/aggregator/Dockerfile \
    --build-context nova-http=../nova-http \
    -t aggregator-demo:local .
docker run --rm -p 8187:8187 aggregator-demo:local
```

Открой `http://127.0.0.1:8187/` — то же демо, что и при локальном запуске.

Опубликованный образ (цель Ред.6 п.3 — публикацию делает владелец):
`ghcr.io/nv-lang/aggregator-demo:0.1.0`; после публикации запуск —
`docker run --rm -p 8187:8187 ghcr.io/nv-lang/aggregator-demo:0.1.0`.

**Bind-адрес и порт.** `src/main.nv` по умолчанию слушает `127.0.0.1`
(локальная разработка, без изменений) — внутри контейнера это не даст
`-p 8187:8187` достучаться снаружи, поэтому образ переопределяет бинд
переменной окружения `AGGREGATOR_BIND=0.0.0.0` (устанавливается в самом
`Dockerfile`, `ENV AGGREGATOR_BIND=0.0.0.0`); `AGGREGATOR_PORT` — тот же
env-override, что и при локальном запуске (по умолчанию `8187`).

**Sibling-зависимости.** `examples/nova.toml` объявляет
`http = { path = "../../nova-http" }` (path-зависимость, Plan 203/204) —
резолвится относительно `examples/` как физический сиблинг каталога `nova`
на диске (`d:/Sources/nv-lang/{nova,nova-http}` у автора). В образ она
попадает через именованный build-context Buildx
(`--build-context nova-http=../nova-http` + `COPY --from=nova-http` в
`Dockerfile`, `# syntax=docker/dockerfile:1`) — **не** `git clone` с
GitHub: на 2026-07-17 в `nova-http`'s GitHub-репозитории нет ещё не
запушенного локального коммита `WriteBuffer .into() → .into_bytes()`
(std-переименование), без которого флагман не собирается; `--build-context`
берёт локальное рабочее дерево as-is. Когда тот коммит попадёт в GitHub —
кандидат на упрощение обратно до `git clone --depth 1` (симметрично
`nova-gate.yml`). `tls`/`compress` — git-зависимости (`{ git = ...,
version = "0.1" }`, `examples/nova.lock.toml`) — резолвятся АВТОМАТИЧЕСКИ самим
компилятором Nova при первом обращении, sibling/clone им не нужен.

Известные ограничения (admission `MAX_INFLIGHT_CONNS = 16`, replay-SSE
и т.д. — см. «Известные ограничения этого прогона» ниже) действуют
одинаково локально и в контейнере — Docker меняет только upstream/сеть
доставки, не рантайм-семантику самого бинаря.

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

Каждый роут — обычная `Router`-запись (`http.server`, Plan 222 — на смену
ретайренному `ServeMux`); `/api/run`/
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
**настоящий HTTPS к `api.open-meteo.com`** (`TlsStream.connect` +
GET + read, `src/app/live.nv`) — работает после закрытия tls-диаманта
(D420 `[replace]` graph-wide) и cross-package consume-cleanup; проверено
нагрузочным гейтом (`loadtest.ps1`: weather/live 10/10, SSE weather-live 50×).
Для локальной разработки path-deps нужен `examples/nova.local.toml` с
`[replace]` на `../../nova-tls` / `../../nova-http` (gitignored, D420).

## Как это тестируется

Ни одного мок-фреймворка — эффект-система подменяет МИР, не код
(`src/app/aggregate_test.nv`):

```nova
test "aggregate: done/failed/cancelled settle correctly on a mixed fan-out" {
    with Net = mock_net(), Emit = null_emit() {
        ro report = aggregate(tiny_sources(), Duration.from_millis(300))
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
    main.nv         — точка входа: accept-loop + Router-роутинг (module src.main)
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

- ~~Live-погода заблокирована~~ **РАЗБЛОКИРОВАНА** (диамант + cross-pkg
  consume-cleanup закрыты 2026-07-15): реальный open-meteo HTTPS, 4/4 done
  в гейте. Историю см. `src/app/live.nv` (doc-comment).
- **Высокая одновременная нагрузка — admission control**: сверх MAX=16
  одновременных fan-out'ов соединения честно закрываются (backpressure на
  ширине worker-pool; сервер выживает P80/P200). Сам scheduler-клин
  `[M-187-high-concurrency-connection-wedge]` ЗАКРЫТ 2026-07-20
  (work-conserving pump, merge `14decdfb1` — см. блок «Стабильность» ниже).
- **`/api/events` — replay, не живой стрим.** Первая запись содержит полный
  таймлайн уже завершённого прогона (с реальными `t_ms`-метками), не
  push по мере выполнения (`[M-187-sse-live-stream]`, решение владельца —
  план 187 §9.3 п.3).
- **Сервер обслуживает подключения ограниченно-конкурентно** (обновлено
  2026-07-20) — до `MAX_INFLIGHT_CONNS = 16` соединений одновременно через
  `detach { }` (D50 fire-and-forget, реальный worker-pool dispatch под
  armed M:N этого бинаря), сверх лимита соединение честно закрывается
  сразу (без чтения/ответа, без очереди). Это осознанный backpressure на
  ширине worker-pool рантайма, НЕ обход бага: исторический обрыв «выше 2 —
  вечный клин» устранён pump-фиксом (см. блок ниже). При изменении
  константы — перегнать burst-гейт `xargs -P80`/`-P200` (см. main.nv,
  док-комментарий `MAX_INFLIGHT_CONNS`).
  `[M-187-nested-spawn-scope-var-cc-fail]` ЗАКРЫТ (фикс `emit_spawn`:
  scope-queue всегда пробрасывается через ctx-поле `_nova_captured_scope_q`;
  заморожен фикстурой `regressions/nested_spawn_scope_var/`) — историческое
  упоминание «в силе» устарело 2026-07-20. Вся spawn-семья компилятор-багов
  демо (throw-segfault, struct-capture, monotonic-ICE, nested-spawn) закрыта
  и заморожена регресс-фикстурами в `regressions/`.
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
- **✅ РЕШЕНО 2026-07-20** (`[M-187-high-concurrency-connection-wedge]`,
  merge `14decdfb1`, opus): вечный клин под массовой одновременной
  нагрузкой соединений. Корень — НЕ-work-conserving pump: под
  connection-storm каждый воркер блокировался в nested `supervised` pump,
  гоняя только файберы СВОЕГО scope и держа ready-child соседнего scope в
  своей deque → взаимный strand → permanent-000. Фикс в рантайме
  (`nova_rt/runtime.c`): pump гоняет popped-файбер независимо от scope —
  глобальный прогресс гарантирован. Историческая bounded-accept-митигация
  (`MAX_INFLIGHT_CONNS = 2`, 2026-07-16) СНЯТА — константа поднята до 16
  (ширина worker-pool) и стала обычным backpressure. Гейты: волна —
  P80×2/P200×2 @MAXPROCS4 (served=16, post=200, без permanent-000);
  follow-up на 16 (Windows, 2026-07-20) — P80×2 (23/28 served) + P200×2
  (38/34 served), post-single 200, idle 15с → 200. Полная история клина —
  `docs/history/simplifications-closed.md`. Plan 211 (park-join) остаётся
  архитектурным research'ем, к живучести демо больше не привязан.
  Нагрузочный тест — `loadtest.ps1` (самодостаточный: сам собирает/
  поднимает/глушит сервер, 7 блоков); в BLOCK 5 часть запросов сверх
  admission-лимита по-прежнему честно отбивается (это design, не клин) —
  критерий живучести (200 ПОСЛЕ BLOCK 5/6) выполняется.
- **✅ РЕШЕНО 2026-07-16** (`[M-187-http-serde-setcookie-serialize-collision]`,
  коммит `96ce6249e`): ручной JSON-обход снят — `main.nv` теперь рендерит
  снапшот через `snapshot_to_json` (typed `std.encoding.serde.json_encode`,
  `report_json.nv`). Корень был НЕ в codegen-диспатче: `nova build`
  (`cmd_build`) не вызывал `inject_synthesized_methods_filtered` для
  `#impl(Serialize)` (в отличие от `nova test`) → mono `json_encode[T]`
  падал в name-only fallback → подхватывал чужой `SetCookie.serialize()`.
  Фикс — один вызов в `cmd_build`. `EmitRecord`'s SSE payload — Plan 221.1
  №141 (2026-07-27) первой попыткой ПОПРОБОВАЛ ту же типизированную
  `json_encode`-миграцию и ОТКАТИЛ её: изолированным репро (2 поля, без
  polaris/http) доказано, что `json_encode`'s порядок полей объекта НЕ
  детерминирован между прогонами одного и того же бинаря для
  record-derived `Serialize` (`JsonSerializer`'s `SerFrame.obj` — обычный
  `HashMap[str, JsonValue]`, без сохранения порядка вставки). Новый маркер
  `[M-json-encode-record-field-order-nondeterministic]` (реестр 221.1 №148)
  зафиксировал корень и ЗАКРЫЛ его 2026-07-29: `JsonObject`
  (`std/src/encoding/json.nv`) — упорядоченная map (порядок вставки, не
  hash-порядок), `SerFrame.obj` теперь несёт её → порядок полей record'а
  на проводе = порядок ОБЪЯВЛЕНИЯ полей, детерминированно между процессами.
  №141 повторён на этой базе и ЗАВЕРШЁН: `EmitRecord` — `#impl(Serialize)`
  + `json_encode`, hand-written строк в `main.nv` больше нет.
- **HTTP JSON-снапшот не байт-в-байт детерминирован** между прогонами с
  одним `seed` — см. «Как это тестируется → Детерминизм» выше (реальные
  часы, не симуляция).
- ~~**Docker — не в этом прогоне**~~ **ЗАКРЫТО** (волна 2 плана 187) —
  см. раздел «Docker» выше: `Dockerfile` + `docker build`/`docker run`
  smoke-гейт зелёный.

## См. также

- `docs/plans/187-flagship-concurrency-demo.md` — план, декомпозиция,
  контракт данных.
- `examples/net/echo_server.nv` — образец accept-loop, на котором построен
  `src/main.nv`.
- `std/src/concurrency/supervised_deadline_test.nv` — образец
  `supervised(deadline:)`, на котором построена настоящая отмена.
- `nova-http/src/servernet/rt/handle_connection_smoke.nv` — образец
  `handle_connection` + `Router` за живым сокетом, на котором построен
  роутинг этого пакета.
