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
| [D111](#d111-assume--assert_static--trusted-external) | `assume` / `assert_static` / `#trusted` external |
| [D112](#d112-bounded-quantifiers-forallexists-по-коллекции) | Bounded quantifiers (`forall`/`exists` по коллекции) |
| [D113](#d113-must_verify_module--strict-mode-на-модуле) | `#must_verify_module` — strict mode на модуле |
| [D114](#d114-smt-cache--parallel-verification) | SMT cache + parallel verification |
| [D116](#d116-z3-backend-через-собственные-ffi-биндинги) | Z3 backend через собственные FFI-биндинги |

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

## D105. Doc-атрибуты

> **Status:** active (spec). Реализация — [Plan 45](../../docs/plans/45-nova-doc.md) Ф.3.
>
> **Опирается на:** [D96](#d96-синтаксис-атрибутов--name-без-квадратных-скобок) (синтаксис атрибутов `#name`), [D101](07-modules.md#d101-doc-module-attr) (module-attr `#doc "..."`).
>
> **Namespace:** `#doc(...)` делит префикс с D101 `#doc "string"`, но эти две формы синтаксически различны (string literal vs. parenthesised key-value list) и не коллидируют. См. подсекцию «Namespace» ниже.

### Что

Фиксированный набор атрибутов, декорирующих items документационной
метаинформацией. Tooling (`nova doc`, type-checker lint'ы) читает их;
runtime — игнорирует.

Каталог для Plan 45 MVP:

| Атрибут | Targets | Назначение |
|---|---|---|
| `#deprecated(since = "X", note = "...", until = "Y"?)` | item | Помечает item как deprecated; lint на use-сайтах. |
| `#since("X.Y")` | item | Записывает версию появления (информационно). |
| `#stable(since = "X.Y"?)` | item, module | Stable API. |
| `#unstable(feature = "name")` | item, module | Unstable за named feature-флагом. |
| `#experimental(note = "..."?)` | item, module | Proof-of-concept; ожидайте breaking changes. |
| `#hide_doc` | item | Item exported, но скрыт из `nova doc` output'а. |
| `#doc_alias("alt-name", ...)` | item | Search-aliases (HTML/JSON search index). |
| `#doc(inline)` / `#doc(no_inline)` | re-export item | Рендерить re-exported target inline у re-export site (`inline`) либо только ссылкой (`no_inline`). |
| `#doc(summary = "...")` | item | Override автоматического first-sentence summary. |
| `#doc(section = "Name")` | item | Поместить item в custom section module rendering (advanced; opt-in MVP). |
| `#doc(test_handlers = "path.to.handlers")` | module, item | Зарегистрировать handler'ы, автоматически wrap'ируемые вокруг doc-test'ов. |

### Синтаксис

Все doc-атрибуты используют форму D96 `#name(...)`. Голые `#stable`,
`#unstable`, `#experimental`, `#hide_doc` валидны без аргументов; их
key-value форма принимает перечисленные параметры.

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

### Семантика

#### `#deprecated`

- **Обязательные параметры:** `since` (string, версия Nova/пакета,
  вводящая deprecation) **и** `note` (string с migration guidance).
- **Опциональный:** `until` (string, версия планируемого удаления). При
  присутствии — включает CI-gate `--deny-overdue-deprecations` в
  `nova doc --check`.
- **Эффект:**
  - `nova check` / `nova test` / `nova build` эмитят warning
    `deprecated` на каждом use-сайте (file:line + `note`).
  - `nova doc` рендерит deprecation-баннер и включает `since`, `until`,
    `note` в JSON output.
- Поле `note` ДОЛЖНО содержать intra-doc link на замену
  (`note = "use [foo.bar] instead"`); lint `deprecated-without-link`
  warning'ит при отсутствии.

#### `#since(version)`

- Записывает версию появления item'а.
- Используется filter'ом `--since <version>` (Plan 45 Ф.12) для
  changelog-генерации.
- Diagnostics не производит; чисто информационный.

#### `#stable` / `#unstable` / `#experimental` (stability tiers)

Три mutually-exclusive tier'а. Item может нести **не более одного**.

- `#stable(since = "...")` — committed API. `since` рекомендован;
  default `unknown`.
- `#unstable(feature = "name")` — opt-in через feature-флаг на этапе
  билда (Plan 42.12 `#cfg(feature = "name")`-precedent). Use-сайт вне
  `#cfg(feature = "name")`-скоупа — hard error.
- `#experimental(note = "...")` — proof-of-concept. Use-сайты эмитят
  warning. `note` ДОЛЖЕН описывать, что может измениться.

**Propagation:** module-level stability tier пропагируется на items
модуля без явного tier'а (через pass `propagate_stability`; Plan 45
§3). Item'ы с явным tier override.

#### `#hide_doc`

- Item **реально exported** (виден `import`-consumer'ам), но **не
  рендерится** через `nova doc`.
- Use case: items, оставшиеся exported для backward compat, которые
  не должны промоутиться в новой документации; internal helpers,
  открытые для testing.
- Runtime-эффекта нет; только `nova doc` collector пропускает item.

#### `#doc_alias("name", "name", ...)`

- Альтернативные имена для search index'ов.
- Пример: `#doc_alias("malloc")` на `fn allocate` — поиск "malloc"
  найдёт `allocate`.
- Каждый alias — string literal; никаких трансформаций.
- Plan 45 MVP: aliases появляются в JSON output; consumption в HTML
  search index — Plan 45.A.

#### `#doc(inline)` / `#doc(no_inline)`

- Контролирует рендеринг re-export'ов.
- `#doc(inline)` (default для same-package re-export'ов): re-exported
  item рендерится у re-export-сайта с теми же docs, что и оригинал.
- `#doc(no_inline)` (default для cross-package re-export'ов):
  короткий стаб «re-export of `path.to.original`» с link'ом.

#### `#doc(summary = "...")`

- Override automatic first-sentence summary extraction.
- Plain string; markdown — только inline-code (через backtick) и
  intra-doc links.
- Используется, когда первое предложение doc-body — не лучший
  summary (например, начинается с setup-clause).

#### `#doc(section = "Name")`

- Помещает item в custom section module rendering.
- Default-секции (`Functions`, `Types`, `Constants`, ...) узнаются;
  этот атрибут создаёт sub-section под соответствующим kind-heading.
- **Plan 45 MVP:** распознаётся parser'ом, игнорируется в рендеринге
  (item помещается в default-секцию). Полный рендеринг — Plan 45.A.

#### `#doc(test_handlers = "path.to.handlers")`

- Module-level или item-level.
- При присутствии все doc-test'ы в scope'е автоматически
  оборачиваются в `with handler from <path> { ... }`. Path резолвится
  как import.
- Снимает необходимость в hidden setup-line'ах в каждом doc-test'е.
- Cross-ref с [D106](#d106-doc-test-semantics) для doc-test-семантики.

### Namespace

Префикс `#doc` делится с формой [D101](07-modules.md#d101-doc-module-attr)
`#doc "string-literal"`. Различаются по **первому токену после
`doc`**:

- `#doc "..."` (string literal) — D101 module-doc-атрибут.
- `#doc(...)` (parenthesised key-value list) — D105 типизированный
  атрибут.
- `#doc_alias(...)` (underscore в имени) — D105 catalog-member.
- `#doc_*` зарезервировано для будущих D105-атрибутов (например,
  `#doc_section`).

Parser дисамбигуирует lookahead'ом за токен после идентификатора
`doc`:
- `STRING_LIT` → D101.
- `LPAREN` → D105.
- `_<ident>` → D105 named member.
- что-либо иное → syntax error.

### Почему

1. **Каталог (не free-form tag soup)** — Go, Rust и TypeScript
   doc-tooling'и все страдали от drift'а конвенций (`@param` vs
   `@parameter` vs ничего в TSDoc; `Deprecated:` proza vs
   `#[deprecated]`-атрибут в Go vs Rust). Фиксация маленького
   именованного каталога на уровне языка — предотвращает это.
   Добавление новых атрибутов требует новой D-decision.
2. **Типизированные параметры** — `#deprecated(since, note, until)`
   имеет структурированные fields, доступные в JSON output. LLM-
   consumer'ы могут читать `since` numerically; «Deprecated: use foo
   instead.» в free-form комментарии — opaque.
3. **`#hide_doc` opt-out, не opt-in** — Rust'овский `#[doc(hidden)]`
   opt-out, mirror'ит `pub`-by-default. Nova private-by-default
   ([D5](04-effects.md#d5)), поэтому `export` opt-in. Прятать export
   из doc — отдельный opt-out — это соответствует ментальной модели
   private-by-default.
4. **Поле `until` для `#deprecated`** — ни у Rust, ни у Go нет. А
   «we're removing this in 1.0» — реальная lifecycle-стадия. С
   `until` `nova doc --deny-overdue-deprecations` становится CI-
   gate'ом против забывчивости удалить.

### Что отвергнуто

- **JSDoc-style теги `@param` / `@returns`** — у Nova типизированные
  параметры и return-типы в сигнатуре; документировать их повторно
  prose'й — duplication и drift. Style guide
  ([Plan 45 §11.5](../../docs/plans/45-nova-doc.md#115-doc-comment-style-guide))
  рекомендует inline-упоминание в description.
- **`#[doc = "raw text"]` alternative form** (Rust precedent) —
  форма `///` достаточна; raw text в атрибутах нужен генераторам
  кода (макросам), которых в Nova нет. Пересмотреть, если появится
  metaprogramming.
- **Multi-tier стабильность сверх трёх** (у Rust много flavour'ов
  unstable) — три tier'а (`stable`/`unstable`/`experimental`)
  покрывают use-кейсы без сложности.
- **User-defined doc-атрибуты** — открывает каталог для произвольных
  тегов, фрагментируя convention. Каталог растёт только через
  D-decisions.

### Связь

- [D96](#d96-синтаксис-атрибутов--name-без-квадратных-скобок) —
  основание `#name(...)` синтаксиса.
- [D101](07-modules.md#d101-doc-module-attr) — module-attr
  `#doc "..."`; namespace сосуществует.
- [D104](03-syntax.md#d104-doc-comment-syntax--outer--inner) —
  лексер `///`/`//!` doc-comment recognition.
- [D106](#d106-doc-test-semantics) — `#doc(test_handlers)`
  referenced.
- [Plan 45](../../docs/plans/45-nova-doc.md) Ф.3 реализация, §11.5
  style guide.

---

## D106. Семантика doc-test'ов

> **Status:** active (spec). Реализация — [Plan 45](../../docs/plans/45-nova-doc.md) Ф.7.
>
> **Reuses:** [D89](#d89-test-tooling-конвенции--expect_-маркеры-для-negative-тестов) (EXPECT-markers); [Plan 24](../../docs/plans/24-cross-platform-test-runner.md) (test_runner). Doc-test'ы компилируются и запускаются через тот же pipeline, что и `*_test.nv`-файлы.

### Что

Code-блок внутри doc-comment'а является **doc-test'ом**, если:

- Огорожен triple-backtick (` ``` `).
- Имеет language tag `nova` либо вообще без language tag (default).

```nova
/// Возвращает true, если `x` чётно.
///
/// # Examples
///
/// (triple-backtick fenced block здесь — code внутри)
/// assert(is_even(2))
/// assert(!is_even(3))
fn is_even(x int) -> bool => x % 2 == 0
```

Выше — один doc-test. Test runner извлекает его, компилирует как
самодостаточный модуль и запускает assert'ы.

### Code-block модификаторы

Language tag может сопровождаться нолём или больше comma-separated
модификаторов, написанных сразу после language tag в строке
fence-opener'а.

**Каталог (MVP):**

| Модификатор | Эффект |
|---|---|
| `no_run` | Только компилируется, не выполняется. |
| `ignore` | Пропускается полностью (не компилируется, не выполняется). |
| `compile_fail` | Code НЕ ДОЛЖЕН компилироваться. Если компилируется — doc-test fail. |
| `should_panic` | Code ДОЛЖЕН компилироваться И паниковать в runtime. Non-panic exit — fail. |
| `must_verify` | Contract verification (`#must_verify` по [D24](#d24-стратегия-smt-проверки-контрактов) / Plan 33) ДОЛЖНА succeed. Failed verification (UNSAT, TIMEOUT) — fail doc-test'а. |

Множественные модификаторы комбинируются там, где имеет смысл
(`no_run,must_verify` означает «verify but do not execute»).
Конфликтующие комбинации (`compile_fail,should_panic`) — configuration
error.

### Hidden lines

Doc-test строка, начинающаяся с `# ` (хеш + пробел) — **скрыта** в
рендеренном output'е, но **компилируется и выполняется** как часть
test'а. Используется для setup'а, который засорил бы примеры
(import'ы, helper-определения и т.п.).

### Privacy

Doc-test'ы имеют **module-private access** к item'у, который
документируют. Doc-test на `export fn foo` (в `std.collections.range`)
может вызывать non-exported helpers внутри `std.collections.range`.
Это соответствует поведению rustdoc и отражает принцип: примеры
демонстрируют использование item'а с same-module-перспективы.

Cross-module doc-test'ы на re-export'ах сохраняют privacy-scope
**оригинального модуля** (того, где item определён), а не re-exporter'а.

### Setup через `#doc(test_handlers)`

[D105](#d105-doc-attributes) определяет атрибут `#doc(test_handlers =
"path")`. При применении к модулю или item'у все doc-test'ы в scope'е
неявно оборачиваются:

```nova
with handler from path.to.handlers {
    ... тело doc-test'а ...
}
```

Снимает boilerplate для типичных setup'ов (test-handler stack'и,
mock filesystems и пр.).

Peer-файл folder-модуля с именем `_doctest_setup.nv` (Plan 42
folder-module convention) также неявно импортируется в doc-test-
scope, если присутствует. Оба механизма аддитивны.

### Модель компиляции

Каждый doc-test компилируется как synthetic module:

```
module __nova_doc_test_<hash>

import <enclosing-module>.*

test "<item-name> example <index>" {
    <hidden-lines + visible-lines>
}
```

- Hash — детерминированная функция от (item-path, doc-test-index).
- Имя теста — `<item-name> example <N>` (1-indexed).
- Import'ы из enclosing-модуля — wildcard-style (peers видимы).

Компиляция переиспользует стандартный pipeline (parser → type-checker
→ codegen / interp). Сбои маршрутизируются как обычные test-failures.

### Выполнение

Doc-test'ы выполняются через тот же `test_runner`, что и обычные
тесты ([Plan 24](../../docs/plans/24-cross-platform-test-runner.md)).
Parallelism (`--jobs N`), output format и exit codes идентичны.

`nova doc --check` запускает doc-test'ы по дефолту; `--no-doc-tests`
отключает. `nova test` **не** запускает doc-test'ы по дефолту
(doc-test'ы принадлежат `nova doc`); `nova test --doc-tests` opt-in.

Exit codes по [D95](#d95-cli-path-конвенции--nova-check-path--nova-test-path):
- 0 — все doc-test'ы прошли.
- 1 — хотя бы один failed.
- 2 — usage error.
- 101 — internal panic.

### Интеграция с EXPECT-markers

Модификаторы `compile_fail` и `should_panic` — syntactic sugar,
транслирующийся в [D89](#d89-test-tooling-конвенции--expect_-маркеры-для-negative-тестов)
EXPECT-markers, вставленные в синтетический test-файл:

| Модификатор | Синтезируемый EXPECT |
|---|---|
| `compile_fail` | `// EXPECT_COMPILE_ERROR` |
| `should_panic` | `// EXPECT_RUNTIME_PANIC` |
| `must_verify` | `// REQUIRES_SMT_BACKEND` + verify-check на `#must_verify` items |

Reuse'ит существующую test_runner инфраструктуру; никакой новой
failure-mode-механики не нужно.

### Почему

1. **Doc-test'ы соседствуют с документируемыми item'ами** — Go'шные
   `Example*`-функции в `*_test.go` (golang/go #16851) дрейфят от
   документируемого item'а. Inline doc-test'ы co-located с тем, что
   документируют; при rename item'а соседние тесты в том же файле
   движутся вместе.
2. **`compile_fail` / `should_panic` first-class** — rustdoc precedent.
   Документирование «это должно failиться» ценно; tooling-проверка —
   убирает целый класс stale-example багов.
3. **`must_verify` — Nova-unique** — leverages Plan 33 SMT
   verification. Doc-comment может демонстрировать, что функция
   удовлетворяет контрактам **под всеми входами**, не только под
   одним примером.
4. **Hidden setup через `# `** — приемлемый компромисс: слишком
   verbose показывать каждый import; `#doc(test_handlers)` и
   `_doctest_setup.nv` покрывают типичные кейсы без per-test
   boilerplate'а.

### Что отвергнуто

- **Markdown-link-style ссылки на внешние example-файлы** — doc-test
  в `examples/foo.nv` добавляет indirection, теряет co-location.
  Inline — каноническая форма.
- **Модификатор `run_only_if_feature("name")`** — дублирует
  `#cfg(feature = ...)` (Plan 42.12). Если документируемый item
  feature-gated, тест наследует gate.
- **Модификатор `expected_output = "..."` для stdout-сравнения** —
  assert'ы внутри теста более гибкие. Если нужно stdout-matching —
  [D89](#d89-test-tooling-конвенции--expect_-маркеры-для-negative-тестов)
  `EXPECT_STDOUT` через hidden line.
- **Doc-test isolation-контейнеры (process-per-test)** — overhead
  слишком высокий; `test_runner` уже изолирует state per-test через
  fresh module-instance.

### Связь

- [D24](#d24-стратегия-smt-проверки-контрактов) — модификатор
  `must_verify` завязан на SMT verification.
- [D89](#d89-test-tooling-конвенции--expect_-маркеры-для-negative-тестов)
  — EXPECT-markers reused.
- [D95](#d95-cli-path-конвенции--nova-check-path--nova-test-path) —
  CLI exit codes.
- [D104](03-syntax.md#d104-doc-comment-syntax--outer--inner) —
  fenced code-блоки внутри doc-comment'ов.
- [D105](#d105-doc-attributes) — `#doc(test_handlers)`.
- [Plan 24](../../docs/plans/24-cross-platform-test-runner.md) —
  test_runner reuse.
- [Plan 33](../../docs/plans/33-contracts-implementation.md) —
  контракты для `must_verify`.
- [Plan 42](../../docs/plans/42-folder-modules.md) —
  `_doctest_setup.nv` folder-module peer.
- [Plan 45](../../docs/plans/45-nova-doc.md) Ф.7 реализация.

---

## D107. JSON output schema v1

> **Status:** active (spec). Реализация — [Plan 45](../../docs/plans/45-nova-doc.md) Ф.9.
>
> **Заметка о состоянии stability:** v1 поставляется маркированный как
> **`mvp-stable`** — только additive minor changes, никаких breaking.
> После ≥ 1 milestone'а реального использования (Plan 45.B stdlib
> doc-pass + ≥ 3 внешних AI-consumer'ов) stability промоутится к
> **`stable`**. Promotion — отдельная spec-ревизия этой D-decision.

### Что

`nova doc --format json` производит JSON-документ, описывающий
public API surface модуля (или workspace'а). Документ соответствует
versioned-схеме (`format_version: u32`); consumer'ы ОБЯЗАНЫ проверять
версию перед парсингом.

Схема **embedded** в бинарь компилятора как JSON Schema 2020-12 и
эмитится через `nova doc --json-schema`.

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

**Обязательные top-level-поля:**

- `format_version` (`u32`) — major-версия схемы. Consumer'ы ОБЯЗАНЫ
  fail-loudly при нераспознанной версии.
- `nova_version` (`string`, semver) — версия компилятора, эмитившего
  документ. Информационно; не stability-контракт.
- `generated_at` (`string`, RFC 3339 UTC) — emission timestamp. Может
  быть elided в reproducible-build mode (`SOURCE_DATE_EPOCH`).
- `modules` (`array<Module>`) — каждый документированный модуль
  (entry + transitive imports при `--workspace`).
- `items` (`array<Item>`) — flat-список всех items; поле `module_path`
  дисамбигуирует ownership.
- `links` (`array<Link>`) — резолвенные intra-doc links от items в
  этом документе.
- `doc_tests` (`array<DocTest>`) — извлечённые (и опционально
  выполненные) doc-test'ы со статусами.

**Опциональные top-level-поля:**

- `source_root` (`string`, абсолютный путь) — корень репозитория.
  Опускается, когда source-paths анонимизированы (флаг
  `--anonymize-paths` — будущий).

### Форма `Module`

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
- `name` — последний segment `path`.
- `kind` — `folder` для folder-модулей, `file` для single-file.
- `peers` — relative paths к peer-файлам (только для `folder`); пустой
  для `file`.
- `summary` — первое предложение, извлечённое из `//!` doc и `#doc`
  module-attr.
- `description` — полное markdown-тело.
- `stability` — `{ tier: "stable" | "unstable" | "experimental",
  since: "..."?, feature: "..."?, note: "..."? }` или `null` для
  неизвестного tier'а.
- `deprecation` — `{ since, note, until? }` или `null`.
- `doc_attrs` — прочие doc-атрибуты (по [D105](#d105-doc-attributes)),
  не имеющие structured top-level-поля.
- `source` — `{ file_id, line }` для «View Source»-links.

### Форма `Item`

Item'ы — tagged unions. Все items делят общий header:

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

`id` — стабильный идентификатор: `<module_path>::<name>` для free
items; `<module_path>::<TypeName>.<method>` для методов. ID'ы —
**канонический link target**.

Объект `sections` содержит распарсенные стандартизованные секции
(`# Examples`, `# Errors` и пр.) как markdown-строки, ключеванные
lowercase-именем секции.

**Kind-specific:**

- `kind: "fn"` — `signature` (params, return type, effect-row, raises,
  generics, contracts).
- `kind: "type"` — `definition` (Record | Sum | Alias | Protocol |
  Effect) с `fields` / `variants` и т.п.
- `kind: "const"` — `type`, `value` (рендерится как Nova source).
- `kind: "effect"` — массив `methods` (effect-op signatures), `axioms`
  (Plan 33.3 D24 `axiom`-clauses).
- `kind: "handler"` — `effect` (резолвенный id), флаг `is_default`.
- `kind: "protocol"` — `methods` (signatures обязательных методов),
  `implementors` (резолвенные item-id'ы).

### Форма `Signature` (для `fn`-items)

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

- Поля `type` — **рендерятся как Nova source** (строки), не как
  структурные AST. Это намеренно: consumer'ы, которым нужна
  структура, могут парсить тем же parser'ом. Рендеринг строк
  сохраняет JSON output портабельным и человекочитаемым.
- `keyword_only: true` ставится, когда параметр имеет `default` по
  [D102](03-syntax.md#d102-именованные-аргументы-и-значения-параметров-по-умолчанию).
- Список `effects` — effect-row (set-typed, упорядочено алфавитно для
  детерминизма).
- `raises` — union вариантов `Fail[X]` из effect-row.
- `receiver` ненулевой для instance/static-методов:
  `{ "type": "Box", "kind": "instance", "mutable": false }`.
- `contracts.verify_status` — одно из `PROVEN | UNVERIFIED | TIMEOUT | TRUSTED`.

### Форма `Link`

```json
{
  "from": "std.collections.range::Range.map",
  "to": "std.collections.iter::Iter.map",
  "kind": "fn",
  "resolved": true,
  "source_span": { "file_id": 12, "line": 45, "col": 10 }
}
```

Запись каждого intra-doc link'а, обнаруженного в этом документе. При
`resolved: false` link-target был unresolvable (broken link).

### Форма `DocTest`

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

- `id` — детерминированный: `<item_id>::doc_<index>` (0-indexed).
- `code` — полный код, включая hidden setup-lines.
- `code_visible` — код без hidden-lines (для HTML/Markdown rendering).
- `status` — одно из `passed | failed | skipped | not_run`.
- `failure` — `null` при успехе; иначе `{ kind, message }`, где
  `kind` ∈ `compile_error | runtime_panic | verification_failure`.
- `status: "not_run"` — был передан `--no-doc-tests`; только извлечён,
  не выполнялся.

### Детерминированный output

Producer'ы ОБЯЗАНЫ эмитить JSON детерминированно:

- Object-keys отсортированы алфавитно.
- Arrays в стабильном порядке: modules и items по `path`/`id`; links
  по `from` затем `to`; doc_tests по `id`.
- Поле `generated_at` опускается, когда в env установлен
  `SOURCE_DATE_EPOCH`.

Тесты в Plan 45 Ф.19 проверяют byte-identical output между двумя
последовательными прогонами.

### Правила стабильности

Полную versioning-политику см. в
[Plan 45 §6](../../docs/plans/45-nova-doc.md#6-json-schema-v1-контракт).
Кратко:

- **Additive minor changes** (не bump'ят `format_version`):
  - Новые опциональные top-level или вложенные поля.
  - Новые enum-варианты в полях, документированных как «extensible».
  - Новые `kind`-specific Item-fields (consumer'ы default-skip'ают).
- **Breaking changes** (`format_version` инкрементится):
  - Удалить или переименовать поле.
  - Сменить тип или семантику поля.
  - Сузить enum (удалить вариант).

`format_version=N` и `format_version=N+1` поддерживаются параллельно
≥ 1 stable-релиз компилятора. Consumer'ы поощряются fail-loudly на
непознанной major-версии.

### `nova-doc-types` consumer-крейт

Отдельный Rust-крейт `nova-doc-types` предоставляет типизированные
bindings к схеме:

```rust
// nova-doc-types = "1.x" — версия-locked с format_version=1.
use nova_doc_types::{Document, Item, ItemKind};

let doc: Document = serde_json::from_str(&json_input)?;
```

Mirror'ит rustdoc'овский `rustdoc-types`-крейт. Versioning параллельный
`format_version`: major-bump'ы lock-step.

### Embedded JSON Schema

`nova doc --json-schema` эмитит схему как JSON-документ,
соответствующий JSON Schema 2020-12. Это включает:

- Offline-валидацию в CI-gate'ах.
- IDE auto-completion в редакторах, потребляющих JSON Schema.
- LLM tool-use prompt context.

Схема **embedded в бинаре компилятора** (`include_str!`). Версии
схемы immutable per `format_version`; бинарь несёт ровно одну
(текущий major).

### Почему

1. **Stable JSON как first-class output** — у godoc'а нет, rustdoc'е
   только unstable-nightly, TypeDoc — unstable. Nova поставляет
   stable-схему **на stable-сборке** с MVP-day-one. AI/LSP-consumer'ы
   могут полагаться.
2. **`format_version` integer, не semver-string** — проверки проще
   (`>= 1 && <= 1` per consumer), parser проще. SemVer-семантика
   запечена в additive-minor / breaking-major правило выше без
   exposure version-string complexity.
3. **String-рендеренные типы vs структурные AST** — exposure полного
   структурного AST в JSON связал бы consumer'ов с internal Nova
   type representations. Рендеринг строк портабельный (любой
   consumer прочитает) и стабильный (parser-changes не ломают JSON
   shape, только содержимое рендеренных strings меняется в step
   с языком).
4. **Sorted, deterministic output** — нужен для `--diff` (Plan 45.A)
   и reproducible builds. Без него doc-as-CI-gate производит
   ложные diff'ы.
5. **Embedded schema** — offline-валидация без сети. CI-gate'ы могут
   крутиться на air-gapped-builder'ах.

### Что отвергнуто

- **Per-module JSON-файлы (один файл на модуль)** — Plan 45 эмитит
  единый документ по дефолту. Per-module-файлы создают
  discovery-проблемы (надо листать директории, нет глобальных
  cross-references). Будущее расширение может добавить
  `--split-by-module` для очень больших workspace'ов.
- **GraphQL endpoint вместо JSON-файла** — server-overhead для CLI-
  инструмента. JSON-документ — consumer-agnostic.
- **Protocol Buffers / MessagePack** — JSON — lowest common
  denominator для AI/LSP/CI tooling. Бинарные форматы — позже,
  если докажут нужду; JSON — канонический контракт.
- **Embedded полный source** — раздувает output и дублирует работу.
  Consumer-сайд может резолвить `source.file_id`, если у него есть
  доступ к source.

### Связь

- [D89](#d89-test-tooling-конвенции--expect_-маркеры-для-negative-тестов)
  — EXPECT-markers транслируются в `DocTest.failure.kind`.
- [D95](#d95-cli-path-конвенции--nova-check-path--nova-test-path) —
  CLI-конвенции для `nova doc --format json`.
- [D104](03-syntax.md#d104-doc-comment-syntax--outer--inner) —
  источник doc-content.
- [D105](#d105-doc-attributes) — поля attribute-metadata.
- [D106](#d106-doc-test-semantics) — источник формы DocTest.
- [Plan 45](../../docs/plans/45-nova-doc.md) §6, §6.5 — versioning
  policy; Ф.9 реализация.

---

## D111. `assume` / `assert_static` / `#trusted` external

**Статус:** Принято (Plan 33.2 Ф.8 + Plan 33.3 Ф.9/Ф.13, реализовано)

### Решение

Три escape-hatch механизма для управления верификацией:

#### `assert_static <bool>`

Промежуточный шаг доказательства: разбивает сложный контракт на части.
SMT видит `assert_static` как дополнительный fact в текущей точке.
В release стирается (verified по SMT); в debug — runtime assert.

```nova
fn transfer(from int, to int, amount money) Db -> ()
    requires amount > 0
    ensures Db.balance(to) == old(Db.balance(to)) + amount
{
    assert_static Db.balance(from) >= amount   // промежуточный факт
    Db.setBalance(from, Db.balance(from) - amount)
    Db.setBalance(to,   Db.balance(to)   + amount)
}
```

#### `assume <bool>`

Escape-hatch для знаний о FFI / внешних инвариантах. SMT получает
`(assert <expr>)` без proof. Вне функции, помеченной `#trusted`, —
**warning** категории `trust-introduced`.

```nova
#trusted
fn call_ffi() -> int {
    let result = extern_fn()
    assume result >= 0   // знаем по документации FFI
    result
}
```

#### `#trusted` external fn

`external fn` с контрактами требует `#trusted`. Контракты регистрируются
как axioms — caller получает `ensures` как предположение без proof.

```nova
#trusted
external fn libc_strlen(s str) -> int
    requires s.is_valid_cstring()
    ensures result >= 0
```

### Обоснование

Полностью pure SMT-proof недостижим для кода с FFI, внешними
библиотеками и непроверяемыми OS-инвариантами. Escape-хатчи
сохраняют expressiveness при сознательном принятии риска. Паттерн
из Dafny (`assume`, `{:axiom}`), F* (`assume_val`).

### Реализация

- `compiler-codegen/src/ast/mod.rs` — `ExprKind::AssertStatic`, `ExprKind::Assume`.
- `compiler-codegen/src/parser/mod.rs` — парсинг `assert_static`, `assume`.
- `compiler-codegen/src/types/mod.rs` — `#trusted` attribute на fn-decl;
  warning для `assume` вне `#trusted`.
- `compiler-codegen/src/verify/encode.rs` — `assert_static` → SMT fact; `assume` → `(assert ...)`.
- `compiler-codegen/src/codegen/emit_c.rs` — `assume` → стирается в release;
  debug → runtime-if с `NOVA_ASSUME` violation.

---

## D112. Bounded quantifiers (`forall`/`exists` по коллекции)

**Статус:** Принято (Plan 33.3 Ф.10, реализовано в AST и SMT-encoder)

### Решение

Nova поддерживает **bounded quantifiers** — только по конкретным
коллекциям или диапазонам. Unbounded quantifiers (`forall x : T : P(x)`)
**запрещены** (compile error).

```nova
requires forall i in 0..xs.len() : xs[i] >= 0
ensures  exists i in indices : result == xs[i]
invariant forall i in 0..k : processed[i] == true
```

**Синтаксис:**
```
forall <ident> in <expr> : <bool>
exists <ident> in <expr> : <bool>
```

`expr` после `in` — `Iter[T]` (array, range, set, map); body — `bool`,
`#pure` (без side effects).

**SMT encoding:**
- Конкретный размер → конъюнкция/дизъюнкция `P(xs[0]) ∧ P(xs[1]) ∧ ...`.
- Символьный размер → `Z3_mk_forall_const` с `:pattern ((select xs i))`.

Unbounded форма вызывает ошибку компиляции:
```
error: unbounded quantifier not supported
  use `forall x in collection : P(x)` (bounded form)
```

### Обоснование

Unbounded quantifiers в SMT практически всегда требуют ручного trigger
annotation и часто зависают. Bounded форма даёт детерминированный
trigger через `select`-pattern и покрывает 95% реальных программных
инвариантов. Паттерн из Dafny, Verus.

### Реализация

- `compiler-codegen/src/ast/mod.rs` — `ExprKind::Forall { var, iter, body }`, `ExprKind::Exists { ... }`.
- `compiler-codegen/src/parser/mod.rs` — парсинг `forall`/`exists`; reject unbounded form.
- `compiler-codegen/src/types/mod.rs` — type-check: iter → `Iter[T]`, body → `bool`, `#pure`.
- `compiler-codegen/src/verify/encode.rs` — конъюнкция (concrete) или `forall` с pattern (symbolic).

---

## D113. `#must_verify_module` — strict mode на модуле

**Статус:** Запланировано (Plan 33.3 Ф.13, Plan 33.4 V2)

### Решение

Атрибут `#must_verify_module` на модуле переводит все функции внутри
в режим `#must_verify` — любой unprovable контракт становится
**compile error** (не fallback на runtime).

```nova
#must_verify_module
module banking.core {
    fn transfer(from int, to int, amount money) Db -> ()
        requires amount > 0
        ensures  Db.balance(to) == old(Db.balance(to)) + amount
    => ...
}
```

Целевой use-case: критичные компоненты (финансы, медицина, авионика)
где runtime-fallback неприемлем. Паритет с Dafny `:verify true` на
модуле.

Функция внутри `#must_verify_module` может явно opt-out через
`#unverified` (задокументированное исключение).

### Обоснование

`#must_verify` на каждой функции — verbose. Module-level атрибут
выражает намерение «этот модуль формально верифицирован» одной строкой.
Позволяет CI-gate отделить critical-core от ordinary code.

### Реализация (V2)

- `compiler-codegen/src/ast/mod.rs` — флаг `must_verify_module` в `ModuleDecl`.
- `compiler-codegen/src/types/mod.rs` — при type-check fn в таком модуле:
  применять semantics `#must_verify` ко всем fn без явного `#unverified`.

---

## D114. SMT cache + parallel verification

**Статус:** Запланировано (Plan 33.3 Ф.12, V2)

### Решение

#### Incremental SMT cache

`target/contracts-cache/<hash>.json` хранит результат верификации
каждой функции:

```json
{
  "fn_id":          "module/path::fn_name",
  "input_hash":     "sha256:<AST + deps + contracts>",
  "smt_query_hash": "sha256:<encoded SMT>",
  "result":         "proven",
  "solver":         "z3-4.13.0",
  "duration_ms":    142
}
```

Pipeline: compute `input_hash` → lookup → cache hit → skip SMT call.
Инвалидация по изменению любой transitive contract-dependency (Salsa-style).

#### Parallel verification

`verify/worker.rs` через `rayon::ThreadPool` с `N = num_cpus` workers.
Каждая `verify_fn` — independent job со своей SMT-context (Z3 thread-safe:
`Z3_global_param_set("parallel.enable", "true")`). Финальный
diagnostics-merge в главном потоке.

**Acceptance targets:**
- Incremental rebuild без изменений на 100-fn corpus — < 2 сек.
- Parallel speedup >= 6× на 8 cores.
- Release binary identical для proven fn (zero-cost erasure).

### Обоснование

Верификация контрактов линейно растёт с размером кодовой базы.
Без кэша и параллелизма time-to-compile с включёнными контрактами
становится непрактичным уже при 500+ функциях. Паттерн из
Dafny incremental / Verus parallel.

### Реализация (V2 plan)

- `compiler-codegen/src/verify/worker.rs` — rayon ThreadPool.
- `compiler-codegen/src/verify/pipeline.rs` — cache lookup/save.
- `target/contracts-cache/` — директория с JSON-артефактами.

---

## D116. Z3 backend через собственные FFI-биндинги

**Статус:** Принято (Plan 33.3 Ф.9, реализовано 2026-05-14)

### Решение

Nova линкует Z3 напрямую через **собственные FFI-биндинги** (`z3_ffi.rs`)
без зависимости от crate-экосистемы (`z3-sys`, `z3` crate).

Биндинги декларируют только те функции Z3 C API, которые реально
используются в `Z3Backend` — менее 30 функций. Выбор конкретной
версии Z3 полностью под контролем Nova.

Backend реализует trait `SolverBackend` и выбирается через
pipeline-selector: `--smt-backend=z3` (default) или env var `NOVA_SMT_BACKEND=z3`.

### Обоснование

Crate `z3-sys` / `z3` — внешние зависимости с независимым release
cycle. Историческая практика в Nova: не патчить и не полагаться на
сторонние аблокации критичных подсистем (ср. политику с minicoro,
Boehm GC). Собственные биндинги дают:
- Полный контроль над версией Z3.
- Минимальный API surface (менее 30 функций vs тысячи в полном `z3-sys`).
- Возможность свитч Z3 -> CVC5 без смены интерфейса.

### Реализация

- `compiler-codegen/src/verify/backend/z3_ffi.rs` — FFI-объявления (~25 функций Z3 C API).
- `compiler-codegen/src/verify/backend/z3.rs` — `Z3Backend` struct,
  реализация `SolverBackend`.
- `compiler-codegen/src/verify/backend/mod.rs` — `SolverBackend` trait,
  factory/selector по env/flag.
- `compiler-codegen/build.rs` — feature `z3-backend`; link Z3 shared lib.
