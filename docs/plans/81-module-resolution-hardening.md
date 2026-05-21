# Plan 81: Module-resolution hardening — production-grade резолв модулей

> **Создан 2026-05-21.** Переработан с чистого листа 2026-05-21 после
> сверки с реальным кодом компилятора (см. «Сверка фактов» ниже).
>
> **Статус:** 🚧 in progress — Ф.1–Ф.6 ✅ + Ф.7.1 ✅ (2026-05-21,
> worktree `nova-p79`, ветка `plan-81-hardening`), Ф.7.2 + Ф.8–Ф.11
> pending.
> - **Ф.1** ✅ visibility enforcement (commit `197480a747c`).
> - **Ф.2** ✅ module-qualified call type-check — `alias.func()` /
>   `mod.func()` резолвятся в `TypeCheckCtx`; неизвестная функция →
>   **E7401**, неверный аргумент → E7301 (через argbind + Ф.1
>   assignability). Закрыты deferred-негативы Plan 70.1
>   (`nova_tests/plan70_1/f3,f4`).
> - **Ф.3** ✅ cross-file generic bounds — эмпирически verify: уже
>   работают (Plan 35 merge → `protocol_specs` → `check_satisfaction`),
>   orphan rule не нужен (структурная конформность D72). Добавлены
>   regression-фикстуры `nova_tests/plan81/bound_*`.
> - **Ф.4** ✅ resolver strictness: (b) case-sensitive пути —
>   `ResolveErr::CaseMismatch` (`verify_case` канонизирует путь,
>   сверяет регистр); (a) `unused-import` lint — per-peer, селективные
>   импорты (whole-module не линтуется — открытый набор bare-имён через
>   Plan 35 merge). 0 ложных срабатываний на `std/`.
> - **Ф.5** ✅ peer mutual recursion — эмпирически verify: уже работает
>   (сигнатуры всех items регистрируются до обхода тел). Добавлен
>   folder-module `nova_tests/plan81/peer_recur/`.
> - **Ф.6** ✅ symbol mangling v0 — Ф.6.1 централизация именования
>   (`free_fn_c_name`, чистый рефактор), Ф.6.2 модульный mangling
>   `nova_fn_<L><seg>…<L><name>` (length-prefix, путь модуля), спека
>   **D134**, 3 unit-теста схемы. Ф.6.3 (`nova demangle`) — stretch,
>   опционально, не блокирует.
> - **Ф.7.1** ✅ linker-level DCE — `-ffunction-sections -fdata-sections`
>   + `-Wl,--gc-sections` (Linux/macOS) / `/Gy` (MSVC): неиспользуемые
>   секции удаляет линкер. Ф.7.2 (compiler-level reachability DCE) —
>   pending (крупная, см. декомпозицию Ф.7).
> - `plan81/lib` — библиотечная фикстура без `main` (test-runner
>   classification gap) — починена self-test'ом.
>
> Suite на момент Ф.7.1: **954 PASS / 0 FAIL**.
>
> **Источник:** аудит module-resolution 2026-05-21 — открытые пункты
> [Plan 35](35-cross-file-resolve.md) (sub-plans 35.B-E + R26),
> [Plan 70.1](70.1-module-alias-resolution.md) (known-limitation) и
> маркеры `simplifications.md`. Эти планы закрыты как bootstrap-MVP —
> доведение до production-grade переносится сюда.
>
> **Цель:** закрыть резолв модулей так, чтобы по каждому аспекту Nova
> была **не хуже Go / Rust / TS**, а где возможно — лучше. Без
> упрощений: каждая фаза доводится до production-grade с позитивными
> и негативными тестами и синхронизацией спеки.

---

## Зачем

