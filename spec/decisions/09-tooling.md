# Tooling — компилятор, верификация, диагностика

Решения этой группы определяют, как **инструменты** Nova работают с
программами: какие проверки выполняются на этапе компиляции, какие —
в runtime, и как ошибки оформлены для AI-генерации кода.

| # | Решение |
|---|---|
| [D24](#d24-стратегия-smt-проверки-контрактов) | Стратегия SMT-проверки контрактов |
| [D89](#d89-test-tooling-конвенции--expect-маркеры-для-negative-тестов) | Test-tooling конвенции: `EXPECT_*` маркеры для negative-тестов |

---

## D24. Стратегия SMT-проверки контрактов

### Что
Контракты в сигнатуре (`requires`/`ensures`/`invariant`) проверяются
**SMT-движком на этапе компиляции** с явным таймаутом и fallback на
runtime-проверку. Контракт, который SMT не смог доказать, **не
блокирует** компиляцию по умолчанию — он становится runtime-assert'ом
в debug и тихо игнорируется в release с предупреждением `@unverified`.
Программист может явно требовать статическое доказательство через
`@must_verify` — тогда компиляция падает, если SMT не справился.

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
   увеличить через `@verify_timeout(seconds=10)` локально или
   глобально в конфигурации проекта.

3. **Поведение по уровням сборки:**
   - **debug:** SMT-проверка + runtime-fallback для непроверенного.
     Нарушение runtime → panic с указанием контракта и точки.
   - **release:** SMT-проверка. Доказанные контракты — стираются
     полностью (zero cost). Недоказанные — игнорируются молча с
     warning'ом на этапе сборки.

4. **Опт-ин строгости через атрибуты:**
   - **`@must_verify`** на функции → если SMT не доказал контракт,
     компиляция **падает**. Для критичного кода (медицина, финансы,
     авионика).
   - **`@unverified`** на функции → отказ от попытки доказательства
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
доказывает → fallback на runtime или ошибка с `@must_verify`.

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
     2. use `@must_verify` if static proof is required
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
- **`@must_verify` на уровне модуля** — глобальный strict mode для
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
