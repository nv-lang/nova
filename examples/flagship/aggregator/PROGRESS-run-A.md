# Plan 187, Ф.MVP-2, прогон А — чекпоинт прогресса

Обновляется ПЕРЕД каждым коммитом (устойчивость к обрыву).

| # | Деливерабл | Статус | Коммит |
|---|---|---|---|
| 0 | Реструктуризация → backend/app + backend/api | done | f8975d63e |
| 1 | real-cancel (supervised(deadline:)) | done (не fallback — настоящая отмена) | 5c9467eee |
| 2 | typed-serde report_json.nv + расширенная схема §9.5 | done | bd98ac530 |
| 5 | Live-легенды (health real_net — РЕАЛЬНО работает; weather — честно заблокирован) | done вне очереди (нужен был перед проектированием main.nv-эндпоинтов) | (этот) |
| 3 | backend/main.nv (accept-loop + endpoints) | pending | — |
| 4 | UI data-glue frontend/index.html | pending | — |
| 6 | README.md | pending | — |

## Деливерабл 0 — детали

- `aggregator/*.nv` → `backend/app/{domain,aggregate,aggregate_test,emit,scenarios}.nv`
  (module `backend.app`, folder-module peers, D78 rev-3 `parent.target`).
- `aggregator/*.nv` (http-слой) → `backend/api/{server,server_test,report_json,report_json_test}.nv`
  (module `backend.api`).
- Кросс-модульные импорты `backend/api` → `backend/app`: относительные
  `import ../app.{...}` (D78 rev-4 относительные импорты `./`/`../`) —
  ПОПЫТКА абсолютного дотted-импорта `import backend.app.{...}` не резолвится
  (dotted absolute импорт мапится на путь ОТ КОРНЯ ВОРКСПЕЙСА посегментно,
  а не на объявленное 2-сегментное имя модуля — `backend.app` искалось как
  `<workspace-root>/backend/app`, не как `examples/flagship/aggregator/backend/app`).
  Внутри `backend/app` файлы стали peers одного модуля — относительные
  `import ./domain.{...}` и т.п. между ними УДАЛЕНЫ (не нужны, общий namespace).
- `.c`-артефакты (gitignored) в корне `aggregator/` удалены (устарели после move).
- Гейт: `nova check flagship/aggregator` — 2/2 PASS (только warnings unused-import,
  унаследованные от старого кода, не новые). `nova test flagship/aggregator
  --strict-effects` — 22/22 PASS (12 aggregate + 10 report_json/server_test).

## Деливерабл 1 — детали

- **Результат: НАСТОЯЩАЯ отмена получилась, fallback НЕ понадобился.**
  `fetch_guarded` (aggregate.nv) теперь гоняет каждый лан против общего
  АБСОЛЮТНОГО дедлайна (`t0 + budget`, один раз в `aggregate()`) через
  реальный `supervised(deadline: shared_deadline) { spawn { fetch_one(...);
  tx.try_send(status) } }` + `with Fail[TimeoutError] = |_| { timed_out = true }`.
  Опоздавший лан ДЕЙСТВИТЕЛЬНО прерывается (его `Time.sleep`/`Net` обрывается
  на дедлайне) — не досиживает свою полную латентность. Разблокировка —
  fibers.h-фикс `[M-178-server-graceful-deadline]` (уже в main до этой сессии).
- Тест-доказательство: `aggregate_test.nv` — лан с latency=300ms, budget=40ms
  → `wall_ms < 250` (было `>= 300` при self-checked). Стабильно 5/5 прогонов
  подряд (без флаки на границе гонки) + существующий 0-leaks тест зелёный
  после реальной отмены.
- **Новый маркер `[M-flagship-spawn-capture-value-struct-ptr-mismatch]`**
  (компилятор-баг, НЕ чинил — обошёл): захват multi-field `value`-структуры
  (`Source`) в ctx вложенного `spawn` внутри `supervised(deadline:)` CC-FAIL'ит
  дважды в одной функции (assign-type mismatch value/pointer + "use of
  undeclared identifier" на read-сайте — макрос-обёртка захвата не
  сгенерирована для этого случая). Скаляры (`idx int`) через тот же путь
  работают нормально. Обход: `fetch_one` принимает поля `Source` как
  отдельные скаляры/`str` (не структуру); `fetch_guarded` разбирает `source`
  на локали ДО spawn. Подробности — комментарий у `fetch_one` в aggregate.nv.
