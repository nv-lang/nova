# План 187 — Флагманское демо: конкурентный агрегатор с живой визуализацией (ЧЕРНОВИК)

> **Статус: ЧЕРНОВИК / PROPOSED** (не в очереди). Родитель-research:
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
  **servernet** — живой HTTP/1.1 accept-loop поверх `Net`: per-conn spawned fiber
  под supervised + `CancelToken` graceful-stop. README-статус «178 READY» отстаёт.
- **Кооперативная отмена/дедлайн СЕГОДНЯ**: `std/concurrency/cancellation.nv` —
  `within(ms)` / `race` / `with_timeout` / `supervised(cancel: CancelToken)` (Plan 47).
- **Мок часов СЕГОДНЯ**: `with Time = th.fixed_ms(...)` (std/time, D316).
- **`mock_net()` / `real_net()`** — оба хендлера `Net` (D407, единый Net after 183 Ф.3).
- **Duration / Timestamp / Monotonic** — рабочие (175 Ф.1-части в main).

## 2. Объём (что делаем)

1. **Бек на Nova** — `aggregate(sources, budget) Net Time Fail -> Report`
   (одноуровневый fan-out через `supervised { parallel for }`, сбор mut-захватом —
   обход `[M-parfor-record-result-miscompile]`, закрывается 173.1). Хендлеры:
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
| **Ф.1 — SSE-стриминг** | `[M-178-server-streaming]` (streaming response + write-backpressure D361) — точечный маркер, НЕ весь 178 | живой поток событий бек→фронт (`text/event-stream`) | M |
| **Ф.2 — runtime-отмена/deadline/0-leaks** | 173 Ф.3 (`deadline:`-параметр; `supervised(deadline:)` unimplemented in main — см. `[M-178-server-graceful-deadline]`) + 173.0 (substrate: multi-worker drain-гонка) + 173.1 (`parallel for → []T`, WIP `parallel-collect-173-1`) | отмена рантаймом + leaks-инвариант честен при MAXPROCS>1; чище код сбора | M |
| **Ф.3a — Live health-check + Live-LLM (Ollama)** | Ф.1 (SSE); TLS НЕ нужен (health = HTTP/TCP-замер; Ollama = `localhost:11434` plain-HTTP) | реальные домены + **реальные LLM с машины пользователя** (одобрено owner 2026-07-09: детект `/api/tags`, модель=строка; нет Ollama → кнопка disabled с подсказкой, мок всегда работает) | M |
| **Ф.3b — Live погода (+опц. поиск)** | Ф.3a + **Plan 116 std/tls (PLANNED, rustls)** — open-meteo/DDG/Wikipedia HTTPS-only, 🔴 внешний гейт | open-meteo по-настоящему; опц. поисковый race (бесключевые провайдеры) | S |
| **Ф.4 — фронт до прода + лендинг** | Ф.1 | шрифты, подключение, публикация демо | M |
| **Ф.5 — (опция) вложенность/граф B** | Ф.2 | scope⊃scope как разворот строки; граф-режим для тизера | S |

Слабосвязанные фазы параллелятся. Фронт — не на Nova (язык не про UI; Nova —
бек-звезда, честный нарратив).

**Известные подводные камни (из аудита std/http):**
- `[M-178-server-typed-body]` — typed body через serdejson имеет codegen-дефект →
  сериализацию событий делать через dynamic json / ручную, не typed `.json[T]`.
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
