// SPDX-License-Identifier: MIT OR Apache-2.0
# Consume-types — developer experience guide (D166)

> **Plan 100.8 (D166).** Закрыт 2026-05-26.
> Охватывает: диагностика, LSP quick-fixes, `nova doc` badges,
> `nova consume-analyze` CLI, C-codegen errdefer-hoisting fix.

---

## Обзор

Consume-типы обязывают программиста явно завершить ресурс до выхода
из scope. Компилятор отслеживает линейность через `ConsumeCtx` и
выдаёт ошибки `D133-not-consumed` и `D162-uncovered-*-path`.

D166 добавляет **developer experience** слой поверх этого:
- Structured diagnostics с machine-applicable suggestions
- LSP hover: статус + coverage (Live/Covered/Consumed)
- LSP quick-fixes: add consume marker, add errdefer, suggest commit
- `nova doc`: badge `🔒 [consume]` + Resource lifecycle section
- `nova consume-analyze`: CI gate для uncovered bindings

---

## Диагностика (D133 / D162)

### D133-not-consumed
Выдаётся когда consume-переменная не consumed до scope-exit.

```
error[D133-not-consumed]: переменная `tx` (тип `Transaction`) не consumed
   ╭─[process.nv:5:5]
 5 │     consume tx = begin()
   │     ━━━━━━━━━━━━━━━━━━━━ consume binding объявлен здесь
 9 │ }
   │ ━ scope exit; tx still Live
   ╰─
   help: consume `tx` via method (D133-not-consumed quick fix)
         suggestion: tx.commit() или tx.rollback()
```

Suggestion (MachineApplicable) именует конкретные consume-методы типа.

### D162-uncovered-error-path
Выдаётся когда failable function (`Fail[E]`) имеет consume binding
без `errdefer` — error-path неочищен.

```
error[D162-uncovered-error-path]: consume binding `tx` в failable function
без errdefer покрытия error-path
   help: add `errdefer { tx.rollback() }` to cover error-path
         [machine-applicable]
```

### D162-uncovered-success-path
Более мягкое предупреждение: errdefer есть, но success-path не
явно покрыт (no okdefer, no trailing method call).

```
note[D162-uncovered-success-path]: success-path может быть не покрыт
   help: add `okdefer { tx.commit() }` or explicit call [maybe-incorrect]
```

---

## Рекомендуемый шаблон

```nova
fn process_db(url str) Fail[str] -> () {
    consume conn = connect(url)
    errdefer { conn.rollback() }   // ← error-path покрыт
    do_work()?
    conn.commit()                  // ← success-path покрыт
}
```

Это полное покрытие — D162 не выдаёт ни error-, ни success-path ошибки.

---

## nova doc — [consume] badge

Consume-типы отображаются в `nova doc` с badge `🔒 [consume]` и
автоматической секцией **Resource lifecycle**:

```markdown
🔒 `[consume]`

### Resource lifecycle

Этот тип является **consume-typed** (D133). Каждый binding типа
`Transaction` **обязан** быть consumed до scope-exit:
- Вызов consume-метода: `commit()`, `rollback()`
- Pass as consume-parameter
- Покрытие через `errdefer` / `okdefer` (D160)

Не consumed → compile error `D133-not-consumed`.
```

---

## nova consume-analyze

CLI инструмент для CI-gate:

```sh
# Human-readable отчёт
nova consume-analyze src/

# JSON (для CI/CD)
nova consume-analyze src/ --format json

# Завершить с ненулевым кодом при uncovered bindings
nova consume-analyze src/ --fail-on-uncovered
```

JSON output:
```json
{
  "schema": "nova-consume-analyze/v1",
  "total_bindings": 42,
  "covered": 42,
  "uncovered": 0,
  "files": []
}
```

---

## C codegen fix (errdefer hoisting)

**Проблема (до D166):** Когда consume-переменная объявлена
(`consume tx = begin()`) ДО `errdefer { tx.rollback() }` в том же
блоке, C codegen генерировал errdefer handler (`setjmp` ветку)
ПЕРЕД объявлением переменной. Clang/GCC отклоняли как
"use of undeclared identifier".

**Решение:** `enter_defer_scope` pre-hoists все переменные,
referenced в errdefer-телах, как `Type* var = NULL;` перед setjmp.
`emit_stmt` для `Stmt::Let` обнаруживает pre-declared var и emits
только assignment (`var = rhs;`), без повторного объявления типа.

Поле: `CEmitter::hoisted_let_vars: HashSet<String>`.

---

## LSP quick-fixes (D166)

| Code | Trigger | Quick fix |
|------|---------|-----------|
| `D133-not-consumed` | consume var not consumed | Suggest method call |
| `D133-type-marker-missing` | type has consume-fields but no `consume` keyword | Add `consume` to type decl |
| `D162-uncovered-error-path` | no errdefer in failable fn | Add `errdefer { var.method() }` |
| `D162-uncovered-success-path` | has errdefer but no success commit | Add `okdefer` or explicit call |

Suggestions с `Applicability::MachineApplicable` могут применяться
автоматически через LSP code action API.

---

*Реализован в Plan 100.8 (D166), 2026-05-26.*