Bootstrap-MVP резолва модулей работает (Plan 35 cross-file resolve;
Plan 42 + 16 sub-plans folder-modules; Plan 70.1 alias codegen). Но
MVP оставил «честные пропуски» трёх сортов: **корректность** (нарушения
спеки — компилируется неверное), **качество codegen** (коллизии имён,
раздувание бинарника) и **диагностика/производительность**. Этот план
собирает их и доводит до уровня state-of-the-art.

## Сверка фактов (2026-05-21, по коду компилятора)

Перед переработкой проверены 7 ключевых фактов — план опирается на
реальное состояние, а не на догадки:

1. **Конформность протоколов — структурная** (D72: «Nova не имеет
   orphan rule — нет `impl Trait for Type`»). → cross-file generic
   bounds **не требуют** orphan/coherence-правила (проще Rust).
2. **`Span`/`FileId`/`SourceMap` уже есть** (`compiler-codegen/src/diag.rs`).
   → FileId — не «строить инфраструктуру», а **аудит и закрытие
   утечек** `file_id=0`.
3. **`export` — строго по-элементно**; field-level visibility отвергнут
   (R28 ❌ D5; `_prefix` полей — конвенция, не enforced, D47). →
   visibility enforcement только для top-level элементов.
4. **External/runtime-функции уже mangled** (`Nova_<Type>_<method>_<name>`),
   а вот **свободные функции пользователя — `nova_fn_<name>` без пути
   модуля** → реальная коллизия между модулями.
5. **Unused imports не детектятся** вообще (нет такого линта).
6. **Идентификаторы ASCII-only** (лексер) → mangling без punycode.
7. **Циклы импортов запрещены** (hard error, Go-style); peer-файлы
   folder-модуля делят namespace и **не** импортируют друг друга.

## Планка: Go / Rust / TS vs Nova

| Аспект | Go | Rust | TS | Nova сейчас | Nova после Plan 81 |
|---|---|---|---|---|---|
| Видимость (`export`/private) | enforced (Caps) | enforced (`pub`) | enforced (`export`) | ❌ **не enforced** | ✅ enforced (Ф.1) |
| Member-call по alias | compile-error | compile-error | compile-error | ❌ link-error | ✅ compile-error (Ф.2) |
| Cross-module generic bounds | ✅ | ✅ (+orphan rule) | ✅ | ❌ не резолвит | ✅ структурно, без orphan (Ф.3) |
| Unused imports | **error** | warning | opt-in | ❌ нет | ✅ warn + opt-in-error (Ф.4) |
| Case-sensitive пути | error | enforced | `forceConsistentCasing` | ❌ нет | ✅ enforced (Ф.4) |
| Order-independent decls / mutual recursion | ✅ | ✅ | ✅ | ⚠️ peers ломаются | ✅ 2-pass (Ф.5) |
| Symbol naming | pkg-path | v0 mangling | n/a | ⚠️ `nova_fn_<name>` без пути | ✅ v0 с путём (Ф.6) |
| Dead-code elimination | linker | compiler+linker | bundler | ❌ всё эмитится | ✅ compiler-level (Ф.7) |
| Cross-file диагностика | FileSet | SourceMap | SourceFile | ⚠️ утечки `file_id=0` | ✅ точная (Ф.8) |
| Multi-error + «did you mean» | терсо | ✅ сильно | ✅ сильно | ⚠️ частично | ✅ multi-error + suggest (Ф.8) |
| Инкрементальная сборка | content-cache | query-incremental | `.tsbuildinfo` | ❌ re-parse каждый раз | ✅ content-addressed cache (Ф.9) |
| Циклы импортов | forbidden | forbidden (crate) | allowed | ✅ forbidden | ✅ (уже соответствует Go) |

## Что НЕ входит

**Отклонено спекой (не недоработки):**
- Относительные пути `import ../sibling` — D29 (import всегда full path).
- Wildcard `import X.Y.*` — R25 spec-rejected (D29/D5).
- pub-гранулярность / field-level visibility — R28 ❌ D5, D47.

