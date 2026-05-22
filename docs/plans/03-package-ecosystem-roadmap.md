// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 03: Package ecosystem — roadmap-индекс

> **Создан:** 2026-05-07. **Редакция 2** (2026-05-22): production-grade
> переработка — честное сравнение с Cargo/npm/Go-mod/Deno/pip,
> Nova-уникальный угол (effect-aware зависимости), production security-
> модель, **декомпозиция** на под-планы 03.1–03.6.
> **Статус:** roadmap-индекс. **Приоритет:** P3 — после bootstrap/stdlib;
> **но 03.1 (`path`/`git`-зависимости) можно делать раньше** (§6).
> **Spec:** [D78](../../spec/decisions/07-modules.md#d78) (`nova.toml`,
> `nova.lock`, registry chain, workspace).

---

## 1. Что

Экосистема пакетов Nova: от зависимостей внутри одного дерева исходников
до публичного registry с подписанными, верифицируемыми библиотеками.
Дополняет [Plan 01](01-roadmap-v0.1.md) (roadmap по версиям компилятора).

## 2. Текущее состояние (2026-05-22)

- ✅ **Spec D78** — `nova.toml`, формат `nova.lock`, registry chain,
  workspace.
- ✅ **Workspace** — корневой `nova.toml` + per-member (`std/`,
  `examples/`, `nova_tests/`).
- ✅ **Module path = file path** enforcement (D78, Plan 81).
- ❌ **Внешние зависимости** — нет вообще. `path`/`git`-deps не
  резолвятся; `nova.lock` не генерируется.
- ❌ `nova add`/`publish`/`update`/`search` — нет.
- ❌ Version resolution (SAT/PubGrub) — нет.
- ❌ Registry HTTP-протокол / `nova-registry.org` — нет.

Единственная единица сборки сейчас — один workspace без внешних deps.

## 3. Честное сравнение с индустрией

| Аспект | Cargo (Rust) | npm (JS) | Go modules | Deno | pip (Python) | **Nova — цель** |
|---|---|---|---|---|---|---|
| Источники deps | registry + path + git | registry + path + git | git/proxy (decentralized) | URL/JSR | registry + VCS | registry + `path` + `git` |
| Version resolution | PubGrub | npm SAT (semver) | MVS (minimal version selection) | — | backtracking | PubGrub (доказан в Cargo) |
| Lockfile | `Cargo.lock` | `package-lock.json` | `go.sum` | `deno.lock` | `requirements`/`uv.lock` | `nova.lock` (D78) |
| Immutable releases | да | да (но npm unpublish issues) | да (через proxy+sumdb) | content-addr | да | **да — content-addressed** |
| Подпись/transparency | minisign (опц.) | provenance (новое) | **sumdb (transparency log)** | — | PEP 458 (медленно) | **sumdb-стиль log** (§5) |
| Security advisories | RUSTSEC + `cargo audit` | npm audit / GHSA | govulncheck | — | OSV / `pip-audit` | OSV-совместимая БД + `nova audit` |
| Видимость supply-chain поведения | ❌ (нельзя узнать, что crate начал ходить в сеть) | ❌ | ❌ | permission-флаги **рантайма** | ❌ | ✅ **эффекты в типах** (§4) |
| Air-gapped / vendoring | `cargo vendor` | offline mirror | `GOPROXY`+vendor | — | devpi/wheelhouse | mirror-friendly registry |

**Что берём:** PubGrub (как Cargo — доказанный resolver), content-
addressed immutable releases, sumdb-стиль transparency log (как Go —
сильнейшая supply-chain защита из мейнстрима), OSV-совместимые advisory.
**Чего избегаем:** npm-стиль `unpublish` (ломал экосистему), глубокие
транзитивные деревья без дедупликации.

## 4. Nova-уникальный угол — где Nova может быть **лучше** Cargo/Go

Nova трекает **эффекты в типах** (D62) и **capabilities** (`forbid`,
D63). Менеджер пакетов языка с эффект-системой умеет то, чего не может
ни один мейнстрим:

- **Effect-surface зависимости видна.** `nova info foo` показывает
  агрегированный effect-row публичного API пакета: «`foo` использует
  `Net`, `Fs`». В Cargo/npm узнать, что библиотека ходит в сеть,
  **невозможно** без аудита кода.
- **Effect-diff как supply-chain сигнал.** Minor-バージيونный bump,
  добавивший `Net` в ранее чистую функцию, — **красный флаг**. `nova`
  показывает effect-diff при `update`; CI может на него падать. Это
  ловит ровно тот класс атак (внезапная сетевая активность в патч-
  релизе), который годами бьёт npm/PyPI.
- **Capability-confined зависимости.** Проект объявляет в `nova.toml`
  границу: `foo` может использовать только `Db`, но не `Net`/`Fs` —
  компилятор **enforce'ит** (через `forbid` на границе пакета).
  Зависимость в песочнице на уровне типов, не рантайма (сильнее
  Deno-permissions — те рантаймовые).
