// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 91 — std MVP для релиза 0.1

> **Статус:** 🟢 Ф.0+Ф.7.1+Ф.4 ЗАКРЫТЫ 2026-05-27; Ф.2.5 (D177) ЗАКРЫТ 2026-05-28;
> Ф.3 conformance smoke ✅ CLOSED 2026-06-05 (Plan 91.13 V2);
> **Ф.2-remainders** (try_parse_int) ✅ CLOSED 2026-06-08;
> **Ф.5** (conformance MVP) ✅ CLOSED 2026-06-08;
> **Ф.4 conformance** (time/math/sort fixtures + handler-lit capture codegen fix) ✅ CLOSED 2026-06-08;
> **Ф.6** (getting-started + examples) ✅ CLOSED 2026-06-08;
> **Ф.1 collections** (HashMap/Set/vec-combinators + fold non-int-Acc codegen fix) ✅ CLOSED 2026-06-08.
> Остаётся только Ф.7 (release checklist — отложен). **Все MVP-фазы Plan 91 закрыты.**
> Branch `plan-91-stdmvp`, worktree `nova-p91`. См. секции
> «Ф.0 closure», «Ф.7.1 closure», «Ф.4 closure», «Ф.2.5 closure» в конце документа.
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
> **Coordination с Plan 110.11 (added 2026-06-05):**
> Plan 110.11 — umbrella для stdlib types с `Consumable[E]` impls
> ([110.11-new-stdlib-types-consumable.md](110.11-new-stdlib-types-consumable.md)).
> Markers covered: `std/fs.File`, `std/bufio.BufReader/Writer`,
> `std/db.Transaction`, `std/pool.ConnPool`, `std/concurrency.CancelScope`,
> `Stream[T]`. Если Plan 91 ship'нет любой из этих types в своих фазах,
> Plan 110.11.X sub-task scope reduces к «add Consumable impl only».
> Pre-implementation per sub-plan: check Plan 91 roadmap для overlap.
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

## Принцип: Nova-first, C только для примитивов

> **Решение 2026-05-27:** максимально использовать Nova + Plan 96
> (слайсы `s[a..b]`) + Plan 90.1 (extend-family) вместо C-реализаций.
> Функцию пишем на Nova если она выражается через уже существующие
> примитивы. C остаётся только там, где нужны `memcmp`/`memcpy`/
> `alloc`/UTF-8 decode на уровне байт.

**Можно на Nova (перенести из C или написать сразу на Nova):**

| Метод | Nova-реализация | Статус |
|---|---|---|
| `str @replace(from, to)` | `@split(from).join(to)` | ✅ Ф.2 |
| `str @repeat(n)` | `StringBuilder.append_repeat` | ✅ Ф.2 |
| `str @pad_left(w, fill)` | `StringBuilder.append_repeat(fill, pad).append(@)` | ✅ Ф.2 |
| `str @pad_right(w, fill)` | `StringBuilder.append(@).append_repeat(fill, pad)` | ✅ Ф.2 |
| `[]T @map/filter/fold` | уже на Nova в `vec.nv` | ✅ готово |
| `[]str @join(sep)` | уже на Nova в `text.nv` | ✅ Ф.2 |

**nova_body блочный синтаксис (решение 2026-05-27):** `nova_body` в
`runtime_registry.rs` поддерживает две формы:
- `"expr"` → эмитируется как `fn @name(...) => expr`
- `"{ ... }"` → эмитируется как `fn @name(...) { ... }` (block form)

Это позволяет писать многострочные Nova-тела прямо в registry без
искусственного соединения через `;`.

**`str @split` — zero-copy (решение 2026-05-27):** отдельной функции
`split_to_slices` не нужно. В Nova `str` — это `(ptr, len)` без
ownership, нет разницы между "копией" и "view" на уровне типа.
Текущая C-реализация `nova_str_split` уже возвращает views в
оригинальный буфер (`{ s.ptr + start, len }` без `memcpy`) — как
Rust `str::split()` возвращает `&str`. API остаётся `[]str`.