- `[M-flagship-spawn-throw-segfault]` — ПЕРЕПРОВЕРЕН (репро НЕ повторял: тот
  баг про `throw`/`Fail[AggError]` unwind из `spawn`, не про `TimeoutError`
  из `supervised(deadline:)`; наш код по-прежнему НЕ throw'ит AggError —
  `fetch_one` остался `Result`-based). Обход ОСТАВЛЕН как есть — корень не в
  списке разблокировок этого прогона.
- `domain.nv`/`emit.nv`: doc-комментарии `TaskStatus.Cancelled`/`lane_cancelled`
  обновлены (self-checked → real cancel), с историческим follow-up про старую
  simplification оставленным для контекста.
- Гейт: `nova test flagship/aggregator --strict-effects` — 22/22 PASS
  (5 прогонов подряд, без флаки).

## Деливерабл 2 — детали

- `report_json.nv` (backend/api) переведён с ручного `JsonValue`/`HashMap`
  на typed `#impl(Serialize)` DTO (`StatusDto`/`ResultDto`/`HandlersDto`/
  `SnapshotDto`) + `json_encode`/`json_encode_pretty` (`std.encoding.serde`,
  D344). `[M-178-server-typed-body]` (закрыт в main) больше не блокирует.
  `TaskStatus` НЕ сериализуется напрямую auto-derive'ом (его wire-форма —
  externally-tagged, `"Cancelled"`/`{"Failed":"..."}`, не то, что нужно UI) —
  `status_dto()` мапит вручную в фиксированную форму `{state, error}`.
- Схема расширена по §9.5: верхний уровень + `budget_ms`, `legend`, `mode`,
  `seed`, `handlers{net,time}`, `fibers_spawned`, `fibers_closed`; per-result
  + `kind`/`probes` (из `Source`, по id — `parallel for` даёт completion-order,
  не исходный порядок sources).
- `[M-187-leaks-introspection]`: `fibers.slot_count()`-семейство — честный
  no-op sentinel на Windows (текущая гейт-платформа) — реальный before/after
  delta показал бы фальшивый 0/0. Fallback (разрешён планом): структурный
  счёт — 2 fiber на лан (`parallel for`-спавн + вложенный
  `supervised(deadline:)`-спавн), оба гарантированно join'ятся до возврата
  `aggregate()` → `fibers_spawned == fibers_closed == 2 × fanout` всегда;
  "0 leaks" — структурный инвариант дизайна, не измеренное число.
- D419 `"${v:#}"` НЕ применим к голому `#impl(Serialize)`-DTO напрямую (нет
  `@display_fmt`, Serialize ≠ Display) — pretty через `json_encode_pretty`
  напрямую (`snapshot_to_json_pretty`).
- `Option[T]::None` сериализуется в JSON `null` СО ВСТАВЛЕННЫМ ключом (не
  опускается) — `#serde(skip)` пока не поддержан синтезом
  (`[M-180-serde-field-attributes]`, чужой существующий маркер, не новый);
  `status.error` в UI — «null или строка», не «есть ключ или нет».
- `server.nv`: `snapshot_mux`/`snapshot_for`/`weather_snapshot_mux`/
  `health_snapshot_mux` обновлены под новую сигнатуру (принимают
  legend/mode/seed/handlers). Тесты (`server_test.nv`, `report_json_test.nv`)
  переписаны под typed API.
- Гейт: `nova test flagship/aggregator --strict-effects` — 24/24 PASS
  (12 aggregate + 12 report_json/server_test).

## Деливерабл 5 — детали (сделан вне очереди, до №3/4)

