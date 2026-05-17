# Plan 60 — nv-lang.org dogfooding initiative

> **Цель:** реальный сайт проекта `nv-lang.org`, написанный (постепенно) на самом
> Nova. Не «сайт-визитка как у всех», а **продакшен-площадка для самого
> языка** — самый честный показатель что Nova работает.

## Context

Сейчас (2026-05-17) у Nova есть зарегистрированный домен `nv-lang.org`, но
страница за ним пустая. Этот план превращает её сначала в минимальный
статический сайт (чтобы URL работал к публичному релизу), а потом — поэтапно —
в полноценный сервер, написанный на Nova.

**Зачем (главные цели по приоритету):**

1. **Dogfooding.** Выявляет реальные дыры в stdlib, заставляет принять
   отложенные дизайн-решения, проверяет runtime под живой нагрузкой.
   Так делают все серьёзные языки (Rust → rustc на Rust, Go → cmd/compile на
   Go, TypeScript → tsc на TS, Zig → компилятор на Zig).
2. **Самый сильный маркетинг.** «Этот сайт написан на Nova и работает»
   убедительнее любых benchmark-таблиц.
3. **Production-площадка.** Cancel, timeout, recovery, M:N scheduler под
   реальным трафиком — нельзя проверить на синтетических тестах.
4. **Reference implementation.** Будущие пользователи увидят рабочий пример
   HTTP-сервера, static-site-generator, blog-движка на Nova.

**Что не цель:** красивый сайт, маркетинговая воронка, лидген, SEO в топ. Цель
— честная техническая площадка с минимально достаточным дизайном.

## Repository

- **`nv-lang/www`** — отдельный репозиторий специально для сайта (не внутри
  `nv-lang/nova`).
- Локально: `D:\Sources\nv-lang-www\`.
- Лицензия контента: CC-BY-4.0. Лицензия кода (когда появится): MIT OR Apache-2.0.

## Фазы

### Ф.0 — Public URL (1-2 часа, **в работе 2026-05-17**)

**Цель:** чтобы `https://nv-lang.org` отдавал реальную страницу к моменту
публичного анонса. Минимальный статический сайт на GitHub Pages.

**Stack:**
- Plain HTML + минимальный CSS (~130 строк). Zero JavaScript.
- Без Jekyll / Hugo / Astro — потом всё равно перепишем через Nova-генератор.
- Hosting: GitHub Pages (бесплатно).
- DNS/SSL/CDN: Cloudflare (бесплатно, уже настроено).

**Файлы (`nv-lang/www`):**
```
index.html        — EN, default (/)
ru/index.html     — RU (/ru/)
style.css         — общий минимальный стиль (Go-inspired typography focus)
CNAME             — nv-lang.org
README.md
LICENSE           — CC-BY-4.0
.gitignore
```

**Контент главных (обе версии):**
- Headline + tagline («general-purpose language built around algebraic
  effects, static contracts, and AI-first ergonomics»)
- **Bootstrap status banner** — «Not for production yet» (критично, чтобы не
  возникало ложного впечатления готовности)
- 4 killer-feature: эффекты, контракты, AI-first, M:N runtime
- Code sample (~15 строк): функция с `requires`/`ensures`/`uses Db, Log`/`match`
- Links: GitHub `nv-lang/nova`, спека, roadmap, контакты (`hello@`, `security@`)
- Language switcher EN | RU
- Light/dark theme через `prefers-color-scheme`
- Акцентный цвет `#7C3AED` (фиолетовый — не Go-blue, не Rust-orange)

**Чек-лист:**
- [x] Создать структуру файлов локально
- [x] Написать `index.html` (EN)
- [x] Написать `ru/index.html` (RU)
- [x] Написать `style.css`
- [x] CNAME + README + LICENSE + .gitignore
- [ ] `git init` + первый коммит
- [ ] Создать репо `nv-lang/www` на GitHub
- [ ] Push на origin
- [ ] Settings → Pages → source: main branch / root
- [ ] Cloudflare DNS: 4 A-записи на GitHub Pages IPs
  (185.199.108-111.153)
