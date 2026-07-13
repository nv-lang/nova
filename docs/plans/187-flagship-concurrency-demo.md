# План 187 — Флагманское демо: конкурентный агрегатор с живой визуализацией

> **Ред.3 (2026-07-13, вечер) — сверка с кодом после дневных слияний; все внешние гейты волны-1 СНЯТЫ:**
> 1. **TLS-гейт Ф.3b МЁРТВ.** «Plan 116 std/tls (PLANNED, rustls)» не существует — TLS приехал
>    планами 193/202/203: публичная репа **nv-lang/nova-tls** (mbedTLS-шим, реальные хендшейки
>    в тестах зелёные), https-транспорт в http-клиенте подключён (`transport/real.nv` → tls).
>    **Live-погода (open-meteo) разблокирована**; §9.4 п.5 «Погода disabled» устарел.
> 2. **173-остатки закрыты целиком:** 173.0 (drain-гонка) ✅, 173.1 (`parallel for → []T`) ✅,
>    173-хвосты 2026-07-13 ✅ (MultiError-агрегация + propagation-trace per-fiber). Оговорка
>    «тесты под NOVA_MAXPROCS=1» снята. Образец real-cancel `supervised_deadline_test`
>    ЗЕЛЁНЫЙ (CC-FAIL WriteBuffer.cancel закрыт миграцией CancelToken в prelude, 2026-07-13).
> 3. **http живёт НЕ в std:** план 203 (2026-07-13) вынес его в публичную **nv-lang/nova-http**
>    (module-path `http.*` НЕ изменился; examples уже подключены зависимостью; path-dep =
>    dev-форма до плана 204). Упоминания «std/http» ниже читать как «пакет http (nova-http)».
> 4. **Гейт-числа:** conformance-база на 2026-07-13 = **113/0 + 7 SKIP** (не 97/0); критерий
>    §9.6 п.2 = δ0 от текущей базы. Плюс: examples обязаны собираться с `--strict-effects`.
> 5. **Известный риск:** гуляющий рантайм-рейс (~30% прогонов больших merged-CU, предсуществующий,
>    класс stale fiber-slot) — может флакать тесты агрегатора; точечный ре-ран = норма протокола,
>    решение о глубокой инвестигации — за владельцем.
>
> **Статус: В РАБОТЕ (Ред.2, 2026-07-13).** MVP-ядро ✅ (коммит `514bcd8d5` — бек-библиотека+тесты),
> но **запускаемого веб-приложения ещё НЕТ** — добор = Ф.MVP-2, см. **§9** (честный статус,
> решения владельца 2026-07-13: запуск обязателен локально И под Docker, декомпозиция для
> исполнителя, контракт данных snapshot↔мокап). Родитель-research:
> [docs/research/15-flagship-concurrency-showcase.md](../research/15-flagship-concurrency-showcase.md)
> — там полный дизайн бека, легенды, история решений по визуалу, мокап.
> **Спека:** нового D-блока, вероятно, НЕ требует (демо на существующих
> примитивах); при вскрытии дыры — завести D в срок.
> **Маркер:** `[M-flagship-concurrency-demo]` (завести в backlog при старте).
> **Гейты (уточнены аудитом кода 2026-07-09, Ред.1):** точечные, НЕ «весь план»:
> SSE ← `[M-178-server-streaming]` (streaming response в server — deferred-маркер,
> остальной std/http УЖЕ в main); Live-Погода ← **Plan 116 std/tls (PLANNED)** —
> open-meteo = HTTPS-only; runtime-отмена/deadline ← 173 Ф.3/173.0/173.1
> (но кооперативная отмена `supervised(cancel:)`/`within` РАБОТАЕТ уже сейчас).
>
> Дата черновика: 2026-07-09 (Ред.1 — сверка с кодом в тот же день). До
> постановки в очередь закрыть открытые решения §7.
>
> **§7 — РЕШЕНО (владелец 2026-07-12):**
> 1. Эмиссия событий визуализации → **Emit-эффект** (не канал): `aggregate` несёт эффект `Emit`,
>    хендлер маршрутизирует события во фронт; ambient, тестируемо, витрина эффектов.
> 2. Хостинг демо → **локальный сервер** (опц. в Docker).
> 3. Fan-out → **плоский для MVP** (N братьев); вложенный (группы→под-источники + каскад-отмена) — расширение позже.
> 4. Live-источники → **оркестратор определяет** сам.
> 5. Seeded-latency API → **оркестратор определяет** сам.
> 6. Сверка README-статуса 178 → док-аккуратность, оркестратор сделает сам (не заявлять SSE готовым до `[M-178-server-streaming]`).
> 7. Ollama-стриминг → **НЕ в MVP** (в будущем да).
>
> **ФРОНТЕНД = МОКАП ВЕРБАТИМ (владелец 2026-07-12, жёстко):** UI берётся ТОЧЬ-В-ТОЧЬ из
> `docs/research/assets/15-showcase-mockup.html` — это уже готовый, утверждённый UI (дизайн + механика:
> waterfall hero-lanes, sequential-ghost, swarm-wall, Demo/Chaos, deadline/cancel, light/dark). UI заново НЕ
> проектировать. Работа 187 = (а) **Nova-backend** — агрегатор с fan-out (плоский) + deadline + каскад-отмена +
> `Emit`-эффект событий; (б) **проводка** Emit-событий в data-модель мокапа (заменить synthetic/demo-данные
> реальными — через SSE `[M-178-server-streaming]` или, до него, снапшот-fallback). Т.е. фронт готов, дело за
> бэком + мостом данных.