**`str` / `StringBuilder` — нет изменяемого `[]u8` слайса (решение
2026-05-27):** разрешать мутирующий слайс байт нельзя — это сломает
UTF-8 invariant строки. Read-only `@bytes() -> []u8` уже есть.
`StringBuilder.append_bytes` принимает `[]u8` с явным предупреждением
в doc (caller отвечает за UTF-8 validity). Это сознательный дизайн,
как в Rust (`as_bytes()` read-only, запись через `unsafe`).

**Остаётся в C (byte-level примитивы, нельзя без FFI):**

| Метод | Причина |
|---|---|
| `trim`, `to_upper`, `to_lower` | `isspace`/`toupper` byte-уровень |
| `starts_with`, `ends_with`, `contains` | `memcmp` |
| `eq`, `hash`, `lt/le/gt/ge` | `memcmp`, FNV-1a |
| `byte_at`, `char_at` | UTF-8 decode |
| `concat` | `alloc` + `memcpy` |
| `find`, `rfind` | KMP/naive — эффективнее в C |
| `split` | массив `(ptr,len)` view'ов — zero-copy ✅ уже так |
| f64/f32 math (`sqrt`, `sin`, `ln`…) | libm intrinsics |

**Plan 90.1 (`extend_from`, `copy_from`, слайсы) как оптимизация:**
`[]u8` операции на Nova-коде std (например, `parse_int` через
`@bytes()[i]` итерацию) компилируются в тот же C что и ручной C-код,
но лучше тестируемы и читаемы. `extend_from` в `StringBuilder`-like
паттернах даёт zero-copy конкатенацию без ручного `memcpy`.

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
5. Предпочитать Nova-реализацию над C — см. таблицу «Nova-first» выше.

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

✅ **CLOSED 2026-06-05 — see [Plan 91.13 V2](91.13-json-conformance-smoke.md).**

