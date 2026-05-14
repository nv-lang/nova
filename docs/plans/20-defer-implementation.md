# План 20: D90 implementation — `defer` / `errdefer`

**Статус:** ✅ **ЗАКРЫТ** — реализовано: лексер `KwDefer`/`KwErrdefer`,
AST `Stmt::Defer`/`ErrDefer`, codegen LIFO defer-scope
(`enter_defer_scope`/`leave_defer_scope`/`emit_defer_body_void`), ~27
тестов (`nova_tests/syntax/` + `expected_runtime/` panic-interaction +
`negative_capability/`) — все в полном прогоне.
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

---

## Production-grade hardening (Ф.8) — после первичного закрытия Plan 20

> **Статус:** в работе (2026-05-11, продолжение).
> **Контекст:** Plan 20 Ф.1-Ф.7 закрыт, но при retro обнаружены упрощения,
> отвечающие критерию «не production-grade». Эта фаза закрывает их все.

### 4 issue для закрытия

**(1) Type-check enforcement: handler-method для Never-op обязан exit-control.**

Spec D61 (стр. 1430-1434) требует: handler-method для `fail() -> Never`
ОБЯЗАН закончиться `interrupt v` или `throw err` (нет значения типа
Never для return). Сейчас type-checker не enforce'ит это — runtime
ловит nonspec handler через safety net (`Nova_Fail_fail` делает
`nova_throw` после handler-return). Фикс: добавить compile-error
проверку в `types/mod.rs` при analyse'е handler-литералов для
эффектов с Never-операциями.

**(2) Defer/errdefer на interrupt-path.**

D90 п.8: «defer запускается на любом exit'е, включая interrupt».
Текущий codegen интегрирует defer cleanup в `leave_defer_scope`
(normal exit) и в NovaFailFrame setjmp (throw-path). На
interrupt-path (longjmp на NovaInterruptFrame, минуя оба) defer
cleanup НЕ запускается. Фикс: интегрировать defer cleanup через
NovaInterruptFrame setjmp wrapper аналогично errdefer pattern, либо
эмитить cleanup перед каждой interrupt-jump эмиссией.

**(3) Loop-body integration в 19+ inline-iteration сайтов.**

Только `emit_for` (range case) переписан на `emit_loop_body_inline`.
Остальные 19+ сайтов с `for stmt in &body.stmts { emit_stmt }`
(for-in-array, while, while-let, loop, match-arm/if-branch bodies,
spawn/supervised body) — legacy inline-iteration. Defer внутри
них **не зарегистрируется** в DeferScope. Фикс: переписать все
сайты через `emit_loop_body_inline` (или общий
`emit_block_inline_stmts`).

**(4) D65 правило 3: re-throw в handler skip current frame.**

Когда `throw err` происходит внутри handler-body, runtime должен
dispatch'нуться на OUTER handler (skip current — иначе infinite
recursion). Сейчас `Nova_Fail_fail` всегда смотрит на
`_nova_handler_Fail`, который при re-throw указывает на тот же
handler. Фикс: emit_with codegen должен сохранять prev handler в
local, и handler-impl (codegen `_nova_handler_lit_N_impl_Fail_fail`)
эмитит swap `_nova_handler_Fail = saved_prev` в prologue, restore
в epilogue. Альтернатива: runtime tracking handler-stack с явным
skip-current.

### Зависимости

- (4) — самая глубокая (codegen emit_with + runtime). Блокирует
  positive-тесты для errdefer-on-rethrow (`syntax/errdefer_throw.nv`).
- (2) — отдельный codegen path, не блокирует другие.
- (3) — механическая работа, не блокирует.
- (1) — type-check addition, после неё nonspec handler'ы в тестах
  отвергаются.

### Порядок исполнения

1. **(1) Type-check** — изолированный, отлавливает nonspec handler'ы
   в новых тестах (catch errors early).
2. **(3) Loop-body** — механический рефакторинг + positive-тесты
   для каждого паттерна (if-branch, while, match-arm, etc.).
3. **(4) D65 re-throw** — codegen emit_with rewrite + Nova_Fail_fail
   адаптация. После — позитив-тест для errdefer-on-rethrow.
4. **(2) Defer-on-interrupt** — InterruptFrame integration в
   emit_with codegen + позитив-тест.

### Positive-тесты для каждого fix'а

- (1): negative-тест `negative_capability/fail_handler_no_exit_control_rejected.nv`
  с handler-method без interrupt/throw — должен fail compile.
- (3): positive-тесты `syntax/defer_in_*.nv` для каждого паттерна
  (if-branch, while, while-let, match-arm, supervised body, etc.).
- (4): `syntax/errdefer_rethrow.nv` — errdefer срабатывает между
  inner-handler-rethrow и outer-handler.
- (2): `syntax/defer_on_interrupt.nv` — defer срабатывает на
  interrupt-path; errdefer корректно skip'ается (это handled exit).

### Definition of Done (Ф.8)

- [ ] Type-check rejection для handler-without-exit (D61 enforcement).
- [ ] Все 19+ inline-iteration сайтов переписаны на emit_loop_body_inline.
- [ ] Positive-тесты defer-in-* для каждого block-pattern.
- [ ] D65 re-throw работает: errdefer срабатывает между inner-handler-rethrow
  и outer-handler-catch.
- [ ] Defer срабатывает на interrupt-path; errdefer корректно skip.
- [ ] Все existing тесты PASS, ноль regressions.
- [ ] Spec D90 обновлён: Bootstrap-status описывает interrupt-path coverage.
- [ ] simplifications.md retro: Ф.8 hardening закрыл 4 упрощения.