## 0. Цель (одна фраза)

Первое флагманское приложение Nova: конкурентный агрегатор, чья работа —
fan-out по N источникам с дедлайном и отменой — **зрима** в живой
веб-визуализации (waterfall), доказывая уникальность structured concurrency +
эффектов «за 5 секунд, а не абзацем README».

Killer-нарратив: одна строка `fn aggregate(...) Net Time Fail -> Report` +
анимация, где видно fan-out, дедлайн, каскадную отмену опоздавших и **0 leaks**.

## 1. Что уже готово (вход в план; сверено с кодом 2026-07-09)

- **Дизайн бека** — research §6 (слои, сигнатура, тесты без моков, обход gap 6.2).
- **Визуальная модель** — waterfall (полоса=время); утверждена владельцем.
- **Мокап фронта** — `docs/research/assets/15-showcase-mockup.html` (v8): обе темы,
  две легенды (Погода/Health), Demo/Chaos/Live, дедлайн, отмена, sequential-ghost.
- **Две легенды** — Погода (open-meteo, без ключа) и Health-check реальных доменов.

**Инфраструктура Nova, которая УЖЕ в main (аудит std/, больше чем ожидалось):**
- **std/http почти целиком** (178 Ф.1-Ф.3 де-факто приземлились): message-model
  (D358/D359 must-consume `Body` + `BodyReader`), client (mock+real transport,
  decompress, typed json), **server** (`ServeMux` + middleware-onion + `serve_once`),
  **servernet** — HTTP/1.1 поверх `Net`. README-статус «178 READY» отстаёт.
  **⚠ Ред.2-поправка (2026-07-13):** аудит переоценил servernet — экспортируется только
  `handle_connection(stream, mux)` + smoke; **accept-LOOP функции нет** — цикл
  `TcpListener.accept → spawn handle_connection` собирается в самом приложении по образцу
  `examples/net/echo_server.nv` (см. §9.4 п.0).
- **Кооперативная отмена/дедлайн СЕГОДНЯ**: `std/concurrency/cancellation.nv` —
  `within(ms)` / `race` / `with_timeout` / `supervised(cancel: CancelToken)` (Plan 47).