**За рамками плана (отдельная будущая работа):**
- Менеджер пакетов / версионирование внешних зависимостей — этот план
  про резолв внутри одного дерева исходников.
- Циклы импортов между модулями остаются **запрещены** (D29, Go-style)
  — осознанное решение, не доработка.

## Что уже сделано (контекст, НЕ задачи)

selective `import X.{A,B}`, `export import` re-export, prelude
auto-import, `#cfg` conditional compilation (Plan 42.12/42.16),
`nova test` cross-file parity (R31), folder-modules + `internal/`,
alias codegen (Plan 70.1), цикл-детекция, `SourceMap`/`FileId` типы.

---

# Фазы

## Группа A — корректность резолва

### Ф.1 — Visibility enforcement (P1 — нарушение спеки) ✅ ВЫПОЛНЕН 2026-05-21

**Проблема:** флаг `is_export` информационный. Не-`export` элементы
импортированного модуля **доступны** снаружи — нарушение D5.

**Цель:** type-checker скрывает не-`export` top-level элементы за
границей модуля. Уровень — как Go (Caps) / Rust (`pub`) / TS (`export`).

**Подзадачи:**
- На границе модуля видимы только `export`-элементы; обращение к
  приватному → **выделенный диагностик** «X приватен для модуля M»
  (не общий «undefined» — UX-уровень Rust «function `foo` is private»).
- Peer-файлы одного folder-модуля видят не-`export` элементы друг
  друга (это правильно — внутри границы). Проверить, что enforcement
  срабатывает **только** на внешней границе.
- `_test.nv` — peer модуля-под-тестом → white-box доступ к приватному
  сохраняется автоматически (как Go white-box / Rust `#[cfg(test)]`).
  Зафиксировать тестом, что не сломалось.
- `import` (без `export`) не реэкспортирует имя: импортированное в A
  имя приватно для A; наружу выходит только через `export import`.
- Согласованность с enforcement `internal/` (Plan 42.13 Rule H).
- **Не** вводить field-level visibility (R28/D47 — отклонено).

**Тесты:** позитив — `export`-элемент виден, приватный виден из peer/
`_test.nv`; негатив — обращение к приватному элементу импортированного
модуля → compile-error; импортированное-не-реэкспортированное имя не
видно у третьего модуля.

### Ф.2 — Alias & cross-module member-call type-check (P1)

**Проблема:** `import X as a; a.unknown()` даёт link-error (undefined
symbol), а не compile-error — `EXPECT_COMPILE_ERROR` не ловит
(Plan 70.1 known-limitation). Type-checker не валидирует Member-call
против сигнатуры функции модуля.

**Цель:** неизвестный метод / неверные аргументы при вызове через
alias или полный путь модуля → **compile-error** (как Go/Rust/TS).

**Подзадачи:**
- Резолв `alias.func(args)` / `mod.path.func(args)` к декларации
  функции целевого модуля.
- Проверка существования функции + arity + типов аргументов против
  сигнатуры; ошибка с точным span (E-код).
- Согласовать с Ф.1 (приватная функция модуля → «private», не
  «undefined»).

**Тесты:** негатив — `a.unknown()`, неверная arity, неверный тип
аргумента; позитив — корректный вызов через alias. Фикстуры
`nova_tests/plan70_1/` (закрывает deferred-негативы Plan 70.1).

### Ф.3 — Cross-file generic bounds (P2)

**Проблема:** `[T Hashable]`, где протокол `Hashable` объявлен в
другом модуле, не резолвится — bound не проверяется.

**Цель:** резолвить имя протокола в bound через таблицу импортов;
дальше — обычная **структурная** проверка (D53/D72).

**Подзадачи:**
- Резолв идентификатора протокола в `[T Protocol]` с учётом импортов
  и prelude.
- Структурная проверка bound'а после резолва (механизм уже есть).
- Убрать workaround «inline-дублирование bound-протокола в каждом
  файле».
