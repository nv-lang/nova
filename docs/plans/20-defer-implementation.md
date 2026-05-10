# План 20: D90 implementation — `defer` / `errdefer`

**Статус:** 🟡 **DRAFT — implementation pending**.
**Дата создания:** 2026-05-10.
**Зависимости:** [D90](../../spec/decisions/03-syntax.md#d90-defer-и-errdefer--scope-level-cleanup-statement).
**Блокирует:** [Plan 21](#) (D91 Channel revision — использует `defer tx.close()`).

---

## Цель

Реализовать `defer` и `errdefer` scope-level cleanup statements
согласно D90 spec'е. Production-grade: без упрощений, покрывает все
позиции в expression-AST где scope может exit'нуть (normal,
return, throw, panic, interrupt), правильная LIFO-семантика,
errdefer запускается только на throw/panic.

---

## Декомпозиция

### Ф.1. Lexer + keyword reservation

**Что:** Добавить `defer` и `errdefer` в список зарезервированных
keyword'ов (см. [D83](../../spec/decisions/03-syntax.md#d83)).

**Файлы:**
- `compiler-codegen/src/lexer/token.rs` — `TokenKind::KwDefer`,
  `TokenKind::KwErrDefer`.
- `compiler-codegen/src/lexer/mod.rs` — recognise keywords.

**Тесты:** Unit-тесты в lexer на recognition.

**Объём:** ~30 строк.

### Ф.2. Parser — statement-level grammar

**Что:** В `parse_statement` добавить две формы:
```
statement = ...
          | 'defer' body
          | 'errdefer' body
body      = expression | block
```

**AST:**
```rust
pub enum StmtKind {
    ...
    Defer { body: Box<Expr>, span: Span },
    ErrDefer { body: Box<Expr>, span: Span },
}
```

`body` — это `Expr` (block-expression если `{ ... }`, обычный expr
иначе). Парсится через `parse_expression()` который уже умеет
оба варианта.

**Файлы:**
- `compiler-codegen/src/ast/mod.rs` — `StmtKind::Defer`, `StmtKind::ErrDefer`.
- `compiler-codegen/src/parser/mod.rs` — `parse_defer_stmt` /
  `parse_errdefer_stmt`.

**Тесты:** parse-тесты на разные формы (expr body, block body, mixed
с другими statement'ами).

**Объём:** ~150 строк.

### Ф.3. Type-checker — body constraints

**Что:** Проверки на body `defer`/`errdefer`:

1. **No `Fail`-эффект.** Тело body не должно иметь `Fail[E]` в
   effect-set. Если использует `throw`/`?`/`!!` — compile error.
2. **No suspend.** Запрещены вызовы функций с эффектами:
   `Net`, `Fs` (read/write), `Db` (только blocking-варианты —
   проверяется через D62 effect-list), `Time.sleep`, `Channel.recv`
   (blocking), `parallel for`, `spawn`, `supervised`, `select`.
3. **No exit-control.** `return`, `throw`, `break`, `continue` в body
   — compile error (нельзя hijack exit-семантику scope'а).

Все три — отдельные walking-passes по body AST.

**Файлы:**
- `compiler-codegen/src/types/mod.rs` — новые проверки в
  `check_module` или в отдельном `defer_check`-pass'е.

**Тесты (negative, через EXPECT_COMPILE_ERROR):**
- `defer file.read()?` — fail (требует Fail).
- `defer Time.sleep(1)` — fail (suspend).
- `defer return 42` — fail (exit-control).
- `defer throw E` — fail (exit-control + Fail).
- `defer break` — fail (exit-control).
- `errdefer { return }` — fail.

**Объём:** ~250 строк (3 walking-passes + diagnostics).

### Ф.4. Codegen — scope-stack defer-callbacks

**Что:** На каждом scope'е (function body, if-branch, for-body,
with-block, supervised-body) ведём stack defer'ов. На exit scope'а —
invoke в LIFO.

**Стратегия:** Generate C-кода с явными labels + goto-cleanup.
Это классический паттерн для defer-in-C (Go runtime, Zig codegen).

#### C-codegen pattern

```c
// Nova:
fn f() Fs -> () {
    let file = Fs.open("a")
    defer file.close()
    let temp = Fs.create_temp()
    errdefer temp.cleanup()
    Db.exec(...)
}

// → C-emit:
static void nv_f(void) {
    NovaFs_Handle file;
    NovaFs_Handle temp;
    int defer_count = 0;            // bitmask активных defer'ов
    NovaFailFrame frame;

    nova_fail_push(&frame);
    if (setjmp(frame.jmp) != 0) {
        // exit через throw/panic — invoke defer'ы + errdefer'ы LIFO
        if (defer_count >= 2) Nova_Fs_temp_cleanup(&temp);   // errdefer
        if (defer_count >= 1) Nova_Fs_file_close(&file);     // defer
        nova_fail_pop();
        nova_throw(frame.error_msg);                          // rethrow
    }

    file = Nova_Fs_open((nova_str){.ptr="a",.len=1});
    defer_count = 1;
    temp = Nova_Fs_create_temp();
    defer_count = 2;
    Nova_Db_exec(...);

    // normal exit — invoke только defer'ы (НЕ errdefer'ы) LIFO
    Nova_Fs_file_close(&file);                                // defer
    nova_fail_pop();
}
```

**Ключевые моменты:**

1. **Per-scope counter** (`defer_count`) — bitmask какие defer'ы
   активны. Активируется при выполнении соответствующего statement'а.
   Защищает от invoke не-инициализированного defer'а (если throw
   до regestration).

2. **NovaFailFrame** — уже существует в `nova_rt/effects.h` для
   `throw`-обработки. Расширяем pattern: при longjmp в frame —
   сначала invoke active defer'ы и errdefer'ы (LIFO), потом
   rethrow.

3. **Normal exit** — invoke только defer'ы (errdefer skip).

4. **NEST**: nested scope'ы (вложенный if внутри функции) — каждый
   ведёт свой defer-stack. На exit из inner scope — invoke только
   его defer'ы. NovaFailFrame пушится для каждого scope с defer'ами.

**Файлы:**
- `compiler-codegen/src/codegen/emit_c.rs` — extend block-emit
  logic. Per-block-tracking: `Vec<DeferEntry>` с типом (Defer / ErrDefer)
  + body-expr. Emit на enter/exit block.
- `compiler-codegen/nova_rt/effects.h` — возможно расширить
  NovaFailFrame если нужны новые поля; в основном переиспользуем.

**Тесты (positive, runtime):**
- defer вызывается на normal exit.
- defer вызывается на return.
- defer вызывается на throw.
- defer вызывается на panic (но не на exit).
- errdefer вызывается на throw/panic, **не** на normal/return.
- LIFO order — 3 defer'а в правильном порядке.
- Nested scope'ы — defer срабатывает на exit inner, не на exit outer.
- defer в if-branch — срабатывает на exit if.
- defer с captures — переменные доступны.

**Объём:** ~600 строк (новая block-emit logic + nested handling +
NovaFailFrame integration).

### Ф.5. Interp — eval logic

**Что:** В interp поддержать defer/errdefer семантику.

**Стратегия:** Per-scope `Vec<DeferEntry>`. На exit scope (любой
способ) — invoke LIFO. Errdefer — флаг «is_error_exit», invoke
только если true.

**Файлы:**
- `compiler-codegen/src/interp/mod.rs` — `eval_block` с
  defer-stack, на exit (нормальный или через `Flow::Throw` /
  `Flow::Return`) — invoke.

**Объём:** ~200 строк.

### Ф.6. Тесты positive + corner cases

**Файлы:**
- `nova_tests/syntax/defer.nv` — все positive cases.
- `nova_tests/syntax/errdefer.nv` — errdefer cases.
- `nova_tests/negative_capability/defer_*.nv` — 6 negative
  (Ф.3-list).

**Pilot tests из Ф.4:**
1. `defer_simple.nv` — basic file-close.
2. `defer_lifo.nv` — 3 defer'а LIFO.
3. `defer_throw.nv` — defer на throw + errdefer.
4. `defer_nested.nv` — vложенный scope.
5. `defer_captures.nv` — capture mut-переменных.
6. `errdefer_normal.nv` — errdefer skip на normal exit.
7. `errdefer_throw.nv` — errdefer выполняется на throw.

**Объём:** ~13 файлов тестов, ~400 строк суммарно.

### Ф.7. Spec uplift и docs

**Что:**
- Обновить D90 Bootstrap-status: 🟡 → ✅.
- Добавить cross-refs в `effects.md`, `syntax.md` где упоминаются
  cleanup-pattern'ы.
- `docs/test-conventions.md` — добавить пример defer-теста.

**Объём:** ~50 строк правок.

---

## ⚠️ Атомарность фаз

Ф.1 (lexer) + Ф.2 (parser) + Ф.3 (type-check) + Ф.4 (codegen) +
Ф.5 (interp) + Ф.6 (тесты) — **один атомарный PR**. Промежуточные
состояния нелегальны:

- Lexer без parser: `defer` token есть, но не парсится.
- Parser без type-check: тесты compile но не валидируют ограничения.
- Type-check без codegen: статья не запускается.
- Codegen без interp: одна mode работает, другая — нет.

Ф.7 (docs) можно в том же PR или отдельным follow-up.

---

## Порядок исполнения

| # | Фаза | Зависимости | Атом? |
|---|---|---|---|
| Ф.1 | Lexer keywords | — | **A** |
| Ф.2 | Parser statements | Ф.1 | **A** |
| Ф.3 | Type-checker | Ф.2 | **A** |
| Ф.4 | Codegen | Ф.3 | **A** |
| Ф.5 | Interp | Ф.3 | **A** |
| Ф.6 | Тесты | Ф.4, Ф.5 | **A** |
| Ф.7 | Spec uplift + docs | Ф.6 | post-A |

**A** = атомарный PR. Ф.7 — отдельный коммит после.

---

## Риски

1. **NovaFailFrame integration.** Defer'ы пересекаются с
   throw-machinery (через NovaFailFrame). Эта machinery уже
   используется assert'ом и spawn-throw. Нужна careful integration —
   defer push'ит на тот же stack, но invoke в обратном порядке от
   nova_throw. **Mitigation:** unit-тест на «throw inside defer
   inside throw» — двойная фаза cleanup.

2. **Mut-captures.** Если defer-body capture'ит mut-переменную через
   reference (D32), eager evaluation может конфликтовать с lazy
   capture. **Mitigation:** spec D90 чётко говорит «eager arguments,
   lazy closure» — closure capture идёт через managed-heap
   reference, передача аргументов eager. Reproduce в тестах
   `defer_captures.nv`.

3. **Nested scope'ы и goto-style codegen.** Каждый scope с defer'ами
   требует NovaFailFrame push + catch-on-exit. Generated C может
   стать verbose. **Mitigation:** оптимизация — frame пушится
   только если в scope есть defer'ы (не для пустых scope'ов).

