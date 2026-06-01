# Plan 64 — nv-lang.org (dogfooding initiative)

**Статус:** Ф.0 ✅ ЗАКРЫТ 2026-05-18

**Репо:** [`nv-lang/www`](https://github.com/nv-lang/www)

**Цель:** реальный сайт проекта `nv-lang.org`, написанный (постепенно) на самом Nova —
production-площадка для runtime под живой нагрузкой и самое убедительное доказательство
что язык работает.

---

## Мотивация и сравнение с конкурентами

Все зрелые языки имеют сайты, написанные на себе:

| Язык | Сайт / инфраструктура | Что использует |
|------|----------------------|----------------|
| Go | go.dev, pkg.go.dev, playground.golang.org | `net/http`, `text/template`, `pkgsite` — всё на Go |
| Rust | doc.rust-lang.org, rustup.rs, crates.io | mdBook (Rust SSG), Actix/Axum сервисы |
| TypeScript | typescriptlang.org | TypeScript Gatsby + TS везде |
| Zig | ziglang.org | Zig-written server, самодостаточно |
| Elixir | elixir-lang.org | Phoenix (own web framework) |

Для Nova это не просто маркетинг:

1. **Dogfooding** — каждый баг в stdlib HTTP, IO, routing мешает нам самим.
   Выявляет дыры до того как их найдут пользователи.
2. **Самый сильный proof-of-work** — «этот сайт работает на Nova» убедительнее
   любых бенчмарков.
3. **Production load для M:N runtime** — cancel, timeout, recovery под реальным
   трафиком нельзя полностью проверить в тестах.
4. **Reference implementation** — будущие пользователи видят рабочий HTTP-сервер,
   static-site-generator, blog-движок на Nova с production-grade кодом.

**Что не цель:** красивый маркетинговый сайт, SEO в топ, лидген.
Цель — честная техническая площадка с минимально достаточным дизайном.

---

## Репозиторий

- **`nv-lang/www`** — отдельный публичный репозиторий (не внутри `nv-lang/nova`)
- Лицензия контента: CC BY 4.0
- Лицензия кода: MIT OR Apache-2.0 (консистентно с Nova compiler)
- Локально: `D:\Sources\nv-lang\www\`

---

## Фазы

### Ф.0 — Public URL: static HTML на GitHub Pages ✅ ЗАКРЫТ 2026-05-18

**Цель:** `https://nv-lang.org` отдаёт живую страницу. Минимальный bootstrap пока
Nova-HTTP-стек не готов. Zero JS, zero build step.

**Stack:**
- Plain HTML + minimal CSS (~160 строк). No JavaScript.
- Хостинг: GitHub Pages (бесплатно).
- DNS/SSL/CDN: Cloudflare (DNS-only mode для GitHub Pages compatibility).

**Структура репо:**
```
index.html          EN, /
ru/index.html       RU, /ru/
style.css
CNAME               nv-lang.org
README.md
LICENSE             CC BY 4.0
.gitignore
```

**Acceptance criteria:**
- [x] `https://nv-lang.org/` → EN страница HTTP 200
- [x] `https://nv-lang.org/ru/` → RU страница HTTP 200
- [x] HTTPS enforced (GitHub Pages + Cloudflare)
- [x] Light + dark тема через `prefers-color-scheme`
- [x] Bootstrap status banner: «Not for production yet»
- [x] Lighthouse performance ≥ 95 (mobile, нет JS, нет внешних ресурсов)

**DNS конфигурация (Cloudflare):**

| Type | Name | Content | Proxy |
|------|------|---------|-------|
| A | `@` | `185.199.108.153` | DNS only |
| A | `@` | `185.199.109.153` | DNS only |
| A | `@` | `185.199.110.153` | DNS only |
| A | `@` | `185.199.111.153` | DNS only |
| CNAME | `www` | `nv-lang.github.io` | DNS only |

**Важно:** Cloudflare proxy (оранжевое облако) несовместимо с GitHub Pages
custom domain verification. DNS-only обязательно для Ф.0.

---

### Ф.1 — HTTP-стек в stdlib

**Зависимости:** Plan 18 P0 (libuv TCP) · Plan 44 M:N runtime ✅ · Plan 22 libuv ✅

**Scope:** production-grade HTTP/1.1 server в `std/net/` + `std/http/`.

#### Ф.1.1 — TCP listener

```nova
// std/net/tcp.nv

#stable(since = "0.2")
export type TcpListener { ... }

#stable(since = "0.2")
export type TcpStream { ... }

/// Bind и начать слушать на `addr`.
///
/// # Examples
/// ```nova
/// let l = listen("0.0.0.0:8080")
/// ```
///
/// #stable(since = "0.2")
export fn listen(addr str, backlog int = 128) TcpListener
    uses Net
    requires addr.len() > 0
    requires backlog > 0

/// Принять следующий входящий connection. Блокирует fiber (не поток).
///
/// #stable(since = "0.2")
export fn accept(l TcpListener) TcpStream
    uses Net
```

Требования (паритет с Go `net` + лучше где возможно):

| Свойство | Go `net` | Rust `tokio::net` | Nova (цель) |
|----------|----------|-------------------|-------------|
| Non-blocking | `goroutine` per conn | `async/await` | M:N fiber per conn, нет `async` |
| SO_REUSEADDR | ✅ | ✅ | ✅ |
| SO_REUSEPORT | только Linux | ✅ | ✅ (Linux; Windows deferred — Plan 44.3 blocked) |
| IPv4 + IPv6 | ✅ dual-stack default | ✅ | ✅ |
| Graceful shutdown | `context.Context` | `CancellationToken` | `supervised(cancel: tok)` — Plan 47 |
| Effect в signature | ✗ | ✗ | ✅ `uses Net` |

#### Ф.1.2 — HTTP/1.1 parser

Требования (RFC 7230-7235, RFC 9112):

- Request line: method + request-target + HTTP-version
- Header fields: case-insensitive names (RFC 7230 §3.2), folded headers rejected (obsolete)
- Body: `Content-Length` + chunked transfer encoding + no-body (HEAD, 204, 304)
- Keep-alive: HTTP/1.1 default persistent connections
- 100-Continue: must send before reading body if `Expect: 100-continue`
- Limits (configurable, hardcoded defaults):
  - Max header block: 1 MB (Go default)
  - Max body: 10 MB (Go default, overridable per-handler)
  - Max header count: 100
- Malformed input → `400 Bad Request` response, не panic
- Partial reads: parser — streaming, не требует весь request в памяти

Не в Ф.1 (будущее):
- HTTP/2 (требует HPACK, multiplexing — отдельный план)
- WebSocket upgrade
- TLS в Nova-коде (Cloudflare termination достаточно для bootstrap)
- Trailers

#### Ф.1.3 — Request/Response API

```nova
// std/http/server.nv

#stable(since = "0.2")
export type Method | Get | Post | Put | Delete | Patch | Head | Options | Trace | Connect

#stable(since = "0.2")
export type Request {
    method  Method
    path    str
    query   str             // raw query string, без `?`
    headers HashMap[str, str]
    body    Bytes
    remote  str             // "ip:port"
}

#stable(since = "0.2")
export type Response {
    status  int
    headers HashMap[str, str]
    body    Bytes
}

/// Тип handler'а. Эффект Http объявляет что функция — HTTP handler,
/// может читать Request-контекст и писать Response.
///
/// #stable(since = "0.2")
export type HttpHandler alias fn(Request) Response
    uses Http

/// Запустить HTTP-сервер на addr. Блокирует fiber (graceful shutdown через cancel).
///
/// #stable(since = "0.2")
export fn serve(addr str, handler HttpHandler) !
    uses Net, Http
    requires addr.len() > 0
```

**Nova advantage vs конкуренты:**

| Аспект | Go `net/http` | Rust `axum` | TypeScript (fastify) | Nova |
|--------|---------------|-------------|----------------------|------|
| Handler side-effects | не tracked | trait bounds (частично) | не tracked | ✅ `uses Db, Log` в сигнатуре |
| Error propagation | panic / ResponseWriter | `Result<_, StatusCode>` | Promise rejection | `Fail[HttpError]` effect |
| Middleware | `http.Handler` wrap | `tower::Layer` | fastify plugins | `fn(HttpHandler) HttpHandler uses Http` |
| Concurrency model | goroutine/conn | tokio task/conn | libuv event loop | M:N fiber/conn (Plan 44) |
| Backpressure | implicit channel | explicit bounds | implicit | explicit в M:N scheduler |
| Static type checks on route params | ✗ | ✅ axum path extractor | ✗ | ✅ typed `PathParams` |

#### Ф.1.4 — Router

Trie-based router с `O(log n)` dispatch:

```nova
// std/http/router.nv

#stable(since = "0.2")
export type Router { ... }

#stable(since = "0.2")
export type PathParams alias HashMap[str, str]

/// Добавить route. Pattern: "/users/:id", "/files/*path", "/api/v1/".
///
/// #stable(since = "0.2")
export fn route(r Router, method Method, pattern str, handler HttpHandler) Router

/// Собрать dispatch handler из таблицы routes.
///
/// #stable(since = "0.2")
export fn dispatch(r Router) HttpHandler
    uses Http

/// Middleware: применить к каждому request перед dispatch.
///
/// #stable(since = "0.2")
export fn use_middleware(r Router, mw fn(HttpHandler) HttpHandler) Router
```

Паритет с Go `http.ServeMux` (1.22+, method+pattern routing) и Rust `axum::Router`.

#### Ф.1.5 — Static file serving

```nova
// std/http/static.nv

/// Serve файлов из директории dir на prefix.
/// ETag (sha256 truncated) + Last-Modified + conditional GET (304).
/// Range requests: поддержка для Ф.3+.
///
/// #stable(since = "0.2")
export fn serve_dir(prefix str, dir str) HttpHandler
    uses Http, Fs
```

Требования:
- `ETag` на основе mtime + size (не sha256 на каждый запрос — дорого)
- `Cache-Control: max-age=0, must-revalidate` для HTML; `max-age=31536000, immutable` для fingerprinted assets
- `Content-Type` из расширения (встроенная таблица ~50 типов)
- Directory listing: **выключено** по умолчанию (безопасность)
- Path traversal protection: `../` → 400

#### Ф.1.6 — Observability из коробки

```nova
// std/http/metrics.nv — встроена в serve(), не opt-in

export type HttpMetrics {
    requests_total     u64
    requests_active    u64
    latency_p50_ms     f64
    latency_p95_ms     f64
    latency_p99_ms     f64
    errors_4xx         u64
    errors_5xx         u64
    // из Plan 32 std.runtime.gc:
    gc_pause_last_ms   f64
    gc_heap_bytes      u64
    gc_live_objects    u64
}

/// Текущий snapshot метрик сервера.
///
/// #stable(since = "0.2")
export fn metrics() HttpMetrics
    uses Http
```

**Почему это важнее чем в Go/Rust:**

Go имеет `expvar` + `net/http/pprof` — но это opt-in, отдельные import'ы.
Rust crates (`tracing`, `metrics`) — отдельные зависимости вне stdlib.
Nova встраивает базовый observability в `std/http` из коробки — нет setup overhead.

Дополнительно: `GET /healthz` и `GET /metrics` (Prometheus text format) как стандартные
builtin-endpoints в `serve()`, включаемые флагом.

#### Ф.1.7 — Structured logging

```nova
// std/log/structured.nv

export type Level | Debug | Info | Warn | Error

/// Логировать structured event. Формат: JSON или logfmt (configurable).
///
/// #stable(since = "0.2")
export fn log(level Level, msg str, fields HashMap[str, str])
    uses Log

// Shorthand helpers:
export fn info(msg str, fields HashMap[str, str])  uses Log
export fn warn(msg str, fields HashMap[str, str])  uses Log
export fn error(msg str, fields HashMap[str, str]) uses Log
```

JSON output example:
```json
{"ts":"2026-05-18T10:23:45.123Z","level":"info","msg":"request","method":"GET","path":"/","ms":2,"status":200}
```

Паритет с Go `log/slog` (1.21+). Rust — `tracing` (внешний crate). Nova — в stdlib.

**Тесты Ф.1:**

```
nova_tests/http/basic_get.nv              — GET / → 200
nova_tests/http/keep_alive.nv             — 100 requests на одном connection
nova_tests/http/chunked_response.nv       — chunked transfer encoding
nova_tests/http/large_body.nv             — 10MB body upload
nova_tests/http/router_params.nv          — /users/:id extract
nova_tests/http/router_wildcard.nv        — /files/*path
nova_tests/http/middleware_chain.nv       — 3 middleware + handler
nova_tests/http/static_serving.nv         — ETag, 304, Content-Type
nova_tests/http/concurrent_1000.nv        — 1000 concurrent connections
nova_tests/http/graceful_shutdown.nv      — drain in-flight, cancel token
nova_tests/http/malformed_request.nv      — 400 на битый HTTP, не panic
nova_tests/http/path_traversal.nv         — /../../etc/passwd → 400
nova_tests/http/metrics_endpoint.nv       — /metrics Prometheus format
nova_tests/http/healthz_endpoint.nv       — /healthz JSON
nova_tests/http/structured_log.nv         — JSON log output
```

---

### Ф.2 — Static-site generator на Nova (SSG)

**Зависимости:** Ф.1 (stdlib IO, str) · Plan 45 `nova doc` tokenizer (reuse)

**Scope:** бинарь `nova-ssg` на Nova. Паритет с Hugo (Go) по скорости,
с mdBook (Rust) по качеству output.

#### Ф.2.1 — Markdown parser

Производительность target: ≥ Hugo = ~1ms/page cold, ~50k pages/s batched.

CommonMark subset (MVP):
- ATX headings `# ## ### #### ##### ######`
- Setext headings (двойное подчёркивание)
- Paragraphs, hard line breaks (`  \n`), soft breaks
- Emphasis `*italic*`, `**bold**`, `***bold italic***`
- Inline code `` `code` ``
- Fenced code blocks ` ```lang ` (3+ backticks или тильды)
- Links: `[text](url "title")`, `[text][ref]`, autolinks `<url>`
- Images: `![alt](url "title")`
- Unordered lists (`-`, `*`, `+`)
- Ordered lists (`1.`, `1)`)
- Block quotes `>`
- Horizontal rules `---`, `***`, `___`
- YAML frontmatter (`---` блок перед контентом)
- HTML entities `&amp;`, `&#123;`
- GFM tables (pipe syntax)
- GFM task lists `- [x]`
- GFM strikethrough `~~text~~`

Не в MVP: footnotes, math (`$...$`), definition lists, custom containers.

```nova
// nova-ssg/src/markdown.nv

export type FrontMatter alias HashMap[str, str]

export type MarkdownDoc {
    front_matter FrontMatter
    ast          BlockNode
    toc          []TocEntry
}

export fn parse(src str) MarkdownDoc
    requires src.len() > 0

export fn to_html(doc MarkdownDoc) str

export fn to_html_with_opts(doc MarkdownDoc, opts HtmlOpts) str
    // opts: syntax_highlight bool, heading_anchors bool, open_links_new_tab bool
```

**Синтаксическая подсветка кода:**
Реиспользовать Nova tokenizer из Plan 45 (`nova doc`) для `nova` блоков.
Для других языков — минимальный generic highlighter (strings, comments, keywords).
Не добавлять зависимость на highlight.js / Shiki / Prism.

#### Ф.2.2 — Template engine

Минимальный, без Turing-complete логики (как Hugo templates, не Jinja/Liquid):

```
{{ .Title }}                              — переменная
{{ .Page.Description | default "..." }}   — pipe + default filter
{{ range .Pages }}...{{ end }}            — итерация
{{ if .Draft }}...{{ else }}...{{ end }}  — условие
{{ partial "header.html" . }}             — вложенный шаблон
{{ .Content }}                            — rendered Markdown body
```

Escape: всё auto-escaped как HTML. Для raw: `{{ .HTML | safe_html }}` (explicit opt-out).

```nova
// nova-ssg/src/template.nv

export type TemplateCtx alias HashMap[str, Value]

export fn render(tmpl str, ctx TemplateCtx) str
    uses Fail[TemplateError]

export fn render_file(path str, ctx TemplateCtx) str
    uses Fs, Fail[TemplateError]
```

#### Ф.2.3 — Build pipeline

```sh
nova-ssg build [--output ./public] [--base-url https://nv-lang.org]
nova-ssg serve [--port 8080] [--livereload]   # dev-server с hot-reload
nova-ssg check                                # broken links + missing front-matter
nova-ssg new-post "Post Title"                # scaffold нового поста
```

**Структура сайта:**
```
content/
  index.md              → /index.html
  ru/index.md           → /ru/index.html
  spec/index.md         → /spec/index.html
  blog/
    2026-05-18-launch.md → /blog/2026-05-18-launch/index.html
layouts/
  base.html
  blog-post.html
  spec.html
static/
  style.css
  favicon.svg
  img/
nova.toml               # конфигурация SSG
```

**`nova.toml` (SSG секция):**
```toml
[ssg]
base_url    = "https://nv-lang.org"
title       = "Nova Programming Language"
description = "..."
default_lang = "en"
langs       = ["en", "ru"]
```

#### Ф.2.4 — Поиск (offline, no external JS)

Pre-build full-text search index → `public/search-index.json`.
Минимальный vanilla JS клиент (~100 строк) для поиска по index.
Паритет с Hugo + Pagefind.

Алгоритм индексации: TF-IDF по заголовкам + первым 300 символам body.
Размер index ≤ 50 KB на 100 страниц.

#### Ф.2.5 — CI / deploy

```yaml
# .github/workflows/build.yml (в nv-lang/www)
on: [push]
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
        with: { submodules: true }
      - name: Build nova-ssg
        run: cargo build --release --manifest-path nova-ssg/nova-codegen/Cargo.toml
      - name: Generate site
        run: ./nova-ssg/target/release/nova-ssg build --output ./public
      - name: Check links
        run: ./nova-ssg/target/release/nova-ssg check
      - name: Deploy to GitHub Pages
        uses: actions/upload-pages-artifact@v3
        with: { path: ./public }
```

**Тесты Ф.2:**

```
nova_tests/ssg/basic_page.nv          — один .md → .html корректный HTML5
nova_tests/ssg/frontmatter.nv         — YAML parse + шаблон vars
nova_tests/ssg/code_highlight.nv      — ```nova блок → span.kw etc.
nova_tests/ssg/table_render.nv        — GFM table → <table>
nova_tests/ssg/toc_generation.nv      — headings → toc sidebar
nova_tests/ssg/broken_link.nv         — [bad](./missing.md) → exit 1
nova_tests/ssg/i18n_langs.nv          — EN + RU pages корректно разделены
nova_tests/ssg/perf_50pages.nv        — 50 страниц < 300ms
nova_tests/ssg/search_index.nv        — search-index.json корректный JSON
nova_tests/ssg/safe_html.nv           — XSS в front-matter escapeится
```

---

### Ф.3 — Server-rendered на VPS

**Зависимости:** Ф.1 (HTTP stdlib) · Ф.2 (SSG) · Plan 47 (cancel, graceful shutdown) · Plan 49 (typed errors)

**Цель:** `nv-lang.org` обслуживается реальным Nova HTTP-сервером на Linux VPS.
Cloudflare edge перед ним (TLS, CDN, DDoS).

#### Ф.3.1 — Deployment stack

```
Cloudflare (SSL termination, cache, DDoS)
    ↓ HTTP/1.1 plain (не TLS — Cloudflare шифрует клиентский трафик)
VPS Ubuntu LTS
    ├── nova-www (Nova binary, слушает 127.0.0.1:8080)
    │     systemd unit: nova-www.service
    │     Restart=on-failure, RestartSec=2
    │     LimitNOFILE=65536
    │     MemoryMax=512M
    └── Nginx (опционально, если нужен static fallback)
```

**VPS выбор:**
- Yandex Cloud burstable-2 (~400₽/мес, 2 vCPU shared, 2 GB RAM) — основной
- Timeweb (~300₽/мес) — альтернатива
- 1 vCPU / 1 GB RAM достаточно для bootstrap трафика, 2 GB рекомендуется под Boehm GC

**Systemd unit:**
```ini
[Unit]
Description=Nova nv-lang.org server
After=network.target

[Service]
User=nova-www
Group=nova-www
ExecStart=/opt/nova-www/nova-www --bind 127.0.0.1:8080 --metrics --healthz
ExecReload=/bin/kill -HUP $MAINPID
Restart=on-failure
RestartSec=2
LimitNOFILE=65536
MemoryMax=512M
# Graceful shutdown: SIGTERM → drain 30s → SIGKILL
TimeoutStopSec=35
KillMode=mixed

[Install]
WantedBy=multi-user.target
```

**Graceful shutdown** (Plan 47 `supervised(cancel:)`):

```nova
// nova-www/src/main.nv
fn main() !
    uses Net, Http, Log, Fs
{
    ro cancel = CancelToken.new()
    signal_handler(SIGTERM, fn() { cancel.cancel() })

    serve("127.0.0.1:8080", router(), cancel: cancel)
    // serve drain'ит in-flight connections, timeout 30s
    info("shutdown complete")
}
```

#### Ф.3.2 — Cloudflare configuration

```
DNS:
  A   @    <VPS IP>   Proxied (оранжевое) ← меняем с Ф.0 DNS-only
  CNAME www nv-lang.org Proxied

Page Rules / Cache Rules:
  /*.css, *.js, *.svg, *.woff2 → Cache-Control: max-age=31536000 (1 year)
  /blog/*, /spec/*             → Cache-Control: max-age=3600 (1 hour)
  /healthz, /metrics           → bypass cache
  /                            → max-age=300 (5 min)

Security:
  SSL/TLS: Full (strict) — Cloudflare ↔ Origin по HTTPS если cert есть, иначе HTTP
  Rate limiting: 100 req/10s per IP (protect от abuse)
  Bot Fight Mode: on

Firewall:
  Origin server: accept только от Cloudflare IP ranges
  (fail2ban rule или ufw whitelist)
```

#### Ф.3.3 — Structured logging → journald

```nova
// Формат: JSON Lines, одна запись на строку
{"ts":"2026-05-18T10:23:45.123Z","level":"info","msg":"request",
 "method":"GET","path":"/","status":200,"ms":2,"bytes":4218,
 "ip":"1.2.3.4","cf_ray":"abc123"}
```

Ротация: journald `--max-retention 30days`, экспорт в файл опционально.

Cloudflare Ray-ID прокидывать как `cf_ray` из `CF-Ray` header.

#### Ф.3.4 — Metrics endpoint (Prometheus text format)

```
GET /metrics   → text/plain; version=0.0.4

# HELP nova_http_requests_total Total HTTP requests
# TYPE nova_http_requests_total counter
nova_http_requests_total{status="200"} 12345
nova_http_requests_total{status="404"} 23

# HELP nova_http_request_duration_seconds HTTP request latency
# TYPE nova_http_request_duration_seconds histogram
nova_http_request_duration_seconds_bucket{le="0.005"} 9876
nova_http_request_duration_seconds_bucket{le="0.01"} 11000
...

# HELP nova_gc_heap_bytes GC managed heap size
# TYPE nova_gc_heap_bytes gauge
nova_gc_heap_bytes 8388608

# HELP nova_gc_pause_seconds Last GC pause duration
# TYPE nova_gc_pause_seconds gauge
nova_gc_pause_seconds 0.003
```

Метрики: из Plan 32 GC introspection + Plan 57 bench timer hooks.

Доступ к `/metrics` — restrict to localhost или Cloudflare Access (не public).

#### Ф.3.5 — Health check

```
GET /healthz   → 200 application/json

{"status":"ok","version":"0.1.0-alpha","uptime_s":86400,
 "gc_heap_mb":8.0,"gc_pause_ms":3.0,"goroutines":4}
```

Cloudflare Health Check: `GET /healthz` каждые 60s. Alert если 2 consecutive failures.
Fallback: при unhealthy origin → Cloudflare отдаёт cached версию (Always Online™).

#### Ф.3.6 — CI/CD pipeline

```yaml
# .github/workflows/deploy.yml
on:
  push:
    branches: [main]
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { submodules: true }
      - name: Build nova-www binary
        run: cargo build --release --manifest-path nova-www/nova-codegen/Cargo.toml
      - name: Build site content
        run: ./nova-ssg/target/release/nova-ssg build
      - name: Test smoke
        run: ./nova-tests/http/production_smoke.sh
      - name: Deploy to VPS
        run: |
          rsync -az nova-www/target/release/nova-www deploy@${{ secrets.VPS_HOST }}:/opt/nova-www/
          rsync -az public/ deploy@${{ secrets.VPS_HOST }}:/var/www/nova-static/
          ssh deploy@${{ secrets.VPS_HOST }} "systemctl reload nova-www"
```

Zero-downtime reload: `systemctl reload` → SIGHUP → nova-www re-binds socket + drains old connections.

**Тесты Ф.3:**

```
nova_tests/http/production_smoke.nv       — startup → /healthz → / → shutdown
nova_tests/http/graceful_reload.nv        — SIGHUP → in-flight не прерваны
nova_tests/http/metrics_prometheus.nv     — /metrics parseable Prometheus format
nova_tests/http/cloudflare_headers.nv     — CF-Ray header propagation
nova_tests/http/rate_limit_passthrough.nv — 429 от Cloudflare не прорывает
nova_tests/http/static_cache_headers.nv   — Cache-Control, ETag, 304
```

---

### Ф.4 — Расширение (ongoing после Ф.3)

Не commitments — priorities определяются по необходимости:

| Фича | Аналог | Зависимость | Сложность |
|------|--------|-------------|-----------|
| `/spec/` — auto-rendered spec docs | Rust Reference | Plan 45 `nova doc` | Низкая |
| `/blog/` — devlog | Zig blog | Ф.2 SSG | Низкая |
| `/changelog/` — из git tags | pkg.go.dev | Ф.2 SSG | Средняя |
| `/doc/std/` — stdlib API docs | pkg.go.dev | Plan 45 nova doc --html | Средняя |
| Search upgrade | Algolia / Pagefind | Ф.2 | Средняя |
| Playground | play.golang.org | WASM Nova backend | Очень высокая |
| Package registry (alpha) | crates.io | отдельный проект | Критически высокая |

**Playground:** требует Nova → WASM backend (не запланирован). Deferred indefinitely.

---

## Конкурентный анализ: Nova advantages

| Свойство | Go | Rust | TypeScript | **Nova** |
|----------|----|----|----|----|
| Effects в HTTP handler signature | ✗ | ✗ (trait bounds косвенно) | ✗ | ✅ `uses Db, Net, Log` |
| Contracts на HTTP endpoints | ✗ | ✗ | ✗ | ✅ `requires`/`ensures` |
| GC metrics в stdlib HTTP | ✗ (отдельный `runtime`) | ✗ | ✗ | ✅ встроено в `std/http` |
| Bench DSL в языке | ✗ (`testing.B`) | ✗ (Criterion) | ✗ (vitest) | ✅ Plan 57 |
| Single binary deployment | ✅ | ✅ | ✗ (node_modules) | ✅ |
| M:N fibers без async/await | ✅ goroutines | ✗ (tokio `async`) | ✗ (`async/await`) | ✅ Plan 44 |
| Structured logging в stdlib | ✅ log/slog (1.21+) | ✗ (tracing crate) | ✗ (pino crate) | ✅ `std/log/structured` |

---

## Зависимости от других планов

| План | Что нужно | Статус | Блокирует фазу |
|------|-----------|--------|----------------|
| Plan 18 (stdlib roadmap) | P0 = libuv TCP (`std/net/tcp`) | proposal, не начат | Ф.1 |
| Plan 22 (libuv integration) | event loop, TCP uv_tcp_t | ✅ ЗАКРЫТ | Ф.1 |
| Plan 44 M:N runtime | scheduler, work-stealing, preemption | ✅ ЗАКРЫТ (44.1-44.7) | Ф.1 |
| Plan 44.3 (Windows fibers) | SO_REUSEPORT на Windows | 🔒 blocked | Windows deploy |
| Plan 45 (nova doc) | tokenizer для syntax highlight | ✅ ЗАКРЫТ | Ф.2 |
| Plan 47 (supervised cancel) | graceful shutdown | ✅ ЗАКРЫТ | Ф.1, Ф.3 |
| Plan 49 (typed errors) | `Fail[HttpError]` typed propagation | не начат, P1 | Ф.1 (опционально) |
| Plan 32 (GC introspection) | heap_size, gc_pause metrics | ✅ ЗАКРЫТ | Ф.3 (metrics) |
| Plan 57 (bench) | bench hooks для perf metrics | ✅ ЗАКРЫТ | Ф.3 (metrics) |

---

## Риски

| Риск | Severity | Mitigation |
|------|----------|------------|
| HTTP stdlib (Plan 18 P0) не начат — Ф.1 заблокирована | High | Сайт остаётся на GitHub Pages всё это время. Ф.0 coverage всегда есть. |
| Nova runtime нестабилен под live load | Medium | **Смысл dogfooding** — найти до пользователей. Cloudflare Always Online™ fallback. |
| Production downtime bites репутацию | Medium | DNS за 1 минуту переключается обратно на GitHub Pages при критичном сбое. |
| Boehm GC паузы под трафиком | Medium | Метрики gc_pause в /metrics. Cloudflare cache снижает load. Concurrent GC — future. |
| Markdown parser scope creep | Low | Чёткий MVP-subset зафиксирован. CommonMark compliance тест как gate. |
| VPS billing failure | Low | Автопополнение + уведомления низкого баланса + резервная карта. |
| Windows SO_REUSEPORT (Plan 44.3 blocked) | Low | Production deployment = Linux-only. Задокументировано честно. |

---

## Acceptance criteria (сводная таблица)

| Фаза | Критерий | Измеримо |
|------|----------|----------|
| Ф.0 | `https://nv-lang.org/` → 200, HTTPS | curl / browser |
| Ф.0 | Lighthouse ≥ 95 (mobile) | Lighthouse CLI |
| Ф.1 | `nova test nova_tests/http/` — все PASS | CI |
| Ф.1 | 1000 concurrent connections без OOM на 512 MB VPS | `concurrent_1000.nv` |
| Ф.1 | `nova doc std/http/` — 0 `public-missing-stability` warnings | `nova doc --check` |
| Ф.2 | 50 страниц → HTML за < 300 ms | `perf_50pages.nv` |
| Ф.2 | Output: валидный HTML5 (W3C validator 0 errors) | `nova-ssg check` |
| Ф.2 | Broken links → exit 1 | CI |
| Ф.3 | nv-lang.org served by Nova server (не GitHub Pages) | `Server:` header |
| Ф.3 | `/healthz` p99 < 5 ms | load test |
| Ф.3 | 30-day uptime ≥ 99.5% | Cloudflare Health Check |
| Ф.3 | `/metrics` scrape-able by Prometheus | promtool check metrics |
| Ф.3 | Graceful shutdown < 30 s | systemd stop timing |
| Ф.3 | Zero in-flight requests dropped on reload | `graceful_reload.nv` |

---

## История изменений

| Дата | Изменение |
|------|-----------|
| 2026-05-17 | План предложен, Ф.0 в работе (файлы в nv-lang/www написаны) |
| 2026-05-18 | Ф.0 ЗАКРЫТ: репо nv-lang/www запушен, GitHub Pages настроен, Cloudflare DNS 4×A + CNAME |
| 2026-05-18 | План переписан с нуля — production-grade spec (Ф.1-Ф.3 детали, конкурентный анализ, acceptance criteria) |