- **Мок часов СЕГОДНЯ**: `with Time = th.fixed_ms(...)` (std/time, D316).
- **`mock_net()` / `real_net()`** — оба хендлера `Net` (D407, единый Net after 183 Ф.3).
- **Duration / Timestamp / Monotonic** — рабочие (175 Ф.1-части в main).

## 2. Объём (что делаем)

1. **Бек на Nova** — `aggregate(sources, budget) Net Time Fail -> Report`
   (одноуровневый fan-out через `supervised { parallel for }`, прямой сбор
   `[]Report` — `[M-parfor-record-result-miscompile]` полностью закрыт
   2026-07-13, mut-захват workaround больше не нужен). Хендлеры:
   `real_net()` и **`mock_net()`** (существует; НЕ «fake_net» — имя из std/net/mock.nv)
   + seeded-обёртка латентностей поверх него.
2. **Эмиссия событий хода задач** — task started/progress/done/failed/cancelled +
   summary (fanout/done/failed/cancelled/wall/sequential/speedup/leaks).
3. **HTTP-эндпоинт со стримингом** — SSE (`text/event-stream`), поверх 178.
4. **Фронт** — довести мокап до продового: подключить к SSE, реальные шрифты
   (inline @font-face), убрать засев в Live-режиме.
5. **Три легенды в проде** — Погода (open-meteo), Health-check и **LLM-роутер**
   (owner 2026-07-09: только бесплатное, «скачал и запустил»): fan-out промпта по
   N моделям, first-wins/лучший, опоздавших отменить. Live-источник — **Ollama**
   (`localhost:11434`, обычный HTTP → TLS/116 НЕ нужен; модели с машины пользователя
   через `/api/tags`; латентности инференса 1-10с = лучшая визуализация из трёх);
   без Ollama — мок-LLM с нейтральными именами (`model-a`, не чужие бренды).
   On-brand для «языка AI-эры»; код `aggregate` тот же, меняется список источников.
   **Два паттерна на экране** (owner 2026-07-09): (a) *aggregate* — собрать всех до
   дедлайна (погода/health); (b) **race / first-wins** — первый пригодный ответ
   побеждает, **проигравшие отменяются** (LLM-роутер; `race` уже есть в
   std/concurrency/cancellation.nv). Опциональная 4-я легенда — **поисковый race**
   (google/yandex-стиль «кто первый»): Live только через бесключевые API
   (DuckDuckGo IA / Wikipedia / SearXNG; все HTTPS → за гейтом 116); официальные
   Google/Yandex API требуют ключи → критерий «скачал и запустил» не проходят,
   только мок-режим.
6. **Тесты без моков** — `with Net = mock_net()` (+ seeded-латентности) и
   `with Time = th.fixed_ms(...)` (оба существуют): проверка
   done/failed/cancelled/leaks==0 на засеянном сценарии (позитив + негатив).
7. **Доки** — README-раздел/лендинг nv-lang.org; ссылка на живое демо (Artifact
   или хостинг); обновить research §9 (ассеты).

## 3. Фазы (сейчас vs позже; по гейтам)