- **Orphan/coherence rule НЕ нужен** — Nova структурна (D72), нет
  `impl`-блоков. Это проще Rust и так же безопасно — отметить как
  преимущество.

**Тесты:** позитив — generic-функция с bound из импортированного
модуля компилируется и работает; негатив — тип без нужных методов →
compile-error с указанием недостающего метода.

### Ф.4 — Resolver strictness: unused imports + case-sensitive пути (P2)

**Проблема:** (а) неиспользуемые импорты не детектятся вообще;
(б) путь модуля не сверяется по регистру с именем файла/папки — на
case-insensitive ФС (Windows!) `import Std.Collections` может ложно
зарезолвиться.

**Цель:** обе проверки на уровне Go/Rust/TS.

**Подзадачи:**
- **Unused imports:** линт, отслеживающий использование каждого
  импортированного имени; по умолчанию warning, opt-in error через
  `nova.toml` (паттерн Plan 71 `enforce-stability`). Покрывает спектр
  Go (error) / Rust (warn) / TS (opt-in) — пользователь выбирает.
- **Case-sensitivity:** путь модуля обязан совпадать по регистру с
  именем файла/папки на диске; рассогласование → compile-error
  (как Go, Rust, TS `forceConsistentCasingInFileNames`). Критично
  для корректности на Windows.

**Тесты:** негатив — unused import → warning (и error при opt-in);
`import` с неверным регистром → compile-error. Позитив — used import
тихо, корректный регистр резолвится.

### Ф.5 — Peer mutual recursion (2-pass typecheck) (P2)

**Проблема:** single-pass typecheck merged-AST folder-модуля —
взаимная рекурсия между peer-файлами может ломаться при некоторых
порядках объявлений (маркер AD3).

**Цель:** объявления в folder-модуле order-independent; взаимная
рекурсия между peers работает (как Go package / Rust / TS).

**Подзадачи:**
- 2-проходный typecheck merged-AST: проход 1 — собрать все сигнатуры
  (типы, функции) всех peers; проход 2 — проверить тела.
- Зафиксировать: cross-**модульная** рекурсия невозможна by design
  (цикл импортов = error, D29) — Ф.5 строго про peers одного
  folder-модуля.

**Тесты:** позитив — два peer-файла со взаимно-рекурсивными функциями/
типами в любом порядке объявления компилируются.

## Группа B — качество codegen

### Ф.6 — Systematic symbol mangling v0 (P2, D-блок D134)

**Проблема:** свободные функции пользователя → `nova_fn_<name>` **без
пути модуля**. Два модуля с `fn foo()` → оба `nova_fn_foo` → коллизия
линковки. Глобальный C-namespace без стабильной схемы.

**Цель:** стабильная версионированная схема mangling «v0», уровня
Rust v0; ноль коллизий между модулями.

**Подзадачи:**
- Схема: каждый user top-level символ → `nova_<mangled-modpath>_<kind>_<name>`
  + кодирование type-аргументов для мономорфизированных generic'ов +
  при необходимости короткий disambiguator-хэш.
- ASCII-only (подтверждено лексером) → punycode не нужен.
- Лимит длины C-идентификатора: при превышении безопасного порога —
  усечение + хэш-суффикс (сохраняет уникальность).
- **Exempt:** runtime/external-функции (`builtins.nv` registry) и
  ABI-символы остаются с текущими именами — это FFI/ABI-поверхность.
- D-блок **D134** — спецификация схемы (D133 занят Plan 80).
- Документировать схему; опционально `nova demangle` для читаемых
  стек-трейсов (как `rustc`-демэнглер) — stretch.

**Тесты:** позитив — два модуля с одноимёнными функциями линкуются
без коллизии; mono'д generic из разных модулей различимы. Юнит-тесты
самой схемы (вход → ожидаемый mangled-name).

#### Декомпозиция Ф.6 (2026-05-21)