- [ ] Подождать 5-15 минут, проверить `https://nv-lang.org/`
- [ ] Проверить `https://nv-lang.org/ru/`
- [ ] Включить HTTPS в Pages settings (Enforce HTTPS)

**Acceptance:**
- `https://nv-lang.org/` → EN страница (HTTP 200)
- `https://nv-lang.org/ru/` → RU страница
- Light + dark тема работают
- HTTPS enforced
- Lighthouse score ≥95 по всем категориям (mobile)

---

### Ф.1 — HTTP-стек в stdlib (2-4 месяца)

**Цель:** реализовать в `std/net/http` всё необходимое для написания
HTTP-сервера на Nova. Это major work, связан с Plan 18 (stdlib roadmap, P0
блок) и Plan 25 (production readiness).

**Компоненты:**

| Модуль | Что | Связь |
|---|---|---|
| `std/net/tcp` | TCP listener/socket через эффект Net + libuv | Plan 18 P0 |
| `std/net/http/parser` | HTTP/1.1 request/response parser | новый |
| `std/net/http/server` | Server loop, request dispatch | новый |
| `std/net/http/router` | Path matching, method routing | новый |
| `std/net/http/static` | Static file serving (with ETag, Last-Modified) | новый |
| `std/html` | HTML escaping, tag builders (опционально) | новый |
| `std/markdown` | MD → HTML (свой минимальный или libcmark FFI) | новый |

**Открытые вопросы (требуют решения по ходу):**
- Markdown parser: написать на Nova (трудозатратно, но dogfooding) vs FFI к
  libcmark (быстро, но FFI-зависимость).
- Template language: string interpolation (D-block уже есть) vs typed HTML DSL
  vs embedded Nova-выражения. Лучший вариант проявится после первого MVP.
- Routing: declarative table (`[("/", handler1), ...]`) vs imperative chain.
- HTTP/2 / HTTP/3: **нет**, на bootstrap-стадии HTTP/1.1 + keep-alive
  достаточно. HTTP/2 — будущее.
- TLS: **нет в Nova-сервере**, термирует Cloudflare. Это упрощает на 2 порядка.

**Acceptance:**
- `std/net/http/server` работает: можно написать 20-строчный hello-world
  HTTP-сервер на Nova
- Покрытие тестами в `nova_tests/std/net/http/*`
- Sub-plan (вероятно Plan 60.1 или отдельный 6X) детально расписывает stdlib работу

---

### Ф.2 — Static-site generator на Nova (2-4 недели после Ф.1)

**Цель:** бинарь `nv-doc-gen` (имя черновое) на Nova, который ест `.md` файлы
из `nv-lang/www` и выдаёт `_site/*.html`. Деплой по-прежнему через GitHub
Pages.

**Поведение:**
```sh
nova run build.nv  # читает content/, выдаёт _site/
```

**Зачем эта промежуточная фаза:**
- Уже **html сгенерирован Nova-программой** — частичный dogfooding win
- Тестирует markdown parser + HTML builder
- НЕ требует сервера (всё ещё static на GitHub Pages)
- Возможно станет основой для Plan 45 (`nova doc`) если архитектурно совпадёт

**Acceptance:**
- `nova run build.nv` → корректный `_site/` идентичный (или лучше)
  результату Ф.0
- GitHub Action: запускает build на каждый push в `main`, деплоит `_site/`
- README документирует как пересобрать локально

---

### Ф.3 — Server-rendered на Nova на VPS (2-3 недели после Ф.2)

**Цель:** реальный HTTP-сервер на Nova живёт на VPS и отдаёт страницы.
**Это и есть финальная dogfooding-победа.**

**Stack:**
- VPS: Yandex Cloud burstable (~300-400₽/мес) или Timeweb (~150-300₽/мес).
  Для bootstrap-стадии хватает 1 vCPU / 1-2 GB RAM.
- OS: Ubuntu LTS (актуальная на момент деплоя)
- Origin: `nova-server` бинарь, запускаемый через systemd
- Edge: Cloudflare (SSL termination, CDN, DDoS, кэширование)
- Process management: systemd unit + restart on failure
- Logs: stdout → journald, экспорт в Loki/файл TBD
- Metrics: TBD (возможно через `/metrics` endpoint Prometheus-format)

