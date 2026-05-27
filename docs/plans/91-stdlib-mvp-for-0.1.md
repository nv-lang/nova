// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 91 — std MVP для релиза 0.1

> **Статус:** 🟡 Ф.0 ЗАКРЫТ 2026-05-27 (branch `plan-91-stdmvp`, worktree
> `nova-p91`); Ф.1-Ф.7 переоценены по факту re-baseline — см. секцию
> «Ф.0 closure (2026-05-27)» в конце документа.
> **Приоритет:** P0 — блокер публичного релиза 0.1
> **Оценка (исходная):** крупная (~3-5 недель работы codegen-агента).
> **Оценка (после Ф.0 re-baseline 2026-05-27):** ~5-7 рабочих дней
> (1 dev-week sequential, или ~2 дня wall-time с parallel-agent split).
> Большинство ожидаемых codegen-блокеров уже закрыты Plan 95/99/101/103
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
| Коллекции | `Vec`, `HashMap`, `HashSet` | Rust |
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

### Ф.1 — Коллекции: `Vec`, `HashMap`, `HashSet` (ядро)

Самые востребованные модули — закрывают наибольшую долю реального
кода. Известные кандидаты-блокеры (подтвердить/опровергнуть в Ф.0):

- generic specialization при monomorphization (`set.nv` —
  type-erased `Iter[T]` без concrete `next`);
- array-type mangling (`vec.nv` — malformed `Nova_[]T*` вместо
  `NovaArray_<T>*`);
- protocol-bound dispatch D72 для generic-erased `K.eq`/`K.hash`
  (`hashmap.nv`);
- tuple type system — mixed-type `(K, V)` (все поля `_NovaTupleN`
  захардкожены в `nova_int`).

Acceptance: `Vec`, `HashMap`, `HashSet` компилируются `→ exe` и
проходят conformance-тесты Ф.5.

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

## Ф.0 closure (2026-05-27)

**Branch:** `plan-91-stdmvp` (worktree `nova-p91` от github/main HEAD
`32f3dd51392`, post-Plan 83.10.4 merge `7b5b2fec8e0`).

### Ф.0.1 Re-baseline — measured data

**Setup:** `cargo build --release --manifest-path nova-cli/Cargo.toml`
(✅ 2m 01s, 4 warnings only — нет regression в codegen-крейте).

**`nova check` per-file matrix (55 файлов в MVP-relevant doменах):**

| Domain | PASS | FAIL | Stack OF |
|---|---:|---:|---:|
| `std/collections/` (10 файлов) | 9 | 1 | 0 |
| `std/identifiers/` (4) | 4 | 0 | 0 |
| `std/checksums/` (2) | 2 | 0 | 0 |
| `std/crypto/` (6) | 1 | 5 | 0 |
| `std/data/` (3) | 3 | 0 | 0 |
| `std/concurrency/` (4) | 3 | 1 | 0 |
| `std/path/` (2) | 2 | 0 | 0 |
| `std/time/` (2) | 1 | 1 | 0 |
| `std/math/` (2) | 1 | 1 | 0 |
| `std/text/` (3) | 2 | 1 | 0 |
| `std/encoding/` (7) | 5 | 1 | 1 |
| **Total** | **43** | **11** | **1** |

`nova check std/` recursive: **stack overflow** на `std/encoding/toml.nv`. С
`--skip std/encoding/toml.nv`: **43 PASS / 12 FAIL** (toml считается FAIL).
Числа стабильны vs 2026-05-23 STATUS.md baseline — Plan 100/103/104 не
трогали std/-source.

**`nova build → exe` smoke на каждый MVP-модуль** (`target/smoke_*.nv`,
realistic мини-программа на каждый домен):

