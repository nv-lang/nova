# Упрощения и отложенные доработки

Живой список осознанных упрощений, сделанных в ходе разработки.
Каждое упрощение попадает сюда в момент принятия решения — чтобы не потерять контекст.

> **Принцип** (см. [`project-philosophy.md`](project-philosophy.md)): Nova не в
> проде, революционный язык важнее обратной совместимости. Упрощения здесь —
> **временные**, должны закрываться по мере роста проекта. Каждое имеет
> rationale и roadmap. **Не использовать этот документ как тихое разрешение
> оставлять tech-debt без плана.**

Формат:
- **Где** — файл/модуль.
- **Что упрощено** — что НЕ делается.
- **Почему** — trade-off на момент принятия.
- **Как чинить** — краткий план.
- **Приоритет** — L / M / H.

---

## Codegen (emit_c.rs)

### [C1] Массивы — только nova_int, нет полиморфизма
- **Где:** `emit_c.rs` / `nova_rt/array.h`
- **Что упрощено:** `NovaArray_T` инстанцирован только для `nova_int`. Массивы других типов (str, bool, record) не поддержаны. Тип элемента всегда `nova_int` в codegen.
- **Почему:** Без type inference невозможно определить тип элемента статически. Достаточно для demo.nv.
- **Как чинить:** Добавить анализ AST (рекурсивный infer типа первого элемента), инстанцировать NOVA_ARRAY_DECL/IMPL для каждого встреченного типа.
- **Приоритет:** M

### [C2] infer_expr_c_type — best-effort без полного type checking
- **Где:** `emit_c.rs` → `infer_expr_c_type`
- **Что упрощено:** Тип выражений инферится эвристически (AST-based, без полного анализа). Может ошибаться для сложных выражений (цепочки вызовов, generics).
- **Почему:** Полный type inference требует отдельного прохода и системы типов. В 90% случаев эвристика достаточна.
- **Как чинить:** Прогнать type checker перед codegen, передавать типы через аннотированный AST.
- **Приоритет:** H (системная проблема, проявится при расширении языка)

### [C3] Match — тип результата из первого arm
- **Где:** `emit_c.rs` → `infer_expr_c_type(Match)` и `emit_match`
- **Что упрощено:** Тип результата match выражения берётся из первого arm который не unit. Может быть неправильным если arms имеют разные типы.
- **Почему:** Без unification нельзя вычислить least upper bound типов.
- **Как чинить:** Type checker.
- **Приоритет:** M

### [C4] Option только через NovaOpt_nova_int
- **Где:** `emit_c.rs` / `nova_rt/array.h`
- **Что упрощено:** `Some`/`None` паттерны работают только для `NovaOpt_nova_int`. При match на других Option-like типах не будет правильного bind.
- **Почему:** Следствие [C1].
- **Как чинить:** Generics в runtime, NOVA_ARRAY_IMPL для каждого типа.
- **Приоритет:** M

### [ЗАКР] for-in только для Range (0..N или m..n) — [C5]
- **Закрыто:** Добавлена ветка для `NovaArray_T*` в `emit_for`.
  `for n in arr` генерирует `for (int64_t _i=0; _i<arr->len; _i++) { T n = arr->data[_i]; ... }`.
  Тип элемента выводится через `infer_expr_c_type`. Тест: `nova_tests/39_for_in_array.nv` (11 assert).

### [ЗАКР] Generics — не реализованы (mangle как Nova_Name) — [C6]
- **Закрыто:** Реализован type erasure для generics (2026-05-06):
  - Generic free functions: T-params → void*, return → void*; call sites box args
  - Generic records: T-fields → void*, []T → NovaArray_nova_int*
  - void* boxing: nova_int через (void*)(intptr_t)(v), nova_str через heap-ptr
  - Tuple returns: generic_fn_tuple_arity + tuple_element_types для p.0/p.1 access
  - Generic methods: arg boxing на call sites, void*→nova_int cast в bodies
  - Match arm coercion: nova_int↔nova_str в erased contexts
  - Все 39 тестов проходят, включая 19_generics и 33_stack_queue.
- **Остаток:** Stack[str] работает через pointer-as-int — значения корректны только для Stack[int]. Полная монаморфизация нужна для Stack[str].
- **Приоритет:** M (монаморфизация)

### [C7] Index выражения — прямое разыменование без bounds check
- **Где:** `emit_c.rs` → `ExprKind::Index`
- **Что упрощено:** `arr[i]` генерируется как `arr[i]` (через указатель или массив C). Нет bounds checking.
- **Почему:** Добавит overhead. В прототипе допустимо.
- **Как чинить:** Выражение вида `nova_bounds_check(arr, i)` или через .get().
- **Приоритет:** L

### [C8] println — тип аргумента по эвристике AST
- **Где:** `emit_c.rs` → `make_print_call` / `infer_print_helper`
- **Что упрощено:** Выбор `nova_print_int` vs `nova_print_str` vs `nova_print_bool` основан на AST-анализе (не типах). Может ошибаться для переменных сложных типов.
- **Почему:** Без type checker нет другого способа.
- **Как чинить:** Type checker с аннотированным AST.
- **Приоритет:** M

### [C9] pre-scan — два прохода, handler/spawn IDs должны совпадать
- **Где:** `emit_c.rs` → `emit_handler_forward_decls` + `emit_fn`
- **Что упрощено:** Pre-scan использует отдельные счётчики, которые должны совпадать с основным проходом. При изменении кодогенерации это хрупко.
- **Почему:** Нужно для forward declarations в одном файле без второго буфера.
- **Как чинить:** Первый проход собирает все handler/spawn в список, второй их использует.
- **Приоритет:** M

---

## Runtime (nova_rt/)

### [R10] Fiber-throw + cooperative cancellation propagation
- **Где:** `nova_rt/fibers.h` (per-fiber fail-frame switching, cancel flag) +
  `emit_c.rs::emit_spawn` (setjmp wrapper) + `Stmt::Throw` (теперь nova_throw).

#### Что реализовано (2026-05-06)
1. **Per-fiber fail-frame chain.** `_nova_fail_top` (thread-local stack
   setjmp-frame'ов) теперь switching: `nova_supervised_step` сохраняет
   текущий top, ставит fiber'у его сохранённый chain (NULL для нового),
   делает `mco_resume`, после resume сохраняет fiber'овский chain
   обратно в `q->fiber_fail_top[i]` и восстанавливает outer top.
2. **Spawn-entry оборачивает body в setjmp.** Codegen `emit_spawn` теперь
   эмитит:
   ```c
   NovaFailFrame _ff;
   nova_fail_push(&_ff);
   if (setjmp(_ff.jmp) == 0) { ...body... nova_fail_pop(); }
   else { nova_fail_pop(); nova_fiber_report_error(_ff.error_msg.ptr); }
   ```
   `throw` внутри body → longjmp в `_ff` (frame на ЭТОЙ fiber-stack'е,
   safe), error пишется в scope queue, fiber завершается чисто.
3. **Cooperative cancellation.** `nova_fiber_report_error` ставит
   `q->cancel_requested = true`. `nova_fiber_yield` перед `mco_yield`
   проверяет флаг — если установлен, `nova_throw("scope cancelled")`,
   который ловится тем же spawn-entry frame'ом. Этот fiber умирает,
   scope переходит к следующему.
4. **Scope rethrow на main.** `nova_supervised_run` после полного drain'а
   проверяет `q->first_error` и если он не NULL — `nova_throw` на
   main-flow. Это безопасно: longjmp идёт по main-stack'у.
5. **`Stmt::Throw` теперь использует `nova_throw`** (раньше был
   `abort()`). Без активного fail-frame nova_throw тоже abort'ит, но
   с сообщением — нормальный graceful path.

#### Почему именно так

**Альтернатива 1: единый thread-local fail-frame (без switching).**
Изначально `_nova_fail_top` был один на thread. Когда fiber A push'ит
frame, yield'ит, fiber B push'ит frame — top.prev указывает на A's
frame, **но A's frame на A's stack'е**. Если B throw'ит → longjmp в
B's frame OK, но если B fail-pop'нет и потом throw'ит на следующем
уровне — top уже A's frame, longjmp пересекает fiber boundary → UB.
Поэтому **switching обязателен**.

**Альтернатива 2: NovaFiberMeta (extension struct в user_data).**
Вместо хранения fail_top в queue хранить в `user_data` через wrapper-
struct `{ NovaSpawnCtx*, fail_top }`. Это потребовало бы изменить
ВСЕ обращения к ctx через прокси-структуру — десятки мест в codegen.
Слишком много change'й. Queue-side storage концентрирует сложность
в одном месте (fibers.h).

**Альтернатива 3: per-fiber dynamic fail-stack.** Хранить указатель
на fail-stack head в `mco_user_data`, на пути save/restore через
обёртки. Сложнее, требует custom user_data routing. Queue-side
проще на 30% кода.

**Cooperative cancellation, не preemptive.** Альтернатива —
preemption (timer-based safepoint check, как Go 1.14+). Требует
сигнал-доставки и safepoint-кода в каждом цикле. Большая работа,
явно отложена до production. Cooperative — норма Erlang/OCaml 5,
spec-faithful по D14/D62.

**Cancel-через-throw, не через флаг-проверку в каждой операции.**
Альтернатива — Go-style context.Done() где fiber сам проверяет.
Это требует API канала. Throw — простой re-use существующего
fail-frame mechanism'а; fiber просто умирает на следующем yield.

#### Что НЕ реализовано (приоритеты)

**[ЗАКР] Positive-тесты на real throw → catch на main (2026-05-06).**
`with Fail = handler Fail { fail(msg) { ... } } { body }` реализован
в codegen + рантайме (Fail pre-registered как built-in эффект,
`throw msg` desugared to `Nova_Fail_fail(msg)` → vtable dispatch →
user handler). Тесты в `nova_tests/45_fail_handler.nv` (7 тестов:
main-flow happy/sad path, divide-by-zero, throw-from-spawn caught,
multiple-fibers throw, cancellation peer behavior). `try/catch`
синтаксис rejected по spec — единственный способ перехвата это
handler через `with`.

**[M] Не-cooperative cancellation.**
Fiber без yield-точек продолжит работу до конца body, даже если
scope cancelled. Это норма для cooperative-only scheduler'а
(Trio, Kotlin coroutines), но в production нужен preemption на
backedge'ах циклов и function entries.
- **Roadmap:** добавить safepoint-полл в codegen for-loop / function-
  entry; timer-based signal в runtime.

**[ЗАКР] `nova_assert` внутри fiber'а — fail-frame routing (2026-05-06).**
До фикса: `nova_assert` в fiber-body делал longjmp на `_nova_test_frame`,
который живёт на main-coroutine-stack — пересечение mco-границы (UB).
После фикса: `nova_assert` проверяет `nova_in_fiber()`. Если true —
longjmp идёт через `_nova_fail_top` (per-fiber chain, который пушится
в spawn-entry). Spawn-entry catch'ит, scope-runner re-throw'ит на
main flow через `nova_throw`; test runner ловит через дополнительный
`_tf_fail` NovaFailFrame. Если false (main flow) — старый путь через
`_nova_test_frame`. Тест `nova_tests/concurrency/assert_in_fiber.nv` (4 теста:
simple spawn, parallel for, after Time.sleep yield, nested supervised).

**[ЗАКР] `interrupt v` через mco-coroutine-boundary (2026-05-07).**
По spec D61/D65 handler-method для Fail (`fail() -> Never`) завершается
через `interrupt v`, не через `return`/trailing. До фикса: если
fail-handler установлен снаружи `supervised`, а throw случается в
spawn-body, `nova_interrupt(v)` делал longjmp на with-frame на main-
stack — пересечение mco-границы, exe crash.

После фикса (runtime):
- `NovaFiberQueue` имеет per-fiber `fiber_interrupt_top[i]` (как
  `fiber_fail_top[i]`), switch'ится в `nova_supervised_step`.
- `NovaFiberQueue.interrupt_pending/interrupt_value` — pending
  interrupt от fiber'а.
- `nova_interrupt(v)`: если `_nova_interrupt_top != NULL` — direct
  longjmp (fiber-local или main-flow with). Если `NULL` И fiber
  активен — set'ит `scope.interrupt_pending = true` + `cancel_requested
  = true` + longjmp на fiber-local fail-frame с sentinel-msg
  `"__nova_interrupt__"`. Spawn-entry catch detect'ит sentinel и
  пропускает `nova_fiber_report_error`. `nova_supervised_run`
  после drain re-issue'ит `nova_interrupt(value)` на main-flow.
- Тесты `nova_tests/effects/fail_handler.nv` — все 7 spec-compliant
  через `interrupt ()` (раньше использовали bootstrap-leniency
  `return ()` — теперь это spec-correct).

**[ЗАКР] Cancel-token API — D75 (2026-05-06).**
`cancel_scope { tok => body }` keyword, `NovaCancelToken` first-class
type, `tok.cancel()`/`is_cancelled()`/`bind()` методы. Реализовано
поверх существующего `cancel_requested` flag из D71. Bind даёт
каскадную отмену (parent.cancel() → child тоже cancel'ится).
- **Тесты:** `nova_tests/52_cancel_scope.nv` (5 тестов).
- **Известные ограничения:** см. D75 «Известные ограничения
  bootstrap-реализации» — re-throw на main приходит как plain
  nova_throw (user `with Fail` handler не вызывается для cancel-throw),
  NOVA_CANCEL_LINKED_CAP=8.

#### Roadmap к полноценной реализации (порядок)

1. ~~**Top-level `try/catch`**~~ → **rejected by spec.** Заменяется
   через `with Fail = handler { ... }` (см. п. 3). **Сделано
   (2026-05-06): nova_tests/45_fail_handler.nv** — 7 positive-тестов
   на throw-paths, в т.ч. throw-from-spawn caught, multi-fiber, cancel.
2. ~~**`_nova_test_frame` switching per-fiber**~~ — **сделано (2026-05-06).**
   nova_assert роутится через nova_in_fiber()/_nova_fail_top.
3. ~~**`with Fail = ... { body }`**~~ — **сделано (2026-05-06).**
   Fail pre-registered как built-in эффект, throw → vtable dispatch.
4. **Preemptive cancellation** — на безopiate-полла (function entry,
   loop backedge). Добавить флаг проверки → `nova_throw("cancelled")`
   если cancel_requested. Аналог Go 1.14+ preemption.
5. **`cancel_scope { tok => ... }`** (D50) — двусторонний cancel
   token. tok.cancel() извне сигналит fibers'ам.

- **Приоритет верхнеуровневой задачи:** M (после [H] try/catch
  работа по [M] preemption и `_nova_test_frame` относительно мала).

### [R9] NovaFiberQueue — фиксированный capacity (1024)
- **Где:** `nova_rt/fibers.h` (NOVA_SCOPE_CAP)
- **Что упрощено:** Очередь fiber'ов в `supervised` scope — фиксированный массив
  `mco_coro* fibers[1024]`. При попытке добавить 1025-й fiber — runtime abort с
  сообщением "supervised scope exceeded NOVA_SCOPE_CAP".
- **По спеке (D14):** ограничения на количество fiber'ов нет ("миллион fiber'ов
  на машину — норма как Erlang"). Это чистое bootstrap-ограничение.
- **Почему:** Динамический массив требует realloc при росте — лишняя сложность
  для bootstrap.
- **Как чинить:** заменить fixed-array на `mco_coro** fibers; int cap;` с
  geometric growth (cap *= 2 при заполнении). ~1 час работы.
- **Приоритет:** L (для большинства тестов 1024 хватает; миллион — отдельная задача
  на performance, требует benchmark'и).

### [R1] Аллокатор — malloc без free (по умолчанию)
- **Где:** `nova_rt/alloc.c`
- **Что упрощено:** `nova_alloc` → malloc, `nova_release` → no-op. Нет GC. Память течёт.
- **Почему:** Для прототипирования достаточно. Boehm GC доступен через `gc=boehm`.
- **Как чинить:** Включить RC (`gc=rc`) или Boehm GC (`gc=boehm`) через build_c.bat.
- **Приоритет:** L (Boehm GC уже есть как опция)

### [ЗАКР] parallel for — реализован — [R8]
- **Закрыто (2026-05-06):** keyword `parallel for x in iter { body }`.
  Десугарится в codegen в `supervised { for x in iter { spawn { body } } }`.
- **Закрыто (2026-05-06): array-mode `parallel for → []T`** (D71). Когда body имеет
  trailing-expression, форма возвращает `NovaArray_T*` (T ∈ {int, bool, f64, str}).
  Каждый fiber пишет результат в `result.data[idx]` по своему индексу — порядок
  записи в slots не зависит от порядка планирования. Реализация в `emit_parallel_for`:
  pre-allocate `NovaArray_T*` размера N (для Range — `end - start [+1]`; для ArrayLit —
  длина литерала), per-iteration ctx содержит `_nova_par_idx` + `_nova_par_result`,
  spawn body's trailing пишет в `_c->_nova_par_result->data[_c->_nova_par_idx]`.
  Без trailing — старая semantic (statement, unit). Spread в array literal не
  поддержан в v1 — degrade to unit.
- **Capture-by-value для immutable scalars:** spawn-capture теперь различает
  `let` (immutable) vs `let mut` (mutable). Immutable scalar (int/bool/f64/byte) →
  capture by value (snapshot в ctx struct). Всё остальное — by pointer (shared mut).
- **Heap-alloc ctx в supervised:** ctx-struct для spawn внутри supervised
  аллоцируется на куче (через nova_alloc), не на стеке — иначе все queued fibers
  внутри loop разделяют один stack-slot и видят последнее значение.
- **Loop-var регистрация:** range-loop в `emit_for` теперь регистрирует binding
  в `var_types` (как nova_int) — без этого capture не находил loop-переменную.
- **Тесты:** `nova_tests/41_parallel_for.nv` — 12 тестов statement-mode (interleaving,
  snapshot semantics). `nova_tests/50_parallel_for_array.nv` — 6 тестов array-mode
  (range/inclusive-range/array-lit → []int, yield-stable ordering, mix mut-capture +
  array-result, regression statement-mode).

### [R2] Fibers — partial structured concurrency (supervised есть, race/parallel/cancel — нет)
- **Где:** `nova_rt/fibers.h` / `emit_c.rs`
- **Что реализовано (2026-05-06):** `supervised { }` scope — round-robin scheduler через
  `NovaFiberQueue` + `nova_supervised_run`. Внутри scope `spawn` кладёт fiber в очередь,
  не запускает сразу; на выходе scope крутит resume по очереди пока все не завершатся.
  Точки yield: `Time.sleep(ms)` → `nova_fiber_yield()` (без timer-wheel, любой ms = один yield).
  Ёмкость очереди: NOVA_SCOPE_CAP=64.
- **Что упрощено:** Нет `parallel for`, `race`, `select`, `cancel_scope`, `with_timeout`.
  `spawn` вне `supervised` остаётся eager-blocking (legacy совместимость, по спеке должна
  быть compile error). `let r = spawn ...` внутри scope возвращает 0 (результат через
  shared mut, как в Go-style). Без cancellation и error-propagation между fibers.
  Размер очереди фиксированный (64), без roll-over.
- **Почему:** Минимальная реализация для interleave-тестов. Cancellation/error-propagation
  требуют интеграции с Fail-frame stack для каждого fiber'а.
- **Как чинить:** добавить cancel-channel в NovaFiberQueue, при error в одном fiber'е —
  ставить cancel-флаг для остальных, при выходе scope — propagate.
- **Приоритет:** M

### [R6] detach — keyword реализован, default handler = SyncDetach (inline)
- **Где:** `emit_c.rs::emit_detach` / spec D50
- **Что реализовано (2026-05-06):** keyword `detach { body }`, AST `ExprKind::Detach`,
  парсер, interp-стаб, codegen. В bootstrap'е default-handler = SyncDetach: body
  исполняется inline в потоке caller'а (как обычный block, без fiber-обёртки).
  Тесты: `nova_tests/40_detach.nv` (13 тестов на capture/control-flow/nesting/
  совместимость с supervised).
- **Что упрощено:**
  * Эффект `Detach` не объявлен в effect-system — компилятор не требует его в сигнатуре.
  * Нет реального глобального supervisor'а: detach исполняется inline, не на отдельном
    OS-thread'е, поэтому "переживёт caller'а" не реализовано (но spec явно описывает
    SyncDetach как валидный handler для тестов — bootstrap-default это и есть SyncDetach).
  * Нет панник-контейнмента (`LogAndDrop`): паника в detach распространится наружу.
- **Как чинить полностью:**
  1. Объявить `Detach` как effect; добавить compile-time проверку требования в сигнатуре.
  2. Сделать глобальный supervisor (OS-thread + queue), routes detach → background.
  3. Default handler `LogAndDrop`: panic в detach → log + сбросить fiber, не propagate.
- **Приоритет:** L

### [R7] Time.sleep(ms) — without timer-wheel (Time-as-effect REALIZED)
- **Где:** `nova_rt/effects.h`/`fibers.h` (vtable + dispatch) / `emit_c.rs`
  (Time pre-registered as built-in effect).
- **Что реализовано (2026-05-06):**
  * `Time` теперь обычный pre-registered эффект в codegen (D11/D62).
  * `Time.sleep(ms)` → `Nova_Time_sleep(ms)` идёт через handler-vtable.
  * `Time.now()` → `Nova_Time_now()` (default returns 0).
  * Default handler `_nova_time_default_sleep`: context-sensitive yield
    (fiber → `mco_yield`, supervised body → `nova_supervised_step`,
    top-level → no-op).
  * User override: `with Time = handler Time { sleep(ms) {...} now() {...} } { body }`
    устанавливает custom handler — для test fixtures с fixed clock
    или mock sleep. Работает (тесты `46_time_handler.nv`).
- **Что упрощено:** `ms` игнорируется в default handler — нет timer-wheel.
  `Time.sleep(100)` и `Time.sleep(0)` неотличимы. Реальной задержки нет.
- **Как чинить полноценно:** Timer-wheel/heap, при `Time.sleep(ms)` fiber
  кладётся в sleep-list с deadline, scheduler пропускает sleeping fibers
  до его наступления. Аналогично `Time.now()` нуждается в реальном
  c-clock (через QueryPerformanceCounter / clock_gettime).
- **Приоритет:** L (для тестов interleave не нужно).

### [R3] nova_str — borrowed slice, нет ownership
- **Где:** `nova_rt/nova_rt.h`
- **Что упрощено:** `nova_str` — `{const char* ptr, size_t len}`. Строки не копируются при присваивании. Строковые литералы — статические данные. Нет проверки lifetime.
- **Почему:** Копирование строк дорого и не нужно для прототипа.
- **Как чинить:** Ref-counted строки или arena allocation.
- **Приоритет:** L

### [R4] Массивы — нет release/GC при shrink или drop
- **Где:** `nova_rt/array.h`
- **Что упрощено:** `nova_array_push` при росте аллоцирует новый буфер через `nova_alloc` но не освобождает старый (alloc.c — malloc без free). При смене на RC нужно явно release старый буфер.
- **Почему:** Пока alloc.c не освобождает ничего — не критично.
- **Как чинить:** При смене на RC — добавить `nova_release(a->data)` перед `a->data = new_data`.
- **Приоритет:** M (при включении RC)

---

## Спецификация (spec/)

### [S1] Q1 — @-методы для эффектов не определены
- **Что упрощено:** Синтаксис `effect.method()` через `@`-синтаксис остался открытым.
- **Приоритет:** L

### [S2] Q5 — граница Panic (stack overflow, assertion failures)
- **Что упрощено:** Что именно является recoverable Panic не зафиксировано.
- **Приоритет:** M

### [S3] Q6 — effect polymorphism синтаксис
- **Что упрощено:** Передача handler-объекта как параметра функции не оформлена в синтаксис.
- **Приоритет:** M

### [S4] Q9 — stdlib скелет
- **Что упрощено:** Нет stdlib. Всё что есть — примеры в examples/.
- **Приоритет:** H

### [S5] Q10 — tooling (LSP, package manager, hot reload)
- **Что упрощено:** Никакого tooling.
- **Приоритет:** M (после стабилизации языка)

---

## Закрытые

### [ЗАКР] Пустая ctx struct → MSVC C2016
- **Закрыто:** `char _dummy;` добавлен при пустом списке captures. MSVC требует ≥1 члена.

### [ЗАКР] Коллизия .obj файлов в build_c.bat
- **Закрыто:** Объектники кладутся в `%TEMP%\nova_build_%RANDOM%` с уникальными именами.

### [ЗАКР] Wildcard binding — C2374 при нескольких `let _ = spawn {}`
- **Закрыто:** `Pattern::Wildcard` → `fresh_tmp()` вместо фиксированного `"_nova_unused"`.

### [ЗАКР] Pre-scan не охватывал While/For/Loop/Match
- **Закрыто:** Добавлены все expression containers в `scan_expr_fwd`.

### [ЗАКР] examples/ разбросаны по compiler-codegen/ и compiler-bootstrap/
- **Закрыто:** Все .nv файлы перемещены в корневой `examples/`.

### [ЗАКР] GC и fibers не имели глубоких тестов
- **Закрыто:** `nova_rt/test_gc_deep.c` (23 assert, malloc+RC) и `nova_rt/test_fibers_deep.c`
  (31 assert) проверяют alloc_count/free_count/live_count, RC lifecycle, раздельность стеков
  fiber, yield/resume порядок, stack isolation, state machine через yield.
  На Nova-уровне: `nova_tests/37_deep_gc.nv` (18 тестов) и `38_deep_spawn.nv` (28 тестов).

### [ЗАКР] Spawn захватывал локальные переменные как внешние (C2065/C2020/C2440)
- **Закрыто:** Три бага исправлены при написании Nova-level deep тестов:
  1. `collect_bound_names`: имена из `let` внутри spawn, for-pattern, match-arm
     теперь исключаются из списка captures (были Nova_Point** вместо Nova_Point*).
  2. Поле результата ctx-struct переименовано `result` → `_nova_result` чтобы
     не конфликтовать с захваченной переменной пользователя named "result".
  3. `infer_expr_c_type`: добавлен кейс `ExprKind::If` — if без else → `nova_unit`
     (раньше → `nova_int`, что давало C2440 при cast результата spawn body).

### [examples/stdlib/] — 11 demo-файлов не компилируются в bootstrap'е (2026-05-06)
- **Где:** `examples/stdlib/*.nv`
- **Что:** complex, duration, hashmap, json, linkedlist, queue, range,
  semver, set, sql, vec — все 11 spec-faithful демо-файлов падают на
  codegen-stage. Подробный список причин см. `examples/stdlib/STATUS.md`.
  Группы блокеров: char-литералы, `&` operator, multi-line handler/if-else,
  `effect` keyword as type, anonymous record literal, `throw` в expression-
  position, generic-syntax парсера.
- **Почему:** Эти файлы — aspirational. Они написаны как «как Nova код
  должен выглядеть в зрелой версии», но bootstrap-codegen фокусировался на
  языковом ядре (concurrency, эффекты, типы) и не покрыл полный stdlib API.
- **Как запустить:** `.\run_tests.ps1 -IncludeStdlib` запускает обычный
  suite + 11 stdlib (опционально). По умолчанию — только nova_tests/.
- **Roadmap:** spec-clarifications (A: char-литералы; B: убрать `&` —
  Nova managed heap; G: throw expr position) → парсер (C, D, F) →
  codegen (E). Финальная цель: 11/11 stdlib PASS.
- **Приоритет:** M (важно для AI-кодинга — без stdlib в зелёном CI
  трудно генерировать пользовательский код, основывающийся на этих типах).

### [ЗАКР 2026-05-07] Q-buffer — Buffer mutable byte accumulator
- **Где:** `nova_rt/buffer.h` + `emit_c.rs` (special-case dispatch для
  Buffer.new/.with_capacity/.from + receiver-typed instance methods).
- **Что реализовано:** unified Buffer для bytes-buffer и string-builder
  (унификация vs Go bytes.Buffer + strings.Builder, Rust Vec<u8> + String).
  API: Buffer.new() / .with_capacity / .from(s str) / .from(b []byte);
  add_str/add_bytes/add_byte/add_char (UTF-8 encode 1-4 байта); len /
  capacity / clone; into() → []byte / try_into() → str (UTF-8 валидация
  через Nova_Fail_fail при ошибке) / into_str_unchecked() — escape hatch.
- **Тесты:** `nova_tests/55_buffer.nv` (16 тестов: basic ops, grow,
  clone independence, UTF-8 1/2/4-byte, hot-loop 1000-add).
- **Закрывает Q-buffer** (open-questions.md).

### [ЗАКР 2026-05-07] Q-char-literals — char literals 'a' / '\n' / '\u{...}'
- **Где:** `lexer/mod.rs` (lex_char) + `lexer/token.rs` (Char(u32)) +
  `ast/mod.rs` (ExprKind::CharLit + Literal::Char) + `parser/mod.rs`
  (parse_primary + parse_pattern) + `codegen/emit_c.rs` (char как
  nova_int в bootstrap'е).
- **Что реализовано:** ASCII char-литералы ('a'), escape sequences
  (\n / \t / \r / \\ / \' / \" / \0), Unicode escapes (\u{HEX}, до
  6 hex digits). Validation: surrogate (0xD800..0xDFFF) и > 0x10FFFF
  отвергаются. Pattern matching: match c { 'a' => ... }.
- **Тесты:** `nova_tests/56_char_literals.nv` (16 тестов: ASCII,
  escape, Unicode, match-pattern, Buffer.add_char, range-check).
- **Закрывает Q-char-literals**, разблокирует stdlib examples
  (complex.nv: 317→560, json.nv: 163→98).

### [ЗАКР 2026-05-07] Trailing-block в head-позиции control-flow
- **Где:** `parser/mod.rs` (no_trailing_block flag).
- **Что было:** `match foo() { Some(i) => ... }` парсился как
  call-with-trailing-block (`foo()` + блок). Падало с
  `unexpected '=>' in expression`.
- **Фикс:** добавлен `with_no_struct_or_trailing` (комбинация
  no_struct_lit + no_trailing_block). Применён в head-позициях
  match/if/while/for scrutinee.
- **Разблокировало:** semver.nv (136→251), sql.nv (201→295).

### [ЗАКР 2026-05-07] D26 prelude API — Option/Result методы + str API
- **Где:** `nova_rt/array.h` (Nova_Option_method_*, Nova_Result_method_*)
  + `emit_c.rs` (special-case dispatch для NovaOpt_T*/Nova_Result*).
