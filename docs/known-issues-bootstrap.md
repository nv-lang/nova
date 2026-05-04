# Bootstrap-компилятор — известные проблемы и пробелы

Это implementation-gaps в bootstrap-интерпретаторе (`compiler-bootstrap/`) — НЕ
вопросы дизайна языка. Дизайн-вопросы → [spec/open-questions.md](../spec/open-questions.md).

Bootstrap намеренно минимальный: его задача — запустить достаточно
Nova-кода чтобы написать на нём self-hosted production-компилятор.
Эти gaps приемлемы для bootstrap'а, но production-компилятор должен
их адресовать.

---

## Lambda без параметров

`() => expr` не парсится — парсер не распознаёт пустые круглые скобки
как начало lambda.

```nova
let f = () => 42      // SYNTAX ERROR: unexpected `=>`
let f = (_unit) => 42 // workaround: dummy-параметр
```

**Где.** [compiler-bootstrap/src/parser/mod.rs](../compiler-bootstrap/src/parser/mod.rs)
`parse_lambda` / `parse_primary`.

**Workaround.** Добавить фиктивный параметр или использовать обычный
`fn` declaration.

---

## `&&` / `||` без short-circuit

Реализованы как обычные eager binary operations — оба операнда
вычисляются всегда. Side-effects во втором операнде выполняются
независимо от первого.

```nova
false && side_effect()  // side_effect() ВЫЗЫВАЕТСЯ (баг)
true  || side_effect()  // side_effect() ВЫЗЫВАЕТСЯ (баг)
```

**Где.** [compiler-bootstrap/src/interp/mod.rs](../compiler-bootstrap/src/interp/mod.rs)
`fn binop` — case `(And, ...)` и `(Or, ...)`. Должны быть выделены
в `eval_expr` отдельной веткой `ExprKind::Binary` с проверкой левого
операнда до вычисления правого.

**Severity.** Семантически неверно, но в bootstrap-коде
side-effects редки.

---

## `loop { break v }` не возвращает значение

`loop` всегда даёт `Unit`. Нет поддержки `break <expr>` как способа
вернуть значение из loop'а.

```nova
let r = loop {
    if cond { break 42 }    // break 42 — parse OK, но `r` всегда Unit
}
```

**Где.** [compiler-bootstrap/src/interp/mod.rs](../compiler-bootstrap/src/interp/mod.rs)
`Flow::Break` не несёт значения.

**Workaround.** Использовать `let mut result` снаружи и присваивать
перед `break`.

---

## Newtype constructor

```nova
type UserId u64        // объявление парсится
let id = UserId(42)    // RUNTIME ERROR: undefined name `UserId`
```

Newtype как декларация (`type T BaseType`) парсится, но конструктор
`T(x)` не зарегистрирован в среде — нет соответствующей записи в
type-table при evaluation.

**Где.** [compiler-bootstrap/src/interp/mod.rs](../compiler-bootstrap/src/interp/mod.rs)
`load_type_decl` или эквивалент — newtype-варианты не строят
constructor-функцию.

**Workaround.** Использовать record-обёртку: `type UserId { value u64 }`.

---

## Imports между файлами

Bootstrap не выполняет реальную загрузку модулей. `import a.b.c`
парсится, но имена из других файлов не доступны.

```nova
module main
import core.array

fn f() => array.len(...)   // RUNTIME ERROR: undefined `array`
```

**Где.** [compiler-bootstrap/src/main.rs](../compiler-bootstrap/src/main.rs) /
loader — модули из `import` не загружаются и не помещаются в среду.

**Workaround.** Все определения в одном файле. Тесты-нова всегда
self-contained.

---

## Nested `fn` в test/блоках

`fn` decl поддерживается только на top-level модуля. Нельзя объявить
функцию внутри `test "..." { ... }` или внутри другого `fn`.

```nova
test "x" {
    fn helper() -> int { 42 }   // SYNTAX ERROR: unexpected `fn`
    assert(helper() == 42)
}
```

**Где.** Парсер blocks парсит только expressions/statements, не
items.

**Workaround.** Объявлять функции на top-level или использовать
lambda (`let helper = () => 42`).

---

## `is` оператор

`expr is Type` всегда возвращает `false` — заглушка.

```nova
let x = Some(42)
assert(x is Option[int])   // FAILS: всегда false
```

**Где.** [compiler-bootstrap/src/interp/mod.rs:355](../compiler-bootstrap/src/interp/mod.rs)
`ExprKind::Is(_inner, _ty) => Ok(Flow::Value(Value::Bool(false)))`.

**Причина.** Для `is` нужен runtime type-tag. В bootstrap-Value нет
типа-носителя информации о statically-declared type.

**Workaround.** Использовать `match` для дискриминации sum-вариантов.

---

## `as` cast

