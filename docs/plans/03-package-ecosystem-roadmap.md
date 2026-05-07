# Plan 03: Package ecosystem roadmap

Детализированный план постройки экосистемы пакетов Nova — от
self-hosted compiler до публичного registry с опубликованными
библиотеками.

Дополняет [01-roadmap-v0.1.md](01-roadmap-v0.1.md) (общий roadmap
по версиям компилятора). Здесь — конкретно про **пакетный менеджмент,
registry и self-hosting** как зависимый от чего-либо набор задач.

Spec-уровень — [D78](../../spec/decisions/07-modules.md#d78) (manifest,
lockfile, registry chain, workspace, package tooling).

---

## Текущее состояние (2026-05-07)

- ✅ **Spec D78** написан: `nova.toml`, `nova.lock` format, registry
  chain, workspace.
- ✅ **Workspace структура** реализована: корневой `nova.toml` + per-
  member `nova.toml` (`std/`, `examples/`, `nova_tests/`).
- ✅ **Module path = file path** enforcement (D78) — bootstrap
  компилятор уже должен это проверять.
- ❌ `[registry]` секция убрана из корневого `nova.toml` —
  преждевременна (placeholder URL без инфраструктуры за ним).
- ❌ `nova` CLI с `add`/`publish`/`update` командами — не написан.
- ❌ Lockfile resolution (SAT-алгоритм) — не реализован.
- ❌ Registry HTTP-протокол — не описан.
- ❌ `nova-registry.org` — домен не куплен, инфраструктуры нет.
- ❌ Опубликованных пакетов — ноль.

Сейчас **единственный способ зависимости** — `path = "../foo"` или
`git = "https://..."` (когда tooling поддержит). Зависимостей с
versioning через registry — нет и не будет до полного roadmap'а
ниже.

---

## Roadmap

### Шаг 1. Self-hosted compiler

**Зависит от:** v2.0+ из [01-roadmap-v0.1.md](01-roadmap-v0.1.md).

Bootstrap-Rust компилятор переписывается на Nova. Нужно для того,
чтобы:

1. Сам `nova` CLI (с командами `add`/`publish`/`update`) был
   написан на Nova — иначе экосистема не self-contained.
2. Compiler-API (parse, type-check, codegen) был доступен из
   Nova-кода — для linter'ов, formatter'ов, IDE-плагинов.

**Критерий готовности:** `nova` CLI собирается из Nova-исходников
и проходит все тесты `nova_tests/`.

### Шаг 2. `nova` CLI с базовыми командами

**Зависит от:** Шага 1.

Команды для работы с пакетами:

```bash
nova new my-project           # создать новый проект (nova.toml + src/)
nova add foo@^1.2             # добавить зависимость в nova.toml
nova add foo --path ../foo    # path-зависимость
nova add foo --git URL        # git-зависимость
nova update                   # обновить nova.lock
nova update foo               # обновить только foo
nova remove foo               # убрать зависимость
nova publish                  # опубликовать пакет в registry
nova search keyword           # искать в registry
nova info foo                 # info про конкретный пакет
```

**Критерий готовности:** `nova add foo --path ../foo` + `nova build`
работают в self-hosted режиме на тестовом проекте; `nova publish`
имеет dry-run mode.

### Шаг 3. Lockfile resolution (SAT-алгоритм)

**Зависит от:** Шага 2 (CLI должен куда-то писать `nova.lock`).

D78 описывает формат `nova.lock` (TOML, version 1, packages с
hash'ами и source-URL'ами). Нужно реализовать **SAT resolver**:
алгоритм который из набора deps + version-ranges в `nova.toml`
выводит конкретный set версий.

Варианты:
- **PubGrub** (Dart, готовая Rust-реализация в `pubgrub` crate) —
  это используется Cargo с 2023, доказано работает. Можно portировать
  на Nova когда self-hosted.
- **Plain backtracking** — проще, медленнее, достаточно для small
  scale.

**Критерий готовности:** `nova update` строит `nova.lock` для
проекта с 5+ зависимостями за <1с. `nova build` использует
`nova.lock` (не пересчитывает версии каждый раз).

### Шаг 4. Registry HTTP-протокол

**Зависит от:** Шагов 2 и 3 (CLI и lockfile уже умеют работать
с зависимостями).

Описать в spec'е (новый D-decision или расширение D78) HTTP API
registry:

```
GET /api/v1/packages/<name>/versions
  → list версий с metadata + content-hash

GET /api/v1/packages/<name>/<version>/manifest
  → nova.toml пакета (для resolution)

GET /api/v1/packages/<name>/<version>/archive.tar.gz
  → тарбол с исходниками + lockfile

POST /api/v1/packages/<name>/<version>
  → publish (требует API token)
```

**Принципы:**

- **Content-addressable.** Каждая версия имеет sha256-хеш, lockfile
  его фиксирует. Нельзя подменить опубликованную версию (immutable
  releases) — как Cargo / npm.
