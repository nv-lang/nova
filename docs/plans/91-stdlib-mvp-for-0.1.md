// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 91 — std MVP для релиза 0.1

> **Статус:** 📋 proposed 2026-05-22, не начат
> **Приоритет:** P0 — блокер публичного релиза 0.1
> **Оценка:** крупная (~3–5 недель работы codegen-агента),
> декомпозируется по модулям; точная оценка — после Ф.0
> **Зависимости:** Plan 14 ✅ (codegen-gaps), Plan 34 ✅ (std type-check
> 45/45), Plan 35 ✅ (cross-file resolve MVP), Plan 15 ✅ (D72 generic
> bounds), Plan 87/88/89 ✅ (точечные codegen-блокеры)
> **Источник:** `std/STATUS.md` — std не компилируется в exe;
> [Plan 01](01-roadmap-v0.1.md) — roadmap релиза 0.1.
>
> **Scope-decision C для 0.1 (2026-05-27):** релиз 0.1 = `std MVP` (этот план)
> **+ `std/net` (Plan 83.12 co-planned)** **+ `std/sync` (Plan 103.x уже закрыт)**.
> Plan 91 и Plan 83.12 запускаются параллельно (разные scope, разные worktrees),
> но shipping gate для 0.1 — совместный (oба должны быть closed одновременно).
> Раньше планы определяли 0.1 противоречиво (91: «без net/sync» vs 83.12: «без TCP
> язык hypothetical»); решение C согласует оба под фактический статус —
> Plan 103.x уже в main, Plan 83.12 — последний крупный модуль для 0.1.

## Worktree setup (pre-flight)

**Convention:** постоянный worktree `nova-p91` (descriptor `p91`).

```bash
# Из main repo (d:/Sources/nv-lang/nova):
git fetch github main
git worktree add ../nova-p91 -b plan-91-stdmvp github/main

# Зарегистрировать worktree (feedback-worktree-auto-register).

# Pre-flight для nova test (memory project-worktree-nova-test-setup):
rm -rf ../nova-p91/compiler-codegen/nova_rt/libuv
cp -r compiler-codegen/nova_rt/libuv ../nova-p91/compiler-codegen/nova_rt/libuv
rm -rf ../nova-p91/compiler-codegen/nova_rt/libuv/.git

mkdir -p ../nova-p91/target
cp -r target/libuv-cache ../nova-p91/target/libuv-cache 2>/dev/null || true

# Env vars (читает detect_boehm в test_runner.rs):
export NOVA_GC_LIB_DIR=d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib
export NOVA_GC_INCLUDE_DIR=d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include

# Verify build:
cargo build --release --manifest-path nova-cli/Cargo.toml
```

## Model / Thinking mode per phase

| Phase | Model | Effort | Thinking | Why |
|---|---|---|---|---|
| **Ф.0 re-baseline (GATE)** | **Opus 4.7** | high | **ON** | decision-heavy: pass-rate calibration + blocker grouping + Ф.0.4 decision point (что в MVP, что карантинить); неправильное решение здесь раздует Ф.1-7 |
| Ф.1 Vec/HashMap/HashSet | Sonnet 4.6 | high | **ON** | generic mono + D72 dispatch + tuple types — алгоритмически сложные codegen-блокеры |
| Ф.2 text | Sonnet 4.6 | high | OFF | mostly mechanical (str.from/interpolation уже есть) |
| Ф.3 json | Sonnet 4.6 | high | **ON** | pattern composition в tuple/list — нетривиальный parser/codegen |
| Ф.4 time/math/sort | Sonnet 4.6 | high | OFF | mostly mechanical (закрытие точечных методов) |
| Ф.5 conformance tests | Sonnet 4.6 | high | OFF | explicit list |
| Ф.6 getting-started | Sonnet 4.6 | high | **ON** | Ф.6.1 decision (in-memory или TCP-echo getting-started) |
| Ф.7 quarantine + checklist | Sonnet 4.6 | high | OFF | mechanical |

**Fallback:** stuck >1 retry на Ф.1/Ф.3/Ф.6 — escalate на Opus 4.7.

**Parallelism note:** Plan 91 запускается параллельно с
[Plan 83.12](83.12-async-net-stdlib.md) — разные scope (`std/{collections,text,
sort,json,time,math}` vs `std/net/`), разные worktrees, 0 пересечений по
файлам. Final shipping gate 0.1 совместный.

## Agent execution rules

Применять **automatically** (memory feedback, проверяется в Ф.7.4 closure):

- `feedback-no-claude-coauthor` — никаких `Co-Authored-By: Claude` trailer в commits.
- `feedback_git_add_specific` — `git add` только конкретных файлов, никогда
  `git add -A` / `git add .`.