- **Что реализовано:** базовые методы Option (is_some/is_none/unwrap/
  unwrap_or/unwrap_or_else/map/ok_or/or) и Result (is_ok/is_err/ok/err/
  unwrap/unwrap_or/unwrap_or_else/map/map_err). unwrap для None/Err
  throw'ит Fail с сообщением.
- **Spec:** D26 (08-runtime.md) дополнен полным API + примерами;
  Q "полный API Option/Result" частично закрыт. Расширенный API
  (and_then, flatten) — Q-monadic-api отдельно.
- **Тесты:** `nova_tests/runtime/unwrap_or.nv` (14 тестов).
- Также формализованы string-методы в D26: find/rfind/contains/
  starts_with/ends_with/split/trim/to_lower/to_upper — все индексы
  byte-offset (consistent с slice).

### [ЗАКР 2026-05-07] Source annotations default-on
- **Где:** `compiler-codegen/src/main.rs` (CLI flag) +
  `emit_c.rs` (emit_source_annotation_for_stmt/expr/span).
- **Что:** `/* SRC: <Nova-line> */` комментарии перед каждым C-stmt
  включены **по умолчанию**. Opt-out через `--no-annotate-source`
  (раньше было opt-in `-a/--annotate-source`).
- **Покрытие:** Stmt::*, block.trailing, FnBody::Expr (4 места —
  обычные fn, generic, methods, main).
- **Sanitize:** `*/` → `* /` (escape comment-close); одинокие `*`/`/`
  сохраняются (multiplication / division читаемы); truncate до 120
  символов с " …" если урезано.
- **Q-source-annotations** — обновлён под default-on.

### [ЗАКР 2026-05-07] D77 4-way auto-derive (from/into/try_from/try_into)
- **Где:** spec/decisions/08-runtime.md (D73 + D77 disclaimer).
- **Что:** программист пишет ОДНУ из 4-х форм, компилятор синтезирует
  остальные. **Рекомендация:** реализовывать `try_from` (Result-стиль
  явный, error type first-class), использовать в коде `from`/`into`
  (короче, идиоматичнее).
- **Алгоритм синтеза** задокументирован в D73 «Auto-derive 4-way».
- **D25** — добавлена секция «Performance: насколько дорогой `throw`»
  с cost-model (~50-200ns в bootstrap, vs Java/C++/Rust/Go) и
  recommendation использовать Result-стиль для hot path.