**Зачем Cloudflare между:**
- SSL termination — снимает с Nova-сервера задачу TLS (большое упрощение)
- Кэширование — статика на edge, origin почти не дёргается
- DDoS защита — origin не доступен прямо
- Always-on staging fallback — если origin упал, Cloudflare может отдать
  cached версию

**Acceptance:**
- `https://nv-lang.org` отвечает из nova-server на VPS (проверяется через
  `Server:` header или custom debug endpoint)
- 30-day uptime ≥ 99% под публичным трафиком
- Recovery: при kill процесса systemd поднимает обратно за < 5 секунд
- Cloudflare cache hit rate ≥ 80% для статики

---

### Ф.4 — Расширение (онгоинг после Ф.3)

Когда базовый сервер работает — наращивать функциональность:
- **Документация спеки** (`/spec/...`) — рендеринг `spec/*.md` через
  тот же generator
- **Blog** (`/blog/...`) — devlog проекта
- **Playground** (`/playground`) — если появится JS runtime для Nova в
  браузере, либо server-side evaluation через эффект-handler
- **Search** — собственный full-text index (минимальный) или lunr.js
- **i18n расширение** — больше языков по необходимости

Acceptance не фиксированный — это онгоинг.

## Зависимости от других планов

| План | Что нужно от него | Статус |
|---|---|---|
| Plan 18 (stdlib roadmap) | P0 = libuv-based net | proposal, не начат |
| Plan 22 (libuv integration) | event loop, timers | ✅ закрыт |
| Plan 25 (production readiness) | runtime достаточно стабильный для live load | roadmap |
| Plan 44 (M:N runtime) | scheduler, work-stealing, preemption | ✅ ключевые части закрыты |
| Plan 45 (nova doc) | если архитектурно совпадёт с SSG в Ф.2 | план, не начат |
| Plan 49 (cancel routing) | для graceful shutdown сервера | план, не начат |

## Риски

| Риск | Как mitigate |
|---|---|
| HTTP-стек в stdlib окажется крупнее ожидаемого (полгода+) | Сайт остаётся на GitHub Pages всё это время — пользователи всегда видят живую страницу. Фаза 1 не блокирует release плана 0.1. |
| Nova runtime нестабилен под живой нагрузкой | Это **смысл** dogfooding — найти проблемы до пользователей. Cloudflare cache защищает от полного downtime. |
| Production downtime ударит по репутации | Cloudflare always-on cached fallback. Staging на GitHub Pages всегда работает. Migration plan: при критичных проблемах за минуту вернуть DNS на GitHub Pages. |
| Markdown parser / HTML builder на Nova окажется сложно | Опция: использовать FFI к libcmark/lowdown в Фазе 2-3, переписать на чистый Nova позже. Pragmatic over pure. |
| VPS оплата прерывается (карта истекла, баланс) | Привязать карту с автопополнением. Уведомления о низком балансе. Backup-метод оплаты. |

## Open questions

- Стоит ли вынести Ф.1 (HTTP stdlib) в отдельный sub-plan (Plan 60.1) или
  отдельный Plan 6X? Решить когда дойдём — зависит от объёма работы.
- Где хостить логи / метрики? Self-hosted Grafana? Внешний сервис?
- Нужен ли CI на VPS для сборки бинаря или сборка локально + scp?

## Verification (для всего плана 60)

Финальная проверка считается пройденной когда:
1. `https://nv-lang.org` работает 30 дней подряд с uptime ≥ 99%
2. Страницы рендерятся Nova-сервером (проверка через response header)
3. Bin reproducible from `nv-lang/www` (анonymous person может склонировать
   репо и собрать тот же бинарь)
4. Source code сервера полностью на Nova (≥ 95% LOC, оставшиеся 5% — FFI
   обёртки если есть, и Cargo для сборочной обвязки)

## История изменений

| Дата | Изменение |
|---|---|
| 2026-05-17 | План создан. Фаза 0 в работе (файлы написаны, осталось push + Pages + DNS). |