- `feedback-commit-per-task` — commit после каждой фазы; не batchить Ф.1-Ф.4 в один commit.
- `feedback-update-logs` — после **каждой** закрытой фазы обновлять
  `docs/project-creation.txt` + `docs/simplifications.md` (main repo) +
  `nova-private/discussion-log.md` (отдельный репо, отдельный commit).
- `feedback_nova_test_one_pass` — `nova test` за один запуск, capture summary + FAIL details одновременно.
- `feedback_targeted_test_per_fix` — per-fix verify = только targeted fixture; full `nova test` только в конце фазы.
- `feedback_nova_syntax` — никогда не выдумывать синтаксис Nova; смотреть `spec/decisions/` и `examples/` перед написанием кода.

## Зачем

`std/STATUS.md` (на 2026-05-09): std-файлы **не компилируются** в
нативный exe — type-check мягче codegen, до `nova build` доходит
~3/50. Без рабочей стандартной библиотеки на Nova нельзя написать
нетривиальную программу → публичный релиз 0.1 бессмыслен, адопшен
языка закрыт.

Plan 18 ([18-stdlib-roadmap.md](18-stdlib-roadmap.md)) описывает
полную stdlib (fs/net/http/sync через libuv) — это горизонт
0.2–0.4. Plan 91 уже — про **минимально достаточный** набор для
релиза 0.1: алгоритмическое ядро std, которое **уже написано** в
`std/` и требует не нового кода, а **снятия codegen-блокеров**.

Это критический путь к 0.1. Не новые фичи языка — доведение
написанного до компиляции.

## Scope — что входит в «std MVP»