Mangling нельзя коммитить «наполовину»: определение функции и каждый
call-site обязаны согласованно использовать одно имя — частичный
коммит = несогласованные имена = link-error. Поэтому фаза разбита так,
чтобы **каждый под-шаг был рабочим коммитом** (build + полный прогон
зелёные).

**Сверка с кодом (2026-05-21):** построение `nova_fn_<name>` для
пользовательских свободных функций разбросано по ~15 сайтам
`emit_c.rs` (определение `decl_c_name` ~6534, регистрация overload'а
~1686, `call_target_c_name` ~7232, fn-as-value ~11201/19666, mono base
~15904, erased instance ~8175, Path-вызов ~15810, thunk ~19689 и др.).
Перегрузки уже различаются param-type-суффиксом; mono'д generic'и —
через `compute_mono_name` поверх base-имени. `nova_fn_main_impl`
(синтетический entry) и closure-адаптеры `nova_fn_vi/ii/...` —
**не** пользовательские, exempt.

- **Ф.6.1 — централизация именования (чистый рефактор, без смены
  поведения).** Ввести единый хелпер `CEmitter::free_fn_c_name(name)
  -> String`, изначально возвращающий `format!("nova_fn_{}", name)` —
  поведение **идентично**. Заменить все ~15 разбросанных сайтов на
  вызов хелпера; логика overload-суффикса и exempt-синтетика остаются.
  Acceptance: полный прогон **без изменений** (954/0) — нулевая
  разница поведения. Это безопасный prerequisite: после него смена
  схемы — точечная правка одного хелпера.

