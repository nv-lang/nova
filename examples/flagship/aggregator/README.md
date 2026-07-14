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
- **Сервер обслуживает подключения ПОСЛЕДОВАТЕЛЬНО**, не параллельно —
  `[M-187-nested-spawn-scope-var-cc-fail]` (компилятор-баг: `spawn`,
  вложенный в `spawn` под одним `supervised`, CC-FAIL'ит на сгенерированном
  C). Приемлемо для этого флагмана (один браузер-таб), внутренний
  fan-out `aggregate()` при этом ПОЛНОСТЬЮ конкурентен.
- **Возможны редкие зависания сервера под длительной нагрузкой**
  (`[M-187-supervised-nested-fiber-slot-race]`, честно задокументировано,
  НЕ обойдено полностью) — вероятностная гонка в fiber-scheduler'е
  (`nova_rt/fibers.h`) на ГЛУБОКО вложенных `supervised`-скоупах
  (accept-loop → per-connection scope → `aggregate`'s `parallel for` →
  `fetch_guarded`'s `supervised(deadline:)`), обнаружена в этой сессии.
  Симптом: запрос виснет, в stderr — `NOVA_RUNTIME_DUMP`
  (`pending_remote=1`, слот-лик). Не деталь этого пакета — глубина такой
  вложенности нигде больше в кодовой базе не тестировалась. Митигация
  (само-контейнерный `supervised{spawn{...}}` на КАЖДОЕ соединение вместо
  голого вызова) снижает частоту, не убирает гонку. При зависании —
  перезапустить бинарь.
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