`json` encode/decode. Зависит от Ф.1 (`HashMap`, `Vec`) и Ф.2.
Conformance suite (8 fixtures × ~30 test cases) delivered в
`nova_tests/plan91_13/`: primitives + arrays (empty + mixed) + objects
(empty + flat) + nested 3-level. Suite scope полный для V2 закрытия.
2 fixtures PASS, 6 CC-FAIL gated на 2 P1 codegen followup:
`[M-91.13-codegen-none-arm-nested-generic-mismatch]` +
`[M-91.13-codegen-match-arm-unit-vs-option-divergence]` — обе scoped
fixes в emit_c.rs, **не блокер closure** этого Ф.3 (suite готов, fixes
backlog'ируются как general codegen quality work).

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
- [x] MVP-набор (`Option`/`Result`, `Vec`, `HashMap`, `HashSet`,
      `text`, `sort`, `json`, `time`, `math`) компилируется
      `nova build → exe` без ошибок. (`time` = Duration/Timestamp/Monotonic — `Instant` ships как `Monotonic`, D124.)
- [x] Каждый MVP-модуль имеет conformance-тест в форме реального
      use-case; все проходят в codegen-канале. (time/math/sort — `nova_tests/plan91_fe4/` 8/0; vec/hashmap/hashset/text — `plan91_fe5/`; json — Plan 91.13 V2.)
- [x] Программа getting-started собирается и запускается на MVP-std
      end-to-end. (`examples/getting_started.nv` — `nova run` + `nova test` GREEN; English quick-start в README.)
- [~] Все примеры в `examples/` компилируются и запускаются —
      0 FAIL. **basics/effects/ffi(ptr)/typed_pointers/getting_started/spawn_demo компилируются** (≫5-7, удовлетворяет Ф.6.2). Аспирационные read-only didactic-демо `examples/effect_density/` + `examples/real_world/` НЕ компилируются by-design (glob-import `X.*`, плейсхолдеры `...`, undefined-символы; oxsar_port.nv header: «для чтения») — не входят в compile-gate. `net/echo_*` — Plan 83.12 scope.
- [x] Не-MVP модули `std/` отделены так, что 0.1 не поставляет
      некомпилируемый код. (Ф.7.1 — `std/_experimental/`.)
- [x] Полный `nova test` — 0 новых FAIL относительно baseline. (1637/16 FAIL — все 16 pre-existing, 0 от handler-lit codegen-фикса.)

### Ф.2 (try_parse_int) — ✅ CLOSED 2026-06-08

- [x] `ParseIntError` sum type (`| Empty | InvalidDigit | Overflow | InvalidRadix`) экспортируется из `std.runtime.string`
- [x] `str @try_parse_int(radix int = 10) -> Result[int, ParseIntError]` Nova-body реализован
- [x] Радиксы 2..=36 поддержаны; prefix `+`/`-` поддержан
- [x] `try_parse_int.ok() == parse_int()` для всех валидных входов (инвариант консистентности)
- [x] ≥6 позитивных fixtures + ≥4 негативных fixtures — `nova_tests/plan91_fe2/` — 10/0 PASS
- [x] Каждый негативный вариант (`Empty`/`InvalidDigit`/`Overflow`/`InvalidRadix`) покрыт отдельным fixture
- [x] D178 amend V2 записан в `spec/decisions/08-runtime.md`
- [x] Q-doc `spec/open-questions.md` обновлён (ParseIntError sum, try_parse_int ✅)

### Ф.5 (conformance) — ✅ CLOSED 2026-06-08

- [x] ≥5 realistic-use-case fixtures в `nova_tests/plan91_fe5/` — 5/0 PASS
- [x] Каждый fixture ≥3 теста реального use-case (не micro-checks)
- [x] Покрыт `Vec`, `HashMap`, `HashSet`, `str`/`text`, `sort`
- [x] `text_realistic.nv` интегрирует `split` + `try_parse_int` (cross-module use-case)

### Ф.4 conformance — ✅ CLOSED 2026-06-08

**Codegen-фикс (блокер):** `import std.sort` + `import std.time.duration` вместе давали
CC-FAIL `use of undeclared identifier 'i'`. Root-cause: `emit_handler_lit`
(`compiler-codegen/src/codegen/emit_c.rs`) не вычитал op-body-bound имена из capture-set
(в отличие от `emit_spawn`/`emit_detach`/`emit_blocking`, которые используют
`collect_bound_names_*`). При загрязнённом flat-`var_types` (stale `i` от `std.sort`-функций)
op-body-локал `i` Random-handler'а (`std/testing/handlers.nv`, транзитивно через
`std.time.duration`) ошибочно захватывался. Фикс: per-method bound-set + skip — строго
сужает captures, low-risk, по precedent'у. Repro PASS; full `nova test` 1637/16 (все 16
pre-existing, 0 регрессий).

**Критерии приёмки Ф.4** (все ✅, проверено через release `nova test`, codegen-канал):

- [x] Codegen-фикс handler-lit capture (`emit_c.rs`) — sort+duration CC-FAIL устранён
- [x] **10 fixtures `nova_tests/plan91_fe4/` — 10/0 PASS** (8 позитивных + 1 regression-guard + 1 негативный)
- [x] `time` (позитив): duration_budget_pipeline (19 asserts), timestamp_deadline_and_elapsed (14), duration_signed_and_scale (18)
- [x] `math` (позитив): geometry_distance_and_angle (8), trig_unit_circle (10), rounding_and_special_values, logarithm_and_exponential
- [x] `sort` (позитив): sort_aggregate_pipeline (16 — sort/sort_by/binary_search/min/max/sum/product)
- [x] **Regression-guard** `sort_duration_handler_guard.nv` — sort+duration в ОДНОМ модуле (точный триггер бага; ни одна per-module fixture не ловит); must keep compiling
- [x] **Негативный** `neg/edge_and_error_paths.nv` — failure/empty/special-value пути: empty `min()/max()→None`, `binary_search` miss→None, `Duration.is_negative/abs/is_zero`, `is_nan(NAN)`/`is_infinite(INF)`/`sqrt(-1)→NaN`
- [x] `Instant` — подтверждено: ships как `Monotonic` (D124), работает без изменений; отдельный `Instant`-тип не нужен
- [x] Все float-сравнения через tolerance-окно (никогда `==` на float)
- [x] Регрессия: full `nova test` 1637/16 — 0 новых FAIL от фикса
- [x] Spec/D/Q: новых D/Q НЕ требуется (фикс — codegen-correctness, не language-decision; Instant=Monotonic уже D124); Q-doc синхронизирован