### [ЗАКР 2026-05-07] `interrupt v` через mco-coroutine-boundary
- **Где:** `nova_rt/fibers.h` (NovaFiberQueue: fiber_interrupt_top[N],
  interrupt_pending/interrupt_value), `nova_rt/effects.c`
  (nova_interrupt с cross-boundary-path), `emit_c.rs` (spawn-entry
  catch detect'ит "__nova_interrupt__" sentinel).
- **Что было:** D61/D65 требует handler-method для Fail (`fail() ->
  Never`) завершаться через `interrupt v`. Когда with-frame на
  main-stack, а throw в spawn-body, longjmp пересекал mco-границу
  → UB. Тесты использовали bootstrap-leniency `return ()`.
- **Фикс:** per-fiber switching `_nova_interrupt_top` в supervised_step
  (как `_nova_fail_top`). Если nova_interrupt не находит fiber-local
  frame — записывает pending в scope, longjmp на fiber-local fail-frame
  с sentinel. supervised_run после drain re-issue'ит interrupt на
  main-flow.
- **Тесты:** все 14 occurrences `return ()` в 4 файлах
  (effects/fail_handler, syntax/throw_in_expression,
  concurrency/cancel_scope_test, runtime/unwrap_or) переведены на
  spec-correct `interrupt ()`.

### [2026-05-07] nova_tests/ — иерархическая реорганизация
- **Где:** все 57 файлов мигрированы из плоского `01_X.nv` в
  `<group>/X.nv` (commit a33b245).
- **Группы:** basics/ types/ syntax/ effects/ concurrency/ runtime/
  modules/ — соответствуют тематическим областям spec/decisions/.
- **Module decls:** `module spec.X` → `module nova_tests.<group>.X`
  (D29-compliant: package name из nova.toml + filesystem path).
- **Keyword collisions:** `cancel_scope_test`, `detach_test`,
  `effects/basic.nv` (избегаем conflict'ов с keyword/runtime files).
- **run_tests.ps1:** recursive search + per-test obj_dir + relative
  display name + case-insensitive path comparison.
- Spec D29 дополнен примером — раздел «Иерархическая структура
  test-suite (D29 в действии)» в 07-modules.md.

### [ЗАКР 2026-05-07] Named tmps в сгенерированном C
- **Где:** `emit_c.rs::fresh_tmp_named(role)` + use-sites.
- **Что было:** `_nova_tmp0`, `_nova_tmp1`, ... — голый счётчик.
- **Что стало:** `_nv_<role>_<n>` — семантическая роль:
  scr/match/matched/if/if_let/while_let/while/loop/println/tmp.
- **Зона покрытия:** match, if, IfLet, WhileLet, While, Loop,
  println. Остальные ~40 fresh_tmp call-sites используют общий
  `_nv_tmp_<n>`.

### Pattern: handler-обёртка для cleanup ресурсов (D10 demo)
- **Где:** `nova_tests/effects/handler_wrappers.nv` (4 теста).
- **Идея:** Nova не имеет defer/RAII (Q20 open). Cleanup через
  функцию-обёртку с body-lambda и внутренним `with Fail = handler`.
  На throw — handler ловит, выполняет cleanup, re-throw'ит наружу.
- **Bootstrap-ограничения, выявленные при написании:**
  - `mut` в свободных fn-параметрах не парсится → record-Tracker.
  - `fn T @method(...) Fail[E] -> R` парсер не любит → throw-методы
    как свободные fn (receiver в первом аргументе).
  - Trailing-block с non-int closure-параметром падает в codegen
    (нет type-erasure для closures) → body принимает int (id), не
    сам Resource.
- **Закрывает:** ничего конкретно (Q20 defer всё ещё open), но
  демонстрирует канонический D10-pattern для cleanup'а.

### [ЗАКР 2026-05-07] D26 str API — школа B (codepoint-indexed)
- **Где:** `spec/decisions/08-runtime.md` (D26), `spec/open-questions.md`
  (Q-string-indexing → закрыта), `nova_rt/nova_rt.h::nova_str_slice`,
  `nova_rt/array.h::nova_str_find/rfind/byte_len`,
  `emit_c.rs::str_method_to_rt` + Member-handler.
- **Что было:** byte-indexed (Rust/Go-style) — `s.len` = bytes,
  slice/find возвращали byte-offsets. Логично для FFI, но нелогично
  для пользователя: `"мир".len` под byte-API даёт 6 (3 codepoint × 2),
  а ожидается 3; индексация в Cyrillic/emoji неинтуитивна.
- **Что стало:** codepoint-indexed (Python/Swift-style):
  - `s.len` — codepoints, O(n).
  - `s.byte_len()` — bytes, O(1).
  - `s.char_len()` — alias `len` (для явности).
  - `s.slice(a, b)` принимает codepoint-индексы.
  - `s.find(needle) / rfind(needle)` возвращают codepoint-offset.
  - Внутреннее хранение остаётся UTF-8. Для FFI/IO — `byte_len()`.
- **Trade-off:** O(n) на `len`/`slice` вместо O(1). Для real-world
  text-handling это не bottleneck (строки обычно небольшие, hot-path
  итерируется без `len`). Если станет проблемой — кэш codepoint-len
  на структуре `nova_str` (поле + invalidation на mutation).
- **Тесты:** `nova_tests/types/str_search.nv` обновлён (section 7
  переписан с `len == bytes` на `len == codepoints` + `byte_len()`,
  добавлена section 8 с Cyrillic/emoji find/rfind/slice).
- **Закрывает:** Q-string-indexing.

### [ЗАКР 2026-05-07] Bitwise операторы — реализованы
- **Где:** `compiler-codegen/src/lexer/{token.rs,mod.rs}`,
  `parser/mod.rs::parse_bit_or/xor/and/shift`,
  `codegen/emit_c.rs::Binary{,_op_str}`,
  `nova_tests/types/bitwise.nv` (28 тестов).
- **Что было:** lexer отвергал single `&` ("did you mean &&?") и `^`
  ("unexpected byte"). Spec D-operators (spec/03-syntax.md уровни 7-10)
  определяет `& | ^ << >>`, но bootstrap не реализовывал.
- **Что стало:** новые токены Amp, Caret, Shl, Shr; новые BinOp варианты
  BitAnd/BitOr/BitXor/Shl/Shr; парсер с правильными приоритетами
  (cmp(6) → bit-or(7) → bit-xor(8) → bit-and(9) → shift(10) → range/add(11)).
  Codegen emit'ит C-операторы 1:1 — биты тождественны.
- **Покрытие:** 28 тестов basic + precedence (5 кейсов проверки spec
  иерархии) + типичные паттерны (mask, set/toggle bit, even-check) +
  u64-литералы за i64::MAX.

### [ЗАКР 2026-05-07] u64 hex/bin литералы > i64::MAX wrapping в i64
- **Где:** `lexer/mod.rs::lex_radix_int`.
- **Причина:** Hash-константы FNV-64 (`0xCBF29CE484222325`),
  UUID-namespace, CRC требуют u64-битовых паттернов. У нас один тип i64
  (nova_int). Лексер падал на `invalid int: number too large to fit`.
- **Что стало:** Если `i64::from_str_radix` падает, пробуем
  `u64::from_str_radix` и приводим к i64 wrapping (`u as i64`). Биты
  тождественны — для bitwise/hash это корректно.
- **Trade-off:** В арифметических контекстах (e.g. `0xFFFFFFFFFFFFFFFF + 1`)
  результат будет арифметикой over signed i64, что отличается от u64
  semantics. Для будущей работы — введение типа `uint`/`u64` (отдельный
  open question; текущее поведение покрывает 95% use-cases).
- **Покрытие:** bitwise.nv section 8 (3 теста — wrapping → negative,
  all-ones = -1, high-bit set).

### [ЗАКР 2026-05-07] Handler-expr non-greedy в `with`-выражении
- **Где:** `parser/mod.rs::parse_expr_or_handler_lit`.
- **Причина:** Форма `with E = (e) => interrupt Err(e) { body }` —
  handler-lambda greedy ела `{ body }` как trailing-block после
  `interrupt Err(e)`. Парсер видел `interrupt Err(e) { body }` как
  call-with-trailing-block.
- **Что стало:** Перед fallback на `parse_expr` устанавливаем
  `no_trailing_block=true`. Теперь handler-выражение не захватывает
  следующий `{`-block — он достаётся внешнему with-парсеру.
- **Эффект:** ~10 stdlib-файлов продвинулись (complex/cron/duration/
  retry/semver/semver_range/snowflake/statistics/rate_limiter/ulid).

### [ЗАКР 2026-05-07] mut-маркер на параметре функции (D6)
- **Где:** `parser/mod.rs::parse_param`.
- **Причина:** D6 говорит, что `fn f(buf mut Buffer, ...)` означает
  внутри fn возможность мутировать значение. Bootstrap не парсил `mut`
  в позиции параметра.
- **Что стало:** После имени параметра optional `mut` ключевое слово
  съедается (игнорируется в семантике — у нас GC + reference, mut
  не меняет поведения). Это spec-faithful — позволяет писать код по
  стилю spec'а.
- **Эффект:** stdlib/uuid и stdlib/uuid_v3_v5 разблокированы.

### [ЗАКР 2026-05-07] D55 anonymous record literal с inferred type
- **Где:** `codegen/emit_c.rs::emit_record_lit` + `expected_record_type`
  state-поле + helper `struct_name_from_c_type`.
- **Причина:** Форма `fn make_point() -> Point => { x: 7, y: 11 }` —
  anonymous record без struct-name. Codegen падал с "anonymous record
  literal without spread not supported". Spec D55 описывает coercion
  в позиции с явным типом, но bootstrap-codegen не имел type-inference
  context.
- **Что стало:**
  - Новое state-поле `expected_record_type: Option<String>`.
  - В emit_method_body / emit_fn_body перед эмитом тела функции
    устанавливаем `expected_record_type` из declared return type
    (через helper `struct_name_from_c_type` извлекая имя из
    `Nova_Foo*`/`Nova_Foo`).
  - В emit_record_lit — новая ветка для случая "type_name=None +
    spread=None + expected_record_type=Some" — эмитит как для
    именованного record.
  - `Self` в expected_record_type разворачивается в
    current_receiver_type.
- **Покрытие:** records.nv — 2 новых теста.
- **Эффект:** stdlib/range, fnv, snowflake, statistics, rate_limiter,
  bloom_filter, ulid, semver — продвинулись на следующие блокеры.
- **Ограничение:** Только при declared return type в fn-сигнатуре.
  Inferred-type для let-bindings (`let p Point = { x:1, y:2 }`) не
  поддерживается — отдельная задача.

### [ЗАКР 2026-05-07] D79 Channel[T] base implementation (bootstrap)
- **Где:** `nova_rt/channels.h` (новый), `nova_rt.h` include,
  `emit_c.rs` dispatch, `nova_tests/runtime/channels.nv` (11 тестов).
- **Что было:** stdlib-агент закрыл D79 spec gap (channels формально
  декларированы); bootstrap-runtime отсутствовал.
- **Что стало:** bounded ring-buffer + send/recv yield + close+drain.
  Sequential-only в bootstrap (D71 single-threaded cooperative).
- **Ограничение:** spawn-block (`spawn { ch.recv() }`) упирается в
  существующий codegen-bug. Channel готов как только fix. Парсер
  `select { ... }` отложен — отдельная задача.

### [ЗАКР 2026-05-07] Lint pass + 2 правила (D65/D62)
- **Где:** новый модуль `src/lints.rs`, `lib.rs` pub mod,
  `main.rs` `--no-lint` флаг.
- **Что стало:** lint pass с двумя правилами:
  - `export-fail-untyped` (D65): `export fn ... Fail` без `[E]` →
    warning. `Fail[E]` typed и `Fail[any]` explicit erasure OK.
  - `protocol-in-effect-position` (D62 matrix): `fn f() Hashable ->
    ()` → warning. Хардкод known protocols (Hashable/Ord/Eq/Iter/
    From/Into/TryFrom/TryInto/ToStr).
- **Архитектура:** возвращает `Vec<LintWarning>`. main.rs выводит в
  stderr с правильным line:col + rule-name. Не блокирует compile.
  6 unit-тестов в lints.rs.

### [ЗАКР 2026-05-07] D28 effect inference для private fn (минимальная)
- **Где:** `types/mod.rs::infer_effects(&mut Module)`,
  `main.rs cmd_compile` вызывает после parse+check.
- **Причина:** D62 strict transitivity создаёт шум в private helper'ах.
  D28 говорит «private — выводится автоматически».
- **Что стало:** mutable walk. Для каждой private fn (`!is_export`):
  - has_throw_in_fn(f) — рекурсивный обход body (Stmt + Expr).
  - Если есть throw и нет Fail в effect-row → добавляем
    `TypeRef::Named "Fail"` (placeholder per D65).
- **Не реализовано в bootstrap'е (TODO production):**
  - Точный E через type-of(throw expr) — добавляем голый `Fail`.
  - Транзитивная inference (callee Fail → caller Fail).
  - Inference других эффектов (Db/Net/etc) — они resource-capability,
    D62 требует явной декларации.
  - Public fn не трогается — D62 strict; lint export-fail-untyped
    warning'ит вместо.
- **Тест:** throws.nv smoke — `fn validate_d28(n int) -> int { if n<0
  { throw "negative" } n*2 }` компилируется без явного Fail.

### [ЗАКР 2026-05-07] D78 path/module enforcement в codegen

- **Проблема:** D78 декларирует "module path = file path", но bootstrap
  не проверял это. Можно было скопировать `std/encoding/json.nv` в
  `examples/json.nv` с тем же `module std.encoding.json` — компилятор
  пропускал.
- **Где:** `compiler-codegen/src/manifest.rs` (новый), вызов в
  `cmd_check / cmd_run / cmd_compile / cmd_test` после parse, до
  type-check.
- **Что делает:** walks parent dirs от файла, ищет `nova.toml`. Из
  manifest извлекает `[package].name` и `[lib].src` (минимальный
  TOML-парсер, не тянем full crate). Source root = `<dir>/<src>`.
  Expected module = `<package>.<rel-path-from-src-without-ext>`. Если
  declared != expected → compile error с hints (move-file vs rename-
  module). Если nova.toml не найден — skip (file вне пакета, ad-hoc
  script).
- **Сразу после реализации:** `tests-nova/` → `nova_tests/`. Имя
  директории должно совпадать с `package.name` внутри `nova.toml`,
  иначе все declared `module nova_tests.<group>.<file>` мисматчат
  file path (строгое enforcement активировалось → проявило старое
  несоответствие).

### [ЗАКР 2026-05-07] tests-nova/ → nova_tests/

- **Причина:** `tests-nova/nova.toml` содержит `[package].name =
  "nova_tests"`. По D78 имя директории == package.name, иначе
  enforcement ругается.
- **Что сделано:** rename + apdate всех refs (root `nova.toml`
  workspace member, `run_tests.ps1`, `compiler-{bootstrap,codegen}/
  tests/spec_nova.rs`, `.gitignore`, спека D78 в 07-modules.md, docs).
  В spec/decisions/history/evolution.md ссылки на `tests-nova/` оставлены
  как frozen historical record.

### [ЗАКР 2026-05-07] D38 turbofish в expression-position

- **Проблема:** Spec D38 декларировала `Cache[K, V].new()` и `parse[T](x)`
  как канонический синтаксис в expression-position, но bootstrap-парсер
  при виде `Ident[...]` всегда трактовал `[` как Index. Падение на
  `expected ']' got ','` в 5+ stdlib-файлах (hashmap, lru, jwt, ini, toml).
- **Где:** `compiler-codegen/src/ast/mod.rs` (`ExprKind::TurboFish`),
  `parser/mod.rs` (`try_parse_turbofish_args` + ветка LBracket в
  `parse_postfix`), `codegen/emit_c.rs` (TurboFish-arm + unwrap helper),
  `interp/mod.rs`, `types/mod.rs`.
- **AST:** новый узел `ExprKind::TurboFish { base, type_args }`. Не
  выбрасываем `type_args` — сохраняем для будущих этапов real type
  inference / monomorphization.
- **Парсер (peek-disambiguation):** speculative-parse `[...]` как
  `parse_type_args`; если успешно И post-`]` token — `(` (call),
  `.IDENT(` (method-call) или `?` (Try) — TurboFish; иначе rollback к
  Index. Multi-arg внутри `[...]` — однозначно turbofish (Index никогда
  не имеет comma). Все edge-кейсы пройдены: `xs[i].field` остаётся
  Index→Member (`.field` без `(` после), `xs[i]` без continuation —
  Index, `Type[K, V].method(...)` — TurboFish, `parse[int]("42")?` —
  TurboFish.
- **Codegen / interp:** `Expr::unwrap_turbofish()` распаковывает в base;
  применяется в emit_call, infer_expr_c_type, emit_stmt (let-decl
  generic-fn-tuple-arity), evaluate_expr — везде, где downstream
  смотрит на конкретный `kind`.
- **Не реализовано:** type-checker не валидирует что `type_args`
  satisfy generic bounds (D72) — bootstrap erases generics.
- **Тесты:** `nova_tests/types/generics.nv` — 4 теста: single-arg
  function, two-arg function, Index-regression, arr[i].field-regression.
  Stdlib-files (hashmap/lru/jwt/ini/toml) теперь проходят
  turbofish-блокер (упираются в следующие — отдельные блокеры).

### [ЗАКР 2026-05-08] D54 `as`-cast — реализован narrowing в codegen [P-as-cast-wraparound]

- **Проблема:** `ExprKind::As(inner, _ty)` в codegen был **no-op** —
  игнорировал target-тип, эмитил inner expression «как есть».
  Narrowing работал только косвенно (через C-narrowing на push в
  uint8_t-слот / param-копировании) — не как следствие `as`.
- **Где:** `compiler-codegen/src/codegen/emit_c.rs`:
  - `ExprKind::As` в emit_expr — теперь `(({c_ty})({inner}))`
  - `ExprKind::As` в `infer_expr_c_type` — возвращает target type из
    annotation, не type-of(inner)
- **Семантика overflow (D54 не уточнял):** **wraparound** в стиле
  C-narrowing для int → меньший int (truncate младших битов).
  Согласовано с C/Go/Rust 1.45+. Checked-cast (panic-on-overflow) —
  отложен в Q-checked-cast / future D-decision.
- **Cases:**
  - int → byte / int → i32 / etc. — wraparound
  - int → f64 / byte → int — identity (numeric promotion)
  - f64 → int — truncate (как в C)
  - newtype-alias ↔ underlying — idempotent (одинаковое C-представление)
- **Тесты:** `nova_tests/syntax/as_cast.nv` — 8 тестов (narrowing,
  bitwise-mask cast, i32 from i64, f64 ↔ int, newtype identity,
  zero-byte, negative wraparound). 63/63 nova_tests PASS.
- **Stdlib regression-проверка:** crc32/fnv/bloom_filter (активно
  используют `as byte` / `as u32` в bitwise pack'ах) продолжают
  работать. Stdlib total: 66 PASS (+1 markdown_minimal который теперь
  считается RUN-FAIL).
- **План:** docs/plans/05-as-cast-codegen.md.

### [ЗАКР 2026-05-08] D54 float→int saturation [P-as-cast-float-saturation]

- **Проблема:** План 05 закрыл основной gap (`as` теперь C-cast), но
  для **float → int** narrowing'а оставил UB на out-of-range / NaN /
  ±Infinity (C-стандарт §6.3.1.4 не определяет behavior). Spec D54
  не специфицировал semantics narrowing — gap-by-omission.
- **Решение:** float → integer narrowing делает **saturation** через
  runtime helper'ы, NaN→0, ±∞→границы. Согласовано с Rust 1.45+
  (RFC #2484 «sealed casts»). `as` остаётся pure — throw-форма
  для checked-cast доступна через D77 `iN.try_from(f)?`.
- **Где:**
  - `compiler-codegen/nova_rt/cast.h` (новый, ~140 строк) — 16
    `static inline` helper'ов: `nova_f64_to_{i8,i16,i32,i64,u8,u16,u32,u64}`
    и аналог для f32. Все ветки: `isnan` → 0, range bounds →
    `INT_MAX/MIN`, иначе truncate towards zero (как C).
  - `compiler-codegen/src/codegen/emit_c.rs::ExprKind::As` —
    детектит `f64/f32 → integer` пару и эмитит helper-call;
    остальные cast'ы остаются прямым C-cast (плана 05).
  - `compiler-codegen/nova_rt/nova_rt.h` — `#include "cast.h"`.
- **Bonus fix:** `ExprKind::FloatLit` codegen теперь принудительно
  эмитит scientific notation (`{:e}`) для очень больших / малых
  значений и `.0`-суффикс для целых f64-литералов. Иначе
  `1e20` эмитился как integer-literal `100000000000000000000`,
  переполняя u64 (MSVC C2177).
- **Тесты:** `nova_tests/syntax/as_cast_float.nv` — 13 тестов:
  in-range, out-of-range positive/negative, NaN, ±Infinity, unsigned
  negative→0, INT64 boundary saturation, int wraparound regression.
  64/64 nova_tests PASS.
- **Spec D54** дополнен таблицей «Семантика narrowing-конверсий» —
  закрывает spec-долг плана 05.
- **Не реализовано:**
  - `unchecked_as` (zero-cost UB-cast) — отвергнут D9 «один путь»;
    если профайлер покажет потребность — escape hatch через FFI.
  - f128 / f16 / bfloat16 не покрыты (нет в bootstrap).
  - Generic `_Generic`-helper отвергнут: 16 явных функций читаются
    прямо.
- **План:** docs/plans/07-as-cast-saturation.md.

### [ЗАКР 2026-05-08] D26 prelude: Result/Option methods полностью покрыты в codegen

- **Что:** реализованы `map_err`, `map`, `unwrap_or_else`, `err()`
  для Result, и `unwrap_or_else`, `map`, `ok_or` для Option в codegen
  (раньше были только `is_ok/is_err/unwrap/unwrap_or/ok` для Result
  и `is_some/is_none/unwrap/unwrap_or` для Option).
- **Где:** `compiler-codegen/src/codegen/emit_c.rs::ExprKind::Member`
  call dispatch для `obj_ty == "Nova_Result*"` и
  `obj_ty.starts_with("NovaOpt_")`. Inline-эмит fresh tmp + tag-check
  + closure-call через NovaClos_ii / NovaClos_vi.
- **Bonus 1:** `emit_lambda` теперь принимает `return_type_ann:
  Option<&TypeRef>` — явная аннотация `(e str) -> str => ...` берётся
  из AST; раньше игнорировалась → C2440 mismatch на str-payload.
- **Bonus 2:** parser lookahead за `(` различает zero-arg lambda
  `() => expr` и unit-литерал `()`. Раньше `() => 0` не парсился —
  `()` сразу проглатывался как UnitLit.
- **Тесты:** `nova_tests/runtime/result_methods.nv` — 22 теста (10
  новых для unwrap/err/Option-методов). 65/65 nova_tests PASS,
  stdlib без регрессий.
- **Spec:** `spec/decisions/08-runtime.md` D26 секция дополнена
  таблицей Bootstrap status с явным мапингом реализованного.
- **Bootstrap-ограничения зафиксированы в spec'е:**
  Q-result-monomorphization (Result hardcoded на nova_int / nova_str),
  Q-closure-param-inference (lambda-параметры требуют явной
  аннотации для не-int типов).

### [ЗАКР 2026-05-08] D26 prelude: Error и RuntimeError встроены в runtime

- **Что:** добавлены prelude-типы `Error` (record с `msg`) и
  `RuntimeError` (sum со 6 вариантами: `DivByZero`, `Overflow`,
  `IndexOutOfBounds {index, length}`, `TypeMismatch(str)`,
  `AssertFailed(str)`, `NoHandler(str)`). Раньше были только в
  spec'е — реальной реализации в bootstrap не было.
- **Где:**
  - `compiler-codegen/nova_rt/array.h` (~80 строк): `Nova_Error`
    struct + `Nova_Error_static_new(msg)`; `Nova_RuntimeError`
    tag-union + 6 `nova_make_RuntimeError_<Variant>` конструкторов.
    Tag константы `NOVA_TAG_RuntimeError_*`.
  - `compiler-codegen/src/codegen/emit_c.rs` в `emit_module`
    pre-population: `record_schemas["Error"] = {msg: nova_str}`,
    `method_receivers["new"] = ("Error", false)`,
    `sum_schemas["RuntimeError"] = {DivByZero: [], ..., NoHandler: [str]}`.
    `record_variant_field_order["RuntimeError::IndexOutOfBounds"] =
    [index, length]`.
  - `infer_expr_c_type` дополнен: `Error.new(...)` → `Nova_Error*`.
- **Тесты:** `nova_tests/runtime/error_runtime_error.nv` — 11 тестов:
  Error.new (basic, empty, в throw), все 6 вариантов RuntimeError
  (unit, record, tuple), independence Error/RuntimeError.
  Используют `if let` extraction вместо assignment-style match
  (избегает nova_assert-void mismatch — bootstrap-codegen ограничение).
- **Spec:** `spec/decisions/08-runtime.md` D26 секция таблицы
  Bootstrap status дополнена 8 новыми строками.
- **Bootstrap-ограничения зафиксированы:**
  - `Error.msg` поле без enforce'а readonly (spec говорит readonly,
    bootstrap-grade compromise).
  - `RuntimeError` варианты доступны user-коду, но **встроенные
    операции** (`a/b`, `arr[i]`, NoHandler) всё ещё throw'ают
    `nova_str` через `Nova_Fail_fail`. Конверсия throw-points в
    `Nova_RuntimeError*` payload — отдельная задача (требует
    расширения fail-frame mechanism).

### [ЗАКР 2026-05-08] Plan 08 Ф.1+Ф.2: try_from / from для встроенных пар

- **Что:** `int.try_from("42")`, `f64.try_from("3.14")`,
  `bool.try_from("true")`, `char.try_from("A")`, `char.try_from(65)`,
  `str.from(42)`, `str.from(true)`, `str.from('A')` — работают через
  codegen-path. Раньше падали на parse-фазе или эмитили raw `int.try_from(...)`
  что не компилировалось C-компилятором.
- **Где:**
  - `compiler-codegen/nova_rt/conv.h` (новый, ~180 строк): runtime-helpers
    `nova_str_to_i64`, `nova_str_to_u64`, `nova_str_to_f64`, `nova_str_to_bool`,
    `nova_str_to_char`, `nova_int_to_char`, `nova_char_to_str` (UTF-8 encode),
    `nova_bool_to_str`, `nova_f64_to_str`. Все `static inline`.
  - `compiler-codegen/src/parser/mod.rs`: parse_primary дополнен — primitive
    type-names (int/i8-i64/u8-u64/f32/f64/byte/bool/char/str) могут
    инициировать Path-конструкцию (`int.try_from(s)` парсится как
    `Path(["int", "try_from"])` вместо `Member { Ident("int"), "try_from" }`).
    Раньше PascalCase-only — поэтому `int.try_from` не работало.
  - `compiler-codegen/src/codegen/emit_c.rs`: Path-call dispatch для
    `T.try_from(v)` (numeric/bool/char через runtime helper'ы) и `T.from(v)`
    (str.from для bool/char/numeric). Type-inference: `T.try_from(...)` →
    `Nova_Result*`, `str.from(...)` → `nova_str`.
- **Bootstrap-ограничение:** Result hardcoded на `(nova_int Ok, nova_str Err)`
  — все try_from эмитят payload как `nova_int` (для f64 это means
  bit-pattern truncation). Полный generic Result — отдельная задача
  Q-result-monomorphization.
- **CharLit detection:** `str.from(arg)` для CharLit-arg (например `'A'`)
  специально проверяется ДО общего numeric-arm, потому что char хранится
  как nova_int но семантика char→str (UTF-8 encode) ≠ int→str (decimal).
- **Тесты:** `nova_tests/runtime/from_into_basic.nv` — 26 тестов:
  int/f64/bool/char.try_from валидное+невалидное+overflow,
  char.try_from(int) range/surrogate, str.from(int/bool/char) ASCII+
  Cyrillic UTF-8. 67/67 nova_tests PASS.
- **Не сделано в этом коммите:** Ф.3 (4-way auto-derive synthesis),
  Ф.4 (strict if cond:bool), Ф.5 (as-cast restrictions), Ф.6 (generic-bound
  enforcement), Ф.7 (spec). Делаются отдельными коммитами.
- **План:** docs/plans/08-from-into-conversions.md.

### [ЗАКР 2026-05-08] Plan 06 Ф.1: Iter[T] protocol fallback в for-in

- **Что:** `emit_for` получил Case 3 — generic loop через
  `Nova_<T>_method_next(it)` для любого user-type'а с
  `mut @next() -> Option[T]`. Раньше падало с
  `for-in: unsupported iterator type 'Nova_X*'`.
- **Где:**
  - `compiler-codegen/src/codegen/emit_c.rs::emit_for`: Case 3 после
    Range и Array. Использует новый registry `all_methods:
    HashSet<(TypeName, MethodName)>`.
  - Multi-key registry избегает single-key last-wins проблемы:
    несколько типов с методом `next` (Counter, Doubler, RangeIter и т.д.)
    больше не вытесняют друг друга.
- **Bootstrap-ограничения:**
  - Element type из `Option[T]` не infer'ится — payload эмитится как
    `nova_int` (соответствует Result hardcoded на nova_int).
  - Tuple destructuring `for (k, v) in m.entries()` ещё не работает
    (Ф.2 plan'а 06).
  - Implicit `.iter()` для коллекций (`for x in deque` без
    `.iter()`) ещё не работает (Ф.3).
- **Тесты:** `nova_tests/syntax/for_iter.nv` — 4 теста: custom Counter
  (basic, empty, single), stateful Doubler (бесконечный с
  None-return). 68/68 nova_tests PASS.
- **Не сделано в этом коммите:** Ф.2 (tuple-destructuring), Ф.3
  (implicit `.iter()`), Ф.5 (sweep std/collections тестов).
- **План:** docs/plans/06-iter-protocol-codegen.md.

### [ЗАКР 2026-05-08] Plan 08 Ф.3: D77 4-way auto-derive synthesis

- **Что:** программист пишет ОДНУ форму конверсии, codegen синтезирует
  обратную:
  - `T.try_from(v V)` → `v.@try_into() -> Result[T, E]` (новое в Ф.3)
  - `T.from(v V)` → `v.@into() -> T` (уже работало; добавлен infer)
- **Где:**
  - `compiler-codegen/src/codegen/emit_c.rs`: новые registries
    `try_from_targets: HashMap<TargetT, Vec<SourceV>>` и
    `try_into_targets: HashMap<SourceV, TargetT>`. Заполняются при
    AST-walk fn-items с receiver'ом.
  - Member-call dispatch для `v.@try_into()`: lookup в
    `try_from_targets` → emit `Nova_T_static_try_from(v)`.
  - Type-inference для `v.@try_into()` → `Nova_Result*`,
    `v.@into()` → `Nova_<Target>*`.
- **Bonus fix:** `nova_type_name_from_c` helper — конвертирует C-type
  обратно в Nova-имя (`nova_int → int`, `Nova_Wrapper* → Wrapper`).
  Без него from_targets/try_from_targets lookup'ы не находили primitive
  receiver'ы (registry хранит Nova-имена, runtime даёт C-имена).
- **Тесты:** `nova_tests/runtime/auto_derive.nv` — 7 тестов:
  Celsius.try_from(int) валидное/невалидное, int.@try_into() через
  synthesis, Wrapper.from(int) + 100.into() через synthesis.
  69/69 nova_tests PASS.
- **Bootstrap-ограничения:**
  - Compile-time check «synthesis target существует?» не реализован
    (если нет ни from, ни into для пары — silent fall-through).
    Делается в Ф.6 (generic-bound enforcement).
  - Транзитивный auto-derive (`int.from(i32)` + `f64.from(int)` ⇒
    `f64.from(i32)`) НЕ делается (consciously, чтобы не выдавать
    surprising paths).
- **План:** docs/plans/08-from-into-conversions.md (Ф.3 закрыт).

### [ЗАКР 2026-05-08] Cleanup: keyword-алиасы or/and/not удалены

- **Что:** в D49 фиксировались `or`/`and`/`not` как keyword-aliases для
  `||`/`&&`/`!`. Реальных употреблений в `.nv` корпусе ноль; алиасы
  нарушают D9 «один очевидный путь» / D40 «один способ».
- **Где:**
  - `compiler-codegen/src/lexer/{mod.rs,token.rs}` — удалены KwAnd/
    KwOr/KwNot из identifier-маппинга, enum, display-имён.
  - `compiler-codegen/src/parser/mod.rs` — упрощены `parse_or`,
    `parse_and`, `parse_unary` (`Bang | KwNot` → `Bang`).
  - `compiler-bootstrap/*` — те же 6 правок симметрично.
  - `spec/decisions/03-syntax.md` D49 — переписан «||/&&/or/and» → «||/&&».
- **Verification:**
  - `grep KwAnd|KwOr|KwNot` — ноль матчей в обоих компиляторах.
  - `cargo check` — оба прошли чисто (без regression-warnings).
  - `nova_tests/` + `std/` — ноль реальных употреблений (single
    match — английское слово в test-name string-literal).
- **Урок:** keyword-алиасы можно безопасно удалять если:
  (a) ноль реальных употреблений в корпусе кода,
  (b) есть символьный эквивалент,
  (c) удаление освобождает identifier для пользовательского кода.
  Все три выполнены — `or`/`and`/`not` теперь обычные identifier'ы.

### [ЗАКР 2026-05-08] Plan 04 зафиксирован: Buffer split на 3 типа + external keyword

- **Что:** текущий `Buffer` (Q-buffer ✅) смешивает text-domain и
  binary-domain. Split на три типа со специализированной семантикой:
  - **StringBuilder** (UTF-8 string accumulator, `@into() -> str`
    infallible).
  - **WriteBuffer** (binary serialization, `@write_*_le/be`).
  - **ReadBuffer** (cursor-style binary reader, `@read_*` Fail-form +
    `@try_read_*` Result-form auto-derive на C-runtime уровне).
- **Plus новый keyword `external`** для stdlib runtime-implemented
  функций. `external` только для функций; типы Builder/Buffer'ов
  built-in opaque как примитивы (не объявляются отдельно).
- **char ↔ str через D73:** `str.from(c char)` external + auto-derived
  `char.@into() -> str`.
- **D30 расширение:** «полные слова, не сокращения», `len`/`iter`
  mainstream exceptions. `@position` не `@pos`, `@capacity` не `@cap`.
- **Где:** план зафиксирован в `docs/plans/04-buffer-split-and-external.md`.
  Реализация после Plan 08 (D73+D77 4-way auto-derive infrastructure).
- **Эволюция дизайна:** длинная итерация по naming (10+ поворотов:
  `add_*`/`append_*`/`write_*`/`put_*`) показала что **не naming
  главное, а split типов**. Когда зафиксировали split — naming
  решился сам:
  - `StringBuilder.@append` (Java/Go convention),
  - `WriteBuffer.@write_*` (Go bytes.Buffer-style),
  - `ReadBuffer.@read_*` (Go/Rust convention).
- **Урок:** когда обсуждение naming идёт в десятках поворотов — это
  signal что **не naming проблема**. Что-то более фундаментальное
  (часто — структура типов / разделение domain'ов) не решено. После
  правильного split'а имена находятся естественно.

### [ЗАКР 2026-05-08] Build pipeline scripts: build_c.ps1 + build_c.sh

- **Что:** документация `.nv → .exe` pipeline была частично сделана
  и **с ошибками** в `compiler-codegen/README.md`:
  - Упоминание `build_c.bat` в неправильном контексте (он принимает
    `.c`, не `.nv`).
  - GCC command с `-Inova_rt` (неправильный path; нужно `-I.` потому
    что codegen эмитит `#include "nova_rt/nova_rt.h"`).
  - MSVC pipeline отсутствовал — был только в `run_tests.ps1`.
  - Top-level README'и build pipeline не упоминали.
- **Где:**
  - **Создан** `compiler-codegen/build_c.ps1` — Windows wrapper
    (`.nv → .c → .exe` one-shot, опции `-Run`, `-Output`, `-KeepC`,
    `-VCVarsPath`).
  - **Создан** `compiler-codegen/build_c.sh` — Linux/Mac wrapper
    (`--run`, `-o`, `--keep-c`, `--cc gcc|clang`).
  - Существующий `build_c.bat` оставлен — другая роль (advanced,
    с `gc=malloc|rc|boehm` для GC backend).
  - `compiler-codegen/README.md` переписан: walkthrough'и, разделение
    ролей трёх wrapper'ов, CLI-флаги, ограничения, batch через
    `run_tests.ps1`.
  - Top-level `README.md` + `README.ru.md` — секции «Building from
    source» / «Сборка из исходников» с ссылками.
- **Verification:** `build_c.ps1` тестирован end-to-end на hello.nv
  (Hello, Nova! работает) + error-path (broken.nv → понятная
  диагностика unresolved symbol).
- **Урок:** документация build-pipeline критична для onboarding'а.
  Ошибки в README жили ~год без detection — нужны end-to-end
  walkthrough-тесты (запуск примеров **из README** в CI), не только
  cargo test самих компиляторов.

### [ЗАКР 2026-05-08] Editor support: sublime/vim/emacs plugin'ы + sync VSCode подсветки

- **Что:** до этой задачи в `editors/` был только VSCode plugin.
  Добавлены plugin'ы для остальных популярных IDE.
- **Где:**
  - `editors/sublime/` — переиспользует TextMate-grammar от VSCode
    напрямую через symlink в `Packages/Nova/`. Sublime parser
    Oniguruma-compatible с VSCode.
  - `editors/vim/{ftdetect,ftplugin,syntax}/nova.vim` — handcrafted
    Vim plugin (~150 строк), filetype detection + comment/indent
    settings + syntax keyword'ы.
  - `editors/emacs/nova-mode.el` — single-file major-mode с font-lock-
    keywords, syntax-table, auto-mode-alist + optional rainbow-
    delimiters integration.
  - `editors/README.md` — общий index по всем IDE с table support'а
    + roadmap (LSP > tree-sitter > JetBrains).
  - `editors/vscode/syntaxes/nova.tmLanguage.json` — sync со spec'ом:
    удалены `resume`/`String`/`Mutex`/`RwLock`/`Atomic`/`money`/etc,
    добавлены `protocol`/`external`/`RuntimeError`/`CancelToken`/
    `TryFrom`/`TryInto`/`StringBuilder`/`WriteBuffer`/`ReadBuffer`/
    `Detach`/`Blocking`/`Mem`.
  - VSCode README исправлен (неправильный path `\.vscode\nova-extension`
    → корректный `editors\vscode`).
  - Bracket pair colorization recommendations во всех 4 IDE README'ях
    (VSCode settings.json, Vim rainbow.vim, Emacs rainbow-delimiters,
    Sublime BracketHighlighter).
  - Top-level README'и расширены таблицей всех 4 plugin'ов.
- **Cover:** VSCode + Cursor + VSCodium + Sublime + TextMate + Vim +
  Neovim + Emacs (8 IDE через 4 plugin'а).
- **Не сделано:** JetBrains plugin (требует Java/Kotlin), tree-sitter
  grammar (Zed/Helix/Neovim 0.5+, отдельный ~10-20ч проект),
  GitHub Linguist (требует PR в чужой repo + 200+ файлов).
- **Source-of-truth для keyword'ов:** `compiler-codegen/src/lexer/mod.rs`
  функция `lex_ident_or_keyword`. Все 4 plugin'а синхронизируются
  против этого файла — задокументировано в editors/README.md.
- **Урок:** TextMate-grammar (Oniguruma) переиспользуется в VSCode
  семействе (Cursor/VSCodium/Sublime/TextMate) без изменений. Для
  Vim/Emacs нужен handcrafted формат. tree-sitter — современный
  стандарт (Zed/Helix/Neovim 0.5+/GitHub web), но единый grammar
  для 4+ редакторов requires separate проекта ~10-20ч. MVP покрывает
  достаточно через TextMate + handcrafted без tree-sitter.

### [ЗАКР 2026-05-08] Plan 08 Ф.4: strict `if cond: bool` в codegen

- **Что:** D54 требует cond обязан быть `bool`, не truthy-int (Rust/
  Swift/Kotlin прецедент). Раньше bootstrap'е `if int_value { ... }`
  silently компилировался — C принимает int как truthy. Закрывает
  silent-bug class.
- **Где:**
  - `compiler-codegen/src/codegen/emit_c.rs::emit_if_expr` и
    `ExprKind::While` arm: проверка `check_bool_condition(cond_ty)`
    перед emit'ом. Если cond_ty в списке non-bool primitives
    (nova_int, nova_f64, nova_str, ...) — compile error.
  - **Conservative**: type-neutral (void*, unknown user types) —
    пропускаем, чтобы не ломать существующий код.
- **Bonus prerequisite fixes** (необходимы для strict-check):
  - `infer_expr_c_type` для `ExprKind::Unary`:
    `!x` → nova_bool, `-x` → тип operand'а. Раньше fall-through на
    nova_int (например `if !cancelled` падал даже когда `cancelled: bool`).
  - `infer_expr_c_type` для `ExprKind::Block(b)`: возвращает тип
    trailing-expression. Раньше fall-through на nova_int (например
    `let cond = { ...; n > 0 }; if cond` падал).
  - `infer_expr_c_type` для closure-call (Ident-call к binding в
    fn_param_sigs): возвращает ret_ty из sig. Раньше fall-through на
    nova_int (`pred(x)` где `pred: fn(int) -> bool` инфер'ился как int).
  - `let pred = (n int) -> bool => ...` — registration в fn_param_sigs
    теперь использует Lambda's return_type-аннотацию.
- **Тесты:** `nova_tests/syntax/strict_if_bool.nv` — 9 positive-тестов:
  bool literal, comparisons, unary !, &&/||, block-expr cond, while
  с bool, fn-call возвращающий bool, closure-call возвращающий bool.
  70/70 nova_tests PASS, без регрессий.
- **Bootstrap-ограничения:**
  - Negative cases (`if int_value` → compile error) проверяются
    вручную через `nova-codegen check`, не в test-runner'е.
  - Compile-error suggestions ("use `n != 0`...") — TBD в Ф.7.
- **План:** docs/plans/08-from-into-conversions.md (Ф.4 закрыт).

### [ЗАКР 2026-05-08] Plan 08 Ф.5: as-cast restrictions для char/byte/bool

- **Что:** D54 явно запрещает некоторые `as`-cast'ы из-за неочевидной
  или небезопасной семантики. Bootstrap раньше silently разрешал всё —
  теперь даёт compile error с suggestion'ом использовать `try_from`
  или explicit comparison.
- **Запрещённые пары** (compile error):
  - `int as char`, `i32/i64/u32/u64 as char` → use `char.try_from(n)?`
  - `char as byte` → use `byte.try_from(c)?`
  - `int/byte/f64/etc as bool` → use `n != 0`
  - `str as int/i32/f64/bool` → use `T.try_from(s)?`
  - `int/f64/bool/char as str` → use `str.from(v)`
- **Исключение для CharLit:** `'A' as byte`, `'A' as int`, `'A' as u8`
  разрешены — программист видит codepoint буквально, range-check не
  нужен (existing stdlib usage в str_search.nv использует этот паттерн).
- **Где:**
  - `compiler-codegen/src/codegen/emit_c.rs::ExprKind::As`: добавлен
    `check_as_cast_allowed(src_nova, tgt_nova, inner_kind)` перед emit'ом.
    Detection через `target_nova` из `TypeRef::Named`, не C-имя
    (char и int имеют одинаковый C-тип nova_int).
  - Helper `nova_type_name_from_c` (Plan 08 Ф.3) reused для извлечения
    Nova-имени src.
- **Тесты:** `nova_tests/syntax/as_cast_restrictions.nv` — 6 positive-
  тестов: char-literal cast разрешён, byte→int widening, int→byte
  wraparound, f64→int saturation, bool→int, newtype-alias identity.
  71/71 nova_tests PASS, без регрессий.
- **Bootstrap-ограничения:**
  - Negative cases — manual check через `nova-codegen check`.
  - Compile-error не имеет file:line:col — использует error из
    emit_expr fallthrough. Полная diagnostic — Ф.7.
- **План:** docs/plans/08-from-into-conversions.md (Ф.5 закрыт).

### [ЗАКР 2026-05-09] Plan 15 Ф.1+Ф.2+Ф.3: D72 generic bounds enforcement

- **Что:** `[T Hashable]` синтаксис теперь парсится и type-checker
  валидирует на use-site, что concrete-тип удовлетворяет protocol-
  bound'у. На mismatch — структурированный R5.3-style diagnostic с
  required/missing методами.
- **Где:** `compiler-codegen/src/{ast/mod.rs, parser/mod.rs,
  types/mod.rs}` — ~600 строк (AST 30 + parser 60 + type-checker 500).
- **Use-site эффект:**
    ```nova
    type Hashable protocol { hash() -> u64 }
    type User { id u64, name str }
    export fn User @hash() -> u64 => @id

    fn dedup[T Hashable](xs []T) -> []T => xs

    let users = [User { id: 1 as u64, name: "a" }]
    let _ = dedup(users)   // ← type-check OK, User satisfies Hashable

    type NoHash { name str }
    let xs = [NoHash { name: "x" }]
    let _ = dedup(xs)
    // ← compile-error:
    //   type `NoHash` does not satisfy `Hashable` bound
    //     `Hashable` requires: hash() -> u64
    //     `NoHash` is missing: hash() -> u64
    //     fix: добавить недостающие методы...
    ```
- **Не покрывается:**
    + D53 разделение Protocol vs Effect в AST — все `protocol`/`effect`
      попадают в `TypeDeclKind::Effect`. BoundCtx permissively принимает
      любой Effect-kind как potential bound. Strict D53 compliance —
      отдельная задача.
    + Self в protocol-методах падает в codegen (vtable-emit issue) —
      pre-existing bug, тесты обходят протоколами без Self.
    + Method calls с bounds (`obj.method[T Hashable]()`) — пропускаются.
    + Bound на ассоциированном типе — open question.
- **План:** docs/plans/15-generic-bounds-enforcement.md Ф.1/Ф.2/Ф.3 ✅;
  Ф.4 partial (positive tests); Ф.5 spec — pending.

### [ЗАКР 2026-05-09] Plan 14 Ф.6: D69 variadic + spread

- **Что:** declaration `fn f(...items []T)` + call-site spread
  `f(...arr)` + mixed `f(a, ...arr, b)` теперь полностью работают.
  Покрывает D69 спецификацию.
- **Где:** `compiler-codegen/src/{ast/mod.rs, parser/mod.rs,
  codegen/emit_c.rs}` — ~500 строк (с учётом mechanical refactor
  CallArg enum во всех readers Call.args).
- **Use-site эффект:**
    ```nova
    // Декларация:
    fn join(...parts []str) -> str { ... }

    // Вызовы:
    join("a", "b", "c")          // → []str = ["a","b","c"]
    join(...arr)                  // → []str = arr
    join(...prefix, "tail")       // mixed
    ```
- **Не покрывается:**
    + Spread в interp (`nova-codegen run` отвергает).
    + print/println — продолжают быть special-case (миграция отложена).
    + Multiple variadic-overloads — отвергаются (single-overload only).
- **Std refactor:** `std/path/path.nv` `Path.join(parts []str)` →
  `Path.join(...parts []str)` — теперь принимает variadic.
- **План:** docs/plans/14-stdlib-codegen-gaps.md Ф.6 ✅.

### [ЗАКР 2026-05-09] Plan 14 Ф.1: Option[T] full refactor (generalize Iter[T])

- **Что:** `Option[T]` в codegen теперь правильно типизирован для любого
  T (primitive / str / tuple / record-pointer / nested Option), вместо
  legacy `NovaOpt_nova_int` int-stomp'а. Изначальная узкая задача
  (Iter[T] generalization) расширилась до полного refactor'а Option-
  эмиссии — work-around на cast'ах не покрывал struct-typed payload
  (str/tuple).
- **Где:** `compiler-codegen/src/codegen/emit_c.rs` — ~250 строк по
  7 codegen-paths:
    + `type_ref_to_c(Option[T])` → `NovaOpt_<sanitized T>`;
    + lazy NovaOpt_<T> typedef'ы через marker+splice;
    + Some(v) / None через compound literal с реальным T;
    + ?-оператор typed early-return None;
    + pattern-match через temp `var_types` registrations;
    + emit_for использует typed `MethodSig.return_c_type`;
    + infer_expr_c_type Some/None → typed NovaOpt_<T>.
- **Use-site эффект:**
    ```nova
    // Iter[bool] — strict bool-check теперь работает на binding'е:
    fn BoolToggler mut @next() -> Option[bool] => Some(true)
    for b in toggler {
        if b { ... } else { ... }   // OK, b: bool, не nova_int
    }

    // Nested Option — typed pattern match:
    let x = Some(Some(Some(42)))
    let v = match x {
        Some(Some(Some(n))) => n      // n: int, не cast'ится
        _ => -1
    }
    ```
- **Не покрывается:**
    + Tuple типизация (`(int, str)` сейчас all-nova_int в
      `_NovaTupleN`) — отдельная задача.
    + Channel.recv остаётся NovaOpt_nova_int (runtime-erased generic).
    + Result[T, E] — аналогичный refactor отложен.
- **План:** docs/plans/14-stdlib-codegen-gaps.md Ф.1 ✅ (Option[T]
  full refactor).

### [ЗАКР 2026-05-09] Plan 14 Ф.7: int-литерал → char без try_from

- **Что:** `0x41 as char`, `65 as char` теперь работают на use-site без
  обёртки `char.try_from(n)?` (которая требует `Fail` в сигнатуре).
  Применимо к **compile-time-known IntLit** в валидном Unicode-
  диапазоне `U+0..=U+10FFFF` исключая surrogate range
  `U+D800..=U+DFFF`. Range-check выполняется статически в checker'е.
- **Где:** `compiler-codegen/src/codegen/emit_c.rs::check_as_cast_allowed`
  (~25 строк). Spec — `spec/decisions/03-syntax.md` D54 (абзац-
  исключение в существующем разделе, без нового D-номера).
- **Use-site эффект:**
    ```nova
    // Было (spec-strict, требует Fail-handling):
    fn ascii_a() -> char ! Fail { char.try_from(0x41)? }

    // Стало (Ф.7):
    fn ascii_a() -> char => 0x41 as char
    ```
- **Не покрывается:** `n as char` где `n` — переменная или арифметика
  (`('0' as int + n) as char`). Нужен либо Ф.7-bis (binary-pattern
  recognition), либо рефактор под `try_from(...)?`. Std/ файлы
  (uuid/ulid/base64/hex) сейчас используют именно arithmetic-pattern
  и Ф.7 их не разблокирует.
- **План:** docs/plans/14-stdlib-codegen-gaps.md Ф.7 ✅ (spec-only).

### [ЗАКР 2026-05-08] Plan 08 Ф.7: spec D54 расширение + conversions.md

- **Что:** spec D54 в `03-syntax.md` дополнен таблицей запрещённых
  `as`-cast'ов для char/byte/bool с suggestion'ами и прецедентами
  (Rust/Swift/Kotlin/Java сравнение). Создана сводная страница
  `spec/conversions.md` (~280 строк) — single source of truth для
  всех правил конверсии.
- **Где:**
  - `spec/decisions/03-syntax.md` D54 — раздел «Запрещённые `as`-cast'ы
    для char/byte/bool» + раздел «Strict `if cond: bool` / `while
    cond: bool`».
  - `spec/conversions.md` — новый файл. Структура: 3 механизма (as /
    from / try_from), полная таблица всех типов конверсий
    (numeric ↔ numeric, numeric ↔ str, char/byte/[]byte/str, bool,
    newtype, sum-discriminant), запрещённые конверсии, auto-derive
    4-way, прецеденты по 6 языкам, bootstrap status.
- **Bootstrap-status таблица:** Plan 05/07/08 Ф.1-Ф.5 ✅, Ф.6 ❌
  (отложено — требует полного type-checker'а), транзитивный
  auto-derive consciously не делается.
- **Не сделано:**
  - Ф.6 (generic-bound `[T Into[X]]` enforcement) — требует
    полноценного type-checker'а; отложен до фазы рефакторинга.
  - Compile-error suggestions с file:line:col — TBD при добавлении
    diagnostic'ов.
- **План:** docs/plans/08-from-into-conversions.md (Ф.7 закрыт; план
  на 5 из 6 фаз готов, Ф.6 отложен).

### [ЗАКР 2026-05-08] Plan 06 Ф.2: tuple-destructuring в for-in

- **Что:** `for (k, v) in iter { ... }` для итераторов возвращающих
  tuple-pairs. Раньше `pattern_binding` падал на Pattern::Tuple
  ("complex pattern in let binding not yet supported").
- **Где:**
  - `compiler-codegen/src/codegen/emit_c.rs::pattern_binding`:
    Pattern::Tuple теперь возвращает fresh_tmp; caller (emit_for)
    делает destructure отдельно.
  - Новый helper `pattern_destructure_tuple(pat, scr_tmp, scr_is_pointer)`:
    эмитит локальные биндинги через `tmp.f0`/`tmp.f1`/etc для каждого
    field tuple'а.
  - `emit_for` Case 3 (Iter[T]): для tuple-pattern эмитит
    `_NovaTupleN tmp = *(_NovaTupleN*)(intptr_t)opt.value`, затем
    destructure.
- **Bootstrap-ограничения:**
  - Все tuple-fields эмитятся как `nova_int` (bootstrap-convention
    для tuple-payload через nova_int slot). Для пар `(str, int)` или
    `(int, Custom)` нужен полный element-type infer (отложено).
  - Wildcard `_` поддержан — emit `(void)(field);` без binding.
  - Nested patterns (`for ((a, b), c) in ...`) пока не поддержаны.
- **Тесты:** `nova_tests/syntax/for_iter_tuple.nv` — 2 теста:
  basic destructure `(i, v)` через EnumeratedCounter, wildcard
  destructure `(_, v)`. 72/72 nova_tests PASS, без регрессий.
- **План:** docs/plans/06-iter-protocol-codegen.md (Ф.2 закрыт; Ф.3
  implicit `.iter()` остаётся).

### [ЗАКР 2026-05-08] Plan 06 Ф.3: implicit `.iter()` для коллекций

- **Что:** `for x in coll` где `coll` имеет `mut @iter() -> IterT` —
  codegen автоматически вставляет `.iter()` и итерируется по
  результату. По D58: «`for x in collection` вызывает
  `collection.iter().next()` в цикле».
- **Где:**
  - `compiler-codegen/src/codegen/emit_c.rs::emit_for` Case 4: после
    Range/Array/Iter[T] fallback'ов проверяем
    `all_methods.contains((iter_struct, "iter"))`. Если есть —
    synthesize'им `iter.iter()` Member-call и рекурсивно дёргаем
    emit_for. Recursion безопасна: ret-type `coll.iter()` имеет
    `next` через iter_returns registry → Case 3 cрабатывает.
  - Новый registry `iter_returns: HashMap<TypeName, IterTypeName>` —
    заполняется при AST-walk fn-items для `mut @iter() -> IterT`.
  - `infer_expr_c_type` для Member-call `coll.iter()`: lookup в
    `iter_returns` → возвращает `Nova_<IterT>*`.
- **Тесты:** `nova_tests/syntax/for_iter_implicit.nv` — 2 теста:
  basic `for x in coll` (implicit .iter()), legacy форма
  `for x in coll.iter()` (explicit). 73/73 nova_tests PASS,
  без регрессий.
- **План:** docs/plans/06-iter-protocol-codegen.md (Ф.3 закрыт;
  все 3 фазы плана 06 закрыты).

### [ЗАКР 2026-05-08] Plan 04 Этап 1: spec changes (D82 external, D30 full-words, D26 prelude split)

- **Что:** Спека для Plan 04 (split Buffer на StringBuilder /
  WriteBuffer / ReadBuffer + новый keyword `external`):
  - **D82** (новый блок в `spec/decisions/08-runtime.md`) — `external fn`
    keyword: модификатор для функций с runtime-implementation в `nova_rt/*.h`.
    Применяется только к функциям, не к типам. Body должен отсутствовать.
    Порядок modifiers: `export external fn`. Whitelisted namespace —
    `std.runtime.*` (программистский Nova-код не пишет).
  - **D30** расширен (`spec/decisions/03-syntax.md`) — раздел «Полные
    слова, не сокращения»: правило, mainstream-исключения (`len`/`iter`/`idx`),
    запрет ad-hoc сокращений (`pos`/`cap`/`dest`/`buf`/`val`/`cnt`/`tmp`/...).
  - **D26** расширен (`spec/decisions/08-runtime.md`) — добавлены
    StringBuilder/WriteBuffer/ReadBuffer как built-in opaque-типы
    (рядом с примитивами); ReadBufferError sum-тип; таблица verb/finalize.
- **Open-questions:**
  - **Q-buffer** помечена `⚠️ REPLACED (2026-05-08)` — split на три
    Q-блока. История сохранена.
  - **Q-string-builder** (новый, ✅ closed) — UTF-8 string accumulator,
    `@into() -> str` infallible, append-only.
  - **Q-write-buffer** (новый, ✅ closed) — binary serialization, 18
    числовых × LE/BE, `@into() -> []byte`.
  - **Q-read-buffer** (новый, ✅ closed) — cursor-style reader, pair
    `@read_*`/`@try_read_*` с auto-derive на C-runtime уровне.
- **Stub:** `std/runtime/builtins.nv` — documentation-stub с
  external-декларациями всех методов трёх типов + `str.from(c char)`.
  Сейчас это **только документация**: bootstrap codegen ещё не парсит
  `external` keyword (Plan 04 Этап 2).
- **Что осталось:** Этапы 2 (codegen), 3 (runtime), 4 (тесты), 5
  (финализация). Итого ~7-9 часов работы.
- **План:** docs/plans/04-buffer-split-and-external.md (Этап 1
  закрыт).

### [ЗАКР 2026-05-08] Plan 04 Этап 2-5: codegen + runtime + tests

- **Что:** Полная реализация Plan 04 (split Buffer на StringBuilder/
  WriteBuffer/ReadBuffer + новый keyword `external`):
  - **Lexer/Parser:** новый `KwExternal` token; `external` modifier
    парсится между `export` и `fn`. Body для external fn должен
    отсутствовать (compile error «cannot have a body» если есть).
  - **AST:** `FnDecl.is_external: bool` flag; `FnBody::External`
    вариант (для функций без тела).
  - **Codegen:** external fn skip'аются в `emit_fn` и
    `emit_fn_forward_decl` — никакого Nova body не эмитится.
    Dispatch для built-in opaque-типов (StringBuilder/WriteBuffer/
    ReadBuffer) — special-case в emit_call (по аналогии с Buffer/
    Channel pattern).
  - **Overload по типу аргумента:** `StringBuilder.from(s)` vs
    `StringBuilder.from(c)` — разные C-funcs (`Nova_StringBuilder_
    static_from_str` / `Nova_StringBuilder_static_from_char`).
    `types::mod` разрешает duplicate top-level names для
    external-fn (single-key registry — last-wins, dispatch
    делается в codegen).
  - **type_ref_to_c:** `StringBuilder`/`WriteBuffer`/`ReadBuffer` —
    fallback на `Nova_<Name>*` (как обычные record-types).
  - **infer_expr_c_type:** возвращает правильные types для всех
    методов трёх типов.
- **Runtime:** Три новых header'а в `nova_rt/`:
  - **`string_builder.h`** — UTF-8 string accumulator. Метод
    `_nova_utf8_encode` для char→bytes (1-4 байта). `Nova_str_
    static_from_char(cp)` для D73 char.into() → str.
  - **`write_buffer.h`** — binary serialization. 18 numeric × LE/BE
    через макросы `NOVA_WB_WRITE_LE/BE_16/32/64`. f32/f64 через
    IEEE 754 bit-cast.
  - **`read_buffer.h`** — cursor-style reader. **Auto-derive
    pattern** (Plan 04 ключевая фича): одна `_nova_rb_read_uN_LE/BE_raw`
    функция → две Nova-обёртки (`@read_*` Fail-form через
    `_nova_read_buffer_throw_unexpected_end`; `@try_read_*`
    Result-form через `_nova_rb_make_err`). Минимизирует C-код в 2x.
- **Тесты (41 новых):**
  - `nova_tests/runtime/string_builder.nv` — 15 тестов (создание,
    append str/char, UTF-8 multi-byte, capacity grow, hot-loop 100 raz,
    clone, into).
  - `nova_tests/runtime/write_buffer.nv` — 14 тестов (создание,
    write_byte/u32_le/be/u64_le/be/u16/i32 с проверкой byte order,
    auto-grow, clone, into).
  - `nova_tests/runtime/read_buffer.nv` — 12 тестов (cursor
    metadata, read_byte advances position, write/read round-trip
    LE/BE, try_read Ok/Err, multi-value sequence, read_bytes,
    remaining_bytes).
- **Bootstrap-ограничения:**
  - **`ReadBufferError` через nova_str.** Bootstrap-codegen Result
    зашит на `(nova_int Ok, nova_str Err)` (D26). Поэтому Err-payload
    — strings вида `"ReadBuffer.UnexpectedEnd: wanted N, available M"`.
    Когда fail-frame mechanism будет расширен на `void*` payload
    (по аналогии с RuntimeError plan), wrappers обновятся для
    структурированного `Nova_ReadBufferError*`.
  - **f32/f64 в Result через bit-cast.** `try_read_f64_le()` упаковывает
    `nova_f64` как `int64` через `_nova_f64_to_bits` (memcpy double→
    uint64). Вызывающий должен распаковать обратно через bit-cast
    (TBD: добавить helper `f64.from_bits(int)` в codegen).
- **Регрессии:** все существующие тесты проходят (buffer.nv 15/15,
  channels 10/10, auto_derive 6/6, from_into_basic 26/26, etc).
- **`std/runtime/builtins.nv`:** теперь parses & codegens'ится
  (пустой output т.к. все external). Live!
- **План:** docs/plans/04-buffer-split-and-external.md — все этапы
  закрыты. Ключевая learn: **dispatch table pattern** через
  receiver-type check + name-match достаточен для opaque built-in
  типов; полный overload-by-arg-type (Q-overloading) пока только
  whitelisted для external — этого хватает для StringBuilder/
  WriteBuffer/ReadBuffer.

### [ЗАКР 2026-05-08] Plan 04 follow-ups: whitelist enforcement + f64.from_bits + macro UB fix

Три follow-up задачи которые остались open после Plan 04 закрытия:

1. **Whitelist `std.runtime.*` enforcement** (D82). `types::check_module`
   проверяет module.name начинается с `["std", "runtime"]`; если нет
   и встретил `external fn` — error с понятным сообщением: «`external fn`
   is only allowed in `std.runtime.*` modules; for FFI use future
   `extern("C")` (Q-ffi)». Ручной negative-test подтвердил: compile
   error даётся; в `std.runtime.*` всё работает.

2. **`f64.from_bits(int)` / `int.to_bits(f64)` helper pair** для
   распаковки `try_read_f64_*` Result-payload. Codegen dispatch для
   обоих (Path-form и Member-form), infer возвращает правильные types.
   C-helpers — `nova_f64_from_bits` / `nova_int_from_f64_bits` в
   `nova_rt/cast.h` через memcpy bit-cast. 3 новых теста добавлены в
   read_buffer.nv (теперь 15/15 PASS).

3. **Bugfix: UB shadowing в WriteBuffer macros.** Macros
   `NOVA_WB_WRITE_LE/BE_16/32/64` объявляли локальную `uint16_t u =
   (uint16_t)(v)`. Когда вызывались из `write_f32_le/f64_le` — outer
   scope тоже имел `uint32_t u`/`uint64_t u`. Declarator в C вводит
   имя в scope **до** инициализатора, поэтому `(uint16_t)(u)` в macro
   читал неинициализированную shadow'ed variable (UB). MSVC давал
   мусор для f32/f64. **Fix:** переименовали macro-internal `u → _nova_u`.
   Round-trip f64 заработал.

   Урок: macros **обязаны** использовать имена с префиксом (`_nova_*`)
   для всех internal variables — иначе риск shadowing с outer scope.

**Тесты:** read_buffer.nv добавлены 3 теста (f64 round-trip Fail-form,
f64.from_bits + try_read_f64_be Result-form, int.to_bits round-trip
pair). 15/15 PASS. Все остальные buffer-тесты регрессий не имеют
(write_buffer 14/14, string_builder 15/15, buffer 15/15).

### [ЗАКР 2026-05-08] Plan 11 Ф.4.5 + Ф.1-Ф.3 + Ф.6: Self in expr + ad-hoc overload + spec

Закрыты три фазы Plan'а 11 (method values + overload). Не закрыты:
**Ф.4** (method values как first-class), **Ф.5** (disambiguation через
`as fn(...)`) — отложены на следующую сессию.

#### Что сделано

1. **Ф.4.5 — D66 Self в expression position** (~50 строк codegen):
   - `Self.method(args)` в теле метода: rebind `Path[Self, ...]` →
     `Path[<current_receiver>, ...]` в начале эмиссии.
   - `Self { fields }` literal — уже работало через
     `current_receiver_type` resolution (D66 type-position).
   - `Self.method(args)` через Member-form (`obj=Ident("Self")`) —
     rebind на Ident(<current>).
   - infer тоже резолвит Self → current.
   - 4 теста в `nova_tests/syntax/self_in_expr.nv`: default →
     parameterized constructor, Self literal, Builder chain, args.

2. **Ф.1 — Multi-overload registry** (~100 строк codegen):
   - Новый `MethodSig` struct: `param_c_types`, `return_c_type`,
     `is_instance`, `is_external`, `c_name`.
   - `method_overloads: HashMap<(type, name), Vec<MethodSig>>`
     рядом с старым single-key `method_receivers`.
   - Регистрация в AST-walk: для каждого fn-item с receiver'ом
     добавляется sig в Vec по ключу (type, name).
   - Backward compat: первая overload использует короткое C-имя
     (`Nova_T_method_m`); ≥2 — с param-types suffix.

3. **Ф.2 — Overload resolution на call-site**:
   - emit_call: для Member-form (`obj.method(args)`) и Path-form
     (`T.method(args)`) — strict matching по типам args.
   - infer_expr_c_type: ranne находит overload через ту же multi-key
     лютбук → return_c_type правильный (раньше был last-wins).
   - 0 matches при ≥2 candidates → fallback на legacy single-key
     путь.

4. **Ф.3 — C-name mangling**:
   - `Nova_T_method_m` (1st overload) → `Nova_T_method_m__nova_str`
     (2nd, str-version) → `Nova_T_method_m__nova_int_p` (3rd, int*).
   - Pointer `*` → `_p`, `[` → `_arr_`, `]` → ``. Sanitized для
     C-identifier.

5. **Ф.6 — Spec update**:
   - D35 расширен разделом «Перегрузка методов» — strict matching,
     mangling, bootstrap-status, Self в expr position.
   - Q-overloading помечен ⚠️ PARTIALLY CLOSED — variant 1 (ad-hoc)
     закрыт для методов; free-functions overload остаётся запрещён;
     variant 4 (protocol-based) — Plan 12.
   - `types::check_module` разрешает duplicate top-level name для
     методов с receiver'ом (overload), но не для free functions.

#### Тесты

- `nova_tests/syntax/self_in_expr.nv` — 4 теста (Self в expr).
- `nova_tests/syntax/overload.nv` — 3 теста: static `from(int|str)`,
  instance `@add(int|str)`, одноимённые `make()` на разных типах.
- **169/169 PASS** на full regression (15 файлов: buffer/auto_derive/
  from_into_basic/result_methods/unwrap_or/error_runtime_error/
  channels/read_buffer/write_buffer/string_builder + 5 syntax).

#### Что отложено (Plan 11 Ф.4)

**Method values как first-class** — `let f = acc.balance` сохраняет
bound method (pointer + self). Требует:
- Runtime struct `BoundMethod_T_m { fn_ptr, self }` с GC integration
  (self должен outlive bound value).
- Codegen для unbound (`Account.@balance`) и static (`Account.new`)
  как plain function pointers.
- Адаптер для передачи в higher-order функции (`nums.map(int.@to_str)`).

Не делается в этой сессии — отдельный план (~150 строк codegen +
runtime). Plan 11 Ф.4.

#### Урок

**Multi-overload registry рядом с single-key** — миграционный паттерн.
Вместо ломки старого single-key `method_receivers` мы добавили
`method_overloads` и сделали путь fallback'ом. Backward compat: все
существующие 169 тестов проходят без изменений; новый функционал
работает через новый путь. Это **safe-rollout pattern**: новая
инфраструктура поверх старой, миграция отдельная задача.

### [ЗАКР 2026-05-08] Plan 11 Ф.9: D39 anonymous embed `use _ Type`

Реализация D39 в bootstrap-codegen с anonymous embed (`use _ Type`)
+ override-precedence (Own > Delegated).

- **Ф.9.1 Parser:** `use name Type` (named) и `use _ Type` (anon).
  Anonymous имя поля — синтезированное `__embed_<TypeName>`.
- **Ф.9.2 AST + MethodSig.is_delegated:** RecordField.is_embed +
  embed_anonymous, MethodSig.is_delegated, embed_fields registry.
- **Ф.9 Auto-proxy generation:** pre-pass регистрирует Delegated
  MethodSig для каждого Own-метода embedded-типа; emit_embed_proxies
  эмитит C-функцию-делегатор `Nova_Wrapper_method_X(self) →
  Nova_Embedded_method_X(self->field)`.
- **Ф.9.3 Override-precedence Own > Delegated:** в emit_call и infer
  paths, после strict-match — фильтр pool на Own (Delegated wins
  только если Own нет).
- **Ф.9.4 Multi-anonymous detection:** declaration-time error если
  ≥2 anonymous embeds одного типа.
- **Ф.9.5 Lint warning:** stderr-warning при detect Own-override на
  Delegated в anonymous embed (невозможен `@<base>.method()` call).

Тесты: anonymous_embed.nv (3 теста) — named auto-proxy, anonymous
auto-proxy, override Own wins. **175/175 PASS** на full regression
(17 файлов).

Spec D39 обновлён: добавлена Bootstrap-status секция.

**Урок:** **auto-proxy через отдельный emit-pass + override-precedence
в shared dispatch path**. Delegated регистрируются в общем
`method_overloads`, эмиттинг C-кода — отдельный pass после Own
fn-emit'ов. Resolution унифицирован для Own/Delegated через
priority-фильтр. Тот же pattern что для overload в Ф.1-Ф.3 —
**common path с priority**.

### [ЗАКР 2026-05-08] Plan 11 Ф.7: расширение тестов

Plan 11 Ф.7 фиксировал тестовые наборы для overload, self_in_expr,
anonymous_embed. Изначально были минимальные (3+4+3 = 10 тестов).
Расширены до полноценного покрытия:

- **overload.nv: 3 → 9 тестов.** Добавлены: arity overload
  (`@log(msg)` vs `@log(level, msg)`), 3+ overloads на одном методе
  (int/str/bool), mixed static+instance одного имени, разные
  return-types по arg-type, multi-arg overload (по first arg type),
  no-arg vs N-arg arity overload.
- **self_in_expr.nv: 4 → 7 тестов.** Добавлены: Self.method из
  instance-метода (`@si_double` вызывает `Self.si_make`), Self в
  return + Self literal в body одновременно, nested Self.method
  calls (`Self.nst_one().depth + 1`).
- **anonymous_embed.nv: 3 → 9 тестов.** Добавлены: explicit base-call
  через `@<alias>.method()` в named embed, несколько auto-proxy
  методов от одного embed, auto-proxy с args, два named embed разных
  типов, anonymous embed coexists с extra fields, override через
  anonymous embed (Own wins, lint warning).

**Negative test verification:** multi-anonymous detection (`use _
Inner / use _ Inner` в одном record'е) даёт ожидаемый compile error
«multiple anonymous embeds of `Inner`». Проверено напрямую через
nova-codegen (без cl.exe — это compile-time check).

**Регрессия 17 файлов: 190/190 PASS** (было 175 после Ф.9, +15
новых тестов).

Урок: **расширение тестов раскрывает граничные случаи**. При
написании arity-overload (`@log(msg)` vs `@log(lvl, msg)`) проверил
что bootstrap правильно различает по `param_c_types.len()` — да.
При написании Self в instance-методе — проверил что Self резолвится
не только в Path-form но и в Member-form (`obj=Ident("Self")`) —
работало благодаря раннему rebind на 4276.

### [ЗАКР 2026-05-08] Plan 04 Этап 6: Buffer удалён из языка

Plan 04 закрыт полностью. Buffer удалён без backward compat
(Nova не в production, неудачное решение).

#### Что сделано

1. **Plan 11 multi-overload generic-boxing fix.** Регрессия
   stack_queue: новый Plan 11 multi-overload путь не делал
   void*-boxing для generic types (`Stack[T]`). Fix: добавлен
   `is_generic_recv = self.generic_types.contains(&rt)` check;
   args боксируются как nova_str* / void* / void* via intptr.
   `nova_tests/modules/stack_queue` снова PASS.

2. **WriteBuffer @write_char + @write_str** (Plan 04 Этап 6.1).
   `nova_rt/write_buffer.h`: использует `_nova_utf8_encode` из
   string_builder.h (1-4 byte). Codegen registry method_receivers
   обновлён. Smoke-tests в write_buffer.nv (4 новых теста).

3. **str.try_from([]byte)** (для финализации mixed text+binary).
   `Nova_str_static_try_from_bytes(arr)` в `nova_rt/string_builder.h`:
   валидирует UTF-8 через `_nova_validate_utf8`, на success
   `Result.Ok(boxed_str)`, иначе `Result.Err("invalid UTF-8...")`.
   Codegen Path-form dispatch для `str.try_from(bs)`.

4. **Buffer удалён из codegen** (Этап 6.3). 31 reference удалена:
   - record_schemas.insert("Buffer"...) и method_receivers
     (init блок).
   - obj_ty == "Nova_Buffer*" instance dispatch (5 методов).
   - Path-form `Buffer.method` (Member-form + Path-form).
   - infer paths для Nova_Buffer* и effect-schema Buffer.

5. **nova_rt/buffer.h удалён** (Этап 6.4). nova_rt.h `#include`
   убран.

6. **nova_tests/runtime/buffer.nv удалён** (Этап 6.5).
   `nova_tests/types/char_literals.nv` и `nova_tests/types/str_search.nv`
   мигрированы на StringBuilder и WriteBuffer соответственно.

7. **Q-buffer ❌ REMOVED** (Этап 6.6). Помечен как удалённый,
   с замечанием что компилятор Buffer не знает; используйте
   StringBuilder/WriteBuffer/ReadBuffer/WriteBuffer+str.try_from.

#### Регрессии

- nova_tests: **78/78 PASS** (было 79; -1 buffer.nv тоже удалён).
- stdlib: pre-existing failures в std/ (parser limitations,
  multi-line types, codegen ограничения) **не от моих изменений** —
  существующие issues от других sweeps.

#### Pre-existing url.nv issue

url.nv не компилируется на HEAD из-за tuple-destructure infer
ограничения (Plan 06 Ф.2: `let (sch, after) = ...` поля типизируются
как nova_int → `if after.starts_with(...)` падает на strict-bool
check). Это **не Plan 04 issue**. Decode_query реализован правильно
(WriteBuffer + str.try_from), но весь файл idle до tuple-destructure
infer fix.

#### Урок

**Multi-overload путь должен учитывать все аспекты dispatch'а** —
не только resolution, но и generic-boxing. Когда я делал Plan 11,
Stack[T] был bypass'ом legacy single-key path где boxing был.
Теперь Plan 11 покрывает все случаи. Уровень покрытия codegen-
dispatch'а можно проверить через regression-suite — stack_queue
поймал регрессию который иначе попал бы в production.

### [ЗАКР 2026-05-08] Plan 12: builtins.nv-driven external dispatch

`std/runtime/builtins.nv` теперь single source of truth для
StringBuilder/WriteBuffer/ReadBuffer. Codegen читает AST через
`ExternalRegistry` и применяет mangling автоматически. Hard-coded
match'и на ~150 строк удалены.

#### Что сделано

1. **Ф.1 ExternalRegistry** (~200 строк нового кода в
   `compiler-codegen/src/codegen/external_registry.rs`):
   - `include_str!("../../../std/runtime/builtins.nv")` — embedded
     в binary; парсится при `CEmitter::new()`.
   - Двухпроходный `from_module`: подсчёт overload'ов per ключ →
     генерация ExternalDecl с правильным mangling'ом.
   - Mangling: для overload'ов суффикс по Nova-type первого param
     (`_str`/`_char`/`_bytes`/...) — compatible с runtime naming.
   - `lookup(recv_ty, method)` → `Option<&[ExternalDecl]>`.

2. **Ф.2 record_schemas + method_receivers из registry**: hard-coded
   таблицы для StringBuilder/WriteBuffer/ReadBuffer удалены из
   init блока. Replace через iteration по
   `external_registry.receiver_types`. method_receivers использует
   `entry().or_insert()` чтобы НЕ перетирать prelude entries
   (Error.new etc.).

3. **Ф.3 emit_call dispatch через registry**: добавлены
   registry-driven path'и (Member-form instance, Member-form
   static, Path-form static) **до** hard-coded блоков. Strict
   match по arg-types + override Plan 11 multi-overload pattern.

4. **Ф.4 str.from(char) — skip-list**: `str.from` имеет hard-coded
   special-case путь для `int/bool/f64 → str` через
   `nova_int_to_str`/etc helpers (НЕ external fn). Registry
   skip'ает `str.from` чтобы старый hard-coded path работал.

5. **Ф.5 удалить hard-coded dispatch**: 3 блока × ~50 строк удалены:
   - StringBuilder/WriteBuffer/ReadBuffer Member-form instance
     (`obj_ty == "Nova_StringBuilder*"` etc.).
   - StringBuilder/WriteBuffer/ReadBuffer Member-form static
     (`name == "StringBuilder"` etc.).
   - StringBuilder/WriteBuffer/ReadBuffer Path-form static
     (`parts[0] == "StringBuilder"` etc.).
   - Runtime renames: `Nova_WriteBuffer_static_from_bytes` →
     `Nova_WriteBuffer_static_from`, `Nova_ReadBuffer_static_from_bytes`
     → `Nova_ReadBuffer_static_from` (consistent с registry naming
     для single-overload methods).

6. **Ф.7 Acceptance test**: добавлено `WriteBuffer @write_zero(n int)`:
   - `builtins.nv`: `export external fn WriteBuffer mut @write_zero(n int) -> ()`.
   - `nova_rt/write_buffer.h`: `Nova_WriteBuffer_method_write_zero` impl.
   - test в `nova_tests/runtime/write_buffer.nv`.
   **Без правки Rust-codegen'а** — registry парсит builtins.nv,
   mangling даёт правильное имя, dispatch находит. PASS.

7. **Ф.6 — отложен**. Type-checker gate для unknown methods на opaque
   types. Сейчас unknown даёт linker error (late stage); ideal —
   early-stage type error. Отдельный refactor `types/mod.rs`.

#### Регрессии

- 78/78 PASS на nova_tests.
- Регрессия в процессе: prelude.Error.new перетёрся registry-init →
  fix через `entry().or_insert()` чтобы не trample existing entries.

#### Урок

**`include_str!` для embedded source** — правильный паттерн для
"compile-time validated config". Альтернативы:
- Хардкод путя через CARGO_MANIFEST_DIR — fragile, зависит от FS.
- Build script — overengineering для одного файла.
- include_str! — atomic, валидируется на compile time, single binary.

**Двухпроходный mangling** — необходимо для overload'ов с suffix.
Single-pass не знает «всего» количества overload'ов на момент
обработки первой; нужен pre-pass count. Этот pattern переиспользуется
в любом mangling'е где decoration зависит от глобального состояния.

**Single source of truth pattern** масштабируется: добавить новый
opaque type → declare в builtins.nv + impl runtime → готово. Ни
codegen, ни method_receivers init не правятся. Это значит **ниже
порог входа** для расширения stdlib runtime.

### [MVP-CLOSED 2026-05-08] Plan 13: Runtime stdlib projection (str/math)

`std/runtime/*.nv` расширен с `builtins.nv` (StringBuilder/WriteBuffer/
ReadBuffer) на str/math API через **auto-generation** из
`runtime_registry.rs`. MVP: Ф.1-Ф.3, Ф.5-Ф.7 готовы. Ф.4 (полная
migration special-case dispatch'ей в emit_call → registry-driven)
отложен — риск регрессий в 78 тестах требует careful refactor.

#### Что сделано

1. **Ф.1 runtime_registry.rs** (~280 строк):
   - Struct `RuntimeFn`: module/receiver/params/return_ty/c_name/doc.
   - 17 str API entries (char_len, byte_len, find, slice, trim, ...).
   - 27 f64 math entries (sin/cos/sqrt/atan2/pow/hypot/is_nan/...).
   - `all()`/`group_by_module()`/`render_nv()` helpers.
   - Stable order (by module → by receiver → by name) для детерминизма.

2. **Ф.2 nova_rt/string.h + math.h umbrella headers**:
   - String функции уже в nova_rt.h; string.h re-includes для stable
     include-point.
   - Math wrappers ↦ libc <math.h>; math.h re-includes для stable point.
   - Future migration: фактические декларации могут переехать сюда без
     ломки user-кода.

3. **Ф.3 emit-runtime-stubs subcommand**:
   - `nova-codegen emit-runtime-stubs [--root <path>] [--check]`
   - Без `--check`: пишет `std/runtime/string.nv` + `math.nv` (44 funcs).
   - С `--check`: сравнивает existing с registry, fail если diff.
     **Используется в CI/pre-commit для предотвращения manual edits.**
   - Bonus: `nova-codegen dump-runtime` — sanity-print реестра.

4. **Ф.5 D26 + D74 spec update**:
   - D26 → раздел "Runtime stdlib проекция (Plan 13)" — explains что
     методы str/f64/f32 живут в std/runtime/*.nv (auto-gen).
   - D74 → cross-link на std/runtime/math.nv.
   - D82 Bootstrap status → расширен Plan 13 projection описанием.

5. **Ф.6 CI guard**:
   - `--check` режим в emit-runtime-stubs.
   - README.md compiler-codegen — раздел "Регенерация
     std/runtime/*.nv" с workflow.
   - Pre-commit hook integration — TBD (можно добавить как opt-in
     git hook позже).

6. **Ф.7 docs**:
   - README.md compiler-codegen обновлён.
   - docs/promts/regen-runtime.md уже существует от user'а.

#### Ф.4 deferred — почему

Полная migration `f64_method_to_c` / `str_method_to_rt` special-case'ов
в emit_call на registry-driven dispatch требует:
- Замена 2 больших match-таблиц (~50 строк каждая).
- Изменение dispatch path'ов для str/math инстанс-методов.
- Обработка edge cases: `str.from(int/bool/f64)` через nova_*_to_str
  (НЕ external fn), оставить hard-coded; runtime registry's `str.find/etc`
  через registry.
- Тщательный regression test 78 nova_tests на каждом шаге.

Попытка Ф.4 в этой сессии (через `merge_runtime_registry`) trigger'ила
регрессию в self_universal — `Nova_str_static_from` (single overload
без suffix) не существует в runtime (`Nova_str_static_from_char`).
Откатил merge, оставил Ф.1-Ф.3 infrastructure.

Следующая итерация Ф.4: **отдельная сессия** с careful step-by-step
+ runtime renames для consistency.

#### Тесты

- 78/78 PASS на nova_tests после Plan 13.
- Detrminism `emit-runtime-stubs --check` после регена → OK.
- Round-trip: dump-runtime print'ает 44 fn; nova-codegen check
  std/runtime/string.nv + math.nv → both PASS.

#### Урок

**Auto-gen separation of concerns**: registry (Rust) — driver,
.nv-файлы — projection. CI guard через `--check` — **lightweight
typesafety**: drift поймается при review даже если разработчик
случайно отредактировал .nv. Прецеденты: Cargo.lock vs Cargo.toml,
go generate, protoc-generated .pb.go — все используют этот pattern.

**Migration risk-management**: full Ф.4 был соблазнительным «всё
одним коммитом», но conservative split (Ф.1-Ф.3 + Ф.5-Ф.7
infrastructure → Ф.4 dispatch отдельно) даёт **safe-rollout**:
каждая фаза независимо проверяема, регрессии не накладываются.

### [ЗАКР 2026-05-08] Plan 13 Ф.8: декомпозиция builtins.nv + f32 math

После Ф.8 **в `std/runtime/` нет ни одного handwritten файла** — всё
auto-generated. Single source of truth pattern окончательно завершён
для opaque types и numeric/str API.

#### Что сделано

1. **Ф.8.1 string registry audit**: убран `is_empty` (нет в runtime),
   добавлен `eq` (есть в runtime, использовался через operator).
   Все 17 special-case'ов в emit_c.rs соответствуют registry.

2. **Ф.8.2 f32 math** (~25 entries):
   - C-имена через `f`-suffix (sqrtf, sinf, cosf, ...).
   - Predicates (isnan/isfinite/isinf) — type-generic C99 macros,
     те же имена.
   - Auto-generated в `std/runtime/math.nv` параллельно f64 секции.

3. **Ф.8.3 декомпозиция builtins.nv** (~70 entries):
   - `string_builder.nv`: StringBuilder API (new/with_capacity/
     from(s|c)/append(s|c)/len/capacity/clone/into).
   - `write_buffer.nv`: WriteBuffer API (write_byte/write_bytes/
     write_zero/write_char/write_str + 18 numeric × LE/BE +
     finalize).
   - `read_buffer.nv`: ReadBuffer API (cursor metadata + 20 read_*
     × Fail-form/try-form pairs = 40 entries).
   - `char.nv`: `str.from(c char)` UTF-8 encode.
   - Box::leak'ом для `'static str` runtime-вычисленных имён.

4. **Ф.8.4 regen + delete**:
   - 6 файлов сгенерированы.
   - `std/runtime/builtins.nv` удалён.
   - 78/78 PASS regression.

5. **Ф.8.5 Multi-file ExternalRegistry**:
   - `include_str!` для 4 файлов (string_builder/write_buffer/
     read_buffer/char) — все embedded в binary.
   - `merge_from_module` aggregator: каждый файл парсится → merge
     entries в общий registry.
   - string.nv/math.nv пока не loaded в codegen (Plan 13 Ф.4
     deferred — special-case dispatch остаётся для str/math).

6. **Spec D26/D82** — описания заменены на per-type файлы.
   Plan 13 раздел в D82 расширен — таблица 6 файлов + объяснение
   ExternalRegistry multi-file load.

7. **regen-runtime.md** prompt обновлён.

#### Total numbers

- Registry entries: **157** (было 44 — +113 от opaque types + f32).
- Auto-generated .nv файлов: **6** (было 2 — string + math; +4
  opaque + char).
- Handwritten .nv в `std/runtime/`: **0** (было 1 — builtins.nv).

#### Тесты

- 78/78 PASS на nova_tests.
- `nova-codegen check std/runtime/*.nv` — все 6 файлов parse'ятся.
- Detrминизм `regen_runtime.bat --check` → OK.

#### Урок

**Декомпозиция handwritten exception**: один handwritten файл рядом
с auto-generated — это «исключение из правила». Plan 13 Ф.8
устраняет его, делая единообразный single source of truth pattern.

**Multi-file include_str!** — паттерн для embedded sources где их
несколько. `include_str!` принимает literal path (не runtime),
поэтому каждый файл — отдельная константа. Загрузка через цикл
`for src in [SRC_A, SRC_B, ...]` aggregator'ом. Это даёт
extensibility без runtime FS dependency.

**Box::leak для 'static str из runtime-computed строк**: registry
содержит ~50 entries которые формируются программно (`format!`).
Для `&'static str` lifetime — leak'аем, один-time alloc. Альтернатива
(static lookup table) — тысячи строк boilerplate'а.


### Plan 13 Ф.9 — API polish, читаемость auto-gen, Self everywhere (2026-05-08)

После завершения Ф.8 ревью сгенерированных `std/runtime/*.nv` файлов
выявило 6 unfortunate API-decisions, которые лучше зафиксировать
до того как пользовательский код устаканится. Ф.9 — точечные правки:

#### Ф.9.0 — пустые строки между методами в auto-gen

Renderer добавляет `\n\n` после каждой `// doc + external fn` пары
(было `\n`). Файлы стали читаться как нормальные spec-документы.

Проблема старого формата: 24+ `external fn` подряд без визуальных
групп — глаз теряется. Diff-review был мучительным.

#### Ф.9.1 — Self-return everywhere для chaining

Mutating-методы (`@append`, `@write_*`) и creation-static (`new`,
`from`, `with_capacity`) — теперь все возвращают `Self`. Единый
паттерн на opaque types вместо «здесь Self, тут явный тип».

```nova
// Было: read_buffer.nv
export external fn StringBuilder.new() -> StringBuilder
export external fn StringBuilder mut @append(s str) -> ()

// Стало:
export external fn StringBuilder.new() -> Self
export external fn StringBuilder mut @append(s str) -> Self
```

Chaining работает: `sb.append("hello ").append(name).append("!")`.
C-side: `Nova_<T>_method_*` возвращают `Nova_<T>*` self-pointer
(тот же receiver, без аллокации). `void`-returning функции стали
identity функции с return value — backward-compat для statement-style
вызовов сохраняется.

#### Ф.9.3 — str.@char_len → str.@len, []char первоклассный

D26 spec говорит «s.len — длина в codepoint'ах». Имя `@char_len`
отражало реализацию (codepoint = char), но противоречило spec.
Переименовано в `@len` (C-name `nova_str_char_len` сохранён).

`str.@chars() -> []int` → `-> []char` — char стал first-class type
в API, eager allocation как минимум (lazy `Iter[char]` — future).

#### Ф.9.4 — read_char/read_str для парсинга текста

ReadBuffer покрывал только числовые типы. Для HTTP headers, CSV,
text-протоколов нужны codepoint-методы:

```nova
fn ReadBuffer mut @read_char()      Fail[ReadBufferError] -> char
fn ReadBuffer mut @read_str(n int)  Fail[ReadBufferError] -> str
```

Plus Result-формы `try_read_char` / `try_read_str(n)`.

`ReadBufferError` расширен вариантом `InvalidUtf8 { position }` —
distinct ошибка от `UnexpectedEnd` (мусорный байт vs неполная sequence).

C-runtime получил helper `_nova_rb_decode_utf8_one(p, avail, *cp,
*consumed)` — общий UTF-8 декодер. Используется в read_char,
read_str, try_read_char, try_read_str — DRY.

#### Ф.9.5 — отмена auto-derive try_read_* из Plan 12 Ф.4.5

Plan 12 Ф.4.5 предлагал: компилятор синтезирует `@try_read_X()` из
`@read_X() Fail[E]`. Отменено в Ф.9.5 по 3 причинам:

1. **Hidden magic.** В registry/.nv видна только Fail-форма, но IDE
   автокомплит показывает try_read_X неоткуда. AI-генерируемому коду
   ещё сложнее.
2. **Edge cases.** UTF-8 ошибки (Ф.9.4) делают universal правило
   хрупким — synth должен мапить и UnexpectedEnd, и InvalidUtf8.
3. **D82 single source of truth.** Auto-derive противоречит принципу
   «всё что компилятор знает — видно в registry».

В runtime_registry все 17 пар read_*/try_read_* (16 numeric + char +
str) явно перечислены. C-функции тоже две (Fail + Result).

D73 From↔Into auto-derive **остаётся** — симметричное правило в D73,
не зависит от Plan 12 Ф.4.5.

Plan 12 Ф.4.5 помечен ❌ ОТМЕНЕНО, spec D82 обновлён.

#### Бонус: str.from(int) regression fix

После Ф.8 в registry появился `str.from(c char)`, что засветило `from`
в `method_receivers` и сломало dispatch для `str.from(int_val)` —
codegen эмитил `Nova_str_static_from(v)` без mangling-suffix'а, а
реальная C-функция называется `Nova_str_static_from_char`.

Fix: routing через method_overloads с поиском подходящего overload'а
по C-типу аргумента. Если match — sig.c_name. Иначе fallback на legacy
`nova_int_to_str(v)`. Применено в обеих точках emit_c.rs.

#### Бонус: hashmap.nv `&T` borrow → plain field

`std/collections/hashmap.nv` использовал `map_ref &HashMap[K, V]` —
`&T` borrow запрещён в Nova (D43, см. spec/decisions/05-memory.md:63).
Поле переименовано в `map HashMap[K, V]` (короче, без borrow). GC
держит мапу живой через field-reference.

#### Что отложено

**Ф.9.2 — оператор `+` как alias** (`StringBuilder + str` → `@append`,
`str + str` → `@concat`). Требует careful routing через
method_overloads и parameter-type mangling. Риск регрессий в 78
тестах. Перенесено в следующую сессию.

#### Total numbers (после Ф.9)

- Registry entries: **161** (было 157 — +4 от Ф.9.4 read_char/str
  пар).
- Auto-generated .nv файлов: **6** (без изменений).
- Handwritten в `std/runtime/`: **0**.
- Self-return mutating/creation методов: **30+** (вместо `()`/тип).

#### Урок

**Plan rev iterations работают.** Ф.9 — это «после ревью
сгенерированного» этап. Без regen → review → fix цикла файлы
выглядели бы хуже. AI-friendly auto-gen означает что один этап
не финализирует API — нужен полный round-trip с ревью.

**Self vs explicit type — единый паттерн лучше микрооптимизации.**
Изначально creation-static возвращали `WriteBuffer` (явный тип),
а instance-mut → `Self`. Ревью показало: непоследовательно.
Унификация на Self (везде в opaque-context) убрала когнитивную
нагрузку.

**Auto-derive symmetry rule (D73 ↔ Plan 12 Ф.4.5).** D73 From↔Into —
симметричное правило: synthesized метод имеет ту же семантику. Plan
12 Ф.4.5 try_read auto-derive — асимметричное (Fail vs Result, разная
семантика для caller'а). Симметричные правила выживают, асимметричные
становятся source of bugs.


### Plan 11 Ф.4+Ф.5 — Method values как first-class (2026-05-08, вечер)

Plan 11 Ф.1-Ф.3 (overload по типу аргумента) был закрыт в первой
половине дня. Ф.4 (method values) и Ф.5 (`as fn(...)` disambig)
оставались deferred до этой сессии.

#### Ф.4 — три формы method values

**Bound** — `obj.@method`. Closure struct {fn_ptr, captured_self}.
При вызове `f(args)` codegen unpacks struct, вызывает fn с env+args,
fn-wrapper извлекает self из env и вызывает реальный
`Nova_<T>_method_<m>(self, args)`.

**Unbound** — `Type.@method`. Closure struct {fn_ptr, dummy_env}.
fn-wrapper принимает self как первый параметр явно, не хранит его в
env.

**Static** — `Type.method` (без `@`). Уже работало через
`nova_fn_<name>` поинтер.

#### NovaClosBase — generic closure layout

До Ф.4 nova_rt.h имел только 5 hardcoded closure structs (NovaClos_vi,
ii, ib, iii, vii) — для конкретных сигнатур lambda. Method values
имеют **произвольные** сигнатуры (Counter*, int) → int, etc.

Решение: добавлен `NovaClosBase = { void* fn; void* env }` —
**generic** layout. Bit-уровень: same as NovaClos_*. На call-site
codegen cast'ит `fn`-поле к нужной сигнатуре:

```c
((ret(*)(void*, args...))((NovaClosBase*)f)->fn)(((NovaClosBase*)f)->env, args...)
```

Это работает для **любой** сигнатуры без per-sig macros. Per-sig
macros остаются для optimization (когда сигнатура hardcoded — компилятор
видит typed call) и backward-compat для NovaClos_* lambda emission.

#### Ф.5 — `as fn(P...) -> R` disambiguation

Когда у метода несколько overload'ов по типу аргумента:

```nova
fn Buf mut @push(n int) -> int => ...
fn Buf mut @push(b bool) -> int => ...

let f = buf.@push                       // ambiguous → берётся first
let g = buf.@push as fn(int) -> int     // выбор первого overload'а
let h = buf.@push as fn(bool) -> int    // выбор второго overload'а
```

В codegen emit_expr для `As(Member, TypeRef::Func)`:
1. Извлекаем target_signature из Func type.
2. Вызываем `emit_method_value_typed(obj, method, Some(sig))`.
3. `emit_method_value_typed` фильтрует overloads по param-types match.
4. Match'ed overload даёт правильный mangled c_name (Plan 11 Ф.3
   уже эмитил `Nova_Buf_method_push__nova_bool` для второго overload'а).

Для unbound `Type.@method as fn(Recv, P...) -> R` skip первый param
(receiver) при сравнении.

#### Ф.7 — тесты

- `nova_tests/syntax/method_values.nv` — 7 тестов: bound (no/one/two
  args), unbound, разные obj несут свои self, as-fn annotation.
- `nova_tests/syntax/overload_method_values.nv` — 3 теста: bound int
  overload, bound bool overload, unbound int overload — все через
  `as fn(...)`.

После Ф.4+Ф.5: **80/80 nova_tests PASS** (было 78 — +2 новых тестовых
файла, оба passes).

#### Bootstrap-ограничение

**External methods (str, int runtime) не доступны как method values.**
`s.@byte_len` сейчас bails: codegen ищет в `method_overloads` registry,
а built-in str API живёт в `ExternalRegistry` (`std/runtime/string.nv`
external decls). Routing через ExternalRegistry — future work.

Workaround: для current bootstrap'а — оборачивать в lambda:
`let f = (s) => s.byte_len()`. Future: emit_method_value_typed
fallback'ом ищет в external registry.

#### Урок

**Generic `NovaClosBase` lifts the «hardcoded sig matrix» limitation.**
Closures с произвольными сигнатурами были невозможны без per-sig macros.
NovaClosBase + cast-at-call-site решает это в ~10 строк runtime'а +
~15 строк codegen-fallback.

**Desugar to lambda был bardziej elegant, но не нужен.** Думал сначала
synthesize Lambda AST для bound case → reuse emit_lambda. Но direct
emission (генерация wrapper-fn + env-struct + closure-alloc inline)
оказался проще: меньше indirections, прозрачнее в emitted C.

**Type annotation как hint для codegen — стандартный приём.**
`as fn(...)` не меняет run-time поведение (остаётся `(void*)expr`
cast), но **меняет codegen** на let-binding и emit_method_value
levels — выбор overload'а. Прецедент: TypeScript type assertions
влияют на overload resolution.

### [ЗАКР 2026-05-08] Plan 13 Ф.9.6: StringBuilder.@len bag-fix (codepoints)

- **Где:** `compiler-codegen/nova_rt/string_builder.h` +
  `compiler-codegen/src/codegen/runtime_registry.rs` +
  `compiler-codegen/src/codegen/emit_c.rs` (type-inference) +
  тесты в `nova_tests/runtime/string_builder.nv` + `types/char_literals.nv`.
- **Bag:** `Nova_StringBuilder_method_len` возвращал `b->len` —
  размер буфера в **байтах** (UTF-8). Но D26 школа B диктует:
  `@len` для текстовых типов = **codepoint count**. `nova_str.@len`
  через `nova_str_char_len` — codepoints, а StringBuilder — байты.
  Ассиметричность ловила пользователей: `StringBuilder.from('Я').len()`
  возвращало 2 (байт), хотя `"Я".len == 1` (codepoint).
- **Фикс:**
  1. `Nova_StringBuilder_method_len` — UTF-8 lead-byte walk (O(n)),
     совпадает с `nova_str_char_len`.
  2. Добавлен `Nova_StringBuilder_method_byte_len` (O(1) — `b->len`)
     для FFI / capacity-планирования.
  3. Registry doc обновлён.
  4. 14 тестов в `string_builder.nv` переписаны с двойным покрытием
     (для каждого теста проверяется `len()` и `byte_len()`).
- **Урок:** имя поля в struct (`b->len` — байты) и имя публичного
  метода (`@len` — codepoints) не должны быть 1:1 если spec
  диктует разную семантику. Field representation — internal,
  method API — public contract. Аудит таких mismatches —
  обязательная часть API review.

### [ЗАКР 2026-05-08] Plan 13 Ф.9.2: оператор `+` через `@plus` Nova-метод (D46)

- **Где:** `compiler-codegen/src/codegen/runtime_registry.rs` (RuntimeFn
  расширен полем `nova_body: Option<&str>` + renderer) +
  `compiler-codegen/src/codegen/emit_c.rs` (BinOp::Add routing) +
  std/runtime/{string,string_builder}.nv (regen) + новый
  `nova_tests/runtime/plus_operator.nv` (9 тестов).
- **Что было:** Bootstrap имел invisible-intrinsic для `str + str`
  (hardcoded `nova_str_concat` в emit_c.rs:3621). Программист не
  видел декларации `@plus` в registry/.nv → IDE / AI помощники
  не знали о существовании оператора.
- **Что стало:**
  1. `RuntimeFn.nova_body: Option<&'static str>` — `Some("@append(s)")`
     для записей с body, `None` для external. `c_name` игнорируется
     для записей с body.
  2. Renderer: `nova_body.is_some()` → `export fn ... -> T => {body}`
     (без `external`).
  3. Registry-записи:
     - `StringBuilder.@plus(s str) -> Self => @append(s)`
     - `StringBuilder.@plus(c char) -> Self => @append(c)`
     - `str.@plus(other str) -> str => @concat(other)`
     После regen `std/runtime/string_builder.nv` + `string.nv`
     содержат явные Nova-fn декларации `@plus` — программисту виден
     contract.
  4. Codegen `BinOp::Add` routing:
     - `Nova_StringBuilder*` + `nova_str` → `Nova_StringBuilder_method_append_str`.
     - `Nova_StringBuilder*` + `nova_int` (char) → `Nova_StringBuilder_method_append_char`.
     - `nova_str` + `nova_str` → `nova_str_concat` (теперь это C-имя
       метода `@concat` объявленного в registry — связь явная).
- **Bootstrap-ограничение:** routing для `BinOp::Add` сейчас hardcoded
  для встроенных типов (str, StringBuilder). User-defined `@plus`
  через `+` ещё не работает — нужен method_overloads lookup в codegen
  для BinOp::Add. Future task (отдельный план или Ф.9.7).
- **Тесты:** `nova_tests/runtime/plus_operator.nv` (9 тестов) —
  str+str (empty, ASCII, Unicode codepoint count + byte count),
  sb+str (sequential append, UTF-8 mixed), sb+char (single ASCII,
  multiple, 4-byte codepoint), смешанно sb+str/sb+char.
- **Урок:**
  - Nova-метод с body в registry — естественное расширение
    single-source-of-truth. `=> @append(s)` это не magic, обычный
    Nova syntax; программист видит делегацию в .nv-файле.
  - Auto-derive паттерны различимы по симметрии: D73 From↔Into
    (симметричное) остаётся, Plan 12 Ф.4.5 try_read auto-derive
    (асимметричное) отменён в Ф.9.5. Plan 13 Ф.9.2 — третий путь:
    body-as-data вместо synth-rule.
  - C-имя метода = invisible intrinsic не хуже visible declaration в
    registry. Inline emit того же C-вызова сохранён для performance,
    но связь через registry делает API discoverable.

---

## Plan 17 — Q-resolutions (2026-05-08, ✅ ЗАКРЫТ)

- **Что упрощено:** в spec'е было 11 полу-открытых Q-вопросов, для
  которых de-facto-поведение существовало (или решение было
  очевидно), но не зафиксировано формально. Plan 17 закрыл 6 из них
  прямой правкой decisions/*.md — теперь LLM-генерируемый код имеет
  однозначный referer для:
    - `@clone()` — shallow по умолчанию (D26),
    - style coercion с D55 — permissive с таблицей рекомендаций,
    - Built-in API для `[]T` — формальный список встроенного vs
      stdlib-расширений (D38),
    - `@method` vs `method()` в protocol-блоках — обе формы валидны (D53),
    - keyword-as-fields — строгий запрет (D83, уже было),
    - string interpolation `${...}` — JS-style sugar над str.from
      (D44; design CLOSED, codegen open).
- **DEFER-rationale.** Для оставшихся 5 Q (pipe-operator, fail-coercion,
  default-generic, numeric-coercion, static-method-protocol) добавлен
  явный rationale «почему сейчас не делаем» + trigger для пересмотра.
  Это дисциплинирует — Q без объяснения = висящий TODO; Q с trigger
  знает «когда вернуться».
- **Audit-fail.** Plan 17 утверждал, что string interpolation
  «работает де-факто в codegen». При прогоне regression-теста
  оказалось — codegen **не разворачивает** `${x}` в обычных
  строковых литералах; `"${x}"` сохраняется как сырая 4-codepoint
  строка. Spec-фиксация скорректирована («design CLOSED, codegen
  NOT YET IMPLEMENTED»), регрессия удалена. Урок: empirical check
  перед фиксацией — даже когда «очевидно работает».
- **Регрессия:** добавлен `nova_tests/runtime/clone_semantics.nv`
  (5 тестов на shallow-семантику record + StringBuilder/WriteBuffer
  deep). 86/86 nova_tests PASS.
- **Файлы:** spec/decisions/{02,03,08}-*.md, spec/syntax.md,
  spec/open-questions.md, docs/plans/17-q-resolutions.md,
  docs/plans/README.md, nova_tests/runtime/clone_semantics.nv.

---

## Plan 17 Ф.4 — string interpolation полная реализация (2026-05-08)

- **Что упрощено в use-site:** `"hello, ${name}, age=${n}"` теперь
  работает в bootstrap'е через все слои компилятора. Программисту
  больше не нужно писать `"hello, " + name + ", age=" + str.from(n)`
  с ручным `str.from`. Это самая частая операция в форматирующем
  коде.
- **StringBuilder-based emit, не цепочка `+`.** Один `+` в hot loop
  даёт O(N²); StringBuilder с pre-size estimate — O(N). Codegen
  делает right-thing-by-default — программист пишет короче и без
  performance-trap'а одновременно. Sub-expressions внутри `${...}`
  диспатчатся per-type: `nova_bool` → "true"/"false", `nova_f64` →
  `%g`, `CharLit` → UTF-8 encode, user-тип через D73 `@into()->str`.
- **Sentinel-байт `\x01` для escape `\$`.** Lexer кладёт SOH+`$`
  при встрече `\${` чтобы parser отличил literal-`${` от
  interpolation-`${`. SOH в обычном Nova-коде не встречается
  (control char). Альтернативы (отдельные TokenKind для частей,
  compound token) — overkill для одной фичи.
- **Audit-fail исправлен.** Изначально Plan 17 утверждал «codegen
  разворачивает де-факто» — оказалось ложным. Empirical check
  выявил → реализовали полный стек. Урок: spec без empirical
  verification = potential lie.
- **Sub-lex/sub-parse expressions внутри строки.** Парсер при
  встрече `${...}` запускает на содержимом отдельный
  `Lexer::new(expr_src) + Parser::parse_expr()`. Поддержаны nested
  `{}` через depth-counter; пустое `${}` — compile error.
- **Const-инициализатор guard.** `static const nova_str FOO =
  "${expr}"` запрещён через явный compile error «not allowed in
  const initialiser» — StringBuilder требует runtime аллокаций.
  Тесты запретного кейса не нужны (это compile-time guard).
- **Регрессия:** `nova_tests/types/string_interpolation.nv` — 13
  тестов: int / negative / str / bool / f64 / char-литерал /
  multi / expression in `${}` / escape `\${` / большая строка
  (12 интерполяций) через StringBuilder. 87/87 nova_tests PASS.
- **Файлы:** lexer/parser/ast/codegen/interp + spec/decisions +
  open-questions.md + nova_tests/types/string_interpolation.nv.

---

## Plan 14 Ф.3 — free fn as first-class value (2026-05-08, ✅ ЗАКРЫТ)

- **Что упрощено в use-site:** `let f = inc` и `xs.map(inc)` теперь
  работают без обёртки в lambda. До Ф.3 код `xs.map(inc)` ломался
  с linker-ошибкой (несовместимые типы Nova fn-pointer ≠ closure-struct);
  программисту приходилось писать `xs.map((x) => inc(x))` с явным шумом.
- **Реестр user_fn_sigs + thunk-генерация.** Каждая top-level fn без
  receiver и без generics автоматически получает (param_c_types, ret_c)
  записанный в реестре. При первом use-as-value codegen эмитит envless
  thunk-adapter `nova_fn_<name>_thunk(void* env, args...) { (void)env;
  return nova_fn_<name>(args...); }` — adapter принимает `env` (для
  closure-протокола) и игнорирует его (free fn без захвата). Дедупликация
  через `emitted_fn_thunks: HashSet<String>` — N use-sites одной fn
  делят один thunk.
- **Closure-литерал на use-site.** Вместо raw fn-pointer на use-site
  alloc'ается `NovaClos_X*` с `fn = &thunk, env = NULL`. Это совместимо
  со всеми callers (HOF принимающие `void*`, fn_param_sigs-driven
  calls через NOVA_CLOS_CALL_* macro).
- **Direct calls не ломаются.** `inc(5)` идёт через `emit_call` без
  `emit_expr` на `Ident(inc)`, поэтому остаётся `nova_fn_inc(5)`.
- **Generic fn fallback.** Generic functions не регистрируются в
  user_fn_sigs (sig зависит от мономорфизации) — для них fallback на
  raw fn-pointer (старое поведение).
- **Известное ограничение:** flat var_types в bootstrap (нет scope).
  Local `let dbl = ...` в одном тесте затмевает global `fn dbl` в другом.
  Тесты используют `top_inc`/`top_dbl` чтобы избежать конфликта.
- **Регрессия:** nova_tests/syntax/fn_first_class.nv — добавлено 5
  тестов на free-fn-as-value. 87/87 nova_tests PASS (без регрессий).
- **Файлы:** compiler-codegen/src/codegen/emit_c.rs (+95 строк),
  nova_tests/syntax/fn_first_class.nv (+50 строк), plan 14 status.

---

## Plan 14 Ф.2 + Ф.4 + bonus codegen-fixes (2026-05-09, ✅ ЗАКРЫТЫ)

### Ф.2 — const с runtime-init (lazy-init геттер)

- **Что упрощено в use-site:** `const SERVER_DEFAULTS = ServerOpts {
  port: 8080, host: "0.0.0.0", ... }` теперь работает. Раньше падал
  на codegen с «non-constant expression in const declaration».
  Программист мог обойти только через геттер-функцию вручную.
- **Lazy-init pattern.** Codegen эмитит storage + init-flag + геттер
  `nova_const_<name>()` с проверкой первого вызова. Вызывается O(1)
  после init. Use-site `Ident(name)` → `nova_const_<name>()`. D55
  coercion работает (`expected_record_type` устанавливается перед
  emit).
- **Constexpr path неизменен.** Простые литералы (int/bool/f64/str/
  char/Unary) по-прежнему `static const` без накладных расходов.

### Ф.4 — fn-поле в record (closure-call routing)

- **Что упрощено в use-site:** `let op = Op { f: (x) => x + 1 };
  op.f(5)` теперь работает. Раньше codegen воспринимал Member-call
  как метод-вызов и не находил method `f` на типе `Op`.
- **Реестр record_field_fn_sigs.** Заполняется при `emit_record_type`
  для всех `TypeRef::Func` полей. Routing в Call: если obj_ty —
  record и (record, method) ∈ record_field_fn_sigs → emit
  `NOVA_CLOS_CALL_*` macro с f-value `(obj->field)`.

### Bonus a — line:col в codegen-ошибках

- **Что упрощено в debug-loop:** ошибки codegen теперь показывают
  `<file>:<line>:<col>:` вместо абстрактного «non-bool cond».
  Найти конкретное место стало тривиально (раньше нужны были грепы).
- **Архитектура:** main.rs всегда передаёт source в emitter (это уже
  было через `set_source_for_annotations`); флаг `annotation_enabled`
  отделяет SRC-комментарии в C-output (--annotate-source) от
  диагностики (всегда). Метод `check_bool_condition_at(span)`
  использует source для line:col.

### Bonus b — D38 built-in is_empty для []T и str

- **Что упрощено в use-site:** `if arr.is_empty { ... }` и
  `if s.is_empty { ... }` работают. Раньше падали на strict bool-check
  потому что infer возвращал `nova_int` (default fallback).
- **Codegen:** прямой emit `(arr->len) == 0` / `(s.len) == 0` (это
  bool-выражение, не cast). Infer возвращает `nova_bool`.

### Bonus c — str-методы ret-type в infer

- **Что упрощено в use-site:** `if s.starts_with(...)` / `.ends_with` /
  `.contains` / `.eq` теперь корректно проходят strict bool-check.
  Раньше infer возвращал default `nova_int` для всех Member-call'ов
  на str (потому что str-методы зарегистрированы в `runtime_registry`,
  не `method_overloads`, который смотрит infer).
- **Решение:** `str_method_ret_type` map (parallel к существующему
  `str_method_to_rt` для emit). Используется в `infer_expr_c_type`
  для Call с Member-func и obj_ty == "nova_str".

### Tests / regression

- nova_tests/syntax/const_complex.nv (новый, 6 тестов) — все слои Ф.2.
- nova_tests/syntax/fn_first_class.nv (+4 тестов) — Ф.4 closure-call.
- 88/88 nova_tests PASS (87 baseline + const_complex).
- 91/137 std PASS — то же overall, но 4 const-related файла
  продвинулись по pipeline'у (упираются дальше в др. gap'ы).

### Файлы

- compiler-codegen/src/codegen/emit_c.rs — Ф.2 (lazy_const) + Ф.4
  (record_field_fn_sigs) + bonus (line:col, is_empty, str-method
  ret-type) — ~220 строк всего.
- compiler-codegen/src/main.rs — source всегда передаётся.
- nova_tests/syntax/const_complex.nv — новый.
- nova_tests/syntax/fn_first_class.nv — +50 строк тестов.
- docs/plans/14-stdlib-codegen-gaps.md — Ф.2/Ф.4 retro + table.
- docs/plans/README.md — статус.
