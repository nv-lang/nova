# Test conventions — `EXPECT_*` маркеры

Практический guide для авторов тестов Nova. Нормативная спецификация —
[D89 в spec/decisions/09-tooling.md](../spec/decisions/09-tooling.md#d89).

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

## 4 стандартных маркера

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

### 2. `EXPECT_RUNTIME_PANIC <pattern>`

Проверяет, что exe **скомпилируется и запустится**, но **упадёт с
panic**, чьё сообщение содержит `<pattern>`.

**Когда использовать:**
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

Проверяет, что **stdout** exe содержит `<pattern>` (substring).

**Когда использовать:**
- Golden-file тесты для format/print-логики.
- Проверка что program вывела ожидаемое сообщение.
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
- Stdout должен содержать `hello world` (substring match).

**Если pattern не найден** — `NEG-WRONG-STDOUT`.

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

| Тип теста | Категория |
|---|---|
| Обычные positive | `nova_tests/<group>/` (basics, syntax, types, runtime, ...) |
| `EXPECT_COMPILE_ERROR` | `nova_tests/negative_capability/` |
| `EXPECT_RUNTIME_PANIC`, `EXPECT_EXIT_CODE`, `EXPECT_STDOUT` | `nova_tests/expected_runtime/` |

(Категории — **convention**, не enforced. Runner ходит по всем
файлам в `nova_tests/`.)

---

## Расширения для своего проекта

Если в твоём проекте появился use-case для нового маркера (например
`EXPECT_LINT_WARNING`) — **сначала** проверь, не покроет ли существующий
один из 4 стандартных. Если нужен новый — обсуди с авторами Nova
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

## Ссылки

- [D89 в spec/decisions/09-tooling.md](../spec/decisions/09-tooling.md#d89) — нормативная спецификация.
- [run_tests.ps1](../run_tests.ps1) — текущая Windows + MSVC реализация.
- [nova_tests/negative_capability/](../nova_tests/negative_capability/) — примеры `EXPECT_COMPILE_ERROR`.
- [nova_tests/expected_runtime/](../nova_tests/expected_runtime/) — примеры остальных трёх маркеров.