`expr as Type` — no-op, значение возвращается как есть. Никакой
type-coercion (int↔float, sub↔super) не происходит.

**Где.** [compiler-bootstrap/src/interp/mod.rs:349](../compiler-bootstrap/src/interp/mod.rs)
`ExprKind::As(inner, _ty) => self.eval_expr_value(inner, env)`.

**Severity.** Семантически некорректно, но bootstrap-код пишется
без зависимости от cast'а.

---

## Tagged template literals

`` tag`hello ${world}` `` — парсится, но tag-функция игнорируется.
Конкатенируются только parts (без интерполяции значений).

```nova
let s = sql`SELECT * FROM users WHERE id = ${id}`
// результат: "SELECT * FROM users WHERE id = " (без id и без вызова sql)
```

**Где.** [compiler-bootstrap/src/interp/mod.rs:649](../compiler-bootstrap/src/interp/mod.rs)
`ExprKind::TaggedTemplate { tag: _, parts, .. }` — tag отбрасывается.

**Severity.** Bootstrap не использует tagged templates; для production
нужно реализовать вызов tag-функции с `(strings, values)`.

---

## Recursion — Rust stack

Bootstrap использует Rust call-stack без TCO. Глубокая рекурсия
вызывает stack overflow.

```nova
ack(3, 3)   // STACK OVERFLOW при ack(3, k>3)
fib(20+)    // медленно, но работает; 30+ — overflow
```

**Workaround.** Ограничивать глубину рекурсии в тестах. Production
должен делать TCO для tail-calls.

---

## Type ascription с `:` в let

В Rust/TypeScript синтаксис: `let x: int = 42`. В Nova используется
без двоеточия: `let x int = 42`. Колонная форма парсится как ошибка.

```nova
let x: int = 42   // SYNTAX ERROR: expected type, got `:`
let x int = 42    // OK
```

**Severity.** Это design-decision (D9-style), не баг — но программисты
из Rust/TS могут попробовать колонную форму. Хорошее место для
улучшения diagnostics парсера.

---

## Dynamic dispatch / interface types

В bootstrap'е нет trait-объектов. Все вызовы — статические по типу
рецептора. Это implementation gap, но в bootstrap-коде не используется.

---

## `r.push(v)` после `match { ... }` в while-блоке — `}` съедается как trailing block

В сочетании `while { ... let v = match ... { ... } ; r.push(v) }`
парсер не видит конец while-блока. Симптом: `expected '{', got newline`
на строке после `}` while'а.

```nova
while !cond {
    let v = match opt { Some(x) => x; None => 0 }
    r.push(v)        // <— здесь парсер думает что push — это trailing-block-call
}                    // <— этот `}` уже съеден, дальше синтакс ломается
```

**Workaround.** Заменить while на for с известным числом итераций.

**Где.** Парсер postfix-вызова — конфликтует с blocks-as-arguments.
Связано с другим багом про `if pred(x) { ... }` в for-блоке.

---

## `let v = match expr.method() { ... }` — парсер путается

Если scrutinee match'а — method call, парсер не корректно обрабатывает
последующий `}`:

```nova
let v = match s.pop() { Some(x) => x; None => 0 }   // SYNTAX ERROR
```

**Workaround.** Вынести вызов в отдельный let:

```nova
let popped = s.pop()
let v = match popped { Some(x) => x; None => 0 }   // OK
```

---

## `if pred(x) { ... }` в for-блоке — парсер путается

В теле `for` после `for x in xs {` парсер не принимает прямой
вызов в условии `if`:

```nova
for x in xs {
    if pred(x) { r.push(x) }     // SYNTAX ERROR в bootstrap'е
}
```

Workaround — вынести вызов в let:

```nova
for x in xs {
    let ok = pred(x)
    if ok { r.push(x) }
}
```

Симптом: `expected '{', got newline`. Видимо conflict между
trailing-block-call (`pred(x) { ... }`) и `if pred(x) { ... }` —
парсер пытается распознать вызов с trailing-блоком.

**Где.** Парсер `if`/call disambiguation. Вероятно нужен флаг
no-trailing-block после `if` (как уже есть для scrutinee в match).

---

## Scope rules для `type` внутри блока

```nova
test "x" {
    type Local { v int }   // SYNTAX ERROR: unexpected `type`
}
```

`type` decls только на top-level модуля, как и `fn`.

---

## Дизайн-вопросы (НЕ implementation gaps)

Эти отсутствующие фичи — открытые design-вопросы, см.
[spec/open-questions.md](../spec/open-questions.md):

- **Pipe operator `|>`** — Q-pipe-operator
- **String interpolation `"${expr}"`** — Q-string-interpolation

Эти фичи не реализованы в bootstrap'е потому что не зафиксированы
в спеке, а не из-за implementation-gap.