- **Минимализм.** Никаких user-аккаунтов в protocol — auth через
  API tokens. UI-сторона registry — отдельная задача.
- **Mirror-friendly.** Любой может стянуть весь registry и поднять
  proxy (для air-gapped / closed-network — банки, гос).

**Критерий готовности:** Reference-implementation registry на
любом языке (Go/Rust для скорости старта); `nova` CLI умеет с ним
говорить через chain в `[registry]` секции `nova.toml`.

### Шаг 5. Запуск `nova-registry.org`

**Зависит от:** Шага 4.

Инфраструктурный шаг, не код:

- Купить домен `nova-registry.org` (или альтернативу).
- Поднять hosting (хосты + база + storage для тарболов).
- Написать политики: ToS, DMCA, security-policy (RUSTSEC-аналог),
  policy для squatting'а имён.
- Реализовать UI: web-страница пакета (как crates.io), search,
  download stats.
- CDN для тарболов.
- Backup strategy.

**Критерий готовности:** `https://nova-registry.org` доступен,
`nova publish` работает на реальный URL, есть policy документы.

### Шаг 6. Первые библиотеки публикуются

**Зависит от:** Шагов 1–5.

Что публикуется первым:

1. **Сам `std/`** — но это спорно: stdlib обычно идёт **с
   компилятором**, не как опциональная зависимость. Решается
   позже: либо vendored в `nova` бинарь (Rust-style `std`),
   либо первый пакет в registry (Python-style `pip install ...`).
2. **Community-либы из `examples/`** — кандидатами были
   аспирационные либы (которые сейчас в `std/` черновиками), если
   они выйдут из stdlib и станут community-пакетами.
3. **Прикладные:** HTTP frameworks (Express-аналог), ORM, CLI
   helpers, etc. — пишут community.

**Критерий готовности:** На `nova-registry.org` хотя бы 10
useful пакетов от 3+ разных авторов; `nova add` на типичный
Nova-проект ставит 3-5 deps без проблем.

---

## Зависимости между шагами

```
Шаг 1 (self-host)  ──┐
                     ├──► Шаг 2 (CLI)  ──► Шаг 3 (lockfile)  ──┐
                     │                                          ├──► Шаг 4 (HTTP-protocol)
                     │                                          │             │
                     │                                          │             ▼
                     │                                          │       Шаг 5 (запуск registry)
                     │                                          │             │
                     │                                          │             ▼
                     │                                          │       Шаг 6 (первые либы)
```

Шаги 1–4 — **код**, могут идти параллельно частично (CLI можно
писать с заглушками registry, lockfile отдельно). Шаги 5–6 —
**инфраструктура и community**, нужны после кода.

## Параллельные задачи

- **Module-resolution в bootstrap'е.** Сейчас бутстрап-компилятор
  знает только path-импорты в одном workspace. Расширить на
  `git`/`registry`-resolution **до** Шага 1 не нужно — пока
  работаем в одном workspace, registry не нужен.
- **`std/` vs registry.** Решить как stdlib доставляется (см. Шаг 6
  пункт 1) — это отдельный D-decision, спекулятивный.
- **Security model.** Cargo/npm имели проблемы supply-chain атак
  (typosquatting, malicious updates). Подумать о signed releases
  / binary transparency log как Go modules sumdb.

---

## Trade-offs / упрощения

- **Шаги 1 и 2 объединить?** Технически self-host нужен только
  чтобы `nova` CLI был на Nova. Можно написать CLI на Rust
  заранее (как сейчас bootstrap-compiler) и потом portировать.
  **Рекомендация:** не объединять — self-host даёт более чистый
  набор API для CLI.
- **PubGrub vs backtracking.** PubGrub сложнее, но работает быстрее
  на больших dep-графах. Для первого release достаточно простого
  backtracking; PubGrub — оптимизация когда dep-graphs реальных
  пользователей этого потребуют.
- **Registry на хостинге vs distributed.** Cargo/npm — централизованный
  registry. Go modules — distributed (любой git URL). Для Nova
  централизованный проще; distributed как backup option (через
  `git = "..."` в `nova.toml`).

---

## Когда начинать

**Не сейчас.** Текущий приоритет — **bootstrap-compiler** (Rust,
v0.1–v1.0), потом **stdlib** (что уже идёт), потом **self-host**.
Package ecosystem — после v2.0+.

В spec'е D78 описан формат **уже сейчас** — это правильно:
формат стабильный с самого начала, infrastructure догоняет.

## Связанные документы

- [01-roadmap-v0.1.md](01-roadmap-v0.1.md) — общий roadmap; v0.6
  упоминает «Package manager», v2.0+ — self-hosting.
- [spec/decisions/07-modules.md → D78](../../spec/decisions/07-modules.md#d78)
  — формат `nova.toml`, `nova.lock`, registry chain, workspace.
- [spec/decisions/01-philosophy.md → D10](../../spec/decisions/01-philosophy.md#d10)
  — AI-first как обоснование явных манифестов и lockfile'ов.