| Фаза | Зависит от (уточнено аудитом) | Что даёт | Оценка |
|---|---|---|---|
| **Ф.0 — прототип на СУЩЕСТВУЮЩЕМ std/http** | ничего: servernet accept-loop + ServeMux уже в main | `aggregate` + **настоящая кооперативная отмена** (`within`/`supervised(cancel:)` — уже есть, НЕ «мягкий бюджет»); polling-эндпоинт `/status` (JSON снапшот хода) | S |
| **Ф.1 — SSE-стриминг** | ~~`[M-178-server-streaming]`~~ ✅ **гейт СНЯТ 2026-07-12** (`ServerResponse.sse`/`sse_event` в main, std/http/server + streaming_test) | живой поток событий бек→фронт (`text/event-stream`); в Ф.MVP-2 — replay-вариант (§9.3 п.3) | M |
| **Ф.2 — runtime-отмена/deadline/0-leaks** | 173 Ф.3 ЗАКРЫТА (2026-07-12/13): `deadline:`/`timeout:`-параметр landed (D408, 2026-07-06) + захардено — нашёлся и починен реальный leak (спавненный child на `Time.sleep` не unwind'ился на отмену области; см. `[M-178-server-graceful-deadline]` в backlog-followups.md). Остаётся: 173.0 (substrate: multi-worker drain-гонка) + 173.1 (`parallel for → []T`, WIP `parallel-collect-173-1`) + servernet accept-LOOP функции ещё нет вообще (только `handle_connection` + smoke) — проводка bounded-drain в реальный цикл не сделана | отмена рантаймом + leaks-инвариант честен при MAXPROCS>1; чище код сбора | M |
| **Ф.3a — Live health-check + Live-LLM (Ollama)** | Ф.1 (SSE); TLS НЕ нужен (health = HTTP/TCP-замер; Ollama = `localhost:11434` plain-HTTP) | реальные домены + **реальные LLM с машины пользователя** (одобрено owner 2026-07-09: детект `/api/tags`, модель=строка; нет Ollama → кнопка disabled с подсказкой, мок всегда работает) | M |
| **Ф.3b — Live погода (+опц. поиск)** | Ф.3a; ~~Plan 116 std/tls~~ ✅ **гейт СНЯТ Ред.3** (TLS = nv-lang/nova-tls, https-транспорт в http подключён) | open-meteo по-настоящему; опц. поисковый race (бесключевые провайдеры) | S |
| **Ф.4 — фронт до прода + лендинг** | Ф.1 | шрифты, подключение, публикация демо | M |
| **Ф.5 — (опция) вложенность/граф B** | Ф.2 | scope⊃scope как разворот строки; граф-режим для тизера | S |

Слабосвязанные фазы параллелятся. Фронт — не на Nova (язык не про UI; Nova —
бек-звезда, честный нарратив).

**Известные подводные камни (из аудита std/http):**
- `[M-178-server-typed-body]` — ЗАКРЫТ (2026-07-12, баг-фиксер 196): typed body
  через serdejson теперь компилируется в http-CU — можно использовать typed
  `json_decode_body[T]`/`.json_as[T]()`, dynamic-JSON workaround больше не нужен.
- Тесты демо гонять под `NOVA_MAXPROCS=1` до закрытия 173.0 (multi-worker
  drain-гонка субстрата); leaks-инвариант при MAXPROCS>1 — после 173.0.
- 173 Ф.3 удалит `with_timeout` в пользу `deadline:`/`timeout:`-параметров (после
  175) — Ф.0-код писать на `within`/`supervised(cancel:)`, миграция на `deadline:`
  в Ф.2 (ожидаемая, не сюрприз).

## 4. Тесты (позитив + негатив)

- **POS:** засеянный сценарий → done==N, failed==k, cancelled==m, **leaks==0**;
  SSE отдаёт корректную последовательность событий; speedup > 1 vs sequential.
- **POS:** переключение источника (Погода↔Health) — тот же `aggregate`, разный
  `with Net` компилируется и работает.
- **NEG:** источник за дедлайном → отменён (не «повис»), ресурсы закрыты.
- **NEG:** источник падает (503/timeout) → `Fail` surface, не роняет весь запрос.
- **EDGE:** все источники медленные → все отменены по дедлайну, 0 leaks.
- **EDGE:** пустой список источников → пустой Report, без паники.

## 5. Критерии приёмки

1. Бек компилируется и работает через C-codegen (`nova build`/`nova test`) —
   интерпретатора нет.
2. **Без упрощений как для прода:** отмена — настоящая (после 173), не «воркер
   сам проверил бюджет»; SSE — реальный стриминг (после 178); каждое временное
   упрощение (мягкий дедлайн в Ф.0) — явный маркер, не молчаливое.
3. **0 leaks** — проверяемый инвариант в тестах (fiber-leak == 0 после отмены).
4. Тот же код `aggregate` обслуживает обе легенды (Погода/Health) сменой хендлера.
5. Живое демо доступно по ссылке; читается «за 5 секунд».
6. Позитивные И негативные тесты зелёные; conformance δ0.

## 6. Правила исполнения (для агентов)

- Тестировать только через C-codegen (`nova build`/`nova test`); `nova run` нет.
- Коммит: `git add` конкретных файлов (никогда `-A`/`.`), `git commit -s` (DCO),
  без `Co-Authored-By`. Перед коммитом `git diff --cached --stat`.
- Фоновые агенты: НЕ `git stash` (общий .git); baseline = temp-worktree/commit-reset.
- Отвечать по-русски; модели по карте (haiku=механика, sonnet=исполнение,
  opus=разведка/архитектура).

## 7. Открытые решения (закрыть до постановки в очередь)

1. **Эмиссия событий из воркера** — доп. эффект `Emit` vs канал в оркестратор?
   (Каналы существуют — docs/channels.md; канал выглядит дешевле нового эффекта.)
2. **Где хостить живое демо** — Artifact / GitHub Pages / nv-lang.org?
3. **Вложенность (Ф.5)** — нужна ли вообще, или одноуровневый fan-out достаточно?
4. **Реальные источники Live** — список доменов для health-check; города для погоды.
5. ~~Готовность гейтов~~ — ✅ ЗАКРЫТО аудитом 2026-07-09: гейты точечные (см. §3);
   std/http Ф.1-Ф.3 уже в main; блокеры только `[M-178-server-streaming]` (SSE),
   116-TLS (Live-погода), 173.0/.1/Ф.3 (runtime-deadline + MAXPROCS>1 leaks).
6. **NEW: seeded-латентности поверх mock_net** — форма API (обёртка-хендлер
   `seeded_net(seed, profile)` в демо-пакете, не в std)?
7. **NEW: договорка о README-статусе 178** — код Ф.1-Ф.3 в main, README «READY»;
   при старте 187 сверить фактический остаток 178 (маркеры) и не дублировать работу.
8. **NEW: Ollama-интеграция** — формат стриминга ответа (`/api/generate` NDJSON
   stream → прогресс строки = токены!); критерий «пригодного» ответа для race
   (первый токен vs полный ответ); поведение при 1 установленной модели (race из
   одного — вырожденный: fallback на aggregate-вид или прогнать одну модель с
   разными промптами?).

## 8. Вне объёма

- Фронт на Nova (язык не про UI).
- Продовый метапоиск с платными API (легенда отвергнута — см. research §5).
- DAP/отладчик, native codegen — отдельные планы.

## 9. Ред.2 (2026-07-13) — честный статус MVP и добор до запускаемого приложения

### 9.1 Что фактически есть после «MVP закрыт» (коммит `514bcd8d5`)

- ✅ Бек-ядро `examples/flagship/aggregator/`: domain / aggregate (плоский fan-out,
  self-checked soft-deadline) / Emit-эффект / scenarios (Demo + Chaos-seeded) /
  report_json (динамический JsonValue — workaround снятого typed-body) /
  server.nv (`snapshot_mux`, тестируется через `serve_once`) + 22 теста.
- ❌ НЕТ точки входа (`fn main`) — приложение **незапускаемо**.
- ❌ НЕТ UI в примере (мокап не скопирован и не подключён).
- ❌ НЕТ живого сервера (accept-loop в примере не собран; servernet даёт только
  `handle_connection` — см. Ред.2-поправку в §1).
- ❌ НЕТ README и Docker.

Причина разрыва: план был ЧЕРНОВИК — ни одна фаза явно не владела «запускаемым
приложением» (main/README/Docker не числились деливераблом ни одной фазы), а
§1-аудит переоценил готовность servernet. Исполнитель закрыл «MVP» узко.

### 9.2 Разблокировки после Ред.1 (все уже в main)

- `[M-178-server-streaming]` ✅ ЗАКРЫТ 2026-07-12: `ServerResponse.sse`/`sse_event`
  (std/http/server + streaming_test) — гейт Ф.1 снят.
- `[M-178-server-typed-body]` ✅ ЗАКРЫТ 2026-07-12 (merge `77239c014`): typed
  `.json[T]` работает — report_json можно перевести на `#impl(Serialize)`.
- 173 Ф.3 ✅: `supervised(deadline:)` (D408) + прерываемый `Time.sleep`
  (`c4b8c38d9`, fibers.h) — настоящий real-cancel доступен; образец:
  `std/concurrency/supervised_deadline_test.nv`.

### 9.3 Решения владельца (2026-07-13)

1. **Запуск обязателен ЛОКАЛЬНО и ПОД DOCKER** (Docker из «опц.» §7 п.2 →
   обязательный деливерабл; волна 2, см. §9.4 п.7).
2. **UI: внешний вид — как в мокапе (утверждён, не трогать); внутренности (JS)
   — переделывать свободно** под реальные данные. Смягчение прежнего «вербатим»:
   вербатим = дизайн/вёрстка/стили/механика анимации; синтетический `providers()`
   и data-модель можно заменять целиком на серверные данные. Файл:
   `examples/flagship/aggregator/frontend/index.html` (копия мокапа с переработанным JS).
3. **SSE в этом заходе = честный replay** Emit-событий завершённого прогона
   (первая запись — `event: replay_info`); живой per-request стрим — отдельный
   маркер `[M-187-sse-live-stream]` (требует эффект-несущего пути в соединение,
   ServeMux-handler'ы чистые — см. заголовок server.nv).
4. LLM/Ollama — НЕ в MVP (подтверждение §7 п.7); в UI сегмент LLM остаётся
   в demo-синтетике или disabled.

### 9.4 Ф.MVP-2 — запускаемое веб-приложение (декомпозиция для исполнителя)

**Структура каталога (решение владельца 2026-07-13: полноценное веб-приложение —
бек и фронт по разным папкам):**

```
examples/flagship/aggregator/
  backend/
    main.nv           — тонкая точка входа: wiring (real_net, порт+env) + accept-loop
    app/              — ядро, чистая логика (свой модуль, D78):
                        domain.nv, aggregate.nv, emit.nv, scenarios.nv
    api/              — HTTP-слой (свой модуль): server.nv (mux/endpoints),
                        report_json.nv, sse.nv
  frontend/
    index.html        — копия мокапа с переработанным JS; self-contained
                        (стили+JS внутри) предпочтителен
  README.md           — сборка + запуск (локально; волной 2 — Docker)
  Dockerfile          — волна 2 (§9.4 п.7)
```

Обоснование: не сваливать исходники в корень backend/ — конвенция соседей
(Go `cmd/` + `internal/{domain,api}`, Rust `main.rs` + модули-папки): тонкий
вход + слои по папкам; флагман — ещё и витрина структуры настоящего
Nova-приложения, D78 «папка = модуль» ложится на слои напрямую. Глубже двух
слоёв для 8 файлов не дробить. `*_test.nv` — рядом со своим модулем.

Миграция существующих `aggregator/*.nv` → `backend/{app,api}/` — тем же заходом
(module-пути выровнять по фактической папке; file+folder одного имени запрещён).
`embed()`-путь фронта из backend-кода — проверить относительно корня пакета
(E_EMBED_OUTSIDE_PROJECT не должен сработать — frontend внутри того же примера).

| # | Деливерабл | Детали |
|---|---|---|
| 0 | `backend/main.nv` | `fn main`: `real_net()` + real time; цикл `TcpListener.bind(127.0.0.1:8187).accept → spawn servernet.handle_connection(stream, mux)` по образцу `examples/net/echo_server.nv`; порт — const + env-override |
| 1 | real-cancel | aggregate.nv: self-checked soft-deadline → `supervised(deadline:)`; снять соответствующие `[M-flagship-*]`-обходы; тест «опоздавший отменяется раньше своей латентности» (wall_ms < бюджет+слак). Если гонка воспроизводится — repro + доклад, оставить self-checked |
| 2 | typed-serde | report_json.nv: JsonValue → `#impl(Serialize)` typed-путь; тесты обновить |
| 3 | endpoints | `GET /` → frontend/index.html (`embed()`); `GET /api/snapshot` (есть, fallback); `GET /api/run?legend=weather\|health&mode=demo\|chaos&seed=N` → JSON свежего прогона; `GET /api/events` (те же параметры) → SSE-replay |
| 4 | UI-внутренности | frontend/index.html: Live-сегмент разблокировать → `EventSource` на /api/events + fallback-poll /api/snapshot; Demo/Chaos-кнопки → /api/run; JS-модель — по контракту §9.5 |
| 5 | легенды Live | Live health-check = plain TCP/HTTP-замер через `real_net()`; **Погода-Live ДОСТУПНА** (Ред.3: https через nova-tls) — open-meteo без ключа; при недоступности сети — мок с подсказкой |
| 6 | README.md | сборка + запуск локально (Windows/Unix), порт, env |
| 7 | Docker (волна 2) | Dockerfile: Linux-сборка компилятора+рантайма+примера. 🔴 ГЕЙТ-РИСК `[M-nova-linux-build]` (Linux-сборка Nova не верифицирована) — при блокере зафиксировать маркер `[M-187-docker-linux-build]`, не обходить молча |

### 9.5 Контракт данных snapshot ↔ UI (карта для исполнителя)

**JSON снапшота** (report_json.nv, расширить текущую форму):
верхний уровень `{fanout, done, failed, cancelled, wall_ms, sequential_ms,`
`budget_ms (NEW), legend (NEW), mode (NEW), seed (NEW), results[]}`;
каждый result: `{id, kind (NEW ← Source), probes (NEW ← Source),`
`status{state: "done"|"failed"|"cancelled", error?}, elapsed_ms}`.

**Маппинг в модель UI** (бывш. SCENARIOS/providers мокапа):
`id→id`; `kind→kind`; `probes→probes`; `status.state`: done→outcome `done`
(`slow`, если elapsed_ms > 1300), failed→`fail` (failAt = elapsed_ms/1000),
cancelled→`cancel` (cancelAt = budget_ms/1000); `elapsed_ms/1000→total`;
`DEADLINE = budget_ms/1000` (снять хардкод 2.0 в JS); HUD: done/fail/cancel +
speedup = sequential_ms/wall_ms + «N reqs closed · 0 leaks» из Report.
Swarm-wall: до вложенного fan-out (Ф.5) скрыть в Live-режиме либо кормить
теми же results.

**SSE-события** (таксономия = emit.nv): `event:` `replay_info` (первым) |
`lane_started` | `lane_done` | `lane_failed` | `lane_cancelled` | `run_summary`;
`data:` JSON тех же форм (result-объект / summary-объект); порядок = порядок
Emit при прогоне.

### 9.6 Гейты приёмки Ф.MVP-2

1. `nova build` → бинарь; запуск; curl `/` (HTML), `/api/snapshot` (JSON),
   `/api/run` (JSON), `/api/events` (SSE) — вывод приложить к отчёту.
2. `nova test examples/flagship/aggregator` зелёный; conformance один CU δ0 (97/0).
3. Каждое временное упрощение — явный маркер (`[M-187-sse-live-stream]`, при
   Docker-блокере `[M-187-docker-linux-build]`).
4. Волна 2: `docker build` + `docker run` → тот же curl-smoke снаружи контейнера.