- **Ф.6.2 — модульный mangling (смена схемы, атомарно через хелпер).**
  Построить `fn_module_map: HashMap<String, Vec<String>>` (имя
  свободной функции → путь объявляющего модуля) из `module.peer_files`
  (`PeerFile.items_here` + `PeerFile.module_name` — атрибуция по
  peer'у объявления). `free_fn_c_name` → `nova_<modpath>_<name>`
  (сегменты пути, sanitize); функции не из карты (runtime/builtin/
  синтетика) → fallback `nova_fn_<name>`. Overload param-суффикс
  добавляется как раньше; mono — `compute_mono_name` поверх mangled
  base. Лимит длины C-идентификатора → усечение + хэш-суффикс. D-блок
  **D134**. Acceptance: полный прогон 0 регрессий; два модуля с
  одноимёнными функциями линкуются; mono'д generic'и из разных
  модулей различимы; unit-тесты схемы.

- **Ф.6.3 — `nova demangle` (stretch, опционально).** Обратное
  преобразование mangled → читаемое имя для стек-трейсов. Может быть
  отложено отдельно — не блокирует закрытие Ф.6.

### Ф.7 — Dead-code elimination (P2)

**Проблема:** все импортированные элементы эмитятся в C, даже
недостижимые (`import std.collections.range` тянет 20+ методов).

**Цель:** эмитить только достижимый код — уровень Go (linker DCE),
с выигрышем по скорости компиляции (отсечение на уровне компилятора).

**Подзадачи:**
- Worklist достижимости от корней: `main` (executable); все блоки
  `test`/`bench` (`nova test`); `export`-элементы корневого модуля
  (library-сборка).
- Обход графа: вызовы функций, ссылки на типы/константы/методы;
  динамическая диспетчеризация (protocol-значение) — консервативно
  метить protocol-методы используемого типа; generic'и — только
  реально инстанцированные мономорфизации.
- Эмитить только элементы из множества достижимости.
- Дополнительно — флаги C `-ffunction-sections -fdata-sections
  -Wl,--gc-sections` как страховка.

**Тесты:** недостижимая импортированная функция отсутствует в
сгенерированном `.c`; достижимая присутствует; protocol-dispatch не
теряет методы.

#### Декомпозиция Ф.7 (2026-05-21)

DCE разбита на два под-шага по уровню риска:

- **Ф.7.1 — linker-level DCE (безопасно, малый коммит).** Передавать
  C-тулчейну `-ffunction-sections -fdata-sections` (компиляция) +
  `-Wl,--gc-sections` / `/OPT:REF` (линковка). Неиспользуемые функции/
  данные удаляет **линкер** — как в Go. Достигает цели «меньше
  бинарник» с near-zero риском (не трогает codegen). Per-toolchain
  (clang/gcc — `--gc-sections`; MSVC link — `/OPT:REF`).

- **Ф.7.2 — compiler-level reachability DCE (крупное, рисковое).**
  Worklist достижимости от корней (`main`, `test`/`bench`, export'ы
  корневого модуля), обход call/type-графа, эмит только достижимого.
  Риск: codegen-completeness merge'и существуют для typedef-ordering
  single-pass codegen'а — агрессивный DCE может уронить порядок;
  protocol dynamic-dispatch требует консервативного удержания методов.
  Делается как единый завершённый блок с полным fallout-прогоном.
  Acceptance плана («недостижимая функция отсутствует в `.c`»)
  закрывается именно здесь.

## Группа C — диагностика и производительность

### Ф.8 — Resolution diagnostics: multi-error + suggestions + FileId-аудит (P2)

**Проблема:** (а) резолв падает на первой ошибке импорта; (б) нет
«did you mean» для опечаток; (в) маркер simplifications утверждает,
что импортированные span'ы имеют `file_id=0` — но инфраструктура
`SourceMap`/`FileId` уже есть и peers получают FileId → маркер
вероятно устарел, нужен аудит.

**Цель:** диагностика резолва уровня Rust/TS (Go здесь терсее —
шанс быть **лучше Go**).

**Подзадачи:**
- Multi-error recovery: не прерываться на первой нерезолвленной
  ссылке — собрать и показать все ошибки резолва.
- «Did you mean?»: предложение по Левенштейну для опечаток в пути
  модуля / имени импорта (как Rust/TS).
- **FileId-аудит:** проверить, действительно ли single-file импорты и
  все peers получают корректный `file_id` и `SourceMap` заполнен их
  содержимым (для рендера сниппета); закрыть остатки `file_id=0`;
  обновить/закрыть устаревший маркер.

**Тесты:** несколько ошибок импорта в одном файле — показаны все;
опечатка пути → подсказка; ошибка в импортированном модуле указывает
на правильный файл+строку+сниппет.

### Ф.9 — Build cache + incremental (P3)

**Проблема:** каждый `nova build` заново парсит все импорты; нет
пересборки по графу зависимостей.

**Цель:** content-addressed кэш модулей — модель Go (`$GOCACHE`).

**Подзадачи:**
- Ключ кэша модуля = хэш(содержимое файлов + версия компилятора +
  активные `#cfg`-флаги + отсортированные хэши ключей прямых
  зависимостей) → транзитивная инвалидация автоматом.
- Значение — разобранный/проверенный артефакт модуля.
- Хранилище — `target/.nova-cache/` (или аналог); гранулярность —
  модуль.
- Полноценный query-level инкремент (стиль Rust — пересборка только
  затронутых функций) — отмечен как будущее, не v1.

**Тесты:** повторный `nova build` без изменений — попадание в кэш;
изменение файла инвалидирует его и зависимых.

### Ф.10 — Entry-folder-module peer-isolation (P3)

**Проблема:** per-peer import isolation не активна, если **сам
entry-модуль** — folder-module (entry парсится как один файл,
MAIN_FILE_ID). Дизайн зафиксирован в `[M-entry-folder-module]`.

**Цель:** per-peer резолв работает и для entry-folder-module.

**Подзадачи:** активировать peer-резолв для entry; resolver-side +
test-runner-side изменения по дизайну из `[M-entry-folder-module]`.

**Тесты:** entry как folder-module с per-peer импортами компилируется
и тестируется; изоляция импортов между peers соблюдается.

## Группа D

### Ф.11 — spec sync + чистка simplifications.md + README

- `spec/decisions/07-modules.md`: visibility enforcement (D5),
  mangling-схема (D134), переподтвердить политику циклов (D29).
- `simplifications.md`: MVP-таблица Plan 35 (~стр. 4595-4612) и секция
  wildcard (~стр. 4405) устарели — отметить сделанное / spec-rejected;
  закрыть маркеры FileId, AD3, `[M-entry-folder-module]`,
  перенесённые в этот план.
- `docs/plans/README.md` — обновить статус.

---

## Где Nova будет ≥ Go/Rust/TS

- **Generic bounds без orphan rule** — структурная конформность (D72):
  cross-file bounds работают без когнитивного налога orphan-правил
  Rust, и так же безопасно. **Лучше Rust.**
- **Две гранулярности модулей** — folder-module (как Go package) +
  file-module: больше гибкости, чем у Go (только package) или Rust
  (только дерево модулей).
- **DCE на уровне компилятора** — отсечение до C-кодогенерации
  ускоряет компиляцию; Go полагается только на линкер. **≥ Go.**
- **Диагностика резолва** — multi-error + «did you mean» + точные
  cross-file span'ы: **лучше терсого Go**, на уровне Rust/TS.
- **Unused imports — гибко** — warn по умолчанию + opt-in error:
  покрывает и строгость Go (error), и мягкость Rust (warn) без
  навязывания.
- **Циклы импортов запрещены** — как Go (чистая архитектура), строже
  TS.

## Приоритеты и порядок

| Фаза | Приоритет | Природа |
|---|---|---|
| Ф.1 visibility | **P1** | корректность (нарушение спеки) |
| Ф.2 alias type-check | **P1** | корректность (compile vs link error) |
| Ф.3 generic bounds | P2 | функц. дыра |
| Ф.4 strictness | P2 | корректность (Windows) + UX |
| Ф.5 peer mutual recursion | P2 | корректность |
| Ф.6 mangling | P2 | безопасность codegen |
| Ф.7 DCE | P2 | качество |
| Ф.8 диагностика | P2 | UX |
| Ф.9 cache | P3 | производительность |
| Ф.10 entry-folder | P3 | edge-case |

**Рекомендованный порядок:** Ф.1 → Ф.2 → Ф.3 → Ф.5 → Ф.4 → Ф.8 →
Ф.6 → Ф.7 → Ф.10 → Ф.9 → Ф.11. Фазы независимы — каждую можно
закрывать отдельным коммитом с тестами.

## Зависимости

- Опирается на закрытые Plan 35 / Plan 42.x / Plan 70.1.
- Ф.6 требует D-блок D134.
- Прочие фазы независимы между собой.

## Ссылки

- [Plan 35](35-cross-file-resolve.md) — cross-file resolve MVP; sub-plans
  35.B-E и R26 перенесены сюда.
- [Plan 42](42-folder-modules.md) / [Plan 42.17](42.17-audit-closure.md)
  — folder-modules; `[M-entry-folder-module]` → Ф.10.
- [Plan 70.1](70.1-module-alias-resolution.md) — alias codegen;
  known-limitation → Ф.2.
- [Plan 71](71-doc-stability-scope.md) — паттерн warn + opt-in-error
  (для Ф.4 unused imports).
- `docs/simplifications.md` — маркеры FileId / AD3 / `[M-entry-folder-module]`.
- `spec/decisions/07-modules.md` — D5 (видимость), D29 (модули,
  циклы), D47 (поля record).
- `spec/decisions/02-types.md` — D72/D53 (структурные bound'ы).
- `compiler-codegen/src/diag.rs` — `Span`/`FileId`/`SourceMap`.
- `compiler-codegen/src/imports.rs` — резолв импортов, цикл-детекция.