- **Верифицированные контракты.** Пакет может поставлять контракты
  (Plan 33); registry фиксирует verification-статус — «эта версия
  прошла SMT-проверку».

Это превращает экосистему из «догнать Cargo» в «структурно безопаснее
Cargo по supply-chain». Дизайн `nova.toml`/`nova.lock`/registry (под-
планы ниже) обязан с самого начала закладывать поля под effect-surface
и capability-границы.

## 5. Production security-модель (обязательна, не v2)

Supply-chain — главная проблема пакетных менеджеров последнего
десятилетия. Закладывается в дизайн сразу:

- **Content-addressed immutable releases.** Каждая версия — sha256;
  `nova.lock` фиксирует хеш. Опубликованную версию нельзя подменить
  (никакого npm-`unpublish`).
- **Подписанные релизы + transparency log.** Append-only auditable log
  (Go sumdb / Sigstore-стиль): скомпрометированный registry не может
  тихо подменить пакет — расхождение с логом ловится. `nova.lock`
  пишет и хеш, и log-inclusion-proof.
- **`nova audit`** — OSV-совместимая БД advisory; падает на known-CVE
  в дереве зависимостей.
- **Effect-diff** (§4) — Nova-уникальный слой supply-chain контроля.
- **Typosquatting-policy** + namespace-резервирование на registry.
- **Mirror/vendoring** — air-gapped (банки, гос): любой может поднять
  proxy/mirror.

## 6. Декомпозиция на под-планы

| # | Файл | Что | Зависимость | Когда |
|---|---|---|---|---|
| **03.1** | [03.1-path-git-dependencies.md](03.1-path-git-dependencies.md) | `path`- и `git`-зависимости в bootstrap-компиляторе + `nova.lock` для них. **Без registry, без self-host.** | нет | **можно сейчас** |
| **03.2** | `03.2-version-resolution.md` (план) | PubGrub resolver + version-ranged deps (`^1.2`) + `nova update`. | 03.1 | после 03.1 |
| **03.3** | `03.3-registry-protocol.md` (план) | HTTP registry protocol + content-addressing + подпись + transparency log (§5). | 03.2 | после 03.2 |
| **03.4** | `03.4-nova-cli-package-cmds.md` (план) | `nova add`/`publish`/`update`/`search`/`info`/`audit` + effect-surface/effect-diff (§4). | 03.1–03.3 | параллельно 03.2/03.3 |
| **03.5** | `03.5-registry-hosting.md` (план) | `nova-registry.org` — хостинг, UI, CDN, policy-документы. Инфраструктура. | 03.3, 03.4 | после кода |
| **03.6** | `03.6-stdlib-delivery-and-first-libs.md` (план) | Решение «stdlib vendored vs пакет» + первые community-библиотеки. | 03.5 | последним |

В этой редакции **создан под-план 03.1**; 03.2–03.6 — слоты, плановые
файлы пишутся по мере подхода очереди (как делалось для 33.x/44.x).

## 7. Порядок и гейтинг — коррекция редакции 1

**Self-hosting — НЕ блокер экосистемы.** Редакция 1 ставила «Шаг 1:
self-hosted compiler» первым и гейтила всё на нём. Это неверно — и сам
раздел «Trade-offs» редакции 1 себе противоречил («можно написать CLI
на Rust заранее»). Коррекция:

- Функциональность менеджера пакетов (`path`/`git`-deps, lockfile,
  resolver, registry-клиент) делается в **bootstrap-Rust** `nova` CLI —
  **сейчас**, без self-host.
- Self-hosting (Plan 01, v2.0+) — про то, **на каком языке написан
  инструмент** (догфудинг), а не про то, **что он умеет**. Когда
  компилятор self-hosted — команды портируются на Nova. Это
  ортогонально, не prerequisite.
- → **03.1 можно начинать в любой момент** — он разблокирует
  multi-package разработку (напр. сайт [Plan 64](64-nova-lang-website.md)
  зависит от `std` как от пакета) задолго до v2.0.

## 8. Связь

- [Plan 01](01-roadmap-v0.1.md) — общий roadmap; self-hosting (v2.0+).
- [D78](../../spec/decisions/07-modules.md#d78) — `nova.toml`/`nova.lock`/
  registry chain/workspace; дизайн полей расширяется под §4 (effect-
  surface, capability-границы).
- [D62](../../spec/decisions/04-effects.md) / D63 — эффекты и `forbid` —
  фундамент Nova-уникального §4.
- [Plan 33](33-contracts-implementation.md) ✅ — контракты; verification-
  статус в registry (§4).
- [Plan 81](81-module-resolution-hardening.md) ✅ — резолв модулей внутри
  дерева; 03.1 надстраивается над ним для **межпакетного** резолва.
- Ориентиры: Cargo (PubGrub, immutable), Go modules (sumdb transparency),
  Deno (permissions — рантаймовые; Nova — типовые), npm (чего избегать).