Порядок исполнения отклонён от плана: исследование live-путей (какие эффекты/
пакеты реально доступны для HTTPS) понадобилось ДО проектирования эндпоинтов
main.nv (№3) — иначе пришлось бы дважды переписывать роутинг. Деливерабл
самодостаточен (`backend/app/live.nv` + `live_test.nv`, не зависит от main.nv).

- **health, live: РЕАЛЬНО работает.** `aggregate_live_health(budget)` —
  та же гонка `supervised(deadline:)`, что и `aggregate()` (деливерабл 1),
  но fetch — настоящий `std.net.resolve` + `TcpStream.connect` (порт 443)
  против 5 реальных доменов (github.com, cloudflare.com, wikipedia.org,
  npmjs.org, pypi.org). Ручной smoke (`nova build` + запуск, см. ниже) —
  5/5 done, ~100мс каждый, wall_ms=147.
- **weather, live: ЧЕСТНО ЗАБЛОКИРОВАН**, не мок с фальшивыми данными.
  Каждый лан `Failed` с текстом-подсказкой, содержащим маркер
  `[M-187-weather-live-tls-diamond-blocked]` — не «недоступность сети», а
  diamond-зависимость: `examples/nova.toml` тянет `tls` ПУТЁМ (локальный
  worktree), `nova-http/nova.toml` тянет `tls` ЧЕРЕЗ git+version (другая
  физическая копия) — D420 `[replace]` не пробрасывается в зависимости
  зависимостей. Два независимых симптома пойманы и задокументированы в
  заголовке live.nv:
  1. через `http.transport.real_http()` — `no function 'read_to_vec'/'close'
     in module 'tls'` (переменная `tls` в nova-http's real.nv затеняет имя
     пакета под diamond'ом — ИМЕННО тот анти-паттерн, о котором предупреждает
     комментарий в `examples/tls/echo_client.nv`);
  2. через прямой `import tls.{TlsStream, ClientConfig}` в live.nv (когда
     `backend/api` с `http.server` и `backend/app` с прямым `tls` попадают в
     ОДИН compile unit) — `[E_METHOD_REDEFINITION] TlsStream.connect уже
     определён` (checker видит ДВЕ физические копии `tls`, не дедуплицирует).
  nova-http/nova-tls (соседние репо) НЕ трогались (вне периметра сессии).
- Попутно пойман и обойдён **[M-187-monotonic-plus-operator-mono]**:
  `t0 + budget` (оператор) ломается при ВТОРОМ/ТРЕТЬЕМ вызове той же
  комбинации `Monotonic + Duration` в одном CU (`aggregate.nv` использует
  оператор один раз — работает; `live.nv` добавляет ещё 2 вызова — CC-FAIL
  "returning 'int' from a function with incompatible result type
  'NovaValue_Monotonic'" в generic-обёртке `nova_vr_binop_...`) — того же
  семейства, что уже задокументированные mono-коллизии в этом кодовой базе.
  Обход: `t0.plus(budget)` (именованный метод) вместо оператора — только в
  live.nv, `aggregate.nv`'s единственный call-site не трогался.
- Ручной smoke (`nova build backend/app/_smoke_live.nv` — временный файл,
  удалён после проверки, НЕ в git) против настоящей сети:
  ```
  --- live health ---
  done=5 failed=0 cancelled=0 wall_ms=147
    pypi.org OK 96ms
    wikipedia.org OK 97ms
    npmjs.org OK 106ms
    cloudflare.com OK 107ms
    github.com OK 107ms
  --- live weather (expected: blocked, honest fail) ---
  done=0 failed=4 cancelled=0 wall_ms=15
    London FAIL transport error: live weather unavailable in this build:
    [M-187-weather-live-tls-diamond-blocked] ...
  ```
- Офлайн-тесты (`live_test.nv`, `mock_net()` — гейт остаётся зелёным без
  сети): fan-out shape (5 хостов), tiny-budget-отменяет-всё, weather честно
  проваливается с маркером в тексте ошибки, source-таблицы совпадают по id.
- Гейт: `nova test flagship/aggregator --strict-effects` — 29/29 PASS
  (17 aggregate [+5 live] + 12 report_json/server_test).
