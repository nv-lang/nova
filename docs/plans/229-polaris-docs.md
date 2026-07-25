<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# План 229 — Полная пользовательская документация Polaris (EN+RU)

**Статус:** 📋 УТВЕРЖДЁН владельцем (2026-07-25: «нужна полная дока по Polaris, русский и анг
как обычно», «можешь отдельным планом»). **Репа-цель:** nova-polaris (`docs/` + README).
**Языковая конвенция:** пара файлов `X.md` (EN) + `X.ru.md` (RU) — образец `channels.md`/
`channels.ru.md` (план 224 Ф.1). **Связи:** 222 (архитектура), 222.3 (extractors — дока-долг
уже записан там баннером), 227 (validate — упомянуть как будущее).

## 0. Принципы (жёсткие)

1. **Источник истины — код**, не планы: каждый описанный тип/метод/сигнатура сверяется с
   фактическим `.nv` в nova-polaris/nova-http master. Выдуманный API = брак волны.
2. **Каждый пример КОМПИЛИРУЕТСЯ**: все код-сниппеты доки собираются в
   `src/doc_samples_test.nv` (module `polaris.doc_samples_test`, test-блоки по разделам) —
   `nova test` этого файла = живой гейт доки; расхождение доки с кодом ловится сборкой.
   В доке у сниппета — тот же текст, что в тест-файле (копия, не «по мотивам»).
3. **Текущая реальность, не будущее**: extractors-сахар (222.3), фаза-B нарезка модулей,
   OpenAPI (222.8) — В РАЗДЕЛЕ «Roadmap», явно помечены «не реализовано», без примеров-фантазий.
   Ручные формы (`req.param/query/json[T]`) документируются как сегодняшний канон.
4. RU — не машинный подстрочник: та же структура, живой язык; терминология Nova-глоссария
   (владение/эффекты/фибра); идентификаторы/код — без перевода.
5. Тон и глубина — уровня Axum/FastAPI-доков: краткое «зачем», пример, ссылки на смежное.

## 1. Состав (docs/ в nova-polaris; каждый файл ×2 языка)

| Файл | Содержание (по фактическому коду) |
|---|---|
| `overview.md` | что такое Polaris; связь с пакетом `http` (ядро) — hyper/axum-модель; минимальный сервер end-to-end; карта модулей |
| `routing.md` | `Router.new`, `@route`/`@get/@post/@put/@delete/@patch` (Result[Router] + цепочки `?`/`!!` — обе формы), шаблоны `{name}`/`{*rest}`, `@nest`, `@fallback` (404), MethodRouter (`get(h).post(h2)`, 405+Allow), конфликты путей = typed-ошибки |
| `handlers-response.md` | `Handler` (голый fn-newtype), `ServerRequest` (param/query/header/json[T]/multipart), `ServerResponse` (text/json/html/bytes/empty/redirect/stream/sse_event), протокол `IntoResponse` (+бланкет Serialize), `StatusCode` (константы `.OK`…, `new -> Result`, `unsafe new_unchecked`) |
| `middleware.md` | `Middleware` (голый newtype), `middleware(fn(req,next))` — канон-форма, `@then`, `Router.@layer` — семантика порядка (первый = внешний, real-chi), что оборачивается (405 per-route — да; глобальный 404 — нет), написание своей batteries-стиль |
| `batteries.md` | cors, compress (gzip), log, ratelimit — конфиг каждой по фактическим сигнатурам, порядок подключения |
| `auth.md` | basic/bearer/JWT-клеймы/session (по auth.nv: конфиги, cookie-политики, 401-поведение) |
| `static-files.md` | `polaris.static`: отдача файлов, safety-границы по коду |
| `websocket.md` | `polaris.ws`: handshake, `WebSocket.with_limit`, upgrade-hook (`@upgrade`), frame-модель |
| `serving.md` | `polaris.serve`/`polaris.net`: serve_router, `ServerPolicy` (лимиты/admission), graceful, background tasks (`BackgroundTasks`), multipart-лимиты, recover-500 |
| `errors.md` | `HttpError`/ErrorKind (из `http`, но через призму сервера), маппинг в статусы (respond-таблица), `@with_url` |
| `roadmap.md` | 222.3 extractors (Path/Query/Json-типы, канон «один Path[T], мульти = record-поля по именам»), 222.8 OpenAPI, фаза B модулей, validate (227) — честно «planned» |

Плюс: **README.md переписать** (EN, короткий: питч + минимал-пример + ссылки на docs/) и
`README.ru.md` добавить.

## 2. Гейты волны

- `nova test src/doc_samples_test.nv` — все сниппеты PASS (главный гейт).
- `nova test src` полный — δ0 (дока-тест ничего не ломает).
- Сверка-греп: каждый упомянутый в доке публичный идентификатор существует в src (скрипт-греп
  списка `@`-методов/типов из доки по исходникам; расхождения = стоп).
- Ссылочная целостность EN↔RU (одинаковый набор файлов/заголовков).

## 3. Исполнение

Одна sonnet-волна в nova-polaris (ветка p-docs): Ф.1 skeleton EN по коду + doc_samples_test →
Ф.2 RU-пара → Ф.3 README×2 + перекрёстные ссылки. Чекпоинт-коммит на фазу. Приёмка
интегратора: гейты + выборочная вычитка фактов по 3-4 файлам против src.

## 4. Поддержание

Правило впредь (в dev-workflow при следующем обновлении): волна, меняющая публичный API
polaris, обязана в том же слиянии править затронутый раздел доки + сниппет-тест (гейт п.2
делает дрейф видимым автоматически).
