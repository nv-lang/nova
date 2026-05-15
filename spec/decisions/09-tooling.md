# Tooling — компилятор, верификация, диагностика

Решения этой группы определяют, как **инструменты** Nova работают с
программами: какие проверки выполняются на этапе компиляции, какие —
в runtime, и как ошибки оформлены для AI-генерации кода.

| # | Решение |
|---|---|
| [D24](#d24-стратегия-smt-проверки-контрактов) | Стратегия SMT-проверки контрактов |
| [D89](#d89-test-tooling-конвенции--expect-маркеры-для-negative-тестов) | Test-tooling конвенции: `EXPECT_*` маркеры для negative-тестов |
| [D95](#d95-cli-path-конвенции--nova-check--nova-test) | CLI path конвенции — `nova check <path>` / `nova test <path>` |
| [D96](#d96-синтаксис-атрибутов-name-без-квадратных-скобок) | Синтаксис атрибутов — `#name` без квадратных скобок |

---

## D24. Стратегия SMT-проверки контрактов

> **Note (Plan 33.1, D96):** Атрибуты используют префикс **`#`** (не `@`),
> см. [D96](#d96-синтаксис-атрибутов-name-без-квадратных-скобок).

### Что
Контракты в сигнатуре (`requires`/`ensures`/`invariant`) проверяются
**SMT-движком на этапе компиляции** с явным таймаутом и fallback на
runtime-проверку. Контракт, который SMT не смог доказать, **не
блокирует** компиляцию по умолчанию — он становится runtime-assert'ом
в debug и тихо игнорируется в release с предупреждением `#unverified`.
Программист может явно требовать статическое доказательство через
`#must_verify` — тогда компиляция падает, если SMT не справился.

### Правило

#### Стратегия SMT

1. **SMT-кодировка** контрактов из `requires`/`ensures`/`invariant`
   в стандартный формат (SMT-LIB v2). Конкретный движок —
   **выбор реализации**, не дизайна. Дизайн фиксирует **класс
   движка**: поддержка теорий **LIA** (linear integer arithmetic),
   **EUF** (equality + uninterpreted functions), **arrays** базовой
   функциональности. Этим требованиям удовлетворяют Z3, CVC5,
   Bitwuzla — выбор делает компилятор-реализация.

2. **Таймаут на функцию** — рекомендуемый дефолт **2 секунды**.
   Превышение → fallback на runtime-проверку. Программист может
   увеличить через `#verify_timeout(10000)` локально или
   глобально в конфигурации проекта.

3. **Поведение по уровням сборки:**
   - **debug:** SMT-проверка + runtime-fallback для непроверенного.
     Нарушение runtime → panic с указанием контракта и точки.
   - **release:** SMT-проверка. Доказанные контракты — стираются
     полностью (zero cost). Недоказанные — игнорируются молча с
     warning'ом на этапе сборки.

4. **Опт-ин строгости через атрибуты:**
   - **`#must_verify`** на функции → если SMT не доказал контракт,
     компиляция **падает**. Для критичного кода (медицина, финансы,
     авионика).
   - **`#unverified`** на функции → отказ от попытки доказательства
     заранее, всегда runtime-check (чтобы не тратить время компиляции
     на заведомо непроверяемое).

#### Что поддерживается в v1.0

**Целевые классы контрактов:**

| Класс | Пример | Решается |
|---|---|---|
| Линейная арифметика над `int`/`money` | `requires amount > 0`, `ensures result == a + b` | да (LIA) |
| Equality для record и sum-type | `requires acc.id == old.id` | да (EUF) |
| Cardinality коллекций | `requires xs.len() > 0`, `ensures result.len() <= xs.len()` | да (через axiomatization) |
| Membership | `ensures result in xs` | да |
| `old(...)` в `ensures` | `ensures balance == old(balance) - amount` | да |
| Условные импликации | `ensures result.is_ok ==> condition` | да |

**Что НЕ поддерживается (research-уровень, отложено):**

- Квантификаторы общего вида (`forall x. P(x) ==> Q(x)`).
- Индукция по структуре данных.
- Рекурсивные предикаты.
- Нелинейная арифметика.
- Floating-point reasoning.
- String reasoning сложнее `len()` и equality.

Контракты с этими конструкциями принимаются грамматикой, но SMT их не
доказывает → fallback на runtime или ошибка с `#must_verify`.

#### Контракты со ссылками на handler-state

Открытый вопрос. Контракт может содержать обращение к операции
эффекта:

```nova
fn transfer(...) Db -> ()
    ensures Db.balance(to) == old(Db.balance(to)) + amount
=> ...
```

Это требует **effect-aware SMT-кодировки**: handler-вызов как
неинтерпретированная функция с теоремами о её поведении. В v1.0
поддержка **частичная** — только для эффектов с явным `pure_view`
(чистая проекция состояния handler'а). Полная поддержка — research,
отдельный D-пункт после v1.0.

### Почему

#### AI-first связь

Когда SMT не справился, ошибка компилятора имеет структурированный
формат:

```
warning C0341: contract not verified statically
   in function `withdraw` at src/account.nv:34
   ┌─ src/account.nv:34:5
   │
   34 │     ensures acc.balance == old(acc.balance) - amount
   │             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ this contract was not proven

   reason: SMT solver returned 'unknown' after 2.0s
   missing facts:
     - relation between `acc.balance` and arithmetic operations on `money`

   fallback: runtime check inserted in debug build

   suggestions:
     1. add intermediate `assert` to break the proof into steps
     2. use `#must_verify` if static proof is required
     3. simplify the contract if possible
```

Это **обучающий сигнал** для LLM. Модель получает не «что-то не так»,
а конкретный класс проблемы и три предложения. Это и есть AI-first
компилятор.

#### Прецеденты

- **Dafny** — SMT-проверка через Z3, fallback на runtime.
- **F\*** — статическое доказательство, без fallback (более строго).
- **Why3** — оркестрация нескольких SMT-движков.
- **Spec#** (Microsoft Research) — пилот для C#, заглох, но идеи
  переехали в Code Contracts.

Nova берёт **прагматичный путь Dafny** (статика + runtime fallback),
не максималистский путь F* (всё статически).

### Что отвергнуто

- **«Всегда статическая проверка, без runtime-fallback»** — сделало
  бы контракты обязательно тотальными, половину прикладного кода
  невозможно было бы аннотировать.
- **«Только runtime-проверка»** — теряется ценность контрактов в
  release (zero cost). Превращает контракты в обычные `assert`.
- **Фиксация конкретного SMT-движка в дизайне.** Дизайн — про
  семантику, не про имя движка. Имя — выбор реализации, как
  fiber-runtime в [06-concurrency.md → D14](06-concurrency.md#d14)
  (там не сказано «Tokio», сказано «fiber-based scheduler»).

### Связь

- [01-philosophy.md → D10](01-philosophy.md#d10) — видимость в типах,
  проверяемость по фрагменту, AI-first как обоснование.
- [04-effects.md](04-effects.md) — handler-state в контрактах требует
  effect-aware SMT.
- [05-memory.md → D6](05-memory.md#d6) — параллель: «дизайн фиксирует
  класс, имя движка — выбор реализации».
- [08-runtime.md → D13](08-runtime.md#d13) — отношение Panic и
  contract-violations: нарушение контракта в runtime — это panic.
- [08-runtime.md → D81](08-runtime.md#d81) — три уровня safety:
  `assert(cond)` (always runtime) < `debug_assert(cond)` (debug-only)
  < `requires`/`ensures` (D24, compile-time где возможно). Контракты —
  «zero-cost» вариант для compile-time; assert'ы — escape hatch для
  ситуаций где SMT недоступен.

### Цена

1. **Реализация требует SMT-интеграции.** Нетривиально, но не
   research — Dafny / F* / Why3 показали, что это работает.
2. **Таймаут зависит от структуры контракта.** Программист иногда
   удивится: «почему не доказывается?». Структурированная ошибка
   должна объяснять.
3. **Effect-aware SMT** — частичная поддержка в v1.0, полная — после.
   Контракты с handler-state — known limitation, не проблема.

### Открытые вопросы

- **Effect-aware SMT** — полная поддержка контрактов с обращениями к
  handler-state.
- **Структура `pure_view`** для эффектов — какие части handler-state
  программист объявляет «чистыми проекциями».
- **`#must_verify` на уровне модуля** — глобальный strict mode для
  критичных компонентов.

---

## D89. Test-tooling конвенции — `EXPECT_*` маркеры для negative-тестов

### Что
Стандартизированный набор **comment-маркеров** в `.nv`-файлах для
тестов, которые **должны не сработать** ожидаемым образом — от compile
error до runtime panic / specific exit code / stdout pattern. Маркеры
интерпретируются **test-runner'ом**, не парсером Nova: для самого
языка это обычные комментарии.

Цель — **унифицировать** test-tooling-конвенцию. Любой Nova-conformant
test-runner (текущий `run_tests.ps1`, будущий `nova test`, CI-
интеграции, third-party fuzzer'ы) **обязан** реализовать стандартные
маркеры. Это снимает вопрос «каждый разработчик придумывает своё» и
делает тесты переносимыми между runner'ами.

### Правило

#### Стандартные маркеры (4 штуки)

Маркер располагается **в первых 30 строках** файла, **в строке-
комментарии**, формат:

```
// EXPECT_<TYPE> <argument>
```

Один маркер на файл. Если в файле несколько маркеров — runner берёт
**первый** найденный.

| Маркер | Аргумент | Поведение test-runner'а |
|---|---|---|
| `EXPECT_COMPILE_ERROR` | substring-pattern | codegen должен **завершиться с ненулевым exit code** и сообщение содержит pattern |
| `EXPECT_RUNTIME_PANIC` | substring-pattern | exe скомпилировался, **запустился** и упал с panic; **stderr** содержит pattern (panic пишет в stderr) |
| `EXPECT_EXIT_CODE` | целое число `N` | exe скомпилировался, запустился и завершился с **exit code = N** |
| `EXPECT_STDOUT` | substring-pattern | exe запустился (любой exit code) и его **stdout** (только stdout, не stderr) содержит pattern |
| `EXPECT_STDERR` | substring-pattern | exe запустился (любой exit code) и его **stderr** (только stderr, не stdout) содержит pattern |

**Семантика логики:**
- При наличии маркера логика test-runner'а **переворачивается**:
  обычное «codegen succeeded → pass» становится «codegen failed
  ожидаемым образом → pass».
- При несоответствии (codegen не упал когда ждали error, или упал
  не с тем pattern, или exe вернул не тот exit code, или нужный
  поток не содержит pattern) — test **fails**.
- Файл с `EXPECT_COMPILE_ERROR` **не компилируется** в exe и
  **не запускается** (предполагается невалидный код).
- Файл с `EXPECT_RUNTIME_PANIC` / `EXPECT_EXIT_CODE` / `EXPECT_STDOUT`
  / `EXPECT_STDERR` компилируется и запускается, runner смотрит на
  runtime-результат.
- **stdout и stderr — разные потоки.** `EXPECT_STDOUT pattern`
  сматчит pattern **только** если он в stdout; `EXPECT_STDERR
  pattern` — только если в stderr. Для проверки combined-вывода
  (любой поток) используйте `EXPECT_RUNTIME_PANIC` (для panic'ов,
  которые идут в stderr).

#### Pattern-matching

- **Substring**, не regex. Должен **присутствовать** в выводе
  компилятора (для `EXPECT_COMPILE_ERROR`) или в panic-сообщении /
  stdout (для runtime-маркеров).
- **Case-sensitive**. Программист пишет точный кусок ожидаемого
  сообщения.
- Multi-line patterns не поддерживаются — runner склеивает вывод в
  одну строку через пробел перед matching.

#### Исключающее поведение

Маркеры **взаимоисключающие** — один файл = один маркер. Если автор
хочет проверить **несколько** error-условий — **отдельные файлы**
для каждого (один файл на один аспект).

Это ограничение **сознательное**:
- Простой mental model для авторов тестов.
- Простой код test-runner'а (одна вилка на файл).
- Force'ит **разделение** тестов по сценариям, что улучшает
  читаемость и точность диагностики падений.

Альтернатива (multi-marker через `EXPECT_*_LINE N: pattern`) —
сложнее, отвергнута для bootstrap'а.

#### Примеры

**`EXPECT_COMPILE_ERROR`:**
```nova
// EXPECT_COMPILE_ERROR duplicate definition

module nova_tests.negative_capability.overload_dup

fn process(n int) -> str { "first" }
fn process(n int) -> str { "second" }    // duplicate sig
```

**`EXPECT_RUNTIME_PANIC`:**
```nova
// EXPECT_RUNTIME_PANIC array bounds

module nova_tests.runtime_panic.array_bounds

fn main() Io -> () {
    let xs = [1, 2, 3]
    let _ = xs[10]                       // out-of-bounds
}
```

**`EXPECT_EXIT_CODE`:**
```nova
// EXPECT_EXIT_CODE 42

module nova_tests.runtime_panic.exit_code

fn main() Io -> () {
    exit(42, "intentional")
}
```

**`EXPECT_STDOUT`:**
```nova
// EXPECT_STDOUT hello world

module nova_tests.runtime.golden_hello

fn main() Io -> () {
    println("hello world")
}
```

#### Compliance

Test-runner называется **Nova-conformant** ⇔ реализует **все 4**
стандартных маркера согласно спецификации выше.

Custom-runner может **расширять** набор маркеров своими (например
`EXPECT_LINT_WARNING`, `EXPECT_MEMORY_LEAK`), но **не должен**:
- Игнорировать стандартные маркеры (молча выполнять файл с
  `EXPECT_COMPILE_ERROR` как обычный тест).
- Изменять семантику стандартных маркеров (например делать
  `EXPECT_COMPILE_ERROR` case-insensitive).
- Использовать имена `EXPECT_*` для своих расширений (зарезервировано).

### Почему

#### Зачем стандартизировать

Без D89 каждый test-runner придумывает свой механизм:
- `run_tests.ps1` — comment-маркер `EXPECT_COMPILE_ERROR`.
- Гипотетический `nova test` — мог бы выбрать атрибут `@expect_error`.
- CI-скрипт — мог бы держать список «ожидаемо падающих» файлов в
  YAML.

Это привело бы к **fragmentation**: тесты, написанные для одного
runner'а, не работают в другом. Авторам тестов пришлось бы дублировать
маркеры или писать «multi-runner adapter». Это **анти-паттерн** —
test-конвенции должны быть **переносимыми**.

D89 фиксирует **минимальный** общий набор. Расширения возможны, но
ядро универсально.

#### Почему comment-маркер, а не часть языка

Альтернатива — сделать маркер **first-class директивой языка** (как
TypeScript `// @ts-expect-error`). Это **отвергнуто** для Nova:

- Test-маркеры — **edge-case фича** (используется в ~5% файлов).
  Загрязнять core-language ради 5% — over-engineering.
- Парсер Nova **не должен знать** про testing — это violation
  separation of concerns.
- TypeScript-precedent специфичен: TS-комментарий-директива нужна
  и **в production-коде** (suppression of compile errors), не только
  в тестах. У Nova такой потребности нет — есть `forbid`/`realtime`
  блоки для сознательных suppressions.

Comment-маркер — **простой и достаточный** паттерн для test-only
конвенции. Прецеденты:
- Rust `compiletest`: `//~ ERROR pattern`.
- Swift test-toolkit: `// expected-error {{pattern}}`.
- Go errorcheck: `// ERROR pattern`.

#### Почему 5 маркеров, не больше

Минимум, покрывающий 95% test-сценариев:
- Compile-time errors → `EXPECT_COMPILE_ERROR`.
- Runtime panics (D13) → `EXPECT_RUNTIME_PANIC`.
- Process-exit codes (D13 exit) → `EXPECT_EXIT_CODE`.
- Output-content tests stdout → `EXPECT_STDOUT`.
- Output-content tests stderr → `EXPECT_STDERR`.

stdout/stderr — два независимых маркера, потому что POSIX-конвенция
разделяет потоки: stdout — для data, stderr — для diagnostics. Тесты
должны различать. Combined-проверка (без разделения) не нужна — для
panic'ов есть специализированный `EXPECT_RUNTIME_PANIC`.

Что **может быть добавлено** позже, при появлении use-cases:
- `EXPECT_NO_STDERR` — exe не должен ничего писать в stderr (нет
  warning'ов).
- `EXPECT_LINT_WARNING pattern` — lint без error.
- `EXPECT_TIMEOUT_MS N` — exe должен **не** превысить N мс.
- `EXPECT_NO_OUTPUT` — exe не должен ничего выводить.

Эти расширения добавляются **отдельным D-блоком** при необходимости,
не предзагружают spec лишним.

### Что отвергнуто

- **Уровень 3 — атрибут языка** (`@expect_error("pattern")`). Test-only
  фича не оправдывает изменения парсера / type-checker'а. См. «Почему
  comment-маркер».
- **Эталонный `.stderr`-файл рядом с `.nv`** (Rust trybuild-style).
  Больше ceremony, не нужен для substring-match.
- **Multi-marker в одном файле** через `EXPECT_*_LINE N: pattern`.
  Усложняет mental model и реализацию runner'а; разделение по файлам
  — лучше для читаемости и точности диагностики.
- **Regex-pattern вместо substring**. Substring проще писать и читать,
  не требует escape метасимволов в типичных сообщениях.
- **YAML / TOML manifest со списком expected-failures** (как у
  некоторых CI-систем). Маркер в самом файле — локально, виден
  автору при чтении кода.

### Связь

- [08-runtime.md → D13](08-runtime.md#d13) — `panic` / `exit`
  семантика, на которой строится `EXPECT_RUNTIME_PANIC` /
  `EXPECT_EXIT_CODE`.
- [D24](#d24) — другой test-related D-блок (SMT-проверка контрактов);
  D89 — общий тестовый tooling.
- `docs/test-conventions.md` — практический guide для авторов тестов
  (как писать каждый тип маркера, типичные паттерны).
- `run_tests.ps1` — Windows wrapper над `nova-codegen test-all`.
  Был заведён в Plan 16 Ф.7 для capability-enforcement, расширен до
  полного набора D89-маркеров. После [Plan 24](../../docs/plans/24-cross-platform-test-runner.md) — thin shim.
- `run_tests.sh` — Linux/macOS wrapper над тем же `test-all`.
- `compiler-codegen/src/test_runner.rs` — каноническая реализация D89
  парсера и pipeline'а (codegen + cc + run + check). Production-grade
  hardening — [Plan 26](../../docs/plans/26-test-runner-hardening.md):
  per-test timeout (`--timeout`), parallel execution (`--jobs`), structured
  output (`--format json|tap|text`), `--rerun-failed`, per-test isolation,
  UTF-8 codepage force.

### Bootstrap-status

- ✅ **`EXPECT_COMPILE_ERROR`** — реализовано в `run_tests.ps1`
  (Plan 16 Ф.7). Используется 8 negative-тестов в
  `nova_tests/negative_capability/`.
- ✅ **`EXPECT_RUNTIME_PANIC`** — реализовано (2026-05-10).
- ✅ **`EXPECT_EXIT_CODE`** — реализовано (2026-05-10).
- ✅ **`EXPECT_STDOUT`** — реализовано (2026-05-10). Только stdout
  (после split'а stdout/stderr).
- ✅ **`EXPECT_STDERR`** — реализовано (2026-05-10). Только stderr.

Future runner'ы (`nova test` CLI, `cargo test` для interp-mode,
CI-плагины) должны переиспользовать эту конвенцию. Реализацию для
других OS / toolchain'ов писать **с теми же маркерами**.

### Цена

1. **Нужно поддерживать в каждом runner'е.** Если появится `nova test`
   на Nova самом — реализовать 5 маркеров обязательно. Не сложно
   (substring-match + condition negation), но **обязательно**.
2. **Маркер — plain comment**, парсер про него не знает. Если автор
   опечатался (`EXPECT_COMPILE_EROR` без R) — runner проигнорирует,
   тест выполнится как обычный (и упадёт на compile-error
   неожиданно). Mitigation: linter может предупреждать о похожих
   на маркер опечатках в первых 30 строках.
3. **Расширения требуют D-блока.** Custom-маркеры в одном проекте —
   допустимы, но если хочется чтобы маркер стал стандартным
   (доступным в любом runner'е) — нужен D-блок-расширение.

---

## D95. CLI path конвенции — `nova check <path>` / `nova test <path>`

### Что

CLI subcommand'ы `nova check` и `nova test` принимают **позиционный
polymorphic path argument** (file-or-directory). Без `--recursive`
флага — directory всегда рекурсивно. Без `--tests-dir`,
`--check-recursive` и подобных — путь **позиционный**.

Прецеденты: `cargo check <path>` (deprecated в cargo, но pattern
в Rust ecosystem standard), `go vet ./...`, `clippy <path>`, `eslint <path>`,
`prettier <path>`, `ruff check <path>`, `black <path>`.

### Правило

#### Signature

```
nova check [<path>...]              # 0+ positional paths
nova test  [<path>]                 # 0..1 positional path
```

**`nova check`:**
- 0 paths → walk parents до `nova.toml` (workspace root), recurse.
- 1+ paths → каждый is_file → single check; is_dir → recurse.
- Multi-path: `nova check std/ examples/` — оба обрабатываются.

**`nova test`:**
- 0 paths → default `<repo>/nova_tests/`.
- 1 path: is_file → single test (filter through display name);
  is_dir → use as tests directory.
- Multi-path не поддерживается в MVP (test_runner ограничение).

#### Семантика

1. **file vs dir дискриминация** через `path.is_file()` / `path.is_dir()`,
   не через флаги.
2. **Recurse default для directory** — без `--recursive` флага
   (clippy/eslint convention).
3. **`.nv` extension required** для file argument. Wrong extension →
   error.
4. **Non-existent path** → error.
5. **`std/runtime/` hard-skip** (auto-gen из registry, D89).
6. **Implicit skip directories**: `target/`, `node_modules/`, `vendor/`,
   `.git/`, `.hg/`, `.svn/`, любые `_*` и `.*` directories.

#### Что НЕ поддерживается в MVP

(Расширения через sub-plans, см. [Plan 36](../../docs/plans/36-cli-production-hardening.md):
sub-plans 36.A-E.)

- **Glob patterns** (`*.nv`, `**`) — shell expansion достаточен.
- **`./...` go-style suffix** — slash-style (`std/`) проще и proven.
- **Multi-path для `nova test`** — однопутевая семантика в MVP.
- **`--recursive` / `--tests-dir` / подобные флаги** — **запрещены**
  (clean break, не deprecation).
- **`compile_commands.json`-style output** — отдельный план.
- **Glob/regex для filter** — `--filter` остаётся substring match.

### Запрещённые флаги

Следующие флаги **не должны существовать** в `nova check` / `nova test`
(R1-R3 plan-36, clean-break policy):

| Запрещённый флаг | Почему | Что вместо |
|---|---|---|
| `--recursive` / `-r` | Дублирует is_dir дискриминацию | Просто `nova check <dir>` |
| `--tests-dir <dir>` | Дублирует path positional | `nova test <dir>` |
| `--check-recursive` | Дублирует path semantic | `nova check <dir>` |
| `--all` / `--workspace` | Cargo-style; у нас walks-parents default | `nova check` без path |

### Почему

#### AI-first связь

LLM генерирует CLI invocations в скриптах / документации. Polymorphic
path arg = **меньше surface для ошибок**. Когда есть `--tests-dir`,
LLM может сгенерировать `nova test --tests-dir foo` где `nova test foo`
работает. Одна форма — одна семантика.

#### Прецеденты

| Tool | Path argument | Recursive default |
|---|---|---|
| `cargo check` (workspace-style) | path не принимает (только `-p`) | да (workspace) |
| `go vet ./...` | `./...` pattern | да |
| `clippy <path>` | да | да (dir) |
| `eslint <path>` | да | да (dir) |
| `prettier <path>` | да | да (dir) |
| `ruff check <path>` | да | да (dir) |
| `black <path>` | да | да (dir) |
| **`nova check <path>`** | **да** | **да (dir)** |

Nova следует **`clippy` / `eslint` / `ruff` / `black` school**: positional
path, file-or-dir, recurse-by-default.

#### Exit codes

| Code | Значение | Условие |
|---|---|---|
| 0 | success | все checks/tests passed |
| 1 | diagnostic failure | type-check error, test fail |
| 2 | usage error | bad flag, path not found, wrong extension |
| 101 | panic | tool bug (cross-platform через `std::panic::set_hook`) |

Реализовано **полностью** (commit 62c04378fa, Plan 36 R7).

### Что отвергнуто

- **`nova check --recursive <dir>`** — дублирует `is_dir()`
  дискриминацию. Каждый currentmainstream linter работает без этого
  флага.
- **`--tests-dir <dir>` deprecation cycle** — bootstrap не в проде,
  clean break лучше (см. `feedback_revolutionary_changes` память).
  Удаление флага → `error: unexpected argument '--tests-dir' found`.
- **Glob patterns в CLI** (`nova check **/*.nv`) — shell expansion
  делает это лучше. Не нужно реализовывать parser glob'ов.
- **Cargo-style `-p <package>` selection** — у Nova workspace concept
  не сформирован (4 nested nova.toml в repo, см. AD6 Plan 36).
  Path-based proще.
- **`./...` go-style suffix** — лишний синтаксис. `nova check std/`
  более интуитивно чем `nova check std/...`.

### Связь

- [01-philosophy.md → D10](01-philosophy.md#d10) — AI-first как driver
  для simplicity.
- [D89](#d89-test-tooling-конвенции--expect-маркеры-для-negative-тестов) —
  test-tooling конвенции (EXPECT_* markers).
- [Plan 36](../../docs/plans/36-cli-production-hardening.md) —
  полная спецификация R1-R30 + AD1-AD12, MVP = Ф.0+Ф.1, sub-plans
  36.A-E для остального.
- [08-runtime.md → D13](08-runtime.md#d13) — panic semantics
  (relates to exit code 101).

### Цена

1. **Несовместимость со старым `--tests-dir`.** Кто-то у себя имел
   `nova test --tests-dir foo` в скрипте → нужно `nova test foo`.
   Bootstrap не в проде → приемлемо.
2. **Path не path-pattern.** Если нужно «все файлы кроме одного» —
   нужны `--no-exclude` flags (sub-plan 36.A). MVP shell-expansion
   достаточен.
3. **MVP — exit 0/1 only.** Quintuplet (0/1/2/3/101) отложен. Скрипты
   которые отличают usage-error от diagnostic пока полагаются на stderr
   message parsing — fragile.

### Открытые вопросы

- **Multi-path для `nova test`** — нужен только если test_runner
  поддержит multi-tests-dir. Сейчас нет use-case.
- **`-` (stdin) input для editor integration** — отдельный план
  (LSP / formatter).
- **`--list` mode** (show files без checking) — useful для отладки
  implicit-excludes; sub-plan 36.D.




---

## D96. Синтаксис атрибутов — `#name` без квадратных скобок

### Что

Function/type/module-level атрибуты в Nova используют префикс **`#`** (а не `@`),
**без обязательных квадратных скобок** (как в Rust `#[name]`).

```nova
#realtime
#pure
#must_verify
fn must_pure(x int) -> int
    requires x > 0
    ensures result > 0
=> x + 1
```

Атрибуты с аргументами — через **круглые** скобки (как Java/Kotlin/Python/Scala):

```nova
#verify_timeout(5000)
#allow_transit(Db, Log)
#derive(Json, FromRow)
fn process_order(o Order) -> Receipt => ...
```

### Правило

#### Грамматика

```
Attribute := '#' Ident ( '(' ArgList ')' )?
ArgList   := Expr (',' Expr)*
```

- Простой маркер: `#pure`.
- С аргументами: `#verify_timeout(5000)`.
- Несколько атрибутов — на отдельных строках перед declaration.

#### Position

Атрибуты разрешены **только перед declarations** (`fn` / `type` / `module`).

**НЕ** разрешены:
- Перед `let` / `const` внутри тела функции.
- Перед expressions внутри тела.
- Inner-attributes (`#![...]` в Rust) — **не вводим**; для module-level
  директив есть keyword'ы (`module`, `import`).

#### Префикс `#` (не `@`)

Префикс `@` уже занят в Nova для другого: **receiver/self-доступ**
в методах ([D35](03-syntax.md#d35)):

| Контекст | Синтаксис | Семантика |
|---|---|---|
| Method-declaration | `fn Account @balance()` | `@` = «instance-метод» |
| Self-field access | `@_balance`, `@owner` | `@` = self.field |
| Self-bare reference | `=> @` (в методе) | `@` = сам receiver |
| **Attribute** | `#realtime`, `#pure` | `#` = модификатор declaration |

Использование одного `@` для receiver-access И для attributes даёт
dual-use символа, что создаёт когнитивную нагрузку и потенциал для
ошибок LLM при генерации кода. Префикс `#` — свободен, не использовался
в Nova (комментарии только `//`).

### Почему

#### AI-first связь

Один символ = одна семантика. LLM, читая `@something`, не должен
гадать «attribute или self-access». `#` для attributes, `@` для self —
прозрачное разделение.

#### Прецеденты

| Язык | Прост. атрибут | С args |
|---|---|---|
| Java | `@Override` | `@SuppressWarnings("...")` |
| Kotlin | `@Composable` | `@JvmName("foo")` |
| Python | `@property` | `@dataclass(frozen=True)` |
| Scala | `@inline` | `@deprecated("msg", "1.0")` |
| C# | `[Obsolete]` | `[Obsolete("msg")]` |
| Rust | `#[derive]` | `#[derive(Debug)]` |
| **Nova** | **`#pure`** | **`#verify_timeout(5000)`** |

**Большинство mainstream языков** используют префикс **без обязательных
скобок** — скобки появляются только когда есть аргументы. Rust с
`#[...]` — исключение, обусловленное необходимостью inner attributes
`#![...]`, proc-macros (token tree forwarding) и атрибутов на
expressions. У Nova ни одной из этих причин нет.

#### Почему не `#[name]` (Rust-стиль)

Скобки `#[...]` в Rust обоснованы тремя историческими факторами,
которые **не применимы** к Nova:

1. **Proc-macros с произвольным token tree.** `#[serde(rename = "x")]`
   передаётся в proc-macro как сырой token stream. Nova **не имеет
   proc-macros** (см. [revolutionary.md](../revolutionary.md): «no macro
   AST-rewriting»); комптайм-метапрограммирование делается через
   typed `comptime`, не через rewriting.

2. **Inner attributes `#![...]`** для модуля / crate-level директив.
   У Nova module declared через keyword `module a.b.c`, никаких inner
   attributes не нужно.

3. **Атрибуты на expressions** (`vec![#[cfg(unix)] 1, 2, 3]`). У Nova
   атрибуты только на declarations — это явное design-ограничение.

С круглыми скобками для arguments (`#name(args)`) парсер однозначно
разрешает в declaration-position. Это работает в Java/Kotlin/Python/Scala
уже десятилетиями.

### Что отвергнуто

- **`@name` для attributes** — конфликт с receiver-prefix `@field`.
  Dual-use символа = плохо для AI-first языка.
- **`#[name]` (Rust-стиль)** — скобки избыточны без proc-macros, inner
  attributes или атрибутов на expressions. Карго-культ к Rust без
  понимания причин.
- **Keyword-форма (`pure fn`)** — ломает существующий синтаксис
  ([D64 @realtime](#d64-realtime-блок); breaking change для каждого
  атрибута); композиция `must_verify pure fn` читается странно
  (два keyword'а подряд).
- **`name fn` (modifier-keyword без префикса)** — конфликт с обычными
  идентификаторами; нужны reserved words для каждого атрибута.

### Цена

1. **Миграция `@realtime` → `#realtime`** (Plan 16 уже использовал
   `@realtime`). На момент D96 — 5 .nv-файлов в repo. Breaking change,
   но Nova не в production (см. `feedback_revolutionary_changes`).
2. **Документация:** все примеры в spec/, docs/, README обновляются.
3. **Будущее расширение:** если когда-либо понадобятся proc-macros
   или inner attributes — добавляется через **отдельный D-decision**
   (например `##name` для inner) без breaking change для `#name` outer.

### Связь

- [D24](#d24-стратегия-smt-проверки-контрактов) — `#must_verify`,
  `#unverified`, `#verify_timeout(N)`, `#pure` атрибуты для контрактов.
- [D64](04-effects.md) — `#realtime` / `#realtime nogc` атрибут.
- [D62](04-effects.md) — `#allow_transit(Effects...)` атрибут для
  transit-effect warning suppression.
- [revolutionary.md](../revolutionary.md) — «no macro AST-rewriting»
  как philosophy reason против Rust-style `#[...]` token tree.

### Используется в

- Plan 16 Ф.5 — `#realtime` / `#realtime nogc`.
- Plan 33.1 — `#must_verify`, `#unverified`, `#verify_timeout(N)`, `#pure`.
- Plan 33.3 — `#verify_handler`, `#trusted`, `#must_verify_module`.

---

## D105. Doc attributes

> **Status:** active (spec). Implementation — [Plan 45](../../docs/plans/45-nova-doc.md) Ф.3.
>
> **Builds on:** [D96](#d96-синтаксис-атрибутов--name-без-квадратных-скобок) (`#name` attribute syntax), [D101](07-modules.md#d101-doc-module-attr) (`#doc "..."` module-attr).
>
> **Namespace:** `#doc(...)` shares its prefix with D101's `#doc "string"` but the two forms are syntactically distinct (string literal vs. parenthesised key-value list) and never collide. See "Namespace" below.

### What

A fixed set of attributes that decorate items with documentation
metadata. Tooling (`nova doc`, type-checker lints) reads them;
runtime ignores them.

The catalog (for Plan 45 MVP):

| Attribute | Targets | Purpose |
|---|---|---|
| `#deprecated(since = "X", note = "...", until = "Y"?)` | item | Marks the item as deprecated; lint on use-sites. |
| `#since("X.Y")` | item | Records introduction version (informational). |
| `#stable(since = "X.Y"?)` | item, module | Marks item as stable API. |
| `#unstable(feature = "name")` | item, module | Marks item as unstable behind a named feature gate. |
| `#experimental(note = "..."?)` | item, module | Marks item as proof-of-concept; expect breakage. |
| `#hide_doc` | item | Item is exported, but hidden from `nova doc` output. |
| `#doc_alias("alt-name", ...)` | item | Search aliases for the item (HTML/JSON search index). |
| `#doc(inline)` / `#doc(no_inline)` | re-export item | Render the re-exported target inline at the re-export site (`inline`) or only show a link (`no_inline`). |
| `#doc(summary = "...")` | item | Override the automatic first-sentence summary extraction. |
| `#doc(section = "Name")` | item | Place item in a custom section in module rendering (advanced; opt-in MVP). |
| `#doc(test_handlers = "path.to.handlers")` | module, item | Register handlers automatically wired into doc-tests for this item/module. |

### Syntax

All doc attributes use D96 `#name(...)` form. Bare `#stable`,
`#unstable`, `#experimental`, `#hide_doc` are valid without
arguments; their key-value form takes the listed parameters.

```nova
#deprecated(since = "0.4.0", note = "use [open_buffered] instead")
fn open(path str) Net -> File => ...

#stable(since = "1.0.0")
type Connection { ... }

#unstable(feature = "channel_select")
fn select_or_default[T](chs []ChanReader[T]) -> T = ...

#hide_doc
fn internal_helper() -> int => 42

#doc_alias("malloc", "alloc")
fn allocate(n int) -> []byte = ...

#doc(inline)
export import std.collections.range.{Range}

#doc(summary = "Compute SHA-256 hash of the input bytes.")
fn sha256(data []byte) -> [32]byte = ...
```

### Semantics

#### `#deprecated`

- **Required parameters:** `since` (string, version of Nova or
  package introducing the deprecation) **and** `note` (string with
  the migration guidance).
- **Optional:** `until` (string, version of planned removal). When
  present, enables `--deny-overdue-deprecations` CI gate in `nova
  doc --check`.
- **Effect:**
  - `nova check` / `nova test` / `nova build` emit a `deprecated`
    warning at every use-site (file:line + the `note`).
  - `nova doc` renders the deprecation banner and includes `since`,
    `until`, and `note` in JSON output.
- The `note` field SHOULD include an intra-doc link to the
  replacement (`note = "use [foo.bar] instead"`); lint
  `deprecated-without-link` warns when missing.

#### `#since(version)`

- Records the version in which the item was introduced.
- Used by `--since <version>` filter (Plan 45 Ф.12) for changelog
  generation.
- Does not produce diagnostics; purely informational.

#### `#stable` / `#unstable` / `#experimental` (stability tiers)

Three mutually-exclusive tiers. An item may carry at most one.

- `#stable(since = "...")` — committed API. `since` recommended,
  default `unknown`.
- `#unstable(feature = "name")` — opt-in via feature flag at build
  time (Plan 42.12 `#cfg(feature = "name")` precedent). Use-site
  outside `#cfg(feature = "name")` scope is a hard error.
- `#experimental(note = "...")` — proof-of-concept. Use-sites emit a
  warning. `note` SHOULD describe what may change.

**Propagation:** A module-level stability tier propagates to items
in the module that have no explicit tier (via the
`propagate_stability` pass; Plan 45 §3). Items with explicit tiers
override.

#### `#hide_doc`

- Item is **really exported** (visible to `import` consumers) but is
  **not rendered** by `nova doc`.
- Use case: items kept exported for backward compatibility that
  shouldn't be promoted in new docs, or internal helpers exposed for
  testing.
- Has no runtime effect; only the `nova doc` collector skips the item.

#### `#doc_alias("name", "name", ...)`

- Alternative names for search indices.
- Example: `#doc_alias("malloc")` on `fn allocate` makes a search
  for "malloc" find `allocate`.
- Each alias is a string literal; no transformations.
- Plan 45 MVP: aliases appear in JSON output; HTML search index
  consumption is Plan 45.A.

#### `#doc(inline)` / `#doc(no_inline)`

- Controls rendering of re-exports.
- `#doc(inline)` (default for same-package re-exports): the
  re-exported item is rendered at the re-export site with the
  same docs as the original.
- `#doc(no_inline)` (default for cross-package re-exports): a short
  "re-export of `path.to.original`" stub is rendered with a link.

#### `#doc(summary = "...")`

- Overrides the automatic first-sentence summary extraction.
- Plain string; no markdown beyond inline code (backtick code) and
  intra-doc links.
- Use when the first sentence of the doc body is not a good summary
  (e.g. begins with a setup clause).

#### `#doc(section = "Name")`

- Places the item in a custom section in module rendering.
- Default sections (`Functions`, `Types`, `Constants`, ...) are
  recognized; this attribute creates a sub-section under the
  appropriate kind heading.
- **Plan 45 MVP:** recognized in parser, ignored in rendering (the
  item is placed in the default section). Full rendering — Plan 45.A.

#### `#doc(test_handlers = "path.to.handlers")`

- Module-level or item-level.
- When present, doc-tests in scope are automatically wrapped with
  `with handler from <path> { ... }`. The path resolves like an
  import.
- Removes the need for hidden setup lines in every doc-test.
- Cross-references [D106](#d106-doc-test-semantics) for doc-test
  semantics.

### Namespace

The `#doc` prefix is shared with [D101](07-modules.md#d101-doc-module-attr)'s
`#doc "string-literal"` form. The two are distinguished by the
**first token after `doc`**:

- `#doc "..."` (string literal) — D101 module-doc attribute.
- `#doc(...)` (parenthesised key-value list) — D105 typed attribute.
- `#doc_alias(...)` (underscore in name) — D105 catalog member.
- `#doc_*` reserved for future D105 attributes (e.g. `#doc_section`).

Parser disambiguates by lookahead at the next token after the
`doc` identifier:
- `STRING_LIT` → D101.
- `LPAREN` → D105.
- `_<ident>` → D105 named member.
- anything else → syntax error.

### Why

1. **Catalog (not free-form tag soup)** — Go, Rust, and TypeScript
   doc tools all suffered from convention drift: `@param` vs
   `@parameter` vs nothing in TSDoc; `Deprecated:` prose vs
   `#[deprecated]` attribute in Go vs Rust. Fixing a small, named
   catalog at the language level prevents this. Adding new
   attributes requires a new D-decision.
2. **Typed parameters** — `#deprecated(since, note, until)` has
   structured fields available in JSON output. LLM consumers can
   read `since` numerically; "Deprecated: use foo instead." in a
   free-form comment is opaque.
3. **`#hide_doc` is opt-out, not opt-in** — Rust's `#[doc(hidden)]`
   is opt-out, mirroring its `pub`-by-default. Nova is private-by-
   default ([D5](04-effects.md#d5)), so `export` is opt-in. Hiding
   an export from docs is a separate opt-out — this matches the
   private-by-default mental model.
4. **`until` field for `#deprecated`** — neither Rust nor Go has it.
   Yet "we're removing this in 1.0" is a real lifecycle stage. With
   `until`, `nova doc --deny-overdue-deprecations` becomes a CI
   gate against forgetting to delete.

### Что отвергнуто

- **JSDoc-style `@param` / `@returns` tags** — Nova has typed
  parameters and return types in the signature; documenting them
  again in prose duplicates info and drifts. Style guide
  ([Plan 45 §11.5](../../docs/plans/45-nova-doc.md#115-doc-comment-style-guide))
  recommends inline mention in the description.
- **`#[doc = "raw text"]` alternative form** (Rust precedent) —
  the `///` form is sufficient; raw text in attributes is for code
  generators (macros) which Nova does not have. Reconsider if
  metaprogramming is added.
- **Multi-tier stability beyond three** (Rust has many flavours of
  unstable) — three tiers (`stable`/`unstable`/`experimental`) cover
  the use cases without complexity.
- **User-defined doc attributes** — opens the catalog to arbitrary
  tags, fracturing convention. Catalog grows only via D-decisions.

### Связь

- [D96](#d96-синтаксис-атрибутов--name-без-квадратных-скобок) —
  `#name(...)` syntax foundation.
- [D101](07-modules.md#d101-doc-module-attr) — `#doc "..."` module
  attr; namespace coexists.
- [D104](03-syntax.md#d104-doc-comment-syntax--outer--inner) —
  `///`/`//!` doc-comment lexer recognition.
- [D106](#d106-doc-test-semantics) — `#doc(test_handlers)` referenced.
- [Plan 45](../../docs/plans/45-nova-doc.md) Ф.3 implementation,
  §11.5 style guide.

---

## D106. Doc-test semantics

> **Status:** active (spec). Implementation — [Plan 45](../../docs/plans/45-nova-doc.md) Ф.7.
>
> **Reuses:** [D89](#d89-test-tooling-конвенции--expect_-маркеры-для-negative-тестов) (EXPECT-markers); [Plan 24](../../docs/plans/24-cross-platform-test-runner.md) (test_runner). Doc-tests are compiled and run through the same pipeline as `*_test.nv` files.

### What

A code block inside a doc-comment is a **doc-test** if it:

- Is fenced with triple backticks.
- Has language tag `nova`, or has no language tag at all (default).

```nova
/// Returns true if `x` is even.
///
/// # Examples
///
/// (triple-backtick fenced block here — code goes inside)
/// assert(is_even(2))
/// assert(!is_even(3))
fn is_even(x int) -> bool => x % 2 == 0
```

The above contains one doc-test. The test runner extracts it,
compiles it as a self-contained module, and runs the assertions.

### Code-block modifiers

The language tag may be followed by zero or more comma-separated
modifiers, written immediately after the language tag in the
fence-opener line.

**Catalogue (MVP):**

| Modifier | Effect |
|---|---|
| `no_run` | Compile only; do not execute. |
| `ignore` | Skip entirely (do not compile, do not execute). |
| `compile_fail` | The code MUST NOT compile. If it compiles, the doc-test fails. |
| `should_panic` | Code MUST compile AND panic at runtime. Non-panic exit fails. |
| `must_verify` | Contract verification (`#must_verify` per [D24](#d24-стратегия-smt-проверки-контрактов) / Plan 33) MUST succeed. Failed verification (UNSAT, TIMEOUT) fails the doc-test. |

Multiple modifiers compose where sensible (`no_run,must_verify`
means "verify but do not execute"). Conflicting combinations
(`compile_fail,should_panic`) are a configuration error.

### Hidden lines

A doc-test line beginning with `# ` (hash + space) is **hidden** in
the rendered output but **compiled and executed** as part of the
test. Used for setup that would clutter examples (imports, helper
definitions, etc.).

### Privacy

Doc-tests have **module-private access** to the item they document.
A doc-test on `export fn foo` (in `std.collections.range`) may call
non-exported helpers within `std.collections.range`. This matches
rustdoc behaviour and reflects the principle that examples
demonstrate using the item from a same-module perspective.

Cross-module doc-tests on re-exports retain the **original module's**
privacy scope (the module that defined the item), not the
re-exporter's.

### Setup via `#doc(test_handlers)`

[D105](#d105-doc-attributes) defines a `#doc(test_handlers = "path")`
attribute. When applied to a module or item, all doc-tests in scope
are implicitly wrapped:

```nova
with handler from path.to.handlers {
    ... doc-test body ...
}
```

This removes boilerplate for common setups (test-handler stacks,
mock filesystems, etc.).

A folder-module peer file named `_doctest_setup.nv` (Plan 42 folder-
module convention) is also implicitly imported into doc-test scope
when present. Both mechanisms are additive.

### Compilation model

Each doc-test is compiled as a synthetic module:

```
module __nova_doc_test_<hash>

import <enclosing-module>.*

test "<item-name> example <index>" {
    <hidden-lines + visible-lines>
}
```

- The hash is a deterministic function of (item-path, doc-test-index).
- The test name is `<item-name> example <N>` (1-indexed).
- Imports from the enclosing module are wildcard-style (peers visible).

Compilation reuses the standard pipeline (parser → type-checker →
codegen / interp). Failures route the same way as regular test
failures.

### Execution

Doc-tests run through the same `test_runner` as regular tests
([Plan 24](../../docs/plans/24-cross-platform-test-runner.md)).
Parallelism (`--jobs N`), output format, and exit codes are
identical.

`nova doc --check` runs doc-tests by default; `--no-doc-tests`
disables. `nova test` does **not** run doc-tests by default
(doc-tests are owned by `nova doc`); `nova test --doc-tests` opts in.

Exit codes per [D95](#d95-cli-path-конвенции--nova-check-path--nova-test-path):
- 0 — all doc-tests passed.
- 1 — at least one failed.
- 2 — usage error.
- 101 — internal panic.

### EXPECT marker integration

The `compile_fail` and `should_panic` modifiers are syntactic sugar
that translate to [D89](#d89-test-tooling-конвенции--expect_-маркеры-для-negative-тестов)
EXPECT-markers inserted into the synthetic test file:

| Modifier | Synthesized EXPECT |
|---|---|
| `compile_fail` | `// EXPECT_COMPILE_ERROR` |
| `should_panic` | `// EXPECT_RUNTIME_PANIC` |
| `must_verify` | `// REQUIRES_SMT_BACKEND` + verify-check on `#must_verify` items |

This reuses existing test_runner infrastructure; no new failure-mode
machinery is needed.

### Why

1. **Doc-tests adjacent to documented items** — Go's `Example*`
   functions in `*_test.go` (golang/go #16851) drift from the
   documented item. Inline doc-tests are co-located with what they
   document; renaming an item adjacent in the same file moves the
   tests with it.
2. **`compile_fail` / `should_panic` first-class** — rustdoc
   precedent. Documenting "this should fail" is valuable; making the
   tool verify it removes a class of stale-example bugs.
3. **`must_verify` — Nova-unique** — leverages Plan 33 SMT
   verification. A doc-comment can demonstrate that a function
   satisfies its contracts under all inputs, not just one example.
4. **Hidden setup via `# `** — accepted compromise: too verbose to
   show every import; `#doc(test_handlers)` and `_doctest_setup.nv`
   handle the common cases without per-test boilerplate.

### Что отвергнуто

- **Markdown-link-style references to external example files** — a
  doc-test that lives in `examples/foo.nv` adds indirection,
  loses co-location. Inline is the canonical form.
- **`run_only_if_feature("name")` modifier** — duplicates
  `#cfg(feature = ...)` (Plan 42.12). If the documented item is
  feature-gated, the test inherits the gate.
- **`expected_output = "..."` modifier for stdout comparison** —
  asserts inside the test are more flexible. If the user wants
  stdout matching, [D89](#d89-test-tooling-конвенции--expect_-маркеры-для-negative-тестов)
  `EXPECT_STDOUT` is available via hidden line.
- **Doc-test isolation containers (process-per-test)** — overhead
  too high; `test_runner` already isolates state per test via
  fresh module instance.

### Связь

- [D24](#d24-стратегия-smt-проверки-контрактов) — `must_verify`
  modifier ties to SMT verification.
- [D89](#d89-test-tooling-конвенции--expect_-маркеры-для-negative-тестов)
  — EXPECT-markers reused.
- [D95](#d95-cli-path-конвенции--nova-check-path--nova-test-path) —
  CLI exit codes.
- [D104](03-syntax.md#d104-doc-comment-syntax--outer--inner) —
  fenced code blocks inside doc-comments.
- [D105](#d105-doc-attributes) — `#doc(test_handlers)`.
- [Plan 24](../../docs/plans/24-cross-platform-test-runner.md) —
  test_runner reuse.
- [Plan 33](../../docs/plans/33-contracts-implementation.md) —
  contracts for `must_verify`.
- [Plan 42](../../docs/plans/42-folder-modules.md) —
  `_doctest_setup.nv` folder-module peer.
- [Plan 45](../../docs/plans/45-nova-doc.md) Ф.7 implementation.

---

## D107. JSON output schema v1

> **Status:** active (spec). Implementation — [Plan 45](../../docs/plans/45-nova-doc.md) Ф.9.
>
> **Note on stability state:** v1 ships marked **`mvp-stable`** —
> additive minor changes only, no breaking. After ≥1 milestone of real
> use (Plan 45.B stdlib doc-pass + ≥3 external AI consumers), the
> stability is promoted to **`stable`**. The promotion is a separate
> spec revision of this D-decision.

### What

`nova doc --format json` produces a JSON document describing the
public API surface of a module (or workspace). The document conforms
to a versioned schema (`format_version: u32`); consumers MUST check
the version before parsing.

The schema is **embedded** in the compiler binary as JSON Schema
2020-12 and emitted by `nova doc --json-schema`.

### Top-level shape

```json
{
  "format_version": 1,
  "nova_version": "0.1.0",
  "generated_at": "2026-05-15T12:34:56Z",
  "source_root": "/path/to/repo",
  "modules": [ ... Module ... ],
  "items": [ ... Item ... ],
  "links": [ ... Link ... ],
  "doc_tests": [ ... DocTest ... ]
}
```

**Required top-level fields:**

- `format_version` (`u32`) — schema major version. Consumers MUST
  fail loudly when encountering an unrecognised version.
- `nova_version` (`string`, semver) — compiler version that emitted
  the document. Informational; not a stability contract.
- `generated_at` (`string`, RFC 3339 UTC) — emission timestamp. May
  be elided in reproducible-build mode (`SOURCE_DATE_EPOCH`).
- `modules` (`array<Module>`) — every module documented in this
  document (entry plus transitive imports if `--workspace`).
- `items` (`array<Item>`) — flat list of all items; `module_path`
  field disambiguates ownership.
- `links` (`array<Link>`) — resolved intra-doc links from items in
  this document.
- `doc_tests` (`array<DocTest>`) — extracted (and optionally run)
  doc-tests with their status.

**Optional top-level fields:**

- `source_root` (`string`, absolute path) — repository root. Omitted
  when source paths are anonymised (`--anonymize-paths`, a future
  flag).

### `Module` shape

```json
{
  "path": "std.collections.range",
  "name": "range",
  "kind": "folder",
  "peers": ["range.nv", "range_test.nv"],
  "summary": "Inclusive/exclusive integer ranges.",
  "description": "Markdown text...",
  "stability": { "tier": "stable", "since": "1.0.0" },
  "deprecation": null,
  "doc_attrs": [ ],
  "source": { "file_id": 12, "line": 1 }
}
```

- `path` — dotted module path.
- `name` — last segment of `path`.
- `kind` — `folder` for folder-modules, `file` for single-file.
- `peers` — relative paths to peer files (only for `folder` kind);
  empty for `file`.
- `summary` — first sentence extracted from `//!` doc and `#doc`
  module-attr.
- `description` — full markdown body.
- `stability` — `{ tier: "stable" | "unstable" | "experimental",
  since: "..."?, feature: "..."?, note: "..."? }` or `null` for
  unknown tier.
- `deprecation` — `{ since, note, until? }` or `null`.
- `doc_attrs` — other doc-attributes (per [D105](#d105-doc-attributes))
  that do not have a structured top-level field.
- `source` — `{ file_id, line }` for "View Source" links.

### `Item` shape

Items are tagged unions. All items share a common header:

```json
{
  "id": "std.collections.range::Range",
  "module_path": "std.collections.range",
  "name": "Range",
  "kind": "fn",
  "summary": "...",
  "description": "...",
  "sections": { "examples": "...", "errors": "..." },
  "stability": { "tier": "stable" },
  "deprecation": null,
  "doc_attrs": [ ],
  "source": { "file_id": 12, "line": 42 },
  "signature": { }
}
```

`id` is a stable identifier: `<module_path>::<name>` for free items;
`<module_path>::<TypeName>.<method>` for methods. IDs are the
**canonical link target**.

The `sections` object contains parsed standardised sections
(`# Examples`, `# Errors`, etc.) as markdown strings keyed by
lowercase section name.

**Kind-specific:**

- `kind: "fn"` — `signature` (params, return type, effect-row, raises,
  generics, contracts).
- `kind: "type"` — `definition` (Record | Sum | Alias | Protocol |
  Effect) with `fields` / `variants` / etc.
- `kind: "const"` — `type`, `value` (rendered as Nova source).
- `kind: "effect"` — `methods` array (effect-op signatures), `axioms`
  (Plan 33.3 D24 `axiom` clauses).
- `kind: "handler"` — `effect` (resolved id), `is_default` flag.
- `kind: "protocol"` — `methods` (required-method signatures),
  `implementors` (resolved item ids).

### `Signature` shape (for `fn` items)

```json
{
  "params": [
    { "name": "x", "type": "int", "default": null },
    { "name": "port", "type": "int", "default": "8080", "keyword_only": true }
  ],
  "return_type": "int",
  "effects": ["Net", "Db"],
  "raises": ["NotFound", "Timeout"],
  "generics": [
    { "name": "T", "bound": "Hashable", "default": null }
  ],
  "receiver": null,
  "contracts": {
    "requires": ["x > 0"],
    "ensures": ["result >= x"],
    "verify_status": "PROVEN"
  }
}
```

- `type` fields are **rendered as Nova source** (strings), not
  structural ASTs. This is intentional: consumers needing structure
  may parse them with the same parser. Rendering as strings keeps
  the JSON output portable and human-readable.
- `keyword_only: true` is set when the parameter has a `default` per
  [D102](03-syntax.md#d102-именованные-аргументы-и-значения-параметров-по-умолчанию).
- `effects` list is the effect-row (set-typed, ordered
  alphabetically for determinism).
- `raises` is the union of `Fail[X]` variants from the effect-row.
- `receiver` is non-null for instance/static methods:
  `{ "type": "Box", "kind": "instance", "mutable": false }`.
- `contracts.verify_status` — one of `PROVEN | UNVERIFIED | TIMEOUT | TRUSTED`.

### `Link` shape

```json
{
  "from": "std.collections.range::Range.map",
  "to": "std.collections.iter::Iter.map",
  "kind": "fn",
  "resolved": true,
  "source_span": { "file_id": 12, "line": 45, "col": 10 }
}
```

Records every intra-doc link discovered in this document. When
`resolved: false`, the link target was unresolvable (broken link).

### `DocTest` shape

```json
{
  "id": "std.collections.range::Range.map::doc_0",
  "item_id": "std.collections.range::Range.map",
  "lang": "nova",
  "modifiers": ["no_run"],
  "code": "...",
  "code_visible": "...",
  "source_span": { "file_id": 12, "line": 67 },
  "status": "passed",
  "duration_ms": 12,
  "failure": null
}
```

- `id` — deterministic: `<item_id>::doc_<index>` (0-indexed).
- `code` — full code including hidden setup lines.
- `code_visible` — code excluding hidden lines (for HTML/Markdown
  rendering).
- `status` — one of `passed | failed | skipped | not_run`.
- `failure` — `null` on success; otherwise `{ kind, message }` where
  `kind` is one of `compile_error | runtime_panic | verification_failure`.
- `status: "not_run"` — `--no-doc-tests` was passed; only extracted,
  not executed.

### Deterministic output

Producers MUST emit the JSON deterministically:

- Object keys sorted alphabetically.
- Arrays in stable order: modules and items sorted by `path`/`id`;
  links sorted by `from` then `to`; doc_tests sorted by `id`.
- `generated_at` field elided when `SOURCE_DATE_EPOCH` is set in the
  environment.

Tests in Plan 45 Ф.19 verify byte-identical output across two
consecutive runs.

### Stability rules

See [Plan 45 §6](../../docs/plans/45-nova-doc.md#6-json-schema-v1-контракт)
for the full versioning policy. Summary:

- **Additive minor changes** (do not bump `format_version`):
  - New optional top-level or nested fields.
  - New enum variants in fields documented as "extensible".
  - New `kind`-specific Item fields (consumers must default-skip).
- **Breaking changes** (bump `format_version`):
  - Remove or rename a field.
  - Change a field's type or semantics.
  - Narrow an enum (remove a variant).

`format_version=N` and `format_version=N+1` are supported in parallel
for ≥1 stable release of the compiler. Consumers are encouraged to
fail loudly on unrecognised major versions.

### `nova-doc-types` consumer crate

A separate Rust crate `nova-doc-types` provides typed bindings to
the schema:

```rust
// nova-doc-types = "1.x" — version-locked with format_version=1.
use nova_doc_types::{Document, Item, ItemKind};

let doc: Document = serde_json::from_str(&json_input)?;
```

Mirrors rustdoc's `rustdoc-types` crate. Versioning is parallel to
`format_version`: major bumps lock-step.

### Embedded JSON Schema

`nova doc --json-schema` emits the schema as a JSON document
conforming to JSON Schema 2020-12. This enables:

- Offline validation in CI gates.
- IDE auto-completion in editors that consume JSON Schema.
- LLM tool-use prompt context.

The schema is **embedded in the compiler binary** (`include_str!`).
Versions of the schema are immutable per `format_version`; the binary
ships exactly one (the current major).

### Why

1. **Stable JSON as a first-class output** — godoc has none, rustdoc
   has unstable nightly-only, TypeDoc has unstable. Nova ships a
   stable schema **on stable builds** from MVP day one. AI/LSP
   consumers can rely on it.
2. **`format_version` integer, not semver string** — checks are
   simpler (`>= 1 && <= 1` per consumer), parser is simpler. SemVer
   semantics are baked into the additive-minor / breaking-major
   rule above without exposing the version string complexity.
3. **String-rendered types vs structural ASTs** — exposing the full
   structural AST in JSON would couple consumers to internal Nova
   type representations. String rendering is portable (any
   consumer can read it) and stable (parser changes do not break the
   JSON shape, only the contents of rendered strings change in step
   with the language).
4. **Sorted, deterministic output** — required for `--diff`
   (Plan 45.A) and reproducible builds. Without it, doc-as-CI-gate
   produces spurious diffs.
5. **Embedded schema** — offline validation without network. CI
   gates can run on air-gapped builders.

### Что отвергнуто

- **Per-module JSON files (one file per module)** — Plan 45 emits a
  single document by default. Per-module files create discovery
  problems (must list directories, no global cross-references).
  Future enhancement may add `--split-by-module` for very large
  workspaces.
- **GraphQL endpoint instead of JSON file** — server overhead for a
  CLI tool. JSON document is consumer-agnostic.
- **Protocol Buffers / MessagePack** — JSON is the lowest common
  denominator for AI/LSP/CI tooling. Binary formats added later if
  proven needed; JSON is the canonical contract.
- **Embedded full source** — bloats output and duplicates work. The
  consumer-side tool can resolve `source.file_id` if it has source
  access.

### Связь

- [D89](#d89-test-tooling-конвенции--expect_-маркеры-для-negative-тестов)
  — EXPECT-markers translated into `DocTest.failure.kind`.
- [D95](#d95-cli-path-конвенции--nova-check-path--nova-test-path) —
  CLI conventions for `nova doc --format json`.
- [D104](03-syntax.md#d104-doc-comment-syntax--outer--inner) — source
  of doc content.
- [D105](#d105-doc-attributes) — attribute metadata fields.
- [D106](#d106-doc-test-semantics) — DocTest shape source.
- [Plan 45](../../docs/plans/45-nova-doc.md) §6, §6.5 — versioning
  policy; Ф.9 implementation.