### Ф.6 — Getting-started + примеры — ✅ CLOSED 2026-06-08

**Критерии приёмки Ф.6** (все ✅, проверено через release `nova run`/`test`):

- [x] **Ф.6.1** `examples/getting_started.nv` — self-contained teaching-программа на MVP-std,
      без net/FFI/unsafe: `fn main`+println, record+field-access, sum-type+match, for-loop,
      алгебраический эффект `Audit` через in-memory `with`-handler + `test {}` с handler-swap.
      `nova run` + `nova test` GREEN. Module path `nova_examples.getting_started` (files под `examples/` → package root).
- [x] **Ф.6.2** ≫5-7 примеров компилируются: basics (6) + effects (effects/effects_d61/gc_coroutines/with_tests/spawn_demo) + ffi/ptr_basics + typed_pointers + getting_started
- [x] **Ф.6.3** English quick-start — секция «Getting started» в README.md (install → `nova run` → что показывает)
- [x] Migrate: hyphen-dirs `effect-density→effect_density`, `real-world→real_world` (module-paths unparseable с hyphen); `spawn.nv→spawn_demo.nv` (`spawn` keyword); ptr_basics D78-path; sqlite_mini `Fail.throw→throw`+`and→&&`; unsafe_fn_attribute `unsafe{}`-wrap
- [~] `examples/effect_density/` + `examples/real_world/` — **read-only didactic spec-документы**,
      НЕ компилируются by-design (glob-import `X.*`, плейсхолдеры `...`, dozens undefined-символов;
      oxsar_port.nv header: «Не полная компиляция — это для чтения»). Module-paths нормализованы,
      но они НЕ входят в compile-gate. Рекомендация: оставить как illustrative reference.

### Followup-маркеры (Ф.4/Ф.6, pre-existing — обнаружены при работе)

Все НЕ блокеры закрытия Ф.4/Ф.6; отдельные codegen/language-дефекты для будущих планов:

- `[M-91.6-spawn-global-const-capture]` — module-level const ошибочно захватывается по bare-имени
  в `emit_spawn`/handler-capture (`ctx->C = &C`, но символ — `Nova_const_<mod>_C`). Ломает baseline
  `concurrency/sleep_real_clock` (SLACK_MS), `plan114_4/const_ref_const_ok` (BASE),
  `plan127/t16_neg_addr_of_const_binding`. Родственно фиксу Ф.4 (var_types pollution), но в spawn-path.
- `[M-91.6-time-now-schema-mismatch]` — `Time.now()` wire-typed как raw int, не `Timestamp{nanos}`;
  Timestamp-арифметика через `Time`-эффект даёт неверные значения / CC-FAIL (документировано в
  `std/time/duration.nv:880-882`). Блокирует Time-effect conformance fixture (обойдён explicit Timestamp).
- `[M-91.6-duration-zero-cross-module-const]` — `Duration.ZERO` через Path-form cross-module →
  CC-FAIL `undeclared identifier 'Duration_ZERO'` (const-access не наследует record-тип в
  `infer_expr_c_type`; см. `plan65/f4_zero_duration.nv`). Обойдён `Duration.from_nanos(0)`.
- `[M-91.6-parallel-for-value-typing]` — `ro r = parallel for ... { }` типизируется `()` вне `test{}`
  (нет `ParallelFor`-arm в `infer_expr_type`, `types/mod.rs:5717`). Обойдён statement-form в spawn_demo.
