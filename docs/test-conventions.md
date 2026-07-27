# Test conventions

> **Нормативный документ** — изменения и отклонения только по согласованию с владельцем; см. [conventions-governance.md](conventions-governance.md).

Практический guide для авторов и пользователей тестов Nova.
Нормативная спецификация D89 EXPECT-маркеров —
[spec/decisions/09-tooling.md](../spec/decisions/09-tooling.md#d89).
Test-runner — [Plan 24](plans/24-cross-platform-test-runner.md) +
[Plan 26](plans/26-test-runner-hardening.md).

---

## Тест или bench?

| Ситуация | Инструмент |
|---|---|
| Проверить корректность — семантика, типы, эффекты, баги | `nova test` (этот документ) |
| Измерить производительность — throughput, latency, регрессия по скорости | `nova bench` → [bench-conventions.md](bench-conventions.md) |
| Не уверен | `nova test`; bench только если план явно требует замеров |

---

## ТРЕБОВАНИЕ: регресс должен быть быстрым (компиляция И выполнение)

Дефолтный `nova test` / CI — это **регресс**, и он **обязан** быть быстрым **как по
компиляции, так и по выполнению**. Это критерий качества тест-сьюта, не пожелание.
Тест, заметно замедляющий компайл или прогон, **не должен** быть в дефолтном пути.

### Большие тесты: хранить в репо, но НЕ в дефолт-регрессе

Большие тесты (полные conformance-наборы, объёмные individual-тесты) **нужны и
хранятся в репозитории** — они ловят краевые баги, которые малый сэмпл пропускает.
Но они **не запускаются** в дефолтном `nova test` / CI.

- **В регресс идёт только малый репрезентативный сэмпл** (uniform-spread, напр.
  **1500** для conformance-фикстур) — быстро компилируется И выполняется.
- **Полный/большой набор** лежит в репо отдельно (см. ниже), помечен **opt-in**,
  прогоняется **вручную / out-of-band при приёмке** (доказательство G0 «без
  упрощений» — этим прогоном, а не дефолт-регрессом).
- НЕ выкидывать большие тесты; НЕ класть их в обычный discovery-путь, который сканит
  `nova test`.

**Прецедент (Unicode conformance, Plan 152.4):** коммит-фикстуры — стайд-сэмпл 1500;
полнота проверена out-of-band (normalization 19965, graphemes 1093, words 1826,
sentences 512, collation 227800). Размер коммит-фикстуры регулируется
`nova-codegen unicode --emit-conformance --conformance-limit <N>`.

### Развилка: коммитить большой набор или регенерить? (для авторов/агентов)

Любой большой/медленный тест помечается суффиксом **`_slow.nv`** (default `nova test`
его пропускает; прогон `--include-slow`/`--slow-only`; нормировано [D376](../spec/decisions/09-tooling.md#d376-test-discovery-skiproute-конвенции--fixtures-os-суффикс-_slownv)).
А вот **хранить полный набор в git или нет** — зависит от того, регенерируем ли он:

- **Регенерируемый** детерминированным генератором (напр. Unicode conformance из UCD):
  полный `*_conformance_slow.nv` **НЕ коммитить** — он `gitignored` (`nova_tests/**/*_conformance_slow.nv`)
  и **регенерируется on-demand** (`nova-codegen unicode --emit-conformance --conformance-full
  --ucd-dir <UCD>` → `nova test --slow-only`; пустой кэш → 0 тестов = skip-never-fail). В git —
  только малый fast-сэмпл `*_conformance.nv`. Причина: коммит регенерируемого build-output зря
  раздувает историю навсегда (модель Go `-long`/CPython; обоснование —
  [docs/research/10-unicode-test-data-storage.md](research/10-unicode-test-data-storage.md)).
- **Нерегенерируемый** (ручной большой/медленный тест, генератора нет): **коммитить** как
  `*_slow.nv` — это и есть «хранить в репо, но вне дефолт-регресса».
- ❌ git-lfs / отдельная тест-репа / submodule — НЕ используем (хуже на exFAT/Windows,
  непрецедентно для текстовых фикстур; см. research/10).

> **Механизм** (lane для больших тестов вне дефолт-прогона) — **РЕАЛИЗОВАН**
> ([Plan 156](plans/156-test-runner-slow-lane.md), `[M-test-runner-large-test-lane]`;
> нормирован [D376](../spec/decisions/09-tooling.md#d376-test-discovery-skiproute-конвенции--fixtures-os-суффикс-_slownv)).
> Конвенция (rev-2 suffix-only) — **per-file суффикс `_slow.nv`** (зеркало семейства
> `_windows.nv`/`_test`; skip на этапе discovery в `walk_nv` → файл-корпус **не
> читается**, нулевой per-file I/O). **Дефолтный `nova test`
> такие файлы пропускает.** Прогон — через флаги:
>
> - `--include-slow` — обычные тесты **И** `*_slow.nv` (merge-gate / nightly);
> - `--slow-only` — **только** `*_slow.nv` (выделенная CI-job, доказательство G0).
>
> Суффикс комбинируется с прочими (`foo_conformance_slow.nv`, `bar_windows_slow.nv`
> гейтится и по OS, и по slow — `_slow` снимается ДО OS-проверки).
>
> **Хранение полных наборов (rev-3):** `*_conformance_slow.nv` **НЕ коммитятся** —
> **регенерируются on-demand** из pinned UCD в gitignored-кэш (`nova-codegen unicode
> --emit-conformance --conformance-full --ucd-dir <UCD>`), затем `nova test --slow-only`.
> Коммитится только fast-сэмпл `*_conformance.nv`. Если кэш пуст — `--slow-only` находит 0
> тестов (skip-never-fail). Модель Go/CPython; обоснование —
> [docs/research/10-unicode-test-data-storage.md](research/10-unicode-test-data-storage.md).
> Отложен (`[M-156-slow-subtree-dir]`) лишь каталог-вариант `slow/` + сентинел `_slow.toml`
> для медленных folder-module — добавится аддитивно, когда появится первый такой тест.

### Когда переносить тест в `_slow`

Критерии — **единая точка правды**: [D298](../spec/decisions/09-tooling.md#d298--test-suite-time-budget).

Кратко: intentional sleep/stress/bench → `_slow.nv`; медленный только из-за compile time → оставить. Агенты используют алгоритм: `elapsed ≥ 60 с` ИЛИ имя содержит `stress`/`bench`/`perf` → `_slow.nv`.

Если тест медленен только из-за большого N — создай **fast-variant** (малый N) + оригинал → `_slow.nv`.

---

## Методология написания тестов

### spec/D-conformance suite (`spec_tests/`) — ЧИСТЫЙ soundness-сигнал · согласовано 2026-06-28

> **`nova_tests/` — наполовину сломанный/дублирующий корпус → НЕ чистый гейт корректности** (годен
> лишь для baseline-DELTA против чистого бинаря, [feedback-nova-tests-not-correctness-gate]). **Источник
> истины о звучности — `spec_tests/`: новые ПРАВИЛЬНЫЕ тесты, покрывающие D-блоки + спеку, по одному
> файлу на D-блок. Цель — заменить `nova_tests/` (его в будущем удаляем как устаревший/дублирующий).**

- **На КАЖДЫЙ затронутый правкой D-блок** (новое наблюдаемое поведение / правило конверсии /
  диагностика — §5 spec-first, §8 pos+neg) — spec-покрывающий тест в `spec_tests/`, проверяющий
  ровно норму D-блока. Растёт как durable spec-conformance-набор.
- **`spec_tests/` — ОТДЕЛЬНЫЙ пакет** (свой `nova.toml`, workspace-member), параллельный `nova_tests/`,
  чтобы корпус можно было удалить независимо. Прогон: `nova test spec_tests`. Через релизный `nova`
  (C-codegen pipeline), не интерпретатор.
- **Размещение — ДВА яруса (директива владельца 2026-06-28; уточнена 2026-07-06 дважды —
  действует ЭТА редакция):**
  1. **`spec_tests/conformance/` — ЯЗЫК + ПРЕЛЮДИЯ, и НИЧЕГО кроме** (в `spec_tests/` НЕТ других
     каталогов). Один folder-module = ОДИН compile unit = один std-parse → быстрый регресс. Сюда
     идут ТОЛЬКО тесты **семантики самого языка**: D-блоки синтаксиса, типов, эффектов,
     консьюм-модели, дженериков, паттернов, FFI-ABI, прелюдии (`Vec`/`str`/`Option`/`Result`/
     числовые ширины и т.п.). Пример язык-D: d54 (as-cast), d85 (`?`-return), d102 (named args),
     d282/d353 (FFI), d325 (Result-everywhere), d347 (rebinding). Тест, который импортит std-тип
     лишь чтобы прогнать **языковую** норму (map-литерал D108, size-accessors D117, consume-guards
     D174, lazy-iter D260, duration-overflow D317) — остаётся здесь. Негативы — в
     `conformance/neg/` (каждый standalone-CU, `module neg.<имя>`).
  2. **Тесты std-модуля — РЯДОМ С МОДУЛЕМ**, пир-файлами `std/src/<модуль>/<имя>_test.nv` (прецедент:
     `std/src/runtime/sync_test.nv`; Plan 195 — std на `src/`, module-path не меняется). Правила:
     - **Позитив** — пир-файл `<имя>_test.nv` с **Тем ЖЕ `module`-декларатором, что у модуля**
       (`module std.fs`, `module encoding.compress`, …), БЕЗ импорта собственного модуля
       (same-module видимость). Суффикс `_test` вырезается из обычной (library) сборки
       (`walk_nv`/`resolve_imports` peel `_test`); в test-режиме пиры включаются, и folder-module
       компилируется **ОДНИМ CU** (модуль + все его `_test`-пиры) — раннер репортит один entry.
       Cross-module импорты в тесте (напр. `std.io.{ErrorKind}` из fs-теста) — обычные.
     - **Негатив (`EXPECT_COMPILE_ERROR`) — ТОЛЬКО в подпапке `std/src/<модуль>/neg/`**, каждый файл —
       standalone-CU со своим `module neg.<имя>` (как в `conformance/neg/`). Пир-файлом класть
       НЕЛЬЗЯ: тип folder-module-entry определяется по алфавитно-первому файлу, а битый пир
       ломает компиляцию ВСЕГО модульного CU. Подпапка `neg/` — отдельные CU, модуль не трогают.
     - Прогон: `nova test std` (или таргетно `nova test std/src/<модуль>`); негативы подхватываются
       тем же обходом (`--compile-error`).
     - **Известный гейт (2026-07-06):** whole-module test-CU `std/src/http` пока НЕ компилируется —
       cross-module коллизия имён sum-типов (`ErrorKind` есть и в `std.http`, и в
       `encoding.compress`; name-keyed `sum_schema_registry` берёт не ту схему → P67-LEGACY panic
       в `emit_match`). Семейство [M-172.1-var-types-cu-name-leak]. До фикса позитивный
       http-тест (d358) временно живёт в `nova_tests/http` (library-mode import не триггерит баг);
       миграция — Plan 182.
  3. **В `nova_tests/` НОВЫЕ тесты НЕ пишутся** (корпус заморожен; судьба — Plan 182 санация).

- **Линт-гейт CI — жёсткий deny (Plan 185 Ф.3-хвост / Plan 212 №6, включён 2026-07-18):**
  workflow `nova-lint.yml` гоняет `nova lint --deny` (W→E: любая находка = exit≠0 самим CLI)
  на std и spec_tests. Предусловие включения выполнено: lint-0 подтверждён CI три прогона
  подряд + локально 1439 файлов / 0 находок при включённых стилевых линтах. Следствие для
  авторов: новый конвенционный дрейф в std/spec_tests НЕ пройдёт CI; осмысленные исключения —
  только `// nova:allow W_КОД -- причина` (D428, причина обязательна). Ослаблять гейт обратно
  в warn — только решением владельца.
- **Авторитетный merge-гейт = conformance + флагман-examples-build (директива владельца 2026-07-16):**
  `spec_tests/conformance` покрывает ТОЛЬКО семантику языка+прелюдии → **app-регрессии он НЕ ловит**.
  Прецедент: Plan 206 trap-default (D423) прошёл conformance 470/0, но уронил
  `examples/flagship/aggregator/src/scenarios.nv` (`splitmix64` на голом `*` → рантайм `integer overflow: *`) —
  баг **протёк в main**, потому что авторитетный гейт собирал только conformance, не examples. **Впредь
  авторитетный пре-merge-гейт (интегратор/оркестратор) для ЛЮБОГО behavior-changing слияния ОБЯЗАН, помимо
  зелёного conformance, собрать флагман-examples** (минимум `examples/flagship/aggregator` + примеры,
  затронутые правкой) под `--strict-effects` (конвенция examples-build 2026-07-13). **Красный examples-build =
  стоп, как красный conformance.** Точечно: не весь examples-corpus, а флагман + релевантные — conformance
  ловит звучность языка, examples-build ловит регрессии прикладного кода (арифметика/эффекты/линковка), которые
  корпус не видит.
  **Под конкретный D — ОТДЕЛЬНЫЙ файл** `d<NNN>_<кратко>.nv` (в conformance — `module
  spec_tests.conformance`; рядом с модулем — `d<NNN>_<кратко>_test.nv` с module-декларатором
  модуля); общие типы — в `types_<domain>.nv` пир-файле, объявлены ОДИН раз (folder = один модуль
  из co-equal файлов).
  **Имена типов domain-prefixed** (один namespace на весь CU → избегаем коллизий между D-файлами).
  **Префиксуй и функции, и ЛОКАЛЬНЫЕ переменные** (`d263_buf`, не `buf`) · согласовано 2026-07-02:
  резолвер пока держит один name-keyed namespace на CU (`var_types` last-wins между пир-файлами) —
  одноимённый локал соседнего D-файла молча перебивает тип твоего, тест падает загадочным
  type-mismatch. Правило нормативно, пока локалы не заскоупятся per-fn
  ([M-172.1-var-types-cu-name-leak]).
  Пример: `spec_tests/conformance/types_value_record.nv` (`Point`) + `d328_value_record_eq.nv`
  (**D328** value-record `==` структурное).
- **Только ПРОХОДЯЩИЕ тесты** коммитим в `spec_tests/`: тест на gated-поведение (правка ещё не
  залендж) добавляется ВМЕСТЕ со своей правкой, не раньше (иначе suite не «всё-зелёный»).
- **Workflow добавления D-теста (НЕ ломать зелёный suite) · согласовано 2026-06-28:** новый D-тест
  сначала разрабатывается в **ОТДЕЛЬНОМ (isolated) модуле** (не в общем `spec_tests.conformance`) →
  доводится до PASS → **ТОЛЬКО passing-тест мержится** в общий пир-модуль. «Довести до PASS» = ЛИБО
  тест mis-written (невалидный Nova / не по D) → чинится ТЕСТ; **ЛИБО тест КОРРЕКТЕН по D, а компилятор
  падает → это компилятор-GAP → чинится КОМПИЛЯТОР / реализуется D по конвенции** (это и есть
  driver-роль suite: D-тест гонит реализацию D). Причина isolation: `spec_tests.conformance` —
  folder-module = ОДИН CU, поэтому один битый WIP-файл ломает ВЕСЬ модуль + даёт cascade-ошибки (ломает
  и уже-зелёные тесты). А зелёный `spec_tests.conformance` — надёжный регресс, нужный при работе над
  базой 172. Развитие suite (наполнение всеми D спеки) идёт **ПАРАЛЛЕЛЬНО базе 172**.
- **Зачем:** детерминированный pos+neg-гейт, независимый от флака-корпуса; ловит регрессию именно в
  нормативной зоне правки (на практике поймал blast-radius, который sample-валидация пропускала); слой
  за слоем заменяет `nova_tests/`.

### Где писать тесты для stdlib

Nova поддерживает два равноправных места для тестов stdlib — как Rust, Zig, D
(ред. 2026-07-06: `nova_tests/` ЗАМОРОЖЕН для новых тестов — контракт-тесты модуля теперь
пир-файлами `*_test.nv` рядом с модулем, см. §spec/D-conformance suite п.2):

| Место | Что тестирует | Как запускать |
|---|---|---|
| `std/**/*.nv` (inline `test`-блоки) | Внутренние инварианты модуля, приватные детали реализации | `nova test std` (только std) |
| `std/src/<модуль>/<имя>_test.nv` (пир-файл) | Публичный контракт модуля (pos); негативы — `std/src/<модуль>/neg/` | `nova test std` / `nova test std/src/<модуль>` |
| `nova_tests/<тема>/` | ЗАМОРОЖЕН (legacy-корпус, Plan 182) | `nova test` |

`nova test std` и `nova test nova_tests std` — **не одно и то же**: первый запускает только `std/` как tests_dir; второй запускает `nova_tests/` + `std/` вместе (multi-path, Plan 36.D.1). Для проверки inline std-тестов в изоляции используй `nova test std`.

**Inline тесты в std** — предпочтительный способ для unit-тестов самого модуля. `test "..."` блоки живут рядом с реализацией, не дрейфуют, могут тестировать приватные детали. Module path файла не меняется (`module collections.hashmap`). Пример: `std/src/collections/hashmap.nv`, `std/src/time/duration.nv` (Plan 195 — std на `src/`).

```nova
// std/src/collections/hashmap.nv
module collections.hashmap

// ... реализация ...

test "insert and get" {
    mut m = HashMap[int, int].new()
    m.insert(1, 42)
    assert(m.get(1) == Some(42))
}
```

Запуск: `nova test nova_tests std` — stdlib-файлы проходят через тот же pipeline что и `nova_tests/` (folder_module, slow-lane, EXPECT-маркеры).

**Тесты в `nova_tests/`** — для интеграционных сценариев, проверки публичного API снаружи, тестов взаимодействия нескольких модулей:

```
nova_tests/
├── str/          ← публичный API str снаружи (char_at, split, conversions…)
├── plan91/       ← stdlib API (sort, text methods…)
├── plan103_*/    ← sync-примитивы (Mutex, Channel…)
└── plan91_12/    ← net (TcpListener, UdpSocket…)
```

**Правило выбора:** если тест проверяет *реализацию* модуля — inline в `std/`. Если проверяет *использование* — в `nova_tests/`.

---

### Структура директории

`nova_tests/` организована по **планам и смысловым группам**:

```
nova_tests/
├── basics/            ← folder-module: один compile unit
├── generics/          ← folder-module
├── plan108_1/         ← folder-module
│   └── neg/           ← standalone EXPECT_COMPILE_ERROR
├── plan115/           ← пакет с nova.toml (особый случай, не folder-module)
├── plan56/            ← standalone файлы (конфликты имён / slow-файлы)
│   └── neg/           ← standalone EXPECT_COMPILE_ERROR
└── ...
```

Для вновь создаваемых тестов в `nova_tests/` — по умолчанию **folder-module** (см. ниже).

---

### Как писать положительный тест

> **ТРЕБОВАНИЕ (приоритет, не пожелание): МАКСИМИЗИРУЙ folder-module — МИНИМИЗИРУЙ число
> compile-unit'ов.** Каждый отдельный `module` = отдельный вызов codegen + clang = прямой налог
> на скорость регресса (§«ТРЕБОВАНИЕ: регресс должен быть быстрым»). Поэтому:
> - **По умолчанию новый позитивный тест — ПИР-ФАЙЛ в СУЩЕСТВУЮЩЕМ folder-module** (тот же
>   `module nova_tests.<тема>`), а НЕ новый standalone-модуль и НЕ новая под-директория.
> - **НЕ плоди per-задача / per-фича standalone-модули** (`module u52.pos`, `module d227.pos`, …) —
>   это анти-паттерн: N задач → N лишних CU. Деталь задачи кладётся в **ОПИСАТЕЛЬНОЕ ИМЯ ФАЙЛА**
>   формата `<ссылка>_<что_тестирует>` (`plan103_2_atomic_i64.nv`, `u52_narrowing_category_pos.nv`)
>   и/или в имя `test "…"`-блока, НЕ в отдельный модуль. Имя должно говорить, ЧТО тестируется —
>   не только код-ссылка: `d227_pos.nv` плохо («что за d227?»), `d227_numeric_literal_range_pos.nv`
>   хорошо (ссылка на D-блок + суть).
> - **Конфликты имён при слиянии** решаются `priv(file)` / префиксом файла (см. ниже), а НЕ
>   выделением в отдельный модуль.
> - Standalone-модуль оправдан ТОЛЬКО когда folder-module технически невозможен (см.
>   «Когда folder-module невозможен»: `nova.toml`-пакет, `_slow.nv` с другим module, `main`/
>   legacy-`EXPECT_RUNTIME_PANIC` (новые runtime-panic — `panics`-клаузулой ПИР-файлом, D348),
>   неразрешимый конфликт). Во всех прочих случаях — пир-файл.
>
> Перед добавлением теста спроси: «есть ли уже folder-module этой темы, куда это ляжет пир-файлом?»
> Если да — добавляй туда. Создание нового модуля/папки требует обоснования из списка исключений.

**Стандартный случай — folder-module**

Все положительные тесты в одной директории объявляют **одинаковый** `module`:

```nova
// nova_tests/plan_foo/feature_a.nv
module nova_tests.plan_foo

test "basic case" {
    assert(1 + 1 == 2)
}

test "another case" {
    // ...
}
```

```nova
// nova_tests/plan_foo/feature_b.nv
module nova_tests.plan_foo          // ← тот же module, что в feature_a.nv

fn helper() -> int { 42 }

test "uses helper" {
    assert(helper() == 42)
}
```

Правила folder-module:
- Все `.nv`-файлы в директории объявляют **ровно одинаковый** `module nova_tests.<dir>`.
- Имена функций и типов внутри директории **не дублируются** — они делят одно пространство имён.
- Тест-runner запускает все файлы директории как **один compile unit** (один вызов codegen + один clang).
- Рекурсия не поддерживается: поддиректории — отдельные folder-module или пакеты.

**Module path**: `module nova_tests.<dirname>`, где `<dirname>` — имя директории (без родительских путей).

Исключение: директории с `nova.toml` (`[package] name = "X"`) используют `module X.<filename>` согласно D78.

---

### Когда folder-module невозможен

Folder-module не применяется если:
1. В директории есть конфликты имён (одна и та же `fn foo` / `type Foo` в двух файлах) — **и** ты не хочешь их разрешать (см. ниже).
2. Директория содержит `_slow.nv`-файлы с другим module-путём.
3. Директория — именованный пакет с `nova.toml`.

В этом случае файлы остаются **standalone**: каждый объявляет свой уникальный `module <dir>.<filename>`.

При конфликтах имён есть выходы (в порядке предпочтения):
- **`priv(file)`** (Plan 170, предпочтительно): пометить конфликтующие top-level `fn`/`const`/тип-без-методов как `priv(file) fn helper()` → file-private, ноль rename, имена читаемы. **Ограничение:** `priv(file)` типы С методами НЕ дискриминируются по файлу (коллизия метод-символа `Nova_<T>_static_<m>`) → для них ordinal-rename.
- **Ordinal-suffix rename**: `Counter` в 3 файлах → `Counter1`/`Counter2`/`Counter3` (алфавитный порядок по имени файла). Массовый рефактор — `python scripts/catb_convert.py <dir>`.
- **Уникальный prefix** (для нового кода): `feature_a_helper()` вместо `helper()`.
- **Оставить standalone** (если переименование ломает смысл теста или dir заблокирована `nova.toml`).

---

### Консолидация по темам (Plan 169.1.2)

Сокращение CU: тесты разных планов одной **ТЕМЫ** (atomics, sync, syntax, …)
сливаются в один folder-module `module nova_tests.<тема>`, а не по номеру плана.

- **Связь тест↔план — через имя файла:** `plan103_2_atomic_i64.nv` в `nova_tests/atomics/`.
  Имена файлов произвольны (на `module` не влияют); происхождение видно по префиксу,
  раннер печатает путь при падении → навигация цела.
- **Module:** все позитивы темы → `module nova_tests.<тема>`; neg → `neg/` (`module neg.<stem>`).
- **Коллизии между планами:** `priv(file)` / rename (см. выше). Между темами часто их нет.
- **EXPECT_TIMEOUT / EXPECT_EXIT (и legacy EXPECT_RUNTIME_PANIC) — НЕ сливаются** в
  folder-module (маркер относится к целому TU; в общем бинаре они бы повесили/уронили
  остальные). Остаются standalone. **Runtime-panic тесты сливаются в folder-module через
  `panics`-клаузулу** (`test "имя" panics "паттерн" { … }` — Plan 173 Ф.6 /
  [D348](../spec/decisions/09-tooling.md#d348--panics-клаузула-тест-блока-инверсия-passfail-для-runtime-panic-тестов-plan-173-ф6), РЕАЛИЗОВАНО);
  timeout — всегда standalone. · согласовано (sign-off Ф.6)
- **Валидация — передавать папки напрямую:** `nova test nova_tests/<тема>` (можно
  несколько папок одной командой: `nova test nova_tests/atomics nova_tests/sync`).
  НЕ `--filter <тема>` — он матчит по подстроке и цепляет лишнее (напр. `--filter sync`
  тянет `std/src/runtime/sync*`). Path-invocation работает при ПРАВИЛЬНОЙ форме модуля
  `module nova_tests.<тема>`; при неверной форме — `E_D78_MODULE_PATH_MISMATCH`.
- **Починка fallout:** приводя старые тесты к актуальному компилятору, чинить новые
  ошибки enforcement'а (напр. `E_LOCAL_NOT_MUT` → добавить `mut` переприсваиваемым
  переменным), **НЕ выхолащивая** тест.

---

### Как писать отрицательный тест (EXPECT_COMPILE_ERROR)

Отрицательные тесты **не объединяются** в folder-module — каждый файл это один compile unit, потому что `EXPECT_COMPILE_ERROR` относится к целому TU. Они живут в **`neg/`-поддиректории**:

```
nova_tests/plan_foo/
├── feature_a.nv        ← positive (module nova_tests.plan_foo)
├── feature_b.nv        ← positive (module nova_tests.plan_foo)
└── neg/
    ├── bad_arg.nv      ← EXPECT_COMPILE_ERROR (module neg.bad_arg)
    └── type_err.nv     ← EXPECT_COMPILE_ERROR (module neg.type_err)
```

Module path в `neg/`: **`module neg.<filename_без_расширения>`** (D29: два сегмента — имя директории + имя файла).

```nova
// nova_tests/plan_foo/neg/bad_arg.nv
// EXPECT_COMPILE_ERROR E_TYPE_MISMATCH

module neg.bad_arg

test "wrong type" {
    ro x int = "not an int"
}
```

Контейнер для провоцирующего кода — **любой** (`test "..."` / `fn` / top-level decl): он **не исполняется**, т.к. файл обязан не компилироваться (runner проверяет ошибку на этапе codegen — `NEG-NO-ERROR`, если компиляция прошла, независимо от наличия `test`-блока). Для читаемости предпочтителен `fn`/top-level (не подразумевает проходящий тест), но `test`-блок **допустим**. Один EXPECT-маркер на файл.

> **Как это видит раннер** (`test_runner.rs`, чтобы конвенция не расходилась с инструментом):
> классификация теста — по **маркеру** `EXPECT_*` (`detect_test_type`), НЕ по суффиксу `_neg` или
> имени папки `neg/`. Группировка файлов в один TU — по **равенству `module`-деклараций**
> (`is_folder_module_dir`): neg с `module neg.<name>` ≠ позитивам → отдельный TU автоматически.
> Поэтому neg **обязан** декларировать отличный `module neg.<name>` и жить в `neg/` — иначе
> деградирует folder-module соседей-позитивов. Суффикс `_neg` и имя `neg/` — сигнал для
> людей/агентов, не для раннера.

---

### Именование файлов

| Тип | Соглашение | Пример |
|---|---|---|
| Позитивный тест | `<feature_or_scenario>.nv` | `option_map.nv`, `closure_capture.nv` |
| Позитивный тест (план) | `<phase_or_feature>.nv` | `f1_basic_case.nv`, `t2_edge_case.nv` |
| Отрицательный (compile error) | внутри `neg/`: `<что_проверяем>.nv`; вне `neg/`: `<что_проверяем>_neg.nv` | `type_mismatch.nv` (в `neg/`), `mut_conflict_neg.nv` (вне) |
| Runtime-panic тест (канон D348) | peer-файл folder-module с `test "…" panics "паттерн"`; имя `<scenario>.nv` | `div_zero.nv` c `panics "division by zero"` |
| Runtime panic (legacy standalone) / exit тест | `<scenario>_panic.nv` / `<scenario>_exit.nv` | `div_zero_panic.nv` — только когда нужна изоляция процесса (D348) · согласовано |
| Медленный тест | `<name>_slow.nv` | `stress_gc_slow.nv`, `cancel_stress_slow.nv` |
| Fast-variant медленного | `<name>.nv` (тот же module, меньший N) | `stress_gc.nv` рядом с `stress_gc_slow.nv` |

Суффикс `_neg`: **необязателен внутри `neg/`** (путь `neg/` + `module neg.<name>` уже сигналят «негатив», и раннер классифицирует по маркеру `EXPECT_*`, а не по имени файла); **обязателен для neg-файлов ВНЕ `neg/`** (standalone / консолидация — там иного сигнала нет). Когда применяется — `_neg` явно сигнализирует агентам и читателям, что файл ожидает ошибку компиляции.

---

### Что проверять (полнота теста)

**Минимально необходимое в тесте:**

1. **Happy path** — базовый сценарий работает.
2. **Edge cases** — граничные значения (0, пустой, максимум, пустая строка и т.д.).
3. **Взаимодействие** — если фича зависит от другой (напр. `Option.map` + `Option.filter`), проверить цепочку.

**Не нужно в unit-тесте:**

- Нагрузочные / stress сценарии — выносить в `_slow.nv`.
- Проверка одного и того же на 20 вариациях — достаточно 3-5 репрезентативных.
- Воспроизведение всего conformance-набора стандарта — выносить в `*_conformance.nv` с лимитом.

**Один тест-блок — одно утверждение или один связный сценарий:**

```nova
// ХОРОШО: каждый test "..." проверяет одну вещь
test "map on Some" {
    assert(Some(2).map(fn(x) { x * 3 }) == Some(6))
}

test "map on None" {
    assert(None.map(fn(x int) -> int { x * 3 }) == None)
}

// ПЛОХО: один тест-блок проверяет несколько независимых фич — при падении неясно что именно
test "option works" {
    assert(Some(2).map(...) == Some(6))
    assert(None.filter(...) == None)
    assert(Some(5).ok_or(...) != None)
}
```

---

### Нормы плана 231 (выход из bug-цикла) · согласовано 2026-07-26 (вопрос владельца «нужно улучшить конвенцию по тестированию?»)

Пять правил, выведенных из фактического профиля багов 221.x (семьи per-position,
контекстозависимость CU, энфорс-гэпы — см. docs/plans/231-bug-cycle-exit.md §0):

1. **Регресс-фикстура на каждый закрытый `[M-…]` — в том же слиянии.** Именная
   (conformance или рядом с модулем), реестр-запись 221.1 ссылается на файл.
   Закрытие «гейт прошёл, фикстуры нет» — не закрытие (урок аудита №92/№97).
2. **Матрица позиций для конструкций-«семей».** Фикстура новой конструкции
   типов/значений обязана крыть канон-набор синтаксических позиций: let-биндинг /
   match-arm / параметр / возврат / поле record-литерала / элемент коллекции /
   generic-arg. Один happy-path не считается покрытием (урок: fn-newtype —
   7 разрывов одной конструкции по позициям, №53→78→90→96→97→104→116).
3. **Дифференциальный контекст CU.** Конструкции, зависящие от накопленного
   состояния компилятора (consume/@cleanup, linearity, mono, перегрузки), —
   фикстура в ДВУХ контекстах: standalone И folder-CU; расхождение вердиктов =
   P1-дефект (класс №122/№99). До появления авто-дифф-раннера (231-Б.2) —
   вручную обе формы.
4. **Инвариант «check ⇒ build».** `nova check` зелёный, а `build` даёт CC-FAIL —
   это P1-дефект чекера (тихая дыра, класс №114б), НЕ «известное поведение»;
   на каждую найденную — фикстура + запись в реестр.
5. **Neg-фикстура на каждый энфорс.** Новый `E_*`/`W_*`-код не вливается без
   neg-фикстуры, ловящей ТОЧНЫЙ код (сверка — таблица 231-А).

---

### Пиннинг-тест для silent-wrong-value багов · согласовано 2026-07-10

Когда чинишь баг, дающий **НЕВЕРНОЕ ЗНАЧЕНИЕ** (не падение, не compile-error, а тихо
неправильный результат) — регресс-тест **обязан УПАСТЬ под старым (баговым) кодом**, а не
просто быть зелёным сейчас. Косвенная проверка (round-trip, сериализация→десериализация→
сравнение) часто **не ловит** такой баг: неверное значение на промежуточном шаге может не
менять конечный результат round-trip'а.

**Мотивирующий пример (реальный, 2026-07-10):** `needs_quoting`/`is_ascii_ident_char` были
**always-true** — ведущий `||` в начале строки-продолжения многострочного `||`-выражения
распарсился как **zero-arg closure-литерал** (`|| ...`), который **отбросился как
discarded statement** (D417-класс), и функция без него падала на дефолтный `true`.
Round-trip-тесты (`parse(print(x)) == x`) этого не ловили: always-true просто добавлял
кавычки лишний раз, а round-trip оставался идентичным.

**ПЛОХО** (проверяет только round-trip — не ловит always-true):

```nova
test "needs_quoting basic" {
    ro v = Foo{ field: "bare" }
    assert(parse(print(v)) == v)
}
```

**КАНОН** (пиннинг: явная проверка конкретного значения в конкретной точке):

```nova
test "needs_quoting: голое поле БЕЗ кавычек" {
    assert(needs_quoting("bare_ident") == false)   // под always-true упал бы
}

test "needs_quoting: мусорный символ отвергается" {
    assert(needs_quoting("bad!char") == true)
}
```

**Правило:** тест на фикс silent-wrong-value бага проверяет **саму функцию/значение
напрямую** в точке, где баг проявляется, а не только сквозной эффект. Перед коммитом теста
мысленно (или явно) прогони его **против старой (баговой) версии** — если он бы прошёл и
тогда, это не пиннинг, а декорация; нужен более прямой assert. Дополняет
[«Что проверять (полнота теста)»](#что-проверять-полнота-теста) выше — «проверка ловит
ИМЕННО этот баг», а не только happy path/edge cases.

---

### Конфликты имён в folder-module

При добавлении нового файла в существующий folder-module проверь, что имена не конфликтуют с соседними файлами:

```bash
grep -h "^fn \|^type \|^const " nova_tests/plan_foo/*.nv | sort | uniq -d
```

**Для нового кода** — префиксируй именем файла:

```nova
// feature_a.nv
fn feature_a_helper() -> int { ... }    // не просто "helper"

// feature_b.nv
fn feature_b_helper() -> int { ... }    // не конфликтует
```

**Для рефактора существующей dir с конфликтами** — ordinal-suffix rename через скрипт:

```sh
# dry-run (показывает что изменится):
python scripts/catb_convert.py --dry-run nova_tests/plan_foo

# применить:
python scripts/catb_convert.py nova_tests/plan_foo
```

Скрипт переименовывает `Counter` → `Counter1`/`Counter2`/... (по алфавиту файла), переносит EXPECT_COMPILE_ERROR → `neg/`, обновляет все ссылки внутри файлов. Не трогает stdlib-типы (Vec, Option, Result, str, int и др.).

---

### Runtime-panic тесты: `panics`-клаузула (канон) и legacy EXPECT_RUNTIME_PANIC · согласовано (Plan 173 Ф.6, D348)

**Канон для новых runtime-panic тестов** — `panics`-клаузула тест-блока
([D348](../spec/decisions/09-tooling.md#d348--panics-клаузула-тест-блока-инверсия-passfail-для-runtime-panic-тестов-plan-173-ф6)):
peer-файл обычного folder-module, никакого standalone CU:

```nova
module nova_tests.plan_foo

test "oob panics" panics "index out of bounds" {
    ro xs = [1, 2, 3]
    ro _ = xs[10]
}
```

PASS ⇔ тело запаниковало (PANIC-класс D13: `panic()`/assert/overflow/OOB/contract)
сообщением ⊇ паттерн (substring, D89-семантика). `throw`/`exit` инверсию НЕ активируют.
Раннер сбрасывает runtime-состояние между panics-тестами (`nova_runtime_reset`) — N паник
в одном CU безопасны.

**Legacy standalone (`EXPECT_RUNTIME_PANIC` + `fn main()`)** — только когда нужна
изоляция процесса; такие файлы не могут быть частью folder-module (main-точка входа
одна на TU). Кладутся рядом с `neg/` или в `rt/`:

```
nova_tests/plan_foo/
├── feature_a.nv             ← folder-module positive (сюда же panics-тесты)
├── neg/
│   └── type_err_neg.nv      ← EXPECT_COMPILE_ERROR
└── rt/
    └── hard_abort.nv        ← legacy EXPECT_RUNTIME_PANIC (standalone, module rt.hard_abort)
```

Законные поводы остаться в legacy (из миграции Plan 173 Ф.6):
- **процессная смерть**, не ловимая test-frame'ом: fiber stack overflow (SEH),
  abort из detach-fiber, проверка uncaught-abort stderr (`(throw site)`-трасса);
- **file-режимные директивы**, действующие на весь CU: `// CONTRACTS off`,
  module-level `#unchecked(...)`;
- **throw-класс**: «паника» на деле = unhandled `throw` (USER) — abort процесса, но НЕ
  PANIC-класс (D348-инверсия его сознательно не принимает);
- смешанные маркеры (`EXPECT_STDOUT`+`EXPECT_RUNTIME_PANIC`) — процессные потоки.

**Легаси-standalone в `std/**` — суффикс `_trap_test.nv` (не просто `_traps.nv`)
· [M-trap-tests-silent-skip-default-lane] 2026-07-21:** для модулей stdlib
легаси runtime-panic файлы кладутся, как и в `nova_tests/`, в под-каталог
`rt/` рядом с модулем (`std/src/<модуль>/rt/`, при уровне вложенности —
`std/src/<модуль>/<под-модуль>/rt/`), каждый файл — **свой standalone CU**
(`module rt.<имя_файла_без_.nv>`, D78: сегмент = имя файла). Имя файла —
**`<scenario>_trap_test.nv`** (не `<scenario>_traps.nv` без `_test` —
голое множественное число ничем не сигналит «это тест-файл» ни человеку,
ни tooling'у на глаз). `_test`-суффикс здесь не «съедается» — для
standalone-файла (не peer'а folder-module) module-декларатор равен
**полному** имени файла (`module rt.dur_add_overflow_trap_test`, не
`module rt.dur_add_overflow`): D78 rev-3 не отрезает `_test` от target для
standalone-случая (отрезание `_test` — только discovery-механика folder-
module peer'ов, `resolve_imports`/`walk_nv`, см. §«Где писать тесты для
stdlib» выше).

Прецедент: `std/src/time/rt/dur_f64_nan_trap_test.nv`,
`std/src/time/rt/dur_div_zero_trap_test.nv`,
`std/src/time/rt/dur_add_overflow_trap_test.nv`,
`std/src/time/civil/rt/date_period_overflow_trap_test.nv`.

**Лейн `EXPECT_RUNTIME_PANIC`/`EXPECT_COMPILE_ERROR`/`EXPECT_TIMEOUT`/
`EXPECT_EXIT` — всегда `--full`:** дефолтный `nova test` (`TestSelection`
default = `{Positive}` без `--include-slow`) эти файлы **не запускает** —
это by-design (регресс должен быть быстрым), НО раннер обязан показать их
как явный `SKIP <path> # <lane> lane — requires --full` (или
`--include-slow`/`--slow-only` для `_slow.nv`) в общем `SKIP:`-счётчике, а
не молчать. Директория, где ЕДИНСТВЕННЫЙ контент — trap-тесты (напр.
`std/src/time/rt/`), под дефолтным `nova test <dir>` отчитывается видимыми
SKIP-строками, а НЕ голым «PASS: 0 FAIL: 0» (неотличимым от пустой/опечатанной
директории — реальный баг, найденный и закрытый этой правкой). Полный прогон
трапов — `nova test --full <dir>`.

---

### Полный checklist для агента при написании тестов

1. **Определи категорию**: позитивный / compile-error / runtime-panic (канон — `panics`-клаузула peer-файлом, D348) / stdout/stderr.
2. **Выбери директорию — СНАЧАЛА ищи существующий folder-module темы** (приоритет: минимум CU). Новая папка/модуль — только если folder-module невозможен (исключения выше).
3. **Добавляй ПИР-ФАЙЛОМ в существующий folder-module** (тот же `module nova_tests.<тема>`); НЕ создавай standalone-модуль на задачу. Конфликт имён → `priv(file)`/префикс, НЕ новый модуль. Имя файла — **описательное** `<ссылка>_<что_тестирует>.nv` (не только код-ссылка).
4. **Негативные → `neg/`**: EXPECT_COMPILE_ERROR → `neg/<name>.nv`, `module neg.<name>` (суффикс `_neg` обязателен только ВНЕ `neg/`; контейнер `test`/`fn` — любой, не исполняется).
5. **Медленные → `_slow.nv`**: по бюджету [D298](../spec/decisions/09-tooling.md#d298-test-suite-time-budget) (единственная точка правды; локальный порог «run > 2s» ретирован · согласовано); создай fast-variant.
6. **Проверь полноту**: happy path + edge cases + взаимодействие фич.
7. **Проверь детерминизм**: `assert` проверяет гарантированный контракт, не эвристику планировщика.
8. **Запусти**: `nova test nova_tests/<dir>/` — все PASS перед коммитом.

---

## Как запускать тесты

### Quick start

```sh
# build nova CLI (one-time, or after changes to compiler)
cd nova-cli && cargo build && cd ..

# run all tests
nova-cli/target/debug/nova test
```

Логика runner'а (детект toolchain'а, EXPECT-маркеры, parallel scheduler,
per-test timeout, JSON output) живёт в Rust в
[compiler-codegen/src/test_runner.rs](../compiler-codegen/src/test_runner.rs).

### Параметры

| Флаг | Что |
|---|---|
| `--filter <substr>` | Прогнать только тесты содержащие substring |
| `[PATH]...` | Один или несколько путей к директориям с тестами (multi-path, Plan 36.D.1). Без аргументов — `nova_tests/`. Пример: `nova test nova_tests std` |
| `--mode dev\|release` | dev (default) или release с `-O3 -flto` |
| `--toolchain auto\|clang\|msvc\|gcc` | Compiler. Default: auto (Clang → MSVC → GCC) |
| `--timeout <secs>` | Per-test timeout. Default 60 |
| `--jobs <N>` | Parallel workers. 0 = num_cpus |
| `--format text\|json\|tap\|junit` | Output format. Default text |
| `--verbose` / `--quiet` | Verbosity |
| `--results-file <path>` | Куда сохранить per-test JSON |
| `--rerun-failed` | Прогнать только тесты которые fail/timeout в results-file |
| `--retries <N>` | Retry transient (AV-race) fails. CI default 2 |
| `--keep-artifacts` | Не удалять .exe/.obj после прогона |

### Примеры

**Дефолтный прогон** (всё параллельно через Clang):
```sh
nova test
```

**Только подмножество** (TDD-loop):
```sh
nova test --filter syntax/closure
nova test --filter "negative_capability/"
```

**Release-сборка** (с оптимизациями для perf-проверки):
```sh
nova test --mode release
```

**JSON output для custom CI parser'ов**:
```sh
nova test --format json --results-file ci-results.jsonl
```

**JUnit XML для CI** (GitHub Actions / GitLab CI / Jenkins / Azure DevOps):
```sh
nova test --format junit --retries 2 > test-results.xml
```
Стандартный JUnit XML schema — нативно парсится всеми mainstream CI:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nova_tests" tests="143" failures="0" time="91.082">
  <testsuite tests="143" failures="0" time="91.082" timestamp="2026-05-11T12:42:47">
    <testcase classname="basics" name="literals" time="0.234"/>
    <testcase classname="syntax" name="bad_test" time="0.514">
      <failure type="expectation" message="expected exit 42, got 0"/>
    </testcase>
  </testsuite>
</testsuites>
```
Каждая строка — событие:
```json
{"event":"finished","test":"basics/literals","status":"pass","stage":"","elapsed_ms":234,"detail":""}
{"event":"summary","pass":140,"fail":1,"elapsed_ms":45678}
```

**TAP-13 output** (для legacy harnesses):
```sh
nova test --format tap | tee results.tap
```

**TDD: перезапустить только упавшие**:
```sh
nova test                     # первый прогон — результаты пишутся в target/last-test-results.json
nova test --rerun-failed       # только бывшие fail-ы; намного быстрее
```

Явный путь к results-file:
```sh
nova test --results-file target/last-test-results.json
# правишь код...
nova test --results-file target/last-test-results.json --rerun-failed
```

**Sequential** (для отладки race conditions):
```sh
nova test --jobs 1
```

**Долгие benchmark-тесты** (override default 60s timeout):
```sh
nova test --timeout 300 --filter concurrency/sleep_leak
```

**Принудительный MSVC** (если хотите тестить под MSVC ABI):
```sh
nova test --toolchain msvc
```

### Запуск одного теста

Для отладки удобно вызывать `nova-codegen test-build` напрямую — он
собирает + запускает один `.nv` файл без overhead'а walkdir/parallel:

```powershell
.\compiler-codegen\target\debug\nova-codegen.exe test-build .\nova_tests\basics\literals.nv `
    --toolchain clang --timeout 30 --keep-artifacts
```

`--keep-artifacts` оставляет `.exe`/`.obj` в `$TEMP/nova_tests/t-<hash>/`
для пост-mortem отладки. Без флага артефакты удаляются после прогона.

### Toolchain setup

**Windows:**
- **Clang (recommended):** `winget install LLVM.LLVM`
- **MSVC fallback:** установить Visual Studio Build Tools (нужен и
  для Clang — даёт MSVC SDK headers + linker).

**Linux:**
- `apt install clang` или `dnf install clang` (Ubuntu/Fedora).
- GCC обычно уже установлен.

**macOS:**
- Clang идёт с Xcode CLI tools: `xcode-select --install`.

Env-override paths:
- `NOVA_CLANG` — путь к `clang.exe`/`clang`.
- `NOVA_GCC` — путь к `gcc`.
- `NOVA_VCVARS` — путь к `vcvars64.bat` (Windows).
- `NOVA_CODEGEN` — путь к `nova-codegen.exe` (обычно target/debug).
- `NOVA_MARCH_NATIVE=1` — `-march=native` вместо `-march=x86-64-v3`
  для release-сборки (не переносится между CPU).

### Известные limitations на Windows

При `--jobs > 1` под активным **Windows Defender** возможны
transient `lld-link: cannot open output file` ошибки — AV держит
handle на свежесгенерированном `.exe` пока соседний worker пытается
linkать. Workarounds:
- **`--retries 2`** (Plan 26 Ф.12) — transient AV/race fails автоматически
  ретраятся. Real test fails не ретраятся (только classifier по
  error-сигнатурам). Это recommended setting для CI.
- `--jobs 1 --timeout 60` — sequential (стабильно, но медленнее).
- **AV exclusion** для `target/`, `$TEMP/nova_tests/` — снимает
  bottleneck полностью.
- В CI без Defender'а (Linux runners) parallel работает корректно.

### Graceful cancellation

`Ctrl+C` во время прогона: worker'ы graceful exit на следующем тесте
(не забирают новые jobs из queue). Уже запущенные child-процессы
получат KILL по `--timeout`. Summary показывает что было выполнено
до cancel'а.

См. [Plan 26 retro](plans/26-test-runner-hardening.md) для деталей.

---

## Зачем маркеры

Обычные тесты в `nova_tests/` пишутся через `test "name" { ... }` —
test-runner запускает блок и проверяет что не упал. Это покрывает
**positive paths** — программа работает как ожидается.

Но иногда нужно проверить **что-то должно упасть** — определённым
способом, с конкретным сообщением, exit-кодом или выводом. Для этого
есть `EXPECT_*` маркеры.

Маркер — обычный комментарий в первых 30 строках `.nv`-файла. Test-
runner его читает, **переворачивает** обычную логику pass/fail.

---

## Стандартные маркеры (D89) и `panics`-клаузула (D348) · согласовано

> Нормативный список маркеров — [D89](../spec/decisions/09-tooling.md#d89-test-tooling-конвенции--expect_-маркеры-для-negative-тестов)
> (+ расширения D304: `EXPECT_TIMEOUT`/`EXPECT_TIMEOUT_MS`, lint-эксперименты).
> Прежние заголовки «4/5 стандартных» разъехались с фактом — счёт больше не
> нормируется здесь; ориентируйся на D89/D348. Runtime-panic канон — НЕ маркер,
> а `panics`-клаузула (D348, см. секцию выше).

### 1. `EXPECT_COMPILE_ERROR <pattern>`

Проверяет, что **codegen упадёт** с error, чьё сообщение содержит
`<pattern>` (substring match, case-sensitive).

**Когда использовать:**
- Type-check errors (duplicate definition, type mismatch).
- Codegen errors (ambiguous overload, no matching overload).
- Capability violations (forbid + call с запрещённым эффектом).

**Пример:**

```nova
// EXPECT_COMPILE_ERROR duplicate definition

module nova_tests.negative_capability.overload_dup

fn process(n int) -> str { "first" }
fn process(n int) -> str { "second" }    // duplicate sig
```

**Поведение runner'а:**
- Codegen вызван, exit code должен быть **ненулевой**.
- Stdout/stderr codegen'а должны содержать `duplicate definition`.
- Файл **не компилируется** в exe (предполагается невалидный код).

**Если codegen прошёл успешно** — `NEG-NO-ERROR` (test fails).
**Если упал, но без pattern в сообщении** — `NEG-WRONG-MSG`.

---

### 2. `EXPECT_RUNTIME_PANIC <pattern>` — LEGACY (D348) · согласовано

Проверяет, что exe **скомпилируется и запустится**, но **упадёт с
panic**, чьё сообщение содержит `<pattern>`.

**⚠ Legacy (Plan 173 Ф.6 / D348):** для НОВЫХ runtime-panic тестов канон —
`panics`-клаузула peer-файлом folder-module (см. секцию «Runtime-panic тесты»).
Маркер остаётся для кейсов с обязательной изоляцией процесса (процессная смерть,
file-режимы, throw-класс) + селектор `--panic` (D304).

**Когда использовать (только legacy-кейсы выше; исторически):**
- Тесты `panic("...")` в коде.
- Runtime errors (out-of-bounds, division by zero, при condition).
- Assertion failures.

**Пример:**

```nova
// EXPECT_RUNTIME_PANIC explicit panic

module nova_tests.expected_runtime.panic_main

fn main() Io -> () {
    panic("explicit panic")
}
```

**Поведение runner'а:**
- Codegen + cl.exe компиляция должны пройти.
- Exe запускается, exit code должен быть **ненулевой**.
- Stdout/stderr должны содержать `explicit panic`.

**Если exe вернул exit code 0** — `NEG-NO-PANIC`.
**Если упал, но без pattern** — `NEG-WRONG-PANIC`.

---

### 3. `EXPECT_EXIT_CODE <N>`

Проверяет, что exe завершится с **конкретным** exit-кодом.

**Когда использовать:**
- Тесты `exit(N, "...")` функции.
- CLI-программы с конкретными exit-кодами для shell-скриптов.
- Различение нескольких error-вариантов через коды.

**Пример:**

```nova
// EXPECT_EXIT_CODE 42

module nova_tests.expected_runtime.exit_code_42

fn main() Io -> () {
    exit(42, "intentional exit with code 42")
}
```

**Поведение runner'а:**
- Codegen + компиляция проходят.
- Exe запускается, exit code должен быть **ровно `N`**.

**Если exit code другой** — `NEG-WRONG-EXIT` с указанием
ожидаемого и фактического.

---

### 4. `EXPECT_STDOUT <pattern>`

Проверяет, что **только stdout** (не stderr) exe содержит `<pattern>`
(substring).

**Когда использовать:**
- Golden-file тесты для format/print-логики.
- Проверка что program вывела ожидаемое сообщение в stdout.
- Smoke-тесты hello-world уровня.

**Пример:**

```nova
// EXPECT_STDOUT hello world

module nova_tests.expected_runtime.stdout_hello

fn main() Io -> () {
    println("hello world from Nova")
}
```

**Поведение runner'а:**
- Codegen + компиляция проходят.
- Exe запускается (любой exit code OK — тест на **вывод**, не на код).
- **Только stdout** должен содержать `hello world` (substring match).
  Если pattern в stderr — тест **не** проходит. Для проверки stderr —
  отдельный маркер `EXPECT_STDERR`.

**Если pattern не найден** — `NEG-WRONG-STDOUT`.

---

### 5. `EXPECT_STDERR <pattern>`

Проверяет, что **только stderr** (не stdout) exe содержит `<pattern>`
(substring).

**Когда использовать:**
- Проверка warning'ов / diagnostic-сообщений в stderr.
- Тесты `panic(msg)` без жёсткой привязки к exit-коду
  (`EXPECT_RUNTIME_PANIC` дополнительно требует ненулевой exit).
- Проверка `exit(N, msg)` сообщения (вместе с `EXPECT_EXIT_CODE`
  это сделать нельзя — один маркер на файл, поэтому используют один
  или другой).
- Проверка output `Logger`-handler'ов, пишущих в stderr.

**Пример:**

```nova
// EXPECT_STDERR custom stderr message

module nova_tests.expected_runtime.stderr_panic

fn main() Io -> () {
    panic("custom stderr message")
}
```

**Поведение runner'а:**
- Codegen + компиляция проходят.
- Exe запускается (любой exit code OK — тест на **вывод**, не на код).
- **Только stderr** должен содержать pattern. Если в stdout — тест
  не проходит.

**Если pattern не найден** — `NEG-WRONG-STDERR`.

**Отличие от `EXPECT_RUNTIME_PANIC`:**
- `EXPECT_RUNTIME_PANIC` требует **ненулевой exit code** + pattern
  в любом потоке (panic-сообщение).
- `EXPECT_STDERR` принимает **любой exit code**, но pattern должен
  быть **именно в stderr**.

Для panic-тестов обычно используют `EXPECT_RUNTIME_PANIC` (он
проверяет два инварианта). `EXPECT_STDERR` — когда нужна только
проверка вывода, без требования к exit code'у.

---

## Правила

### Один маркер на файл

Маркеры **взаимоисключающие**. Если хочешь проверить несколько
ошибок — **отдельные файлы**. Это сознательно: один файл = один
test scenario, проще читать и точнее диагностировать падения.

### Substring, не regex

Pattern — **substring**, искать в выводе как есть. Не regex, никаких
escape'ов.

```
// EXPECT_COMPILE_ERROR duplicate definition `process`
//                                            ^^^^^^^^^
//                          backticks буквальные, не экранируются
```

### Case-sensitive

`EXPECT_COMPILE_ERROR Foo` НЕ сматчит сообщение «foo not found».

### Один pattern на одну строку

Multi-line patterns не поддерживаются. Runner склеивает вывод в
одну строку через пробел перед matching.

---

## Куда класть тесты

| Тип теста | Куда |
|---|---|
| Позитивный `test "..."` | `nova_tests/<group>/<name>.nv` (folder-module, `module nova_tests.<group>`) |
| Runtime-panic `test "..." panics "pat"` (канон, D348) | peer-файл того же folder-module `nova_tests/<group>/<name>.nv` · согласовано |
| `EXPECT_COMPILE_ERROR` | `nova_tests/<group>/neg/<name>.nv` (`module neg.<name>`; суффикс `_neg` обязателен только вне `neg/`) |
| `EXPECT_RUNTIME_PANIC`, `fn main()` — **legacy** (только изоляция процесса, D348) | standalone в `nova_tests/<group>/` или `nova_tests/<group>/rt/` |
| `EXPECT_EXIT_CODE`, `EXPECT_STDOUT`, `EXPECT_STDERR` | standalone в `nova_tests/<group>/` |
| Медленный тест | `nova_tests/<group>/<name>_slow.nv` |

Конвенция folder-module (`module nova_tests.<group>`) применяется для всех позитивных тестов начиная с Plan 169.1 Ф.8 (Cat-A+Cat-B). Оставшиеся standalone-файлы — только dirs с `nova.toml` (named packages) или с pre-existing compile errors в positives.

---

## Расширения для своего проекта

Если в твоём проекте появился use-case для нового маркера (например
`EXPECT_LINT_WARNING`) — **сначала** проверь, не покроет ли существующий
один из стандартных (D89/D348). Если нужен новый — обсуди с авторами Nova
(возможно, маркер должен быть стандартизирован через D-block).

**Custom-маркеры** в одном проекте — допустимы, но **не используй
имена `EXPECT_*`** — они зарезервированы. Используй project-specific
префикс: `MYPROJ_EXPECT_*` или `INTERNAL_*`.

---

## Прецеденты в других языках

| Язык | Маркер | Похоже |
|---|---|---|
| **Rust** (`compiletest`) | `//~ ERROR pattern` | Nova `EXPECT_COMPILE_ERROR` |
| **Swift** (`utils/test`) | `// expected-error {{pattern}}` | то же |
| **Go** (`errorcheck`) | `// ERROR pattern` | то же |
| **TypeScript** | `// @ts-expect-error` | другой подход — атрибут языка |
| **LLVM lit** | `// CHECK: pattern` | универсальный для тестов tooling'а |

Nova ближе к Rust/Swift/Go — comment-маркер на уровне test-runner'а.

---

## CLI Category Selectors (Plan 169.1.1)

`nova test` supports additive category flags. Multiple flags = OR (union).

| Flag               | Selects                              |
|--------------------|--------------------------------------|
| (default, no flag) | positive ∩ fast                      |
| `--positive`      | tests without EXPECT_* marker        |
| `--compile-error` | EXPECT_COMPILE_ERROR tests           |
| `--panic`         | EXPECT_RUNTIME_PANIC tests           |
| `--timeout`       | EXPECT_TIMEOUT tests                 |
| `--exit`          | EXPECT_EXIT tests                    |
| `--slow`          | include *_slow.nv (any type)         |
| `--full`          | all types + slow                     |

Examples:
```
nova test nova_tests                        # positive-fast (default)
nova test --compile-error nova_tests        # compile-error only
nova test --panic --compile-error nova_tests # panic OR compile-error
nova test --full nova_tests                 # everything
```

Filter by marker, not by folder: compile-error and panic tests outside `neg/` are caught.

Backward-compat: `--include-slow` = `--slow`; `--slow-only` deprecated (use `--full` or combine flags).

---

## Ссылки

- [D89 в spec/decisions/09-tooling.md](../spec/decisions/09-tooling.md#d89) — нормативная спецификация.
- [nova-cli/src/main.rs](../nova-cli/src/main.rs) — `nova test` CLI entry point.
- [compiler-codegen/src/test_runner.rs](../compiler-codegen/src/test_runner.rs) — runner implementation.
- [nova_tests/negative_capability/](../nova_tests/negative_capability/) — примеры `EXPECT_COMPILE_ERROR`.
- [nova_tests/expected_runtime/](../nova_tests/expected_runtime/) — примеры остальных трёх маркеров.


---

## Fixture directories (Plan 55 Ф.8, 2026-05-16)

> Нормативная спецификация discovery-конвенций (skip-каталоги + per-file суффиксы) —
> [D376 в spec/decisions/09-tooling.md](../spec/decisions/09-tooling.md#d376-test-discovery-skiproute-конвенции--fixtures-os-суффикс-_slownv).

Не каждый `.nv` файл в `nova_tests/` — это runnable test. Иногда нужны
**input fixtures** для tooling (Plan 45 `nova doc` ingestion samples,
intermediate doc-pipeline data). Такие файлы:
- часто **не имеют** `main`/`test "..."` блоков;
- не должны компилироваться как standalone tests (CC-FAIL `undefined
  symbol: nova_fn_main_impl`);
- доступны через explicit `nova check <path>` или integration tests
  (cargo tests).

### Convention

`nova test` (test discovery walker) **исключает**:

1. **Любую директорию с именем `fixtures`** — стандартная конвенция
   (параллель с Rust `tests/data/`, Python `fixtures/`).
2. **Любую директорию с sentinel-файлом `_fixture.toml`** — explicit
   override для случаев когда имя `fixtures` нельзя/нежелательно.

```
nova_tests/
├── doc/
│   ├── f24_*_positive.nv         ← обычные tests, run'аются
│   └── fixtures/                 ← skipped (по имени)
│       ├── basic/sample.nv
│       └── ...
└── plan42/
    └── custom_data/              ← обычная папка, run'ается
        └── _fixture.toml         ← теперь skipped (через sentinel)
```

### Доступ к fixtures извне

- **Type-check:** `nova check nova_tests/doc/fixtures/basic/sample.nv`
  работает напрямую (path-based, не через discovery).
- **Plan 45 `nova doc`:** ingestion pipeline принимает explicit paths.
- **Integration tests:** cargo tests в `compiler-codegen/tests/`
  могут load fixtures как Rust string includes.

### Параллель с другими языками

| Язык | Convention |
|---|---|
| **Rust** | `tests/data/`, `tests/fixtures/` (cargo test игнорирует sub-dirs без `mod` декларации) |
| **Python** | `fixtures/` (pytest skips если нет `test_*.py` или `*_test.py`) |
| **Go** | `testdata/` (стандартный exclude в go test) |
| **JS/TS** | `__fixtures__/`, `fixtures/` |
| **Nova** | `fixtures/` ИЛИ `_fixture.toml` sentinel |

## Протухающие ожидания — тест не хардкодит снимок внешних данных (2026-07-27)

Родня флаки-политики ниже, но причина другая: тест не мигает, он **краснеет
навсегда** в момент, когда меняются ВНЕШНИЕ данные, о которых он судит. Такой
тест ловит не регрессию, а течение времени — и съедает время разбора ровно так же.

**Правило: ожидание вычисляется из источника, а не вписывается константой.**
Если тест судит о живом внешнем состоянии (теги соседней репы, содержимое
каталога, системное время, номер версии), он обязан ВЫЧИСЛИТЬ ожидаемое
значение из того же источника и проверить ПРАВИЛО, а не застывший результат.

Канонический случай (Plan 233-волна, `plan204_replace_e2e::resolve_real_nova_tls_v0_1_0_via_file_url`):
тест резолвил зависимость `{ git = <локальная nova-tls>, version = "0.1" }` и
ассертил `version == "0.1.0"` с комментарием «^0.1 resolves to tag v0.1.0». Это
было верно ровно до того дня, когда у `nova-tls` появились теги `v0.1.1`..`v0.1.3`:
резолвер отработал ПРАВИЛЬНО (взял наибольший совместимый — `0.1.3`), а тест
покраснел. Чинится не «обновлением константы» (протухнет снова на v0.1.4), а
переносом правила в сам тест: прочитать `git tag -l "v0.1.*"`, взять
максимальный patch, ожидать его. После правки тест проверяет БОЛЬШЕ прежнего —
именно semver-max-семантику, а не совпадение с одним тегом.

**Признак диагноза при разборе красноты:** упавший ассерт сравнивает с
литералом, который описывает НЕ поведение нашего кода, а факт о чужих данных
(«тогда старшим тегом был v0.1.0»). Если так — виноват тест, но чинится он
усилением, не ослаблением: константа заменяется вычислением, а не удаляется
вместе с проверкой.

## Флаки-тесты — политика (Plan 92, 2026-05-22)

**Флаки-тест** — тест, который проходит/падает недетерминированно при
неизменном коде. Это **производственная проблема, не косметика**: он
роняет «зелёный» прогон на ровном месте и **маскирует настоящие
регрессии** — при `1 FAIL` нельзя без ручного разбора отличить «флаки»
от реального бага. Молча мигающий тест эродирует доверие ко всему
прогону.

### Правило: тест проверяет только детерминированный контракт

`assert` в тесте должен проверять **гарантированное** свойство —
то, что обязано выполняться при любом валидном расписании потоков/
fiber'ов и любой нагрузке. Нельзя `assert`'ить наблюдаемое следствие
**эвристики планировщика**.

Канонический анти-паттерн (Plan 92 Ф.0 — `mn_runtime_actual_workload`):

```nova
// ПЛОХО: проверяет, что work-stealing РАСПРЕДЕЛИЛ 16 fibers по worker'ам.
//   Под CPU-starvation worker-потоков ОС один worker законно
//   выполняет всё → on_w0 == 16 → assert падает (66% под нагрузкой).
//   work-stealing — оппортунистичная эвристика, НЕ контракт.
assert(on_w0 < 16)
```

Что **детерминировано** и проверяемо у того же теста:

```nova
// ХОРОШО: все 16 fibers ОТРАБОТАЛИ на worker-пуле (worker_id >= 0,
//   а не -1 = main-thread). Это контракт M:N-рантайма — держится
//   при любой нагрузке.
assert(total_ran == 16)
```

### Вероятностные свойства — проверять статистически, не одним сэмплом

Если свойство по природе вероятностное (распределение work-stealing'а,
наличие параллелизма), оно проверяется **bounded-sampling**'ом, а не
одним наблюдением:

- прогнать свойство до `N` независимых раз;
- pass — если оно наблюдалось хотя бы раз (рабочая система даёт его
  с высокой вероятностью за попытку → `P(false-fail) = (1-p)^N`
  выбором `N` сводится к ничтожной);
- настоящая поломка (свойство недостижимо) → не наблюдается **ни
  разу** за `N` попыток → детерминированный fail.

Это корректный метод проверки вероятностного свойства (ср. Go
`runtime.TestGoroutineParallelism`), **не упрощение**: реальный баг
остаётся loud-детектируемым.

### Чего НЕ делать

- **Не** «лечить» флаки молчаливым `retry`. Retry **маскирует** —
  допустим лишь как явная временная quarantine-мера с tracking-планом,
  не как тихий дефолт.
- **Не** оставлять флаки-тест мигать. Либо чинить root cause (привести
  `assert` к детерминированному контракту / статистике), либо —
  если корень в реальной гонке рантайма — это soundness-дефект,
  эскалировать отдельным планом, тест в явный quarantine со ссылкой.

### Параллель с индустрией

| Экосистема | Практика |
|---|---|
| **Go** | `-race` для гонок; параллелизм-тесты структурно «complete-or-hang» + gated; флаки → fix или `t.Skip` + issue |
| **Rust** | детерминизм по умолчанию; флаки → `#[ignore]` + tracking issue; гонки — `loom`/`tsan` |
| **Industry** | флаки — либо root-cause fix, либо quarantine с tracking; **никогда** не «молча мигает» |
| **Nova** | тест проверяет детерминированный контракт; вероятностное — bounded-sampling; quarantine только явный, с планом |
