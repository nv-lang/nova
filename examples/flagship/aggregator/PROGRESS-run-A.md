# Plan 187, Ф.MVP-2, прогон А — чекпоинт прогресса

Обновляется ПЕРЕД каждым коммитом (устойчивость к обрыву).

| # | Деливерабл | Статус | Коммит |
|---|---|---|---|
| 0 | Реструктуризация → backend/app + backend/api | done | f8975d63e |
| 1 | real-cancel (supervised(deadline:)) | done (не fallback — настоящая отмена) | (этот) |
| 2 | typed-serde report_json.nv | pending | — |
| 3 | backend/main.nv (accept-loop + endpoints) | pending | — |
| 4 | UI data-glue frontend/index.html | pending | — |
| 5 | Live-легенды (health/weather) | pending | — |
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