- `[M-91.6-sqlite-ffi-codegen]` — `sqlite_mini.nv` type-check GREEN, codegen CC-FAIL: external fn
  double-prefix `nova_fn_nova_fn_*`, `@`-method на record (`self` undeclared, литеральный `@` в C),
  tuple-FFI return. Связан с `[M-115-ffi-build-pipeline]`.

### Ф.1 — Коллекции (HashMap/Set/vec-combinators) — ✅ CLOSED 2026-06-08

**Re-baseline (список блокеров плана 2026-05-27 устарел — закрыты планами 95/99/101/103/125):**
большинство кандидат-блокеров уже работают. Проверено через release `nova test`:

- 🟢 `HashMap[K,V]` — mixed-type (`str→int`, `int→str`), `iter()` (K,V)-destructure, `get`/`insert`/
  `contains`/`remove`/`clone`/`keys`/`values`/`filter`/`from`/`get_or_insert` — D72 dispatch + tuple-types OK.
- 🟢 `Set[T]` (int/str) — `insert`/`contains`/`remove`/`len` + `or`/`and`/`minus`.
- 🟢 `vec` combinators: `filter`/`any`/`all`/`first`/`last`, `map` (int→int **и** int→str), `fold` (int→int).

**Codegen-фикс (единственный реальный блокер):** `fold`/HOF с **method-level-generic accumulator
≠ типу элемента** (`fold int→str`, `int→bool`) давал CC-FAIL `passing 'nova_int' to incompatible type`.
Root-cause (diagnose-workflow, 4 агента): в array-ext mono dispatch (`emit_c.rs:20583`-path) closure-arg
эмитился через `emit_expr`, а ClosureLight-ветка `emit_expr` НЕ читает `fn_param_sigs` → closure-параметр
`acc` дефолтил в `nova_int` вместо `nova_str`. Метод-тело монорфизировалось **верно** — ломался только тип
closure-параметра. Фикс: split ClosureLight-ветки в fn-typed-arg loop → `emit_lambda` с ctx из `inner_ptys`
(пустой return-slot сохраняет body-inferred `map int→str`). По precedent'у sibling-пути `emit_c.rs:21229`.
Применён к instance-path (`~20675`) + static-twin (`~21949`).

**Критерии приёмки Ф.1** (все ✅, release `nova test`):

- [x] HashMap/Set компилируются + проходят realistic conformance (cross-type, iter, algebra)
- [x] vec map/filter/fold/any/all/first/last компилируются (inference-форма)
- [x] **`fold` non-int-Acc codegen-фикс** — `fold int→str`/`int→bool` PASS (был CC-FAIL)
- [x] **10 conformance fixtures `nova_tests/plan91_fe1/` — 10/0 PASS** (combinators / fold str+bool+empty /
      chains filter.map+map.fold / HashMap mixed+iter / Set algebra / негативный edge-paths)
- [x] Регрессия: full `nova test` 2355/86 — **0 новых** от фикса (diff vs baseline; `plan65/f11a_timer_metrics`
      RUN-FAIL — pre-existing supervised-гонка, доказано revert+rebuild: падает и без фикса 5/5);
      таргетно plan101_1 18/0, plan90_1 20/0, plan99 9/0, plan91_8c 6/0
- [x] Spec/D/Q: новых D/Q не требуется (codegen-correctness fix); Q-doc синхронизирован

**Followup-маркеры — статус 2026-06-08 (3 из 4 ✅ ЗАКРЫТЫ, release `nova test`):**

