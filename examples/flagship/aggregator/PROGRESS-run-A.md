# Plan 187, Ф.MVP-2, прогон А — чекпоинт прогресса

Обновляется ПЕРЕД каждым коммитом (устойчивость к обрыву).

| # | Деливерабл | Статус | Коммит |
|---|---|---|---|
| 0 | Реструктуризация → backend/app + backend/api | done | (этот) |
| 1 | real-cancel (supervised(deadline:)) | pending | — |
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
