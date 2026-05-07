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
  Тип элемента выводится через `infer_expr_c_type`. Тест: `tests-nova/39_for_in_array.nv` (11 assert).

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
user handler). Тесты в `tests-nova/45_fail_handler.nv` (7 тестов:
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
`_nova_test_frame`. Тест `tests-nova/concurrency/assert_in_fiber.nv` (4 теста:
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
- Тесты `tests-nova/effects/fail_handler.nv` — все 7 spec-compliant
  через `interrupt ()` (раньше использовали bootstrap-leniency
  `return ()` — теперь это spec-correct).

**[ЗАКР] Cancel-token API — D75 (2026-05-06).**
`cancel_scope { tok => body }` keyword, `NovaCancelToken` first-class
type, `tok.cancel()`/`is_cancelled()`/`bind()` методы. Реализовано
поверх существующего `cancel_requested` flag из D71. Bind даёт
каскадную отмену (parent.cancel() → child тоже cancel'ится).
- **Тесты:** `tests-nova/52_cancel_scope.nv` (5 тестов).
- **Известные ограничения:** см. D75 «Известные ограничения
  bootstrap-реализации» — re-throw на main приходит как plain
  nova_throw (user `with Fail` handler не вызывается для cancel-throw),
  NOVA_CANCEL_LINKED_CAP=8.

#### Roadmap к полноценной реализации (порядок)

1. ~~**Top-level `try/catch`**~~ → **rejected by spec.** Заменяется
   через `with Fail = handler { ... }` (см. п. 3). **Сделано
   (2026-05-06): tests-nova/45_fail_handler.nv** — 7 positive-тестов
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
- **Тесты:** `tests-nova/41_parallel_for.nv` — 12 тестов statement-mode (interleaving,
  snapshot semantics). `tests-nova/50_parallel_for_array.nv` — 6 тестов array-mode
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
  Тесты: `tests-nova/40_detach.nv` (13 тестов на capture/control-flow/nesting/
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
  На Nova-уровне: `tests-nova/37_deep_gc.nv` (18 тестов) и `38_deep_spawn.nv` (28 тестов).

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
  suite + 11 stdlib (опционально). По умолчанию — только tests-nova/.
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
- **Тесты:** `tests-nova/55_buffer.nv` (16 тестов: basic ops, grow,
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
- **Тесты:** `tests-nova/56_char_literals.nv` (16 тестов: ASCII,
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
- **Тесты:** `tests-nova/runtime/unwrap_or.nv` (14 тестов).
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

### [2026-05-07] tests-nova/ — иерархическая реорганизация
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
- **Где:** `tests-nova/effects/handler_wrappers.nv` (4 теста).
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
- **Тесты:** `tests-nova/types/str_search.nv` обновлён (section 7
  переписан с `len == bytes` на `len == codepoints` + `byte_len()`,
  добавлена section 8 с Cyrillic/emoji find/rfind/slice).
- **Закрывает:** Q-string-indexing.

### [ЗАКР 2026-05-07] Bitwise операторы — реализованы
- **Где:** `compiler-codegen/src/lexer/{token.rs,mod.rs}`,
  `parser/mod.rs::parse_bit_or/xor/and/shift`,
  `codegen/emit_c.rs::Binary{,_op_str}`,
  `tests-nova/types/bitwise.nv` (28 тестов).
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