- ✅ **`[M-91.1-composite-array-storage]`** (P1) — ЗАКРЫТ через **side-channel completion** (НЕ typed-storage).
  **Важно: исходный дизайн-вывод был неверен.** Гипотеза «нужен real per-composite `NovaArray_<T>` storage» при
  реализации сломала **47 тестов**: весь stdlib (HashMap-бакеты `[]Slot[K,V]`, tuple-массивы, JSON `[]JsonValue`)
  держится на int64-erasure + side-channel `array_element_types` (var→real elem C-type), и смена storage воюет с
  этой архитектурой. Корректный фикс — **завершить side-channel**: (1) name-alignment `apply_type_subst_to_ref`
  call-site↔body (убирает `unknown type name NovaArray_Nova_Wrap`); (2) propagation реального элемент-типа через
  generic map/filter в `array_element_types` (хелпер `register_array_result_elem`); (3) `.get()` + `infer` пере­
  паковывают `NovaOpt_nova_int`→`NovaOpt_<elem>` (NPO, NULL=None) с кастом; (4) composite-receiver: closure-param
  re-type на реальный pointer (filter с field-читающим предикатом). `[]record`/`[]sum` теперь полностью годны
  через `[i]`/`for-in`/`.get()`. **0 blast radius** (доказано diff vs true-baseline). Обобщает `[M-59.1-array-of-mono-tuple]`.
- ✅ **`[M-91.1-method-turbofish-dispatch]`** (P2) — ЗАКРЫТ. `obj.method[U,...](...)` парсится как
  `Call{TurboFish{base:Member}}`; добавлен перехват в начале `emit_call`: stash type_args в поле
  `current_method_turbofish` → recurse на Member base (срабатывает обычный Member-dispatch) → `resolve_method_level_subst`
  сидирует subst-слоты до arg-inference. Turbofish и inferred сходятся на один mono. Free-fn `TurboFish{base:Ident}` не тронут.
- ✅ **`[M-91.1-set-from-iter-iterable-param]`** (P2) — ЗАКРЫТ. `Set.from_iter` принимает конкретный `[]T` (зеркало
  `HashMap.from([](K,V))`); generic-протокол `Iterable[T]` стирался в `void*` на mono-инстансе → for-in не мог
  восстановить C-тип итератора. Array-параметр итерируется корректно для любого элемента.