4. **Interaction с `with`-блоком.** `with X = h { body }` —
   exit через нормальное завершение body или через interrupt.
   defer/errdefer в body — должны invoke перед exit'ом with-блока.
   **Mitigation:** with-блок ведёт свой scope, defer'ы инкапсулированы.

5. **Suspend-обнаружение.** Ф.3 запрещает suspend в body, но что
   считать suspend? Список из spec D90: `Net`, `Fs`, `Db`, `Time.sleep`,
   `Channel.recv` (blocking), `parallel for`, `spawn`, etc. Текущий
   type-checker имеет effect-tracking — проверять через intersect
   с `SUSPEND_EFFECTS` set. **Mitigation:** константа в codegen
   `const SUSPEND_EFFECTS: &[&str] = &["Net", "Fs", "Db", "Time", ...]`,
   проверка intersect с body's effect-set.

---

## Definition of Done

- [ ] Ф.1-Ф.6 атомарный PR замерджен; полный test-suite **0
  regressions**.
- [ ] 7 positive pilot-тестов проходят.
- [ ] 6 negative-тестов через `EXPECT_COMPILE_ERROR` проходят.
- [ ] D90 Bootstrap-status обновлён до ✅.
- [ ] Запись в `docs/project-creation.txt` + `docs/simplifications.md`.
- [ ] discussion-log в nova-lang-private обновлён.

---

## Связь с другими планами

- [Plan 21](#) (после переименования из Plan 22) — D91 Channel
  revision использует `defer tx.close()`. Plan 21 ждёт Plan 20.
- [D90 spec](../../spec/decisions/03-syntax.md#d90) — нормативная
  спецификация.
- [D6](../../spec/decisions/05-memory.md#d6) — managed heap без
  RAII, мотивирует defer.
- [D85](../../spec/decisions/04-effects.md#d85) — `?`/`!!` запрещены
  в defer body (Fail).
- [D13](../../spec/decisions/08-runtime.md#d13) — `exit` не
  запускает defer'ы.
