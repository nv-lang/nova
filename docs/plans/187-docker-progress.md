<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 187, волна 2 — Docker-образ живого флагман-демо: прогресс

Чекпоинт для §9.4 п.7 / Ред.6 п.3 (`docs/plans/187-flagship-concurrency-demo.md`).
Ветка `docker-187`, worktree `d:/Sources/nv-lang/nova-docker187`.

## Блокер (снят)

Первый заход (эта же сессия) упёрся в диск D: — 0 байт свободно из 1.9 ТБ
(десятки параллельных `nova-*` worktree на машине). `git worktree add`/
`git reset --hard` рвались с `No space left on device`. Битый worktree снесён
(`git worktree remove --force --force`), отчёт передан оркестратору. Диск
расчищен внешне (не этой сессией) — на возобновлении `df -h d:/` показал
~113 ГБ свободно. Worktree пересоздан командой БЕЗ `-b` (ветка `docker-187`
уже существовала) — `git worktree add ../nova-docker187 docker-187`, затем
fast-forward на актуальный `main` (`git merge --ff-only main`,
`4317ebd93..8b4dee2a0`).

## Сделано

1. **`examples/flagship/aggregator/src/main.nv`** — bind-override:
   - добавлена `fn resolve_addr(port u16) Os -> SocketAddr` — читает
     `AGGREGATOR_BIND` (комбинаторная форма, как canonical `resolve_port`
     уже мигрировал на `main`: `env(...).flat_map(...) ?? DEFAULT`),
     парсит через `SocketAddr.from_str(host + ":" + port)`, фоллбэк —
     прежний `SocketAddr.loopback(port)` (локальный дефолт 127.0.0.1 НЕ
     ломается).
   - `ro addr = SocketAddr.loopback(port)` → `ro addr = resolve_addr(port)`.
   - лог-строка `println("... listening on http://127.0.0.1:...")` →
     `addr.ip()` (честный лог вместо захардкоженного 127.0.0.1, актуально
     когда `AGGREGATOR_BIND=0.0.0.0` в контейнере).
2. **`examples/flagship/aggregator/Dockerfile`** (новый файл) — multi-stage:
   builder (`ubuntu:22.04` + `clang cmake make libgc-dev build-essential
   git ca-certificates curl` + rustup 1.85 pinned) → `cargo build --release`
   (`nova-cli`) → sibling-клон `nova-http` (`git clone --depth 1`, path-dep
   `examples/nova.toml`) → `nova build .../main.nv --strict-effects`;
   runtime (`ubuntu:22.04` + `libgc1 ca-certificates`) + сам бинарь.
   `ENV AGGREGATOR_PORT=8187 AGGREGATOR_BIND=0.0.0.0`, `EXPOSE 8187`.
   Найденный факт (эта же сессия, из `nova-gate.yml` + `test_runner.rs`
   `build_missing_vendor_ffi_libs`/`build_vendor_ffi_lib`): `tls`/`compress`
   — git-зависимости, компилятор Nova фетчит их САМ по `nova.lock` (первое
   обращение); sibling-клон нужен ТОЛЬКО для `http` (path-зависимость).
   mbedTLS-vendor собирается плоским `cc`+`ar` (не cmake/make — но
   `cmake make` оставлены в apt-списке, как в проверенном
   `docs/guide/linux-build.md`-рецепте, лишними не мешают).
3. **`examples/flagship/aggregator/README.md`** — новый раздел «Docker»
   (build+run в 2 команды, тег `ghcr.io/nv-lang/aggregator-demo:0.1.0` как
   цель публикации владельцем, объяснение bind 0.0.0.0/портов,
   sibling-зависимости), обновлена строка «Известные ограничения».
4. `git submodule update --init compiler-codegen/nova_rt/libuv` в worktree
   (нужен ДО `docker build` — submodule не чекаутится автоматически в
   build context).

## В процессе / далее

- `docker build -f examples/flagship/aggregator/Dockerfile -t
  aggregator-demo:local .` — см. дословный вывод и итоговые размеры/тайминг
  в финальном отчёте сессии.
- `docker run --rm -d -p 8187:8187 aggregator-demo:local` + curl-smoke с
  хоста (`/`, `/api/snapshot`, `/api/run?legend=weather&mode=demo&seed=42`,
  `/api/run?legend=health&mode=live`, `/api/events`).
- 15с простоя + 5 последовательных запросов (Linux-путь watchdog/slot-race
  фиксов — впервые под Docker).
- Коммиты по шагам (main.nv bind / Dockerfile / README+checkpoint),
  греп конфликт-маркеров перед каждым.

## Финал волны (2026-07-17, оркестратор)

- Сборочная часть ✅: образ aggregator-demo:local 126МБ (multi-stage; build-context
  nova-http локальный; находка: std/ = Rust-compile-time зависимость → порядок COPY).
- Рантайм-гейт ✗: сервер в контейнере не живёт — [M-187-docker-linux-runtime-hang]
  P1 (mprotect/VMA-шторм арены; с NOVA_MAX_FIBERS=2048 — PID1 D-state в do_exit,
  запросы не читаются). Нужен отдельный заход «Linux M:N server profile».
- Волна закрыта частично: Dockerfile/README/bind-правка влиты; docker run-гейт
  переедет в закрытие маркера.

## 2026-07-20 — docker run гейт (§7.5) ЗЕЛЁНЫЙ: волна 2 закрыта целиком

Образ пересобран на свежем main (все рантайм-фиксы: guard-page слой 1
`f6bb896da`, GC-marker слой 2 + spawnctx GC-roots `0cdd6140d`, work-conserving
pump `14decdfb1`, admission 16 `57ee49073`; nova-tls с leak-фиксом по git):
126МБ, `docker build ... && docker run --rm -p 8187:8187 aggregator-demo:local` —
буквально «одной командой».

Smoke в контейнере (Windows-хост, Docker Desktop/WSL2): boot `/` 200 →
5× `/api/run?legend=demo` все 200 → 15с простоя → chaos-run 200 (раньше
контейнер умирал именно тут) → snapshot жив (`fibers 12/12`, wall 1201ms).
Burst `xargs -P40`: **ровно 16/40 отвечено 200** (= MAX_INFLIGHT_CONNS,
admission работает как спроектирован), 24 честно отбиты, post-burst 200,
контейнер Up. [M-187-docker-linux-runtime-hang] подтверждён закрытым и в
докере, не только под WSL-гейтом волны слоя 2.

Остаток волны 2: ghcr-публикация — за владельцем (решение о неймспейсе/тегах).