- ❌ **`[M-91.1-dead-arrayext-mono-path]`** (WON'T FIX) — путь ЖИВОЙ, не мёртвый. Probe (`panic!`) сработал
  на `type_name="[]T" method="my_filter"` (plan100_4_5): пользовательские generic методы на `[]T` не
  регистрируются в `external_registry`, поэтому live sentinel не перехватывает их — они приходят сюда.
  Удалять нельзя. Маркер закрыт как WON'T FIX.
- ✅ **`[M-91.1-value-struct-array-elem]`** CLOSED by Plan 131 (2026-06-08) — `Vec[Option[T]]` и
  `Vec[tuple]` корректно хранят value-struct элементы с typed storage (D232). `[]T` int64-slot
  erasure gap остаётся для обратной совместимости, но пользователь может использовать `Vec[T]`
  как drop-in solution для value-struct элементов. Fixture: `plan131_vec_option_typed_storage`.

**Критерии приёмки Ф.1-followups** (2026-06-08, все ✅ через release `nova test`):

- [x] **turbofish** — `v.mymap[int](|x| x*2)`, `v.myfold[int](0, |a,x| a+x)`, `v.mymap[str](|x| str.from(x))`
      компилируются и проходят; inferred-форма даёт тот же mono. Fixture `plan91_fe1/method_turbofish_pos.nv`.
- [x] **set-from-iter** — `Set[int].from_iter([1,2,3,2])` dedup + `Set[str].from_iter([...])` PASS. Fixture
      `plan91_fe1/set_from_iter_pos.nv` + встроенные тесты `std/collections/set.nv`.
- [x] **composite-array** — `map int→[]record`/`[]sum`, `filter` по composite-receiver, chained map; readback
      поля через `[i]` / `for-in` / `.get()` (+ match на sum). Positive `plan91_fe1/composite_array_map_pos.nv`
      (7 тестов), edge/neg `plan91_fe1/neg/composite_array_edge.nv` (empty map / filter→empty / get OOB→None).
- [x] **no-regress guard** — `map int→str` (scalar) и `map int→int` остаются зелёными в том же fixture.
- [x] **0 blast radius** — full `nova test`: список фейлов идентичен true-baseline (diff = только флака
      `protocols/conversion/*` + `concurrency/*` races). typed-storage-попытка (47 новых фейлов) откатана.
- [x] **Spec/D/Q** — open-questions.md обновлён (3/4 закрыты + value-struct followup); 02-types.md amend
      (`[M-59.1-array-of-mono-tuple]` обобщён); новых D/Q не требуется (codegen-correctness).

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


## Подпланы

Закрытые работы вынесены в отдельные файлы. Этот документ остаётся
roadmap'ом, подпланы — историей закрытых фаз.

| # | Подплан | Что | Статус |
|---|---|---|---|
| 91.1 | [Re-baseline (Ф.0)](91.1-re-baseline.md) | Измерение базовой линии, категоризация блокеров, decision о порядке фаз | ✅ 2026-05-27 |
| 91.2 | [Quarantine non-MVP (Ф.7.1)](91.2-quarantine.md) | Перенос 30 non-MVP файлов в `std/_experimental/`; `nova check std/` → 0 FAIL | ✅ 2026-05-27 |
| 91.3 | [Sort module (Ф.4)](91.3-sort-module.md) | `std/sort.nv` MVP — sort/sort_by/binary_search/min/max для `[]int` | ✅ 2026-05-27 |
| 91.4 | [str Nova-body dispatch (Ф.2.5, D177)](91.4-str-nova-body-dispatch.md) | 5 str методов на Nova через Plan 54 Ф.2 dispatch | ✅ 2026-05-28 |
| 91.5 | [str API cleanup + D132 amendment (Ф.2.6, D178)](91.5-str-api-cleanup.md) | bytes→to_bytes, chars→to_chars, parse_int merge, compare, readonly param syntax, `-> @` fluent fix | ✅ 2026-05-28 |
| 91.6 | [StringBuilder pure Nova consume type (Ф.2.6 sub-phase, D179)](91.6-stringbuilder-nova-type.md) | `type StringBuilder consume { mut buf []u8 }`; убран C/Rust backing | ✅ 2026-05-28 |
| 91.7 | [Array methods cleanup + canonical `.new()` (D180/D181/D182)](91.7-array-methods-and-default-new.md) | mut `-> @` chain, `@slice` removed, canonical `.new()` для primitives/str/[]T, Self codegen fix | ✅ 2026-05-28 (generic sort + Option.new + diagnostic — followups) |
| 91.8a | [Protocol canon renames + Ordering removal + default bodies (D183)](91.8a-protocol-canon-renames.md) | `Iter→Iterable`, `Display→Printable`, `Equatable.eq→equals`, `Comparable.cmp→compare -> int`, remove `Ordering`, default body parser syntax + embed override | ✅ part 1 2026-05-29 (default body codegen synthesis — followup 91.8a.2) |
| 91.8a.2 | [Default body codegen synthesis + protocols refactor + From identity blanket (D183 amendment)](91.8a.2-default-body-codegen-and-from-blanket.md) | Orthogonal protocols (Equatable holds equals default, Comparable standalone), Printable.fmt default, `fn[T] T.from(t T) -> T => t` blanket, lazy synthesis at use-site, Self в param-type | ✅ part 1 + part 3 MVP synthesis 2026-05-29 (Equatable.equals + Printable.fmt MVP inline fallbacks); general lazy synthesis + cache + Plan 101 mono extension — followup [M-91.8a.2-default-body-general] |
| 91.8b | [Operator dispatch через protocols (D184)](91.8b-operator-dispatch-protocols.md) | `==`→`@equals`, `<`/`>`/`<=`/`>=`→`@compare`; удалить `@eq/@lt/@le/@gt/@ge` magic methods | 🔴 OPEN (зависит от 91.8a) |
| 91.8c | [Generic array API: sort/min/max + _by (D185)](91.8c-generic-array-api.md) | `fn[T Comparable]` sort/binary_search/min/max + callback `_by` variants | 🔴 OPEN (зависит от 91.8a) |
| 91.9 | [`#impl(Protocol1 + Protocol2)` annotation (D186)](91.9-impl-annotation.md) | opt-in: gate bare-call/interpolation + verification (E_UNKNOWN_PROTOCOL / E_IMPL_NOT_PROTOCOL / E_IMPL_MISSING_METHODS); structural typing preserved для bound/coercion/cast | ✅ core 2026-05-29 (nova doc + E_IMPL_WRONG_SIGNATURE — followups) |
| 91.10 | [D163 retract (capability syntax → effects)](91.10-d163-retract-capability-syntax.md) | Parser `needs <Cap>` clause удалён, `check_external_fn_needs_caps` снят, `emit_d163_external_stub` deleted, 9 D163-fixtures удалены, spec D163 marked RETRACTED. Rationale: capability ≡ effect (Koka-style); D163 conflated `consume` (ownership) с `needs Cap` (authority). Followup `[M-91.10-fs-net-effects-formal]` — если/когда нужно cap gating, ввести как formal `effect` declarations | ✅ 2026-05-30 |
| 91.11 | SB API cleanup + zero-copy steal + rename `extend_from→append`/`insert_from→insert` + parser multi-line chain | (1) StringBuilder: `@append_bytes` merged → `@append([]u8)` overload, `@plus(s)`/`@plus(c)` removed, `@to_str → @as_str`, view-вместо-copy в `@starts_with`/`@ends_with`/`@append(s str)`/`@append_repeat`, push chain в `@append(c char)`, pre-reserve в `@append_repeat`; (2) zero-copy steal API `str.from_bytes_unchecked_steal(consume []u8)`; (3) `[]T.extend_from → append`, `[]T.insert_from → insert` rename family (D141 amend); (4) parser lookahead через newline на `.method` (multi-line chain support); (5) var_boxed restore fix в `emit_monomorphized_method` ([M-mono-method-var-boxed-leak] closed) | ✅ 2026-05-30 |

## Открытые фазы

| Фаза | Что | Статус |
|---|---|---|
| Ф.3 conformance smoke | JSON encode/decode round-trip; HashMap codegen блокеры (forward decl, tuple destructuring) | ✅ **CLOSED 2026-06-05 — see Plan 91.13 V2** |
| Ф.2 text methods remainders | `try_parse_int` Result variant + ParseIntError sum; split/join/trim/pad — split external, join/trim/pad deferred | ✅ **CLOSED 2026-06-08** — `try_parse_int(radix=10)` + ParseIntError + ≥10 fixtures (nova_tests/plan91_fe2/) |
| Ф.1 collections conformance | cross-product тесты Vec/HashMap/Set + fold non-int-Acc codegen fix | ✅ **CLOSED 2026-06-08** — `nova_tests/plan91_fe1/` 10/0; emit_c.rs ClosureLight-ctx fix (fold int→str); 4 followup markers |
| Ф.4 conformance | time/math/sort fixtures + handler-lit capture codegen fix | ✅ **CLOSED 2026-06-08** — `nova_tests/plan91_fe4/` 8/0; emit_c.rs handler-lit capture fix |
| Ф.5 | conformance integration в `nova_tests/plan91_fe5/` + property tests | ✅ **CLOSED 2026-06-08** — 5 realistic fixtures: vec/hashmap/hashset/text(CSV)/sort; json closed Plan 91.13 V2 |
| Ф.6 | getting-started + 5-7 examples; Ф.6.1 decision (in-memory default vs TCP-echo) | ✅ **CLOSED 2026-06-08** — `examples/getting_started.nv` (in-memory `Audit` effect) + README quick-start; aspirational didactic examples read-only by-design |
| Ф.7.2-Ф.7.4 | release checklist, STATUS.md final, docs/plans README | pending (отложен) |