Не вся std. Минимальный набор, позволяющий написать реальную
программу (CLI-утилита, обработка данных в памяти, демонстрация
эффектов/handler'ов). Дизайн модулей берётся по мотивам Rust / Go /
TS, как и просит источник.

| Домен | Модули MVP | Ориентир |
|---|---|---|
| Опционал / ошибки | `Option`, `Result` + комбинаторы (`map`/`unwrap_or`/`?`) | Rust |
| Коллекции | `[]T` (встроен, Vec не нужен), `HashMap`, `HashSet`; vec-комбинаторы (`map`/`filter`/`fold`) | Rust |
| Текст | split / join / trim / pad / parse чисел; форматирование через `str.from` + интерполяцию `"${}"` | Go `strings`, TS |
| Сортировка / поиск | `sort[T Ord]`, `sort_by`, `binary_search`, `min`/`max` | Go `slices`, Rust |
| JSON | encode / decode | TS |
| Время | `Instant`, `Duration` (без календаря и tz) | Go `time` |
| Математика | базовые функции (`abs`/`min`/`max`/`pow`/`sqrt`/округления) | общее |

**Co-planned для 0.1 (отдельные планы, общий shipping gate):**

- `std/net` (TCP/UDP/addr/error) — **[Plan 83.12](83.12-async-net-stdlib.md)**,
  ~1 dev-week. Параллельный track, не пересекается по файлам с Plan 91.
- `std/sync` (`Atomic*`, `Mutex`, `RwLock`, `ReentrantMutex`, `Once`,
  `OnceCell`, `Lazy`, `Semaphore`/`Barrier`/`CountDownLatch`/`Condvar`)
  — Plan 103.x; 103.1/103.2/103.3/103.4/103.5 ✅ уже в main;
  остаются 103.6 (realtime/blocking type-checker enforcement),
  103.7 (spec D-blocks final), 103.8 (V1 close) — spec/audit/closure
  работа, не impl. Для 0.1 sync edет «как есть» — все primitives уже
  shipped.

**Явно отложено за пределы 0.1** (релизы 0.2+):

- `fs` / `io` / `os` — файловый и системный ввод-вывод (libuv,
  отдельная крупная работа, Plan 18);
- `http` — HTTP layer (Plan 18 P1; зависит от 83.12);
- `crypto`, `checksums`, `regex`, `encoding` (кроме base64 — опц.),
  `data/sql`, `markdown`, `url`, полноценный календарь времени;
- остальные «аспирационные» модули `std/`.

Getting-started для 0.1 — два варианта на выбор после Ф.6.1 (decision
по wow-эффекту): (a) алгоритмическое ядро + эффекты с in-memory
handler'ами (killer-пример из README — `Db` через `in_memory_db`); либо
(b) TCP-echo сервер/клиент поверх 83.12 — backend-claim демонстрация.
Оба варианта работают на MVP-std, выбирается **один** для shipping.

## Метод

std-код написан — блокеры на стороне codegen. План закрывает блокеры
**группами по модулям**, в порядке «сколько MVP-модулей разблокирует».
После каждой группы — `nova test --include-stdlib` без новых FAIL.

Алгоритм для каждого MVP-модуля:
1. `nova build std/<...>.nv` → зафиксировать конкретный codegen/CC
   блокер (не доверять STATUS.md — он от 2026-05-09, устарел).
2. Закрыть блокер в `compiler-codegen/` (parser / type-checker /
   `emit_c.rs` / `nova_rt/`).
3. Conformance-тест на модуль (раздел Ф.5) — реальный use-case.
4. Повторять до `→ exe` PASS.

## Декомпозиция

### Ф.0 — Re-baseline (GATE, ~0.5 д)

STATUS.md и таблица «Накопленные блокеры std/» из
[Plan 14](14-stdlib-codegen-gaps.md) — от 2026-05-09, более 50
планов назад. Plan 15/35/87/88/89 могли закрыть часть блокеров
попутно. **Нельзя планировать по устаревшим данным.**

- **Ф.0.1** Прогнать `nova test --include-stdlib` + поштучно
  `nova build` по каждому файлу из MVP-набора. Зафиксировать
  **актуальный** pass-rate (`check` / `→ exe`) и **актуальный**
  список блокеров с точными ошибками.
- **Ф.0.2** Сгруппировать блокеры по природе (parser / type-checker /
  codegen / runtime / CC-stage) и по числу разблокируемых
  MVP-модулей. Это authoritative-список — он, а не STATUS.md,
  управляет фазами Ф.1–Ф.4.
- **Ф.0.3** Обновить `std/STATUS.md` под актуальное состояние;
  пометить дату.
- **Ф.0.4** Decision point: уточнить порядок Ф.1–Ф.4 и оценку
  трудоёмкости по результату Ф.0.2.

### Ф.1 — Коллекции: `HashMap`, `HashSet` + vec-комбинаторы (ядро)

> **Решение 2026-05-27:** `Vec[T]` как отдельный тип **не нужен** —
> `[]T` уже является встроенным динамическим массивом в Nova (`Vec`
> в Nova-семантике). `vec.nv` содержит функциональные комбинаторы
> (`map`/`filter`/`fold`/`any`/`all`/`first`/`last`) поверх `[]T`
> через D35 (`fn []T @method`). Никакой Vec-обёртки нет и не нужно.
> Пересмотр только если появится обоснованная причина (например,
> отдельный ownership-семантический тип).

Самые востребованные модули — закрывают наибольшую долю реального
кода. Известные кандидаты-блокеры (подтвердить/опровергнуть в Ф.0):

- generic specialization при monomorphization (`set.nv` —
  type-erased `Iter[T]` без concrete `next`);
- vec-комбинаторы (`vec.nv` — `map`/`filter`/`fold` на `[]T`,
  array-type mangling `Nova_[]T*` вместо `NovaArray_<T>*`);
- protocol-bound dispatch D72 для generic-erased `K.eq`/`K.hash`
  (`hashmap.nv`);
- tuple type system — mixed-type `(K, V)` (все поля `_NovaTupleN`
  захардкожены в `nova_int`).

Acceptance: `HashMap`, `HashSet`, vec-комбинаторы компилируются
`→ exe` и проходят conformance-тесты Ф.5.

### Ф.2 — Текст и форматирование

Строковые утилиты (`text/`): split/join/trim/pad, парсинг чисел.
Часть операций уже в runtime-stdlib (Plan 13) — Ф.2 закрывает то,
что написано на Nova поверх. Форматирование — через `str.from(v)`
(D73) + интерполяцию `"${}"` (Plan 17 ✅), без `Display`/`Debug`.

### Ф.3 — JSON

`json` encode/decode. Зависит от Ф.1 (`HashMap`, `Vec`) и Ф.2.
Известный кандидат-блокер: pattern composition в tuple/list
(«expected pattern, got `,`» — группа B STATUS.md). Conformance —
round-trip encode→decode.

### Ф.4 — Время, математика, сортировка

`time` (`Instant`, `Duration` — без календаря), `math` (базовые
функции; кандидат-блокер — отсутствующие методы `f64.ln`/`.sqrt`,
группа L STATUS.md), `sort` (`sort[T Ord]`/`binary_search`).

### Ф.5 — Conformance-тесты MVP

Для каждого MVP-модуля — тест в `nova_tests/` в форме **реального
use-case целиком**, не микро-проверки:

- `Vec`/`HashMap`/`HashSet`: построение, обход, типовые операции,
  смешанные типы значений;
- `json`: round-trip encode→decode нетривиального документа;
- `text`: парсинг строкового ввода в структуры;
- `sort`: сортировка + `binary_search`, property-стиль (отсортировано
  ⇒ найдено);
- `time`/`math`: типовые вычисления.

Прогон в codegen-канале (`nova test`) обязателен — это release-путь.
Interp-канал — желательно (урок Plan 14 Ф.6: проверять оба канала).

### Ф.6 — Getting-started + примеры релиза

- **Ф.6.1** Программа getting-started, работающая end-to-end на
  MVP-std: установка → `nova run` → небольшая реальная программа
  (CLI или обработка данных в памяти + демонстрация эффектов с
  in-memory handler'ом).
- **Ф.6.2** 5–7 примеров в `examples/`, которые компилируются и
  запускаются на MVP-std (прогнать все — 0 FAIL).
- **Ф.6.3** Английский quick-start (README + getting-started;
  полную спеку не переводить).

### Ф.7 — Карантин не-MVP модулей + release-checklist

Релиз 0.1 не должен поставлять десятки модулей, которые не
компилируются.

- **Ф.7.1** Не-MVP «аспирационные» модули `std/` — явно отделить:
  каталог `std/experimental/` либо явные маркеры статуса +
  MVP-набор, зафиксированный в `std/nova.toml`. Решение — в Ф.0.4.
- **Ф.7.2** `std/STATUS.md` — финальная актуализация: что входит в
  0.1, что отложено в Plan 18.
- **Ф.7.3** `docs/plans/README.md` — статус Plan 91; `docs/plans/01-roadmap-v0.1.md`
  — отметка готовности std-части 0.1.
- **Ф.7.4** `docs/project-creation.txt` +
  `nova-private/discussion-log.md` — записи.

## Acceptance criteria

- [ ] Ф.0 дал актуальный (не из STATUS.md-2026-05-09) список
      блокеров; `std/STATUS.md` обновлён.
- [ ] MVP-набор (`Option`/`Result`, `Vec`, `HashMap`, `HashSet`,
      `text`, `sort`, `json`, `time`, `math`) компилируется
      `nova build → exe` без ошибок.
- [ ] Каждый MVP-модуль имеет conformance-тест в форме реального
      use-case; все проходят в codegen-канале.
- [ ] Программа getting-started собирается и запускается на MVP-std
      end-to-end.
- [ ] Все примеры в `examples/` компилируются и запускаются —
      0 FAIL.
- [ ] Не-MVP модули `std/` отделены так, что 0.1 не поставляет
      некомпилируемый код.
- [ ] Полный `nova test` — 0 новых FAIL относительно baseline.

## Non-scope

- `fs`/`io`/`http`/`os` — Plan 18, релизы 0.2–0.4 (требуют libuv,
  отдельная крупная работа).
- `net` — co-planned для 0.1 как **отдельный** [Plan 83.12](83.12-async-net-stdlib.md);
  Plan 91 его **не** реализует, но shipping gate 0.1 общий.
- `sync` — co-planned для 0.1 через [Plan 103.x](103-sync-primitives-spec-formalization.md);
  103.1/103.2/103.3/103.5 уже в main. Plan 91 sync **не** трогает.
- `crypto`/`checksums`/`regex`/`encoding`/`sql`/`markdown`/`url`,
  полноценный календарь и timezones.
- Новые фичи языка и D-блоки. Plan 91 — только доведение
  написанного std-кода до компиляции, не расширение языка.
- Self-hosting stdlib на Nova — Plan 90 / горизонт v2.0+.

## Связь с другими планами

- [14-stdlib-codegen-gaps.md](14-stdlib-codegen-gaps.md) — закрытые
  codegen-gaps + таблица «Накопленные блокеры std/» (база для Ф.0,
  но устарела — Ф.0 её перепроверяет).
- [18-stdlib-roadmap.md](18-stdlib-roadmap.md) — полная stdlib
  (fs/net/http/sync); Plan 91 — её MVP-подмножество для 0.1.
- [34-stdlib-typecheck-and-compile-fix.md](34-stdlib-typecheck-and-compile-fix.md)
  — std type-check 45/45 (предусловие Ф.0).
- [35-cross-file-resolve.md](35-cross-file-resolve.md) — cross-file
  resolve (нужен для `collections/`, импортирующих `HashMap`).
- [01-roadmap-v0.1.md](01-roadmap-v0.1.md) — roadmap 0.1; Plan 91
  закрывает его std-часть.
- [83.12-async-net-stdlib.md](83.12-async-net-stdlib.md) — **co-planned
  для 0.1** (scope-decision C 2026-05-27); параллельный track, общий
  shipping gate. Не пересекается по файлам.
- [103-sync-primitives-spec-formalization.md](103-sync-primitives-spec-formalization.md)
  — sync primitives umbrella; 103.1/103.2/103.3/103.5 уже в main,
  shipping в 0.1.