| MVP module | build | run | result |
|---|---|---|---|
| Option (prelude) | ✅ | ✅ | map + unwrap_or → корректные числа |
| Result (prelude) | ✅ | ✅ | Ok/Err pattern match с sum-type ошибкой |
| Vec (`[]T` ext) | ✅ | ✅ | map/filter/fold/any/all/first на `[]int` |
| HashMap | ✅ | ✅ | new/insert/get/len/match Option |
| HashSet (Set) | ✅ | ✅ | insert/contains/len |
| Duration | ✅ | ✅ | from_secs/from_millis/plus/as_nanos/as_millis |
| Math (runtime stubs) | ✅ | ✅ | f64.sqrt/.pow/.ln |
| Text basic | ✅ | ✅ | split/trim/to_upper/starts_with + for-in []str |
| Text extended | ❌ | n/a | нет `[]str.join`, `str.parse_int`, `str.parse_f64`, `str.pad_*`, `str.repeat`, `str.replace` |
| JSON | ❌ | n/a | 5 D52 §2 source errors в std/encoding/json.nv |
| Sort | n/a | n/a | модуля `std/sort.nv` не существует |

### Ф.0.2 Категоризация блокеров

**Группа A — D52 §2 shorthand violations (parser-strict regression, ~6 файлов).**
Тривиальный source-level fix (`name: name` → `name`, `name: @name` → `@name`).
Затрагивает std/encoding/json.nv (MVP), std/math/complex.nv (Ф.4 fringe — выносим
в experimental), std/time/cron.nv (non-MVP), std/text/regex.nv (non-MVP),
std/crypto/jwt.nv (non-MVP). **Effort:** 5-15 минут.

**Группа B — Array literal parser issue ("expected `,` или `]`, got int literal",
4 файла).** Все в `std/crypto/{hmac,md5,sha1,sha256}.nv` — **non-MVP**. Diagnose
позже; в Ф.7.1 уезжают в experimental.

**Группа C — E_UNUSED_PREFIX_TYPEVAR (Plan 101.1 / D145 strictness, 1 файл).**
`std/concurrency/retry.nv:107` — non-MVP. Удалить неиспользуемый `E` из prefix.

**Группа D — Missing runtime methods (real Ф.2 work).** В runtime_registry:
`[]str.join`, `str.parse_int` (+radix), `str.parse_f64`, `str.pad_left`/`pad_right`,
`str.repeat`, `str.replace`. ~8 методов через runtime_registry + C stubs.
**Effort:** 0.5-1 день.

**Группа E — Missing module `std/sort.nv` (real Ф.4 work).** Создать модуль с
canonical API: `fn[T Ord] []T @sort()`, `@sort_by(cmp)`, `@binary_search`,
`@min`/`@max`. Алгоритм MVP — insertion-sort или дешёвый pdq-sort на Nova.
**Effort:** 1-2 дня (с conformance).

**Группа F — Parser stack overflow на TOML.** Single-file, non-MVP. Карантин в
Ф.7.1; deep-recursion fix — отдельный bug-track вне Plan 91 scope.

**Группа G — Standalone `nova check` import-cycle (range.nv).** Tooling polish,
не блокер; не Plan 91 scope.

### Ф.0.3 STATUS.md

Обновлён в commit Ф.0 (см. `std/STATUS.md` секция «Текущий статус
(2026-05-27, Plan 91 Ф.0 re-baseline)»). Старая секция «Группы B-M» помечена
как устаревшая; новый категоризованный список — выше.

### Ф.0.4 — Decision: новый порядок и оценки

Исходный порядок Plan 91 (Ф.1 collections → Ф.2 text → Ф.3 json → Ф.4
time/math/sort) предполагал Ф.1 = major codegen work. **Реальность 2026-05-27:**
Vec/HashMap/Set компилируются и работают end-to-end. Закрыто Plan 95/99/101/103
попутно. Ф.1 теперь — **conformance + API extension**, не core-блокеры.

**Новый рекомендуемый порядок (по «cheapest first + parallel where independent»):**

| # | Phase | Что | Effort | Parallel? |
|---|---|---|---:|---|
| 1 | **Ф.7.1 (вперёд)** | Quarantine non-MVP modules → `std/experimental/` + nova.toml. Убирает 12 FAIL → 0 в `nova check std/` без `--skip`. | 0.5 дн | sequential (foundation) |
| 2 | Ф.3 | JSON D52 §2 fix (5 edits) + smoke compile→exe + round-trip conformance | 0.5 дн | parallel с (3,4) |
| 3 | Ф.4 | sort module create + Instant smoke + canonical min/max wrappers + 4 conformance | 1-2 дн | parallel с (2,4) |
| 4 | Ф.2 | text join/parse/pad/repeat/replace — 8 methods через runtime_registry + C stubs | 1-2 дн | parallel с (2,3) |
| 5 | Ф.1 | conformance-валидация Vec/HashMap/Set (cross-product тесты) | 0.5 дн | sequential after (1) |
| 6 | Ф.5 | conformance integration в `nova_tests/plan91/` + property tests | 1 дн | sequential after (2-5) |
| 7 | Ф.6 | getting-started + 5-7 examples; Ф.6.1 decision (in-memory default; TCP-echo если 83.12 готов) | 1 дн | sequential after (5) |
| 8 | Ф.7.2-Ф.7.4 | release checklist, STATUS.md final, docs/plans README | 0.5 дн | sequential final |

**Total:** ~5-7 рабочих дней sequential = ~1 dev-week.
**Parallel split (Plan 103.4 pattern):** Ф.7.1 → 3 parallel Sonnet agents
(Ф.2/Ф.3/Ф.4) → Opus merge → Ф.1/Ф.5/Ф.6 sequential → Ф.7.2-Ф.7.4.
**Wall-time ~2-3 дня с parallel split.**

### Ф.0.4 — Phase entry conditions для следующих сессий

Чтобы Ф.1-Ф.7 могли запускаться отдельными агентами (или в новых сессиях),
зафиксируем точные entry-точки:

**Ф.7.1 entry (рекомендуемая первая фаза):**
- Worktree `nova-p91` от branch `plan-91-stdmvp` (HEAD после Ф.0 closure commit).
- Цель: создать `std/experimental/` + переместить файлы из «Группа B/C/F»
  (toml + 4 crypto + retry + 5 non-MVP encoding/text/math/identifiers/checksums
  по списку в std/STATUS.md «Ф.7.1 — Quarantine»).
- Acceptance: `nova check std/` (no `--skip`) → 0 FAIL.

**Ф.3 entry:** worktree `nova-p91`, после Ф.7.1.
- 5 D52 §2 edits в `std/encoding/json.nv` (lines 289:36, 443:47, 593:66 + 2 ещё).
- `target/smoke_json.nv` round-trip encode→decode на нетривиальном документе.
- Conformance test в `nova_tests/plan91/json_*.nv`.

**Ф.4 entry:** worktree `nova-p91`, после Ф.7.1.
- Create `std/sort.nv` с canonical API (см. Группу E).
- Create `nova_tests/plan91/sort_*.nv` (4+ conformance).
- Optionally — canonical `int @min/@max(other)` через runtime_registry.

**Ф.2 entry:** worktree `nova-p91`, после Ф.7.1.
- 8 methods в `compiler-codegen/src/codegen/runtime_registry.rs`
  (`[]str.join`, `str.parse_int`/`parse_int_radix`/`parse_f64`/`pad_left`/`pad_right`/
  `repeat`/`replace`).
- C stubs в `compiler-codegen/nova_rt/string.c`.
- Регенерация `std/runtime/string.nv` (auto-generated from registry).
- 6+ conformance tests в `nova_tests/plan91/text_*.nv`.

### Артефакты Ф.0 в worktree

- `std/STATUS.md` — обновлён.
- `target/smoke_{vec,hashmap,set,time,math,text,option,result,vec_full}.nv` — smoke
  programs (рабочие, build→exe→run проверены).
- `target/smoke_runner.sh`, `target/smoke_runner2.sh` — repro scripts для smoke
  matrix.
- `target/p91_check.sh`, `target/p91_check_results.txt` — per-file `nova check`
  matrix (raw output).
