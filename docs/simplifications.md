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

### [ЗАКР 2026-05-10] Plan 16: D63 forbid + D64 realtime capability enforcement

- **Что:** `forbid X { body }` теперь действительно блокирует вызовы
  fn'ов с эффектом X внутри body. `realtime { body }` блокирует
  suspend-effects (Net/Fs/Db/Time/Blocking). `realtime nogc { body }`
  дополнительно блокирует alloc-fn'ы (`[]T.new`, `HashMap.new`,
  `StringBuilder.new`, `str.from`, etc.). `with X = ...` внутри
  `forbid X` — compile error (D63 «forbid непреодолим»).
- **Где:** `compiler-codegen/src/types/mod.rs::CapabilityCtx` (~492
  строки). AST + parser: новый `RealtimeAttr` enum + `@realtime`
  attribute parsing (~50 строк). Test infra: `// EXPECT_COMPILE_ERROR`
  маркер в `run_tests.ps1` (~46 строк).
- **Use-site эффект:**
    ```nova
    type Net effect { fetch(url str) -> str }
    fn http_get(url str) Net -> str => Net.fetch(url)

    fn run_user_script() Fail -> () =>
        forbid Net, Fs {
            http_get("/api")  // ← compile error: requires effect Net,
                              //   forbidden by enclosing `forbid Net`.
        }

    @realtime
    fn checksum(data []byte) -> int {
        let mut sum = 0
        for b in data { sum += b as int }
        sum
    }

    realtime nogc {
        let xs = []int.new()  // ← compile error: cannot allocate
                              //   inside `realtime nogc`.
    }
    ```
- **Не покрывается:**
    + Транзитивные effect-tracking (callee → callee → effect) — пока
      pure name-based, без effect-row inference.
    + Closure-capture handler'ов через `with` — не отслеживается.
    + User-defined record-конструкторы как alloc-fn'ы (требует
      heap-alloc inference).
    + Runtime sentinel-frame для transitive effects (D63 mentions it
      как production runtime mechanism, отдельная задача).
- **План:** docs/plans/16-capability-enforcement.md ✅ ЗАКРЫТ
  (Ф.1-Ф.9). nova_tests **97/97 PASS** (92 baseline + 5 negative).

### [ЗАКР 2026-05-09] Plan 15 Ф.5: D53 strict-mode (split Protocol vs Effect)

- **Что:** AST теперь различает `protocol` и `effect`-keyword'ы через
  отдельные `TypeDeclKind` variants (раньше оба попадали в `Effect`).
  BoundCtx (D72) регистрирует только Protocol-kind; попытка использовать
  effect как bound — R5.3-style compile-error с hint'ом «`X` is an
  effect, declare as `protocol`». Codegen пропускает vtable-emission для
  Protocol → попутно фиксит pre-existing Self-bug.
- **Где:** `compiler-codegen/src/{ast/mod.rs, parser/mod.rs,
  codegen/emit_c.rs, types/mod.rs, lints.rs}` — ~70 строк.
- **Use-site эффект:**
    ```nova
    type Db effect { query(q str) -> []str }
    fn bad[T Db](x T) -> T => x
    // ← compile-error:
    //   type `Db` is an effect, not a protocol — generic bounds
    //     require protocol-types (D72/D53).
    //   Hint: declare `Db` as `type Db protocol { ... }`.

    type Hashable protocol { hash() -> u64; eq(other Self) -> bool }
    // ← Self в protocol-методе теперь работает (Self-bug fix bonus).
    ```
- **Не покрывается:** D53 §628 анонимные protocol-литералы в позиции
  типа (`fn close(c protocol { close() -> () })`) — требует нового
  `TypeRef::Protocol(...)` variant'а, отдельная задача.
- **План:** docs/plans/15-...md Ф.5 ✅.

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

### [ЗАКР 2026-05-09 codegen + 2026-05-10 interp Ф.6-bis] Plan 14 Ф.6: D69 variadic + spread

- **Что:** declaration `fn f(...items []T)` + call-site spread
  `f(...arr)` + mixed `f(a, ...arr, b)` работают **в обоих pipeline'ах**:
  C-codegen (production) и interp-mode (`nova-codegen test/run`).
  Покрывает D69 спецификацию полностью.
- **Где:**
    + `compiler-codegen/src/{ast/mod.rs, parser/mod.rs, codegen/emit_c.rs}`
      — ~500 строк (CallArg enum + variadic-routing в codegen).
    + `compiler-codegen/src/interp/{mod.rs, value.rs}` — ~170 строк
      (Closure.variadic_last + spread unfolding в eval_call +
      try_member_call_values).
- **Use-site эффект:**
    ```nova
    fn join(...parts []str) -> str { ... }
    join("a", "b", "c")          // → []str = ["a","b","c"]
    join(...arr)                  // → []str = arr
    join(...prefix, "tail")       // mixed
    ```
- **Verification history:** изначально (2026-05-09) считалось ✅
  закрытым по `run_tests.ps1` (codegen pipeline). 2026-05-10
  обнаружен gap: interp-mode даёт 7/7 FAIL для variadic.nv. Я
  проверял только один pipeline. Ф.6-bis закрыл interp.
- **Не покрывается:**
    + print/println — продолжают быть special-case (миграция на
      variadic — отдельная задача).
    + Multiple variadic-overloads — отвергаются (single-overload only).
- **Std refactor:** `std/path/path.nv` `Path.join(parts []str)` →
  `Path.join(...parts []str)` — теперь принимает variadic.
- **План:** docs/plans/14-stdlib-codegen-gaps.md Ф.6 ✅ + Ф.6-bis ✅.
- **Verification practice TODO:** добавить в CI / pre-commit hook'е
  параллельный прогон `nova-codegen test` для всех `nova_tests/**.nv`
  (interp baseline 43/92 — pre-existing bootstrap limits в
  concurrency/effects/runtime).

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

---

## 2026-05-10 — Closure-rev: `|x|` + `fn(...)` (Plan 19, spec-only)

### Что и почему

D22 заменена с `(params) =>` на двухуровневый closure:
- `|x| body` — closure-light (untyped, контекст определяет sig).
- `fn(x int) -> R body` — closure-full (типизированная, идентична
  named fn без имени).

Сразу убирается:
- Перегруз `=>` (был: тело named fn + лямбды + match-arm +
  handler-method + trailing-params; стал: тело named fn / closure-full
  + match-arm + handler-method).
- Unbounded look-ahead на `(` в expression-position (group vs lambda).
- Запрет блок-формы лямбды `(x) => { ... }` — closure-light теперь
  поддерживает block-форму нативно: `|x| { stmts; expr }`.
- Ambient-effect inference для closure-light: эффекты не пишутся,
  наследуются от parent fn + active `with`-blocks.
- Запрет анонимной `fn`-формы — теперь `fn(...)` без имени это
  closure-full, симметрично named fn.

### Trailing расщеплён

- `f(args) { block }` — без params (DSL).
- `f(args) fn(p) body` — с params (closure-full без имени).
- Старая `f(args) { x => body }` отменена.

### Captures упрощены

- Никаких `move`, `&mut`, lifetime'ов.
- Через managed-heap (D32) — captures работают автоматически, escape
  переезжает на heap прозрачно.
- Multiple closures на одну `let mut` переменную разделяют capture.

### Trade-offs

- Effect inference оставлен **только для closure-light** — named fn
  обязана объявлять эффекты в сигнатуре (R1 не ослабляется).
- Closure-full обязана иметь типы параметров — нет overlap с
  closure-light, граница чёткая.
- `||` для no-arg closure (Rust-style); парсер различает от binary
  OR по позиции (expression-start vs after-operand).

### Файлы

- spec/decisions/03-syntax.md — D22, D40, D43 переписаны.
- spec/syntax.md, spec/effects.md, spec/revolutionary.md — примеры.
- spec/decisions/{04,05,06,08}-*.md — точечные правки.
- spec/decisions/closure-rev2026-05-DRAFT.md — DRAFT-зеркало.
- docs/plans/19-closure-and-error-ops.md — план реализации (closure-rev + D85 error-ops в одном атомарном PR).

### Status

- Spec: ✅ ЗАКРЫТ.
- Implementation: 🟡 Plan 19 DRAFT (parser/interp/codegen TODO).

---

## 2026-05-10 — Sweep: эффекты-формулировки + D84 overloading + D85 ?/!! + exit

### Что упрощено

**1. Один оператор — одна семантика (D85).** Раньше D67: `?` имел
**две разные семантики** в зависимости от типа — для Result через
Fail (engaged эффект), для Option через ранний return (без эффекта).
Это «признанное напряжение» в D67 на деле было кривым обоснованием:
ничего философского, просто два разных оператора впихнули в один
символ ради краткости. **D85 разводит:** `?` всегда return-стиль
(для обоих Option/Result), `!!` всегда throw-стиль. Программист
выбирает стиль на месте использования.

**2. `effect` vs `protocol` через одно правило.** D62 правило 4
имел два sniff-вопроса (Q1 «resource-capability?» + Q2
«continuation-capture?») с длинными объяснениями через внутренние
термины. Свернули к **одному** проверяемому правилу: «хочется в
тесте подменить handler — это effect, нет — protocol». `Fail` под
него подходит без отдельного «особого случая» (catch-handler в
тесте — это и есть подмена).

**3. Overloading свободных функций (D84).** Раньше Plan 11 запрещал
дубликат имён для свободных функций, разрешая методы и `From[T]`.
Несимметрично без обоснования. **D84 снял запрет** — единый механизм
для receiver-методов, static-функций и свободных функций. Один файл
`spec/decisions/10-overloading.md` собирает все 4 оси перегрузки в
одно правило (раньше D35/D46/D73/Plan 11 — разрозненно).

**4. `panic` vs `exit` разведены.** Раньше формулировка «в CLI panic =
exit процесса» сливала уровни — один и тот же `panic("foo")` вёл
себя по-разному в разных средах. **D13 уточнён + новая `exit(code,
msg)`:** panic — fiber-уровень, exit — process-уровень. Программист
выбирает между ними.

### Trade-offs (что усложнилось)

- **Цена миграции stdlib (D85).** Все `parse(s)?` в `Fail[E] -> T`
  функциях перестают работать. Десятки-сотни мест, каждое требует
  смыслового решения (переход на `!!` или смена сигнатуры на
  `-> Result`). Окупается чистотой дизайна.
- **`!!` — новый оператор для запоминания.** Раньше был только `?`
  и `??`, теперь добавился `!!`. Но взамен: каждый оператор делает
  **одно**, без двух семантик одного символа. Чище для AI/LLM
  (предсказуемее) и для людей (видишь `!!` — сразу понимаешь
  «throw, серьёзно»).

### Файлы

**Новые:**
- `spec/decisions/10-overloading.md` (D84).

**Spec — крупные:** `spec/overview.md`, `spec/effects.md`,
`spec/syntax.md`, `spec/revolutionary.md`, `spec/decisions/04-effects.md`
(D67 отменён + D85 + D86 + D62 правило 4 свёрнуто), `spec/decisions/08-runtime.md`
(D26 prelude: `exit` + `RuntimeNoneError`; D13 panic vs exit).

**Spec — точечные:** `spec/decisions/{01,02,03,07}*.md` —
cross-refs D35→D84, D67→D85, формулировки.

**Plans:** `docs/plans/19-closure-rev.md` →
`19-closure-and-error-ops.md` — добавлены Ф.10 + Ф.8b + Ф.9
error-ops, риски и DoD дополнены.

**Implementation:** `compiler-codegen/src/interp/stdlib.rs` —
`exit` native function.

### Status

- Spec: ✅ ЗАКРЫТ.
- Tests: 97/97 PASS после всех правок.
- Implementation Plan 19 (включает D85 ?/!!): 🟡 DRAFT.
- C-codegen для `exit`: ✅ ЗАКРЫТО (см. секцию ниже от 2026-05-10).
- C-codegen overloading свободных функций: 🟡 TODO (D84 Bootstrap-status: ⚠️).

---

## 2026-05-10 (продолжение) — C-codegen для panic + exit (production-grade)

### Что упрощено / закрыто

**1. panic + exit в C-codegen.** Закрывает следствие D13 «два уровня
катастрофы»: обе функции prelude'а теперь работают и в interp, и в
C-codegen. До этого `panic` был только в interp (никем в .nv не
использовался — баг не проявлялся), `exit` был добавлен только в
interp вчерашним коммитом C2.

**2. Comma-expression паттерн для Never.** Существующий ExprKind::Throw
использует statement+dummy через self.line() — это работает в
branching contexts (if/match) но **ломает short-circuit** в тернарных
(?? coalesce). Для panic/exit взял другой паттерн —
`(nv_panic(msg), (nova_int)0LL)` через C comma-operator. Это
**inline expression** без нарушения семантики родительских конструкций.
Throw тоже стоит мигрировать — Q-throw-comma на будущее.

### Trade-offs

- **type-checker про panic/exit не знает** — обрабатывается в codegen
  special-case (как и assert). Это существующее упрощение bootstrap-
  types, не относится к этой задаче. Закрыть когда понадобится full
  type-check свободных функций.
- **exit без cleanup** — гасит процесс как C exit() / Go os.Exit без
  defer'ов / destructor'ов / handler'ов. На v1.0+ можно добавить
  atexit-style hook для критических cleanup'ов (закрыть файлы, flush
  логов).

### Файлы

- `compiler-codegen/nova_rt/effects.h` — `nv_panic` + `nv_exit`
  функции с production-grade routing.
- `compiler-codegen/src/codegen/emit_c.rs` — special-case для
  `panic(msg)` и `exit(code, msg)` через comma-expression.
- `nova_tests/runtime/panic_exit.nv` — 10 тестов (компиляция,
  if-else, ?? panic/exit, разные exit-code'ы, пустые msg'и).

### Status

- C-codegen: ✅ ЗАКРЫТ.
- Tests: ✅ 98/98 PASS (включая новый panic_exit).
- ~Open~ Q-throw-comma — ✅ ЗАКРЫТО (см. секцию ниже).

---

## 2026-05-10 (продолжение 2) — Q-throw-comma: Throw на comma-expression

### Что упрощено / закрыто

**Системность codegen-паттерна для Never-функций.** Все три Never-вызова
(panic, exit, throw) теперь эмитируются через **один** паттерн —
comma-expression `(call(args), (nova_int)0LL)`. До этого Throw
использовал отличающийся statement+dummy паттерн через self.line(),
что создавало латентный баг с short-circuit в `?? throw` (никем не
использовалось в .nv → не проявлялось).

### Trade-offs

- **Никаких** — comma-expression строго лучше для Never в expression-
  position. Statement+dummy остался только в случаях, где нужен
  именно statement-level эффект (например, NovaInterrupt — эмиттит
  `nova_interrupt(...)` как statement; там это правильно потому что
  родитель — block-level).

### Файлы

- `compiler-codegen/src/codegen/emit_c.rs` — ExprKind::Throw мигрирован.
- `nova_tests/syntax/throw_in_expression.nv` — SECTION 3 (3 теста на
  short-circuit ?? throw).

### Status

- ✅ ЗАКРЫТ.
- Tests: 98/98 PASS, никаких регрессий.

---

## 2026-05-10 (продолжение 3) — D84 overloading свободных функций (оси 1, 2, 4)

### Что упрощено / закрыто

**1. Единый overload mechanism для методов и free-functions.**
До этого Plan 11 покрывал только методы (с receiver'ом), а
free-functions имели жёсткий запрет на duplicate name. После D84 +
этой реализации — оба пути используют один и тот же registry
(`method_overloads` с sentinel-key `("", name)` для free-fn).

**2. Type-checker ModuleEnv.fns переведён на Vec<FnDecl>.** Раньше
single FnDecl (last-wins при duplicate). Теперь корректно хранит
все overloads. BoundCtx и CapabilityCtx тоже обновлены под Vec.

### Trade-offs (известные ограничения)

- **Q-overload-result-type (D84 ось 3) — НЕ реализовано в codegen.**
  Type-checker регистрирует overloads с разным return-type, но при
  call-site resolve без expected-type propagation возникает
  ambiguity. Реализация требует переделки emit_expr на context-driven
  type-resolve. Отложено отдельной задачей.
- **Q-overload-generic-vs-concrete (D84 правило «concrete > generic»)** —
  type-checker не различает generic и concrete signatures, считает
  одинаковые arg-shapes как duplicate. Тоже отдельная задача.

### Файлы

- `compiler-codegen/src/types/mod.rs` — ModuleEnv.fns Vec, BoundCtx
  и CapabilityCtx обновлены, typeref_equal helper.
- `compiler-codegen/src/codegen/emit_c.rs` — pass 1c регистрация
  free-functions, mangle_fn для free-fn, call-site overload-resolve.
- `nova_tests/syntax/overload_free_fn.nv` — 8 тестов на оси 1, 2, 4.
- `spec/decisions/10-overloading.md` — Bootstrap-status updated:
  free-functions ✅, добавлена пометка ⚠️ для result-type.

### Status

- D84 оси 1, 2, 4 для free-functions: ✅ ЗАКРЫТ.
- Tests: ✅ 99/99 PASS (98 предыдущих + новый overload_free_fn).
- D84 ось 3 (result-type): 🟡 Q-overload-result-type — отдельная задача.
- D84 generic vs concrete: 🟡 Q-overload-generic-vs-concrete — отдельная задача.

---

## 2026-05-10 — Plan 19 ЗАКРЫТ (closure-rev + error-ops + handler-rev)

### Что упростилось в языке

**До Plan 19:**
- Лямбды через `(params) => expr` — strictly one-expression body.
- Block-форма для closure запрещена → выносить в named fn ради
  `let y = ...`.
- Trailing-block с params через `{ x => body }` — отдельная
  грамматика (D43-old), параметры через `=>`.
- `?` имел двойную семантику (D67): для Result через Fail-эффект,
  для Option через ранний return.
- `Handler[E]` без второго параметра — нельзя выразить interrupt
  семантику в типе.

**После Plan 19:**
- Двухуровневый closure: `|x| body` (light, untyped) + `fn(x T) -> R body`
  (full, typed). Block-form поддерживается natively.
- `f(args) { block }` — DSL-trailing БЕЗ params; `f(args) fn(p) body` —
  trailing-fn С params.
- `?` — унифицированный early-return для Option/Result (D85).
- `!!` — throw-стиль через Fail (D85).
- `??` — coalesce (D86, выделен из D4).
- `Handler[E, IRT]` — interrupt return type явно в сигнатуре (D87).
- Default generic params: `[T = int]` (D88).
- handler-лямбда `with E = |err| body` (D31-rev).

### 14 коммитов C1-C14, baseline 65/65 lib + 102/102 nova_tests

Detailed retro в docs/plans/19-closure-and-error-ops.md.

### Отложено

- ~~C16: mut-capture codegen.~~ → **ЗАКРЫТО** (2026-05-11, см. ниже)
- ~~C6: bidirectional inference HOF arg → closure.~~ → **ЗАКРЫТО** (2026-05-11, см. ниже)
- Codegen handler-лямбда (D31-rev codegen-side).

---

## 2026-05-10 (продолжение 5) — D84 negative-тесты

### Что закрыто

D84 codegen бросает три типа compile error'ов («duplicate signature», «no matching overload», «ambiguous overload»), но **ни один не тестировался**. Аудит тестов вскрыл этот gap; заведены три negative-теста через `EXPECT_COMPILE_ERROR` marker:

- `nova_tests/negative_capability/overload_duplicate_signature.nv` — две функции с identical sig.
- `nova_tests/negative_capability/overload_no_match.nv` — вызов `handle(true)` где есть `handle(int)` и `handle(str)`.
- `nova_tests/negative_capability/overload_ambiguous.nv` — две overloads с одинаковыми arg-types и разным return-type (фиксирует Q-overload-result-type ⚠️).

### Status

- ✅ ЗАКРЫТО.
- 3/3 negative-тестов PASS.
- Когда Q-overload-result-type будет закрыт — `overload_ambiguous.nv` нужно переделать в positive (let-аннотация выбирает overload).

---

## 2026-05-10 (продолжение 6) — D89 EXPECT_* маркеры (Вариант C)

### Что упрощено / закрыто

**1. Унификация test-tooling-конвенций.** До D89 был только один маркер (`EXPECT_COMPILE_ERROR`, Plan 16 Ф.7), документирован только в комментарии скрипта. Любой alternative test-runner мог бы изобрести свой механизм → fragmentation. D89 фиксирует **4 стандартных маркера** (compile error, runtime panic, exit code, stdout) как часть Nova-conformant tooling'а.

**2. Cleanup путей в run_tests.ps1.** Раньше пути захардкожены `d:\Sources\nova-lang\...` — скрипт не работал на чужих машинах и в CI. Теперь все пути относительно `$PSScriptRoot` с env-var override; vcvars64.bat ищется через vswhere. Можно clone'нуть репо в любой каталог и запускать.

### Trade-offs

- **Comment-маркер vs first-class директива.** Выбрали comment-маркер (как Rust/Swift/Go), не атрибут языка (как TypeScript `@ts-expect-error`). Trade-off: парсер про маркер не знает (опечатка `EXPECT_COMPILE_EROR` без R пройдёт незаметно). Mitigation — linter может предупреждать о похожих на маркер опечатках.
- **Один маркер на файл.** Не поддерживается multi-marker. Force'ит разделение тестов по сценариям — лучше для читаемости и точности диагностики.
- **Pattern — substring, не regex.** Проще писать, но менее точно.

### Файлы

- `spec/decisions/09-tooling.md` — D89 нормативный D-блок.
- `run_tests.ps1` — реализация всех 4 маркеров + cleanup путей.
- `docs/test-conventions.md` — практический guide для авторов тестов.
- `nova_tests/expected_runtime/` — 3 pilot-теста (по одному на новый маркер).

### Status

- ✅ ЗАКРЫТ.
- Tests: ✅ 111/111 PASS (108 предыдущих + 3 новых pilot).
- Open: `nova test` CLI на Nova — будет реализовать D89 одинаково; gap до тех пор — `run_tests.ps1` единственная реализация.

---

## 2026-05-10 (продолжение 7) — D89 EXPECT_STDERR + split stdout/stderr

### Что упрощено / закрыто

**Естественный gap в D89.** Вчера зафиксировали 4 маркера, `EXPECT_STDERR` оставили как «будущее расширение». Через день — добавили как 5-й, потому что:
- POSIX-конвенция различает stdout/stderr, тесты должны тоже.
- Симметрия с `EXPECT_STDOUT` естественная.
- Раньше `EXPECT_STDOUT` де-факто проверял combined-вывод (через `2>&1`), теперь точно — только stdout.
- 30 минут работы, закрывает gap пока контекст свежий.

### Trade-offs

- **Breaking change для `EXPECT_STDOUT`:** ранее combined → теперь только stdout. Один существующий тест (`stdout_hello.nv` — `println` пишет в stdout) не сломался. Other test-runners (если появятся) должны учесть.
- **`EXPECT_RUNTIME_PANIC` остался combined.** Logically panic пишет в stderr, но runner проверяет любой поток для устойчивости — это spec-важно (mitigates future runtime changes которые могут перенаправить panic).

### Файлы

- `spec/decisions/09-tooling.md` — D89 расширен до 5 маркеров.
- `run_tests.ps1` — split stdout/stderr, EXPECT_STDERR ветка.
- `docs/test-conventions.md` — секция «5. EXPECT_STDERR» + уточнение EXPECT_STDOUT.
- `nova_tests/expected_runtime/stderr_panic.nv` — pilot-тест.

### Status

- ✅ ЗАКРЫТ.
- Tests: ✅ 120/120 PASS (предыдущие + новый stderr_panic).

---

## 2026-05-11 — codegen: const u32/u64 type-inference + native-typed integer literals (FNV bug)

### Что починено

В `std/checksums/fnv.nv` user заметил неожиданный C-вывод:
```c
nova_int h = FNV1A_32_OFFSET;  // FNV1A_32_OFFSET — u32 const
```
— переменная инициализируется из u32-const'а, но получает тип `nova_int` (int64). И сами const'ы инициализированы как `((nova_int)NLL)` — signed-cast в unsigned, что **implementation-defined** для значений вне диапазона int64 (например FNV-64 offset `0xCBF29CE484222325` представляется как отрицательный `-3750763034362895579LL`).

Два связанных бага в codegen:

**Баг 1 — use-site inference let'а из const'а.** `infer_expr_c_type(Ident(name))` смотрел только в `var_types`, но обычные (non-lazy) const'ы туда не регистрировались (только lazy через `emit_lazy_const`). Fallback — `"nova_int"`. Fix: после успешного `emit_const_decl` регистрируем `c.name → ty_c` в `var_types`, чтобы Ident на use-site инферился с правильным c-типом.

**Баг 2 — integer literals в const-init без учёта target-типа.** `emit_const_expr` всегда эмитил `((nova_int)NLL)` независимо от типа const'а. Для unsigned-целевых типов это implementation-defined conversion. Fix: новый `emit_const_expr_typed(expr, target_ty_c)` + helper `emit_typed_int_literal(n, ty_c)` — эмитят правильный suffix/cast по c-типу:
- `uint32_t` → `((uint32_t)NU)` (через `n as u32`)
- `uint64_t` → `((uint64_t)0xNULL)` (bit-pattern u64, чтобы корректно представить значения вне i64)
- `int32_t/int16_t/int8_t` → `((int32_t)N)` 
- по умолчанию (`nova_int`, `int64_t`) — `((<ty>)NLL)` (старое поведение, не сломано)

`emit_const_decl` передаёт `ty_c` в `emit_const_expr_typed` как ожидаемый тип.

### До/после

```c
// До:
static const uint32_t FNV1A_32_OFFSET = ((nova_int)2166136261LL);
static const uint64_t FNV1A_64_OFFSET = ((nova_int)-3750763034362895579LL);
static uint32_t Nova_Fnv_static_hash32(...) {
    nova_int h = FNV1A_32_OFFSET;  // ← неправильный тип
    ...
}

// После:
static const uint32_t FNV1A_32_OFFSET = ((uint32_t)2166136261U);
static const uint64_t FNV1A_64_OFFSET = ((uint64_t)0xCBF29CE484222325ULL);
static uint32_t Nova_Fnv_static_hash32(...) {
    uint32_t h = FNV1A_32_OFFSET;  // ← правильный тип
    ...
}
```

### Trade-offs

- **Локальный fix:** только `emit_const_expr_typed` для const-init. `emit_expr` (runtime expressions) не трогали — там `((nova_int)NLL)` остаётся как есть. Это OK: in-function integer literals неявно конвертятся в target-тип через C-implicit conversions; проблема была только в file-scope const-init где cast → unsigned давал implementation-defined для overflow-значений.
- **Lazy-const'ы** уже регистрировались в `var_types` (через `emit_lazy_const`), это значит баг был только в non-lazy ветке. Fix симметризовал поведение.
- **Char-литералы:** `emit_typed_int_literal(cp as i64, ty_c)` — codepoint конвертится через i64, для u8/u16/u32 это OK (codepoint ≤ 0x10FFFF помещается в u32).

### Файлы

- `compiler-codegen/src/codegen/emit_c.rs` — `emit_typed_int_literal`, `emit_const_expr_typed`, регистрация const'ов в `var_types`.
- `std/checksums/fnv.c` — regenerated с правильными типами (артефакт верификации, не commited).

### Status

- ✅ ЗАКРЫТ.
- Tests: ✅ lib 65/65, nova_tests 120/120, std/checksums 2/2 PASS.
- Lesson: «code generator с типизированным IR — но IntLit без target-типа в emit'е». Любой code-gen, который compile'ит typed source → нетипизированный target language (C), должен везде, где есть target-type-info, эмитить native-typed литералы. Особенно для unsigned/signed мостов. Если IR теряет тип к моменту emit'а (как было) — баг неизбежен на edge-cases (overflow, large hex constants, u64 ≥ 2^63).

---

## 2026-05-11 (продолжение) — codegen: typed-integer promotion в Binary infer

### Что починено

В `std/checksums/crc32.nv` функция:
```nova
fn table_value(i u8) -> u32 {
    let mut c = i as u32
    for k in 0..8 {
        c = if c & 1 == 1 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 }
    }
    c
}
```
эмитила:
```c
nova_int _nv_if_1;  // ← должно быть uint32_t
if (...) {
    _nv_if_1 = (nova_int)((((nova_int)3988292384LL) ^ (c >> ((nova_int)1LL))));
}
```

Объяснение: `emit_if_expr` определяет тип tmp-переменной через `infer_expr_c_type(then.trailing)`. Trailing — это `0xEDB88320 ^ (c >> 1)` — `Binary{BitXor, IntLit, ...}`. Logic в `Binary` brunch'е был: вернуть `lt` (left type). А `lt = infer(IntLit) = "nova_int"`. → tmp тоже `nova_int`.

### Fix

В `infer_expr_c_type` для `Binary` integer-операций добавлена **typed-integer promotion**: если один из операндов — typed integer (`uint8/16/32/64_t`, `int8/16/32_t`), а другой — `nova_int` (т.е. дефолтный IntLit), результат — **typed integer**, не nova_int. Это правило симметрично для left/right.

Helper `is_typed_integer(ty) -> bool` для предиката (включает все signed/unsigned кроме `nova_int`/`int64_t`, т.к. это и есть дефолт).

После fix:
```c
uint32_t _nv_if_1;  // ← правильно
if (...) {
    _nv_if_1 = (uint32_t)((((nova_int)3988292384LL) ^ (c >> ((nova_int)1LL))));
}
```
Литералы внутри XOR/shift остаются `((nova_int)NLL)` (это safe — implicit C-conversion при assignment к u32). Главное — outer tmp типизирован правильно.

### Trade-offs

- **Promotion правило простое (1 typed + 1 nova_int → typed).** Не покрывает все edge-cases full C-promotion. Если оба операнда typed разной ширины (например `u32 & u8`) — берём `lt` (левый), полагаясь на implicit C narrow/widen. В реальном коде такие случаи редки.
- **Литералы внутри Binary остаются nova_int.** Можно было thread'ить target-type рекурсивно (как в `emit_const_expr_typed` из предыдущего фикса) и эмитить `((uint32_t)NU)` сразу. Но это много больше работы и риска регрессий — текущий подход «typed outer + literal cast'ится на assign» работает и проще.
- **`int64_t`/`nova_int` сознательно не считаются "typed":** их роль — быть дефолтным IntLit'ом, должны "уступать" более конкретным типам.

### Файлы

- `compiler-codegen/src/codegen/emit_c.rs` — `is_typed_integer` helper, promotion правило в `Binary` ветке `infer_expr_c_type`.

### Status

- ✅ ЗАКРЫТ.
- Tests: ✅ lib 65/65, nova_tests 120/120, std/checksums 2/2 PASS.
- Lesson — третий codegen-баг этой серии в `std/checksums/*.nv`: stdlib **показателен**. Stdlib пишется на real Nova, использует все edge-cases (typed const'ы, u32-арифметика, hex-литералы). Каждый «странный фрагмент сгенерированного C» в stdlib — реальный баг кодогена. Stdlib де-факто работает как fuzzer.

---

## 2026-05-10 (продолжение 8) — D90 (defer/errdefer) + D91 (Channel revision)

### Что зафиксировано

**D90 — defer/errdefer:**

Zig-style scope-level cleanup statements. Закрывает Q20 «Нужен ли defer?» Мотивирован отсутствием RAII в Nova (D6 managed heap, нет destructor'ов): без `defer` resource cleanup пишется через handler-блоки, что многословно (10+ строк boilerplate на transactions).

**D91 — Channel revision:**

Уточняет D79: API меняется с Go-style (один Channel объект) на Rust mpsc-style (`Channel.new(cap) -> (Sender, Receiver)`). Capability-split: producer не может recv, consumer не может send. Close — explicit через `defer tx.close()` (D90), не auto-on-drop.

### Trade-offs

- **D90 body infallible.** Если cleanup может упасть — программист обязан handle явно через handler-блок. Double-throw невозможно сделать корректно.
- **D90 no-suspend.** Cleanup быстрый — иначе exit-семантика scope'а непредсказуема.
- **D91 explicit close.** В Nova нет destructor'ов; auto-on-drop через GC flaky. Программист обязан `tx.close()` (идиома — `defer`). Отличие от Rust mpsc.
- **D91 sender.clone() не нужен.** Managed heap shared by default.

### Файлы

- `spec/decisions/03-syntax.md` — D90.
- `spec/decisions/06-concurrency.md` — D91, D79 помечен «частично уточнено D91».
- `spec/open-questions.md` — Q20 закрыто → D90.

### Status

- D90: ✅ spec. 🟡 Implementation — Plan 21+.
- D91: ✅ spec. 🟡 Implementation — Plan 22+. Breaking change для nova_rt/channels.h, миграция тестов.

### Открытые задачи

- Plan 21 — defer/errdefer implementation.
- Plan 22 — Channel revision implementation.
- Sequential: Plan 21 → Plan 22 (D91 использует defer).

---

## 2026-05-11 (продолжение) — codegen: typed-integer promotion в Binary infer

### Что починено

В `std/checksums/crc32.nv` функция:
```nova
fn table_value(i u8) -> u32 {
    let mut c = i as u32
    for k in 0..8 {
        c = if c & 1 == 1 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 }
    }
    c
}
```
эмитила:
```c
nova_int _nv_if_1;  // ← должно быть uint32_t
```

Причина: `emit_if_expr` берёт тип tmp'а из `infer_expr_c_type(then.trailing)`. Trailing — `0xEDB88320 ^ (c >> 1)` — Binary. Logic в Binary-branch'е был «вернуть lt» (левый тип). `lt = infer(IntLit) = nova_int`. → tmp = `nova_int`.

### Fix

Promotion-rule в Binary-integer branch'е `infer_expr_c_type`: если один operand — typed integer (u8/u16/u32/u64/i8/i16/i32), а другой — `nova_int` (дефолт IntLit), результат — typed-integer. Симметрично для left/right.

Helper `is_typed_integer(ty) -> bool` — predicate (`nova_int`/`int64_t` сознательно НЕ включены — они дефолт IntLit'а, должны "уступать").

После fix:
```c
uint32_t _nv_if_1;  // ← правильно
if (...) {
    _nv_if_1 = (uint32_t)((((nova_int)3988292384LL) ^ (c >> ((nova_int)1LL))));
}
```
Внутренние литералы оставлены `((nova_int)NLL)` — implicit C-conversion на assignment безопасен. Главное — outer tmp типизирован.

### Trade-offs

- **Простое правило (1 typed + 1 nova_int → typed)**, не полный C-promotion. `u32 & u8` (оба typed) → берём lt (левый). Safety не нарушает.
- **Не трогали `emit_expr`** — литералы внутри Binary остаются nova_int. Можно было thread'ить target-type рекурсивно как в const-fix, но это больший рефакторинг и риск регрессий. Текущий подход проще.
- **`int64_t`/`nova_int` сознательно "не-typed":** их роль — уступать конкретным типам.

### Файлы

- `compiler-codegen/src/codegen/emit_c.rs` — promotion rule в Binary-branch'е, `is_typed_integer` helper.

### Status

- ✅ ЗАКРЫТ.
- Tests: ✅ lib 65/65, nova_tests 120/120, std/checksums 2/2 PASS.
- Lesson — **stdlib работает как fuzzer codegen'а**. Три codegen-бага этой сессии — все в `std/checksums/*.nv`. Stdlib пишется на real Nova, использует все edge-cases (typed const'ы, hex-литералы, u32-арифметика, bitwise). Каждый «странный фрагмент C» в stdlib — реальный баг кодогена. Тесты могут PASS через C-implicit conversions, но это маскирует bugs. Регулярно читать generated stdlib C — не только assert'ы тестов.

---

## 2026-05-10 (продолжение 9) — Plan 20 + Plan 21 (DRAFT планы реализации)

### Что зафиксировано

Implementation roadmaps для D90/D91 заведены как DRAFT-планы.

**Plan 20** — D90 (defer/errdefer): 7 фаз lexer/parser/type-check/codegen/interp/тесты/spec. Атомарный PR Ф.1-Ф.6. Объём ~1500 строк.

**Plan 21** — D91 (Channel revision): 7 фаз nova_rt/codegen/type-check/select/migration/negative/spec. Зависит от Plan 20.

### Trade-offs

- **DRAFT планы вместо немедленной реализации.** Spec крупных design-изменений требует реализации в несколько сессий (Plan 19 был ~14 коммитов). Trade-off: spec и codebase расходятся пока impl не сделан. Mitigation: Bootstrap-status 🟡 в D-блоках сигнализирует «spec ahead of code».

### Файлы

- `docs/plans/20-defer-implementation.md`.
- `docs/plans/21-channel-revision-implementation.md`.
- `docs/plans/README.md` — обновлён индекс.

### Status

- Plan 20: 🟡 DRAFT.
- Plan 21: 🟡 DRAFT (зависит от Plan 20).

---

## 2026-05-11 — Plan 15 Ф.4 retro (closing negative-test gap)

### Что зафиксировано

Plan 15 (generic bounds, D72) фазы Ф.1-Ф.3, Ф.5 уже реализованы; Ф.4 тесты покрывали только позитивные кейсы (5 файлов в `nova_tests/types/generic_bounds.nv`). Добавлены недостающие:

- **3 negative-теста** через D89 `EXPECT_COMPILE_ERROR` маркер:
  - `bound_not_satisfied_rejected.nv` — тип без required-методов протокола → «type X does not satisfy P bound».
  - `bound_missing_method_rejected.nv` — метод с правильным именем, но другой арностью → «does not satisfy» (BoundCtx матчит по name + arity).
  - `bound_effect_not_protocol_rejected.nv` — effect-kind тип как bound → «is an effect, not a protocol» (D53 strict Ф.5).
- **1 позитивный тест** — forward-dependency `[K Hashable, V From[K]]`. Парсер принимает (через `parse_type` который допускает type-args), чекер permissive для параметризованных bound'ов (early-return для non-single-name path).

### Trade-offs

- **Anonymous protocol bound** `[T protocol { ... }]` не добавлен — `parse_type` не принимает keyword `protocol` в позиции типа. Откладывается до отдельной задачи (D53 §628 inline protocol-литералы).
- **Wrong-return-type** не тестируется как отдельный кейс — текущий BoundCtx match'ит по name + arity, return type игнорируется. Полная sig-сверка с `Self → T` substitution — будущая фаза. Negative `bound_missing_method_rejected` фиксирует arity-mismatch, что покрывает большую часть случаев.

### Файлы

- `nova_tests/negative_capability/bound_not_satisfied_rejected.nv` (новый).
- `nova_tests/negative_capability/bound_missing_method_rejected.nv` (новый).
- `nova_tests/negative_capability/bound_effect_not_protocol_rejected.nv` (новый).
- `nova_tests/types/generic_bounds.nv` — добавлен forward-dependency тест (5 → 6 тестов).
- `docs/plans/15-generic-bounds-enforcement.md` — Ф.4 retro, статус ✅ ЗАКРЫТ.

### Status

- ✅ ЗАКРЫТ. Plan 15 целиком закрыт.
- Tests: 123/123 PASS (включая 3 новых negative + 1 новый positive).

### Lesson

**D89 `EXPECT_COMPILE_ERROR`** — proven tool для negative-coverage без custom-харнеса. Pattern — substring через `regex.escape`. Workflow: запустил codegen на тестовом .nv, посмотрел текст ошибки, выбрал uniquely-identifying substring («does not satisfy», «is an effect, not a protocol»). Все 3 теста matchнулись с первой попытки.

**Plan-spec-vs-bootstrap reality.** План Ф.4 предполагал «anonymous protocol bound (если spec позволяет inline)» — но bootstrap-парсер не поддерживает. Правильное решение — документировать gap явно («not supported in bootstrap, see D53 §628»), не пытаться расширить парсер ad-hoc. Аналогично forward-dep: тест есть, но чекер permissive — это документировано в комментариях и retro.

---

## 2026-05-10 (продолжение 10) — Plan 20 Ф.1 (lexer keyword reservation)

### Что закрыто

Первая фаза Plan 20 (D90 implementation): `defer` и `errdefer` зарезервированы как keyword-токены в lexer'е. Полностью изолированная фаза — токены добавлены, но грамматикой не используются.

### Trade-offs

Закоммитил **отдельно** от планируемого атомарного PR Ф.1-Ф.6 — Ф.1 обратно совместима (не ломает существующий код). Negative-тесты подтверждают что reservation работает. Incremental progress без риска отката.

### Файлы

- `compiler-codegen/src/lexer/token.rs` — TokenKind::KwDefer/KwErrDefer.
- `compiler-codegen/src/lexer/mod.rs` — keyword recognizer.
- `spec/decisions/03-syntax.md` D83 — keyword список расширен «Cleanup: defer, errdefer».
- `nova_tests/negative_capability/defer_keyword_reserved.nv` — negative.
- `nova_tests/negative_capability/errdefer_keyword_reserved.nv` — negative.

### Status

- Plan 20 Ф.1: ✅ ЗАКРЫТО (commit 75673d7).
- Plan 20 Ф.2-Ф.6: 🟡 не начато (атомарный PR в будущей сессии).
- Tests: ✅ 122/122 PASS.

---

## 2026-05-11: name-resolution фаза в типчекере (NameResCtx)

### Что упрощено

NameResCtx ловит undefined идентификаторы в expr-position, но
**пропускает Capitalized-имена**. Точечно НЕ проверяются:

1. **Cross-file types/variants (Capitalized).** `HashMap[K,V].new()`
   в std/collections/lru.nv использует `HashMap` без import — типов
   нет в текущем модуле. Эвристика: имя начинается с заглавной → known.
2. **TaggedTemplate tags** (sql / json / html). Special-form syntax.
3. **Member access name** (`obj.method`, `obj.field`) — резолв через
   method_table / record_schemas в codegen.
4. **Path-сегменты** (`module::name`) — first segment не валидируется.
5. **Generic-params в TypeRef** — type-position, не expr.

### Почему

- **Bootstrap не имеет cross-file name resolution.** Имена из других
  .nv файлов попадают сюда не задекларированными. Полноценный
  import-graph + module-loader — большая инфраструктура; для
  bootstrap'а заменена convention'ом «Capitalized = type/variant».
- **Method-resolution требует type-inference.** `obj.method` — тип
  obj может быть generic-param, чужой type, или primitive с
  встроенным методом. Не делаем в name-resolution фазе.

### Trade-offs

- ✅ Ловятся **snake_case опечатки** (`undefined_var`, `fixed_ms`,
  `seeded`) — самый частый класс ошибок в expr-position.
- ❌ Опечатки в **Capitalized** именах (`HashMpa` вместо `HashMap`)
  НЕ ловятся. Это компилятор подсветит на cc-этапе через
  «undeclared type» — все ещё неудобно, но менее частый случай.
- ❌ Method-typos (`xs.lenghth` вместо `xs.length`) НЕ ловятся.
  Это **отложено** до полноценного type-inference / method-table-aware
  фазы (требует bidirectional inference).

### Когда закрывать

Полноценное cross-file name resolution планируется в self-hosted
compiler'е (после Plan 22+, когда появятся stable module-loader +
type-inference). До этого — bootstrap convention достаточен.

### Файлы

- `compiler-codegen/src/types/mod.rs` — `NameResCtx` (lines ~1255–1670):
  build/check_module/walk_fn/walk_block/walk_stmt/walk_expr/
  walk_trailing/collect_pattern_bindings/is_known.

### Status

- ✅ ЗАКРЫТ (как bootstrap-фаза).
- Tests: ✅ cargo test --lib 65/65, nova_tests 121/121 PASS (120 baseline + 1 negative).
- Roadmap: расширение до Capitalized-проверки — после self-host
  compiler, не в bootstrap.

---

## 2026-05-11 — Plan 09 Ф.1-Ф.4: Clang toolchain в run_tests.ps1

### Что упрощено / закрыто

Реализован Plan 09 Ф.1-Ф.4 (`docs/plans/09-clang-migration.md`):
LLVM 22.1.5 поставлен через `winget install LLVM.LLVM`, `run_tests.ps1`
получил параметры `-Toolchain auto|clang|msvc` и `-Mode dev|release`.
По умолчанию — Clang если найден, иначе MSVC с warning'ом.

### Изменения относительно плана

- **march = x86-64-v3 (вместо `native`).** Изначальный план предлагал `march=native`. Сменил: `native` не переносится между CPU, для distributable binary нужен **portable march**. v3 = Haswell+ (2013+), покрывает ≈99% десктопов 2026. `native` доступен через env `NOVA_MARCH_NATIVE=1` для локальных перф-эксперименов.
- **Ф.6 (бенчмарки) отложен.** Не делаем сейчас: bench/json_parse требует std/encoding/json (неполная), sha256 требует tight I/O (libuv Plan 22). Делаем когда есть готовый realistic workload + конкретный perf-claim. В фокусе features (Plan 20/21/22), не perf.
- **Ф.5 docs (README) — частично:** docs/plans/09 retro обновлён, README.md / compiler-codegen/README.md — отдельная doc-задача.

### Trade-offs

- **MSVC fallback оставлен.** Не enforce'им Clang — кто-то может работать без LLVM (особенно на CI/cloud-VMs без install прав). Warning сообщает что perf на 10-15% хуже.
- **`-Wno-everything` для Clang.** Codegen эмитит много типичных warnings (unused-result, sign-compare, parenthesis) которые не баги; муссировать их в test-runner'е не нужно. На самом деле hide'им сигнал — отдельная задача почистить codegen warning-free.
- **Не удалили MSVC-workarounds в codegen.** План явно сохраняет 8+ обходок в emit_c.rs (compound literals, ≥1 struct field, etc.) — Clang это принимает, удаление потребовало бы условного codegen'а. Stays as-is.

### Plan 09 как fuzzer (сюрприз)

Полный прогон тестов на Clang выявил **2 реальных codegen-бага**:

1. **block-scope `static` fwd-decl** (`basics/trailing_block`): codegen эмитил `static foo(void);` внутри тела функции — нарушение C99 §6.2.2¶7. MSVC принимает (extension), Clang/GCC отвергают. Fix: fwd-декларация в `lambda_forward_decls` (file-scope buffer).

2. **SRC annotation gap в with-body**: `emit_with` не вызывал `emit_source_annotation_for_expr(trailing)` — SRC-комменты терялись для последнего expression в with-body. Fix: добавлена аннотация.

### Lesson

**Strict-compiler как detective tool.** Каждое отличие в обработке нестандартного C выявляет latent codegen-bug. Сильный аргумент за CI-прогон на нескольких toolchain'ах (MSVC + Clang + позже GCC на Linux) — не для perf, а для **portability/correctness**. Если бы переход на Clang случился через год (на Linux CI), эти баги всплыли бы в более болезненной форме.

### Файлы

- `run_tests.ps1` — параметры -Toolchain/-Mode, детект Clang, fallback на MSVC.
- `compiler-codegen/src/codegen/emit_c.rs` — fix #1 (fwd-decl file-scope), fix #2 (SRC annotation в emit_with).
- `docs/plans/09-clang-migration.md` — retro Plan 09 + Ф.6 отложен.

### Status

- ✅ Ф.1-Ф.4 ЗАКРЫТЫ.
- ⏸️ Ф.5 (README docs) — частично, отдельная задача.
- ⏸️ Ф.6 (benchmarks) — отложен до std/json + libuv готовности.
- Tests: Clang dev 130/130 PASS, MSVC dev (regression) 130/130 PASS.

---

## 2026-05-11 (продолжение) — Plan 20 Ф.2-Ф.7: defer/errdefer полная реализация

### Что закрыто

Plan 20 (D90 defer/errdefer) закрыт полностью — 7 фаз, 9 коммитов:
- Ф.1 lexer (75673d7, ранее) — `defer`/`errdefer` keyword-токены.
- Ф.2 parser+AST (380b457, ранее) — `Stmt::Defer { body }`, `Stmt::ErrDefer { body }`.
- Ф.3 type-check (fdb53be + 3faf9f0) — body constraints + **revision Вариант 3** (local control разрешён).
- Ф.4 codegen (94151c3 + b058968) — per-scope DeferScope, NovaFailFrame throw-path, early-exit cleanup.
- Ф.5 interp (c96f7f3, ранее) — per-scope defer-stack, LIFO, errdefer skip non-error.
- Ф.6 positive-тесты (c57d098 + 4cd8abe + 24196f2) — `syntax/defer_basic.nv` (4 кейса) + `syntax/errdefer_basic.nv` (3 кейса).
- Ф.7 spec uplift (4fcb6b8) — D90 Bootstrap-status 🟡 → ✅.

### Изменения относительно плана

- **Ф.3 переписан как Вариант 3 (local control).** Изначальный план запрещал return/break/continue в defer body везде (Zig-style strict). По user-feedback в ходе работы — ослаблено: top-level всё ещё запрещено (нельзя hijack scope-exit), но **внутри nested fn-литерала/loop в defer body — разрешено** (local control). Реализовано через `DeferBodyCtx { loop_depth, fn_depth }`, инкремент при заходе в loop/fn-literal. Negative-тесты обновлены под новый wording error-сообщения.

### Упрощения / отложенные элементы

- **Q-errdefer-handler — errdefer не работает с user-installed Fail handler.** `with Fail = handler Fail { fail(msg) { interrupt v } }` устанавливает user handler в `_nova_handler_Fail`, который перехватывает `Fail.fail` dispatch ДО того, как throw достигнет local `_defer_BID_ff` setjmp-frame. Errdefer срабатывает **только** на unhandled-throw путях (через default fail-frame). Корректное взаимодействие требует пересмотра handler-dispatch model'и (longjmp first, dispatch inside frame). Зафиксировано как Q-errdefer-handler в `spec/open-questions.md`.

- **Loop-body integration — только range-for.** 20+ мест в codegen с прямым `for stmt in &body.stmts { emit_stmt }` (for-in-array, while, while-let, loop, match-arm bodies, if-branch bodies). Только **for-range body** переписан через `emit_loop_body_inline` (вызывает enter/leave_defer_scope). Остальные продолжают legacy inline-iteration. В fast-path (блок без defer'ов) — поведение идентично, регрессий нет. Defer внутри inline-iterated блока **не зарегистрируется** в DeferScope — будет добавлено incrementally при появлении positive-теста, который зацепит конкретный path.

- **Throw-path positive-тест отложен.** 3 errdefer-теста покрывают только normal-exit семантику. Throw-path задизайнен в codegen (NovaFailFrame setjmp wrapper, longjmp re-throw), визуально подтверждён в generated C, но не покрыт positive-тестом. Reach throw-path в Nova-тесте без handler'а нетривиально (test-runner ловит throw как fail теста). Обход через `EXPECT_RUNTIME_PANIC` маркер возможен, но не реализован — отложено.

### Trade-offs

- **Активационные флаги вместо jump-list.** Codegen эмитит `int _defer_N_active = 0;` + inline `= 1;` при достижении defer'а + cleanup `if (active) { body; active = 0; }`. Простой и читаемый, но O(N) проверок на cleanup. Production-grade компилятор мог бы использовать explicit goto-cleanup-label или jump-table. Для bootstrap'а активационные флаги оптимальны — clang оптимизирует константные `if (1)` в линейный код.

- **`is_error_var = 2` sentinel.** Для double-pop guard'а (когда early-exit cleanup уже popnул fail-frame, leave_defer_scope не должен повторно): использован magic number 2 в `is_error_var` (0=normal, 1=error, 2=already-popped). Не самое читаемое — но работает и self-contained.

- **DeferScope.clone() на каждый early-exit.** `emit_early_exit_cleanup` клонирует `defer_scopes: Vec<DeferScope>` (включая AST `Expr` тел) чтобы итерироваться. Аллокации не критичны (defer'ы редки на hot path), и Rust borrow checker не позволяет иначе без рефакторинга на `&mut self` invariants.

### Файлы

- `compiler-codegen/src/lexer/`, `parser/`, `ast/` — Ф.1, Ф.2 (ранее).
- `compiler-codegen/src/types/mod.rs` — Ф.3 + revision (DeferBodyCtx).
- `compiler-codegen/src/codegen/emit_c.rs` — Ф.4 (DeferScope/DeferEntry, helpers, integration).
- `compiler-codegen/src/interp/mod.rs` — Ф.5 (ранее).
- `nova_tests/syntax/defer_basic.nv` + `errdefer_basic.nv` — Ф.6.
- `nova_tests/negative_capability/defer_*_rejected.nv`, `errdefer_*_rejected.nv` — updated wording.
- `spec/decisions/03-syntax.md` — Ф.7 D90 Bootstrap-status ✅.
- `spec/open-questions.md` — Q-errdefer-handler (новый).

### Status

- ✅ Plan 20 Ф.1-Ф.7 все ЗАКРЫТЫ.
- Tests: 7 positive-тестов defer/errdefer PASS + 6 negative-тестов PASS.
- Defer-relevant suite: 130/130 PASS на момент закрытия. Lingering failures в session (11 CC-FAIL) — от Plan 22/23 sched.h/fibers.h, не от Plan 20.

### Lesson

- **Семантическая ревизия в ходе implementation — нормально.** Изначальный Ф.3 strict (запрет return/break везде в defer body) выглядел проще, но после реального примера use-case (nested loop в defer для batched cleanup) — ослаблено до Вариант 3. Урок: spec-decision на стадии design'а не всегда оптимален; implementation проявляет practical edge cases. **Не цементировать spec до первой реальной реализации.**

- **Handler-dispatch vs setjmp-frame interaction — нетривиально.** Q-errdefer-handler всплыл только на этапе positive-тестирования. Изолированный design «defer работает через fail-frame» был корректен, но не учёл что user handler перехватывает dispatch ДО фрейма. Урок: при проектировании cleanup-механизма учитывать **где** в стеке handler-семантики сидит наша точка catch.

---

## 2026-05-11 — Plan 24 Ф.1-Ф.3: cross-platform test runner

### Что упрощено

`run_tests.ps1` был Windows-only монолит ~320 строк (PowerShell + vswhere + vcvars64 + EXPECT regex parsing + toolchain detection + build flags + run + compare + table formatting). Linux-разработчик не мог запустить тесты вообще.

Решение — вынести логику в Rust subcommand:

- **`nova-codegen test-build <file.nv>`** — один тест: codegen → cc → run → check.
- **`nova-codegen test-all [--filter] [--mode] [--toolchain] [--include-stdlib]`** — рекурсивный прогон с summary.

`.ps1` сократился с 320 до 60 строк (только: пути относительно репо + Windows-specific vcvars detection + pass-through флагов).

Новый `.sh` — 30 строк, такой же thin wrapper для Linux/macOS.

### Trade-offs

- **Blind Linux/macOS implementation.** Реализуем cross-platform через `std::path` / `std::process::Command` + `cfg!(target_os)`. Smoke-test на Linux/macOS отложен (нет access). Уверенность ~85%; первый Linux пользователь увидит точечные edge cases.
- **`std::process::Command` quoting на Windows.** Для invocations через `cmd /c "..."` нужен `raw_arg` из `std::os::windows::process::CommandExt`, иначе Rust auto-escape'ит внутренние кавычки и ломает `cmd /c "call \"vcvars\" && ..."`. Cross-platform через `#[cfg(target_os = "windows")]`.
- **Не выносим `cargo build` в test-all.** Если binary не собран — fail с clear msg, не пытаемся build на лету. Безопаснее: нет race с активной разработкой.
- **vcvars detection — Windows-only.** `find_vcvars()` short-circuit'ит на не-Windows.

### Lesson

**Shell wrappers — анти-паттерн как primary test-runner.** Дублирование логики между bash и PowerShell неизбежно ведёт к drift'у. Перенос в compiler/CLI решает это сразу: один источник правды, type safety (Status enum), unit-тесты для парсеров.

Аналог cargo: начинался как Makefile-обёртки вокруг rustc, постепенно поглотил build/test/run logic. Тот же путь.

### Файлы

- `compiler-codegen/src/test_runner.rs` — новый модуль (~600 строк).
- `compiler-codegen/src/main.rs` — `Cmd::TestBuild`, `Cmd::TestAll`.
- `run_tests.ps1` — упрощён до thin wrapper.
- `run_tests.sh` — новый Linux/macOS wrapper.
- `docs/plans/24-cross-platform-test-runner.md` — план.

### Status

- ✅ Ф.1-Ф.3 ЗАКРЫТЫ (Windows-verified).
- ⏸️ Ф.4 docs (README с инструкциями build на Linux/macOS) — отдельная задача.
- ⏸️ Linux/macOS smoke-test — нужен access.
- ⏸️ CI на Linux — отдельная задача.
- Tests: `cargo test --lib` 77/77 PASS (было 65 + 12 от test_runner).


═══════════════════════════════════════════════════════════════════
Plan 22 production upgrades — 2026-05-11

Closed all simplifications выявленные в /retro Plan 22:

| Упрощение | Решение |
|---|---|
| **NovaSchedState side-table** (16→256 cap) | → **Вариант B**: lazy pointer-в-NovaFiberQueue. O(1) lookup, unlimited nested. |
| **silent fail uv_timer_init/start** | → abort() с FATAL message |
| **top-level no-scope native sleep fallback** | → abort() с D92 invariant violation message |
| **busy-yield под NOVA_USE_LIBUV=1** | → `#ifdef` — compiled только для no-libuv build |
| **bench 500 + 100k** | → 1000 concurrent + 1M yields |
| **spec sync incomplete** | → D71 evolution + D75/D80 cross-refs |
| **libuv не интегрирован в test_runner.rs** (regression от Plan 24) | → detect_or_build_libuv + lazy auto-build, cross-platform Windows/Linux/macOS |

Trade-offs:
- Lazy alloc в park-path = new GC-граница. Acceptable bootstrap; Plan 23 M:N ревизия.
- close-callback wait через `uv_run NOWAIT` остался busy (короткий 1-2 iter).
  Park-based вариант = UB (повторный mco_yield после wake).
- SIGINT handler через uv_signal_t — отложен в D92 Правило 7 (future).
- Leak verification — manual; CI Valgrind отдельной задачей.


═══════════════════════════════════════════════════════════════════
Plan 22 hardening Ф.7-Ф.11 — 2026-05-11

После production pass'а (37dd1a6) — дополнительный hardening для
high-load / long-running. 5 фаз, 3 реализованы, 2 deferred с clear
blockers.

| Фаза | Что | Status | Trade-off |
|---|---|---|---|
| **Ф.7** Heap-allocated NovaFiberQueue/NovaSchedState arrays | ✅ Done | +6 nova_alloc per scope-grow (micro-overhead, acceptable) |
| **Ф.8** Close-cb state machine | ⏸ Deferred | D93 sync-vs-async stop_cb contract нужен (Q-D93-sync-async-stop) |
| **Ф.9** Leak verification bench | ✅ Done | Implicit через bounded-time, не automated CRT/Valgrind |
| **Ф.10** SIGINT handler через uv_signal_t | ✅ Done | Только main-scope; nested supervised через cancel_requested chain |
| **Ф.11** Linux smoke verification | ⏸ Deferred TBD | Нет Linux env; build_libuv готов, never tested |

**Главные решения:**

- **Ф.7 — capacity-doubling вместо fixed cap.** Раньше `NOVA_SCOPE_CAP=1024`
  как hard limit на fiber'ов в scope (DoS-vulnerable + nested supervised
  stack-overflow). Теперь pointer'ы + capacity field, grow через
  managed nova_alloc от initial=16 doubling. Idle scope ~100 bytes
  стека (было ~50 KB embedded array). Unlimited nested. sleep_bench
  upscaled 1000 → 10k concurrent.

- **Ф.8 deferred — нашли блокирующий design issue в D93.** Прототип
  state-machine `{PENDING, CLOSING, CLOSED}` логически корректен,
  но `cancel_all_pending` делает synchronous `parked[i] = false`
  после stop_cb. Sleep handle требует ASYNC close-wait —
  cancel race'ит close_cb, fiber resume'ится до final state → sanity-
  check abort. Откат к Ф.6. Открыт Q-D93-sync-async-stop: enum
  SYNC vs ASYNC в `NovaCancelStopCb`, Plan 21 (channels) требует SYNC,
  sleep/socket — ASYNC. Перед каналами фиксируется.

- **Ф.9 — bounded-time как leak-proxy.** 100k uv_timer create+close
  + 1k repeated sleep + 1000 scope-cycle. Если leak — runaway
  accumulation overflows time bound (5s/15s/timely). Не automated
  CRT, но pragmatic confidence для bootstrap.

- **Ф.10 — uv_unref на signal handle.** Signal handler passive, не
  должен держать loop alive. После set'а cancel_requested +
  cancel_all_pending → handler возвращается, parked fiber'ы wake'ются
  immediate, defer/errdefer → scope-drain → process exits cleanly.

- **Ф.11 deferred TBD.** Cross-platform build infra готова, но Windows-
  only dev-loop. Откладывается до Linux deployment trigger либо
  параллельно с Plan 18 std.net validation.

**Главный вывод:** Production-grade для Windows закрыт. Оставшиеся
deferred фазы имеют **формализованные blockers**, не неопределённость:
Ф.8 → Q-D93-sync-async-stop; Ф.11 → Linux env access.

Final tests: 138/138 nova_tests + sleep_leak_check 3/3 PASS.

Commits:
- 8c1da32 Ф.7 heap-allocated arrays
- cd55cf2 Ф.10 SIGINT через uv_signal_t
- a4321d7 Ф.9 leak-check + Ф.8/Ф.11 deferred + Q-D93-sync-async-stop

---

## 2026-05-11 (продолжение) — Plan 20 Ф.8: production-grade hardening

### Что закрыто

Все 4 упрощения обнаруженные при retro Plan 20 — закрыты production-
grade fix'ами (5 коммитов):

- `e04ca85d` план Ф.8 (записан в docs/plans/20-defer-implementation.md).
- `61af5af4` Ф.8 (1) type-check Never-op enforcement.
- `007bb9ba` Ф.8 (3) loop/branch body defer integration (7 block-сайтов).
- `d913aa08` Ф.8 (4) D65 правило 3: re-throw в handler skips current frame.
- `33c1e050` Ф.8 (2) defer/errdefer на interrupt-path через InterruptFrame.

### Что было упрощено в Plan 20 и почему закрыто сейчас

1. **Q-errdefer-handler mythical issue** — было записано как
   ограничение, оказалось — правильная Fail-strict семантика.
   Удалено из open-questions (предыдущий retro, commit 24196f2).

2. **Type-check для D61 §1430-1434** — handler-method для
   Never-операции должен закончиться exit-control'ом. Раньше
   gap: type-checker не enforce'ил, runtime ловил через safety
   net (после handler return → nova_throw). Закрыто Ф.8 (1):
   static analysis в `check_handler_never_ops` + helpers
   `expr_diverges`/`block_diverges`/`stmt_diverges`. Negative-
   test `fail_handler_no_exit_rejected.nv` отвергается на
   compile-stage с явным error message.

3. **Loop-body integration в 19+ inline-iteration сайтов** — раньше
   только for-range body был переписан на `emit_loop_body_inline`.
   Defer внутри while/loop/while-let/for-in-array/for-in-iter/else-
   branch/match-arm-block не регистрировался в DeferScope.
   Закрыто Ф.8 (3): все эти 7 сайтов теперь эмитят defer scope.
   Positive-тесты `defer_in_blocks.nv` (9 кейсов).

4. **D65 правило 3 re-throw** — `throw err` внутри handler-body
   снова попадал на тот же handler → infinite recursion. Раньше
   не реализовано в runtime. Закрыто Ф.8 (4): NovaVtable_Fail.prev
   хранит outer handler; Nova_Fail_fail swap'ает на prev на время
   invocation. Positive-тесты `errdefer_rethrow.nv` (3 кейса
   включая 3-уровневый re-throw).

5. **Defer/errdefer на interrupt-path** — раньше defer не запускался
   когда handler делал `interrupt v` (longjmp на NovaInterruptFrame,
   минуя fail-frame и leave_defer_scope). По D90 п.8 defer должен
   срабатывать на ВСЕХ exit'ах. Закрыто Ф.8 (2): codegen эмитит
   local NovaInterruptFrame setjmp wrapper для каждого defer scope,
   на interrupt — invoke только defer (skip errdefer как handled
   exit), pop interrupt-frame, re-interrupt с тем же value.
   Positive-тесты `defer_on_interrupt.nv` (4 кейса).

### Trade-offs

- **handler-stack через prev pointer (D65 п.3)** — добавляет 8 байт
  к NovaVtable_Fail. Альтернатива — thread-local handler-stack — была
  бы dynamic-alloc-heavy. Prev pointer fix size, embedded в vtable.

- **NovaInterruptFrame push на каждый defer scope** — overhead две
  jmp_buf на scope (fail-frame + interrupt-frame если errdefer + defer).
  Для bootstrap'а приемлемо; production-уровень оптимизации может
  combine оба frame'а в один conditional setjmp.

- **(prev: NULL) для не-Fail effects** — codegen hardcoded инициализирует
  prev только для эффектов имя которых "Fail". Production-уровень
  generalize'ит на любой effect с Never-операцией через runtime-mapping
  effect-name → has-prev-field. Bootstrap-stage OK.

- **emit_defer_body_void для interrupt-path** — тот же helper что для
  fail-path. Body клонируется implicit'ом emit_defer_body_void (re-emit
  AST). Эффект size of generated C: ~2x для блоков с >1 defer (cleanup
  cascade повторяется в fail и interrupt branches). Production-уровень
  factor через generated cleanup function. Bootstrap-stage OK.

### Lessons

- **Spec нарушения часто скрыты под runtime safety nets.** D61
  §1430-1434 «handler для Never-op обязан exit-control» нарушалось
  тестами (handler без interrupt) — runtime ловил через дополнительный
  nova_throw после handler return. Это работало по факту, но семантика
  скрывала bugs. Type-check enforcement важен для production grade —
  компилятор должен отлавливать spec-нарушения до runtime.

- **runtime cleanup-machinery требует cooperation от ВСЕХ codegen
  paths.** Plan 20 Ф.4 покрывал только основные emit_block_* helpers,
  inline iteration в 19+ сайтах оставались legacy. Defer внутри них
  silently не работал. Урок: при введении новой sema-конструкции —
  проверить ВСЕ места эмиссии block'ов, не только наиболее популярные.

- **handler-stack semantics нетривиальна.** D65 правило 3 «re-throw
  skips current frame» — лёгкое правило в спеке, но требует prev-pointer
  в vtable + swap-on-invoke в runtime. Без этого user видит infinite
  recursion stack overflow. Spec → runtime semantics не всегда одно-к-
  одному.

- **interrupt и throw — разные паттерны exit, но оба нужны для defer.**
  Спека D90 определяет defer как «cleanup на ANY exit». В реализации
  это значит **два** независимых setjmp/longjmp механизма (FailFrame +
  InterruptFrame), и каждый должен interception'нуть defer body
  отдельно. Один объединённый mechanism — упрощение, не работает.

### Файлы

- `compiler-codegen/nova_rt/effects.h` — NovaVtable_Fail.prev + Nova_Fail_fail swap.
- `compiler-codegen/src/codegen/emit_c.rs` — DeferScope расширен intframe_var/popped;
  enter_defer_scope эмитит interrupt-frame setjmp wrapper; emit_with
  устанавливает vtable->prev; 7 inline-iteration сайтов переписаны
  на emit_loop_body_inline / enter_defer_scope.
- `compiler-codegen/src/types/mod.rs` — check_handler_never_ops + helpers.
- `nova_tests/negative_capability/fail_handler_no_exit_rejected.nv` — negative.
- `nova_tests/syntax/defer_in_blocks.nv` — positive (9 кейсов).
- `nova_tests/syntax/errdefer_rethrow.nv` — positive (3 кейса).
- `nova_tests/syntax/defer_on_interrupt.nv` — positive (4 кейса).
- `spec/decisions/03-syntax.md` — D90 Bootstrap-status расширен Ф.8.
- `docs/plans/20-defer-implementation.md` — Ф.8 план.

### Status

- ✅ Plan 20 Ф.8 (все 4 issue) ЗАКРЫТЫ.
- Tests: 12/12 defer-relevant + 10/10 effects + 17/17 concurrency PASS.


═══════════════════════════════════════════════════════════════════
Plan 22 Ф.8 reopened — close-cb state-machine — 2026-05-11 late

После Plan 25 «honest production readiness» (G7 = Ф.8 close-cb) обсудили
с user'ом: формальный enum SYNC/ASYNC в D93 API closes G7. Сделано.

| Упрощение | Решение |
|---|---|
| **busy-loop `while !handle_closed uv_run NOWAIT`** в _nova_sleep_via_libuv | → УДАЛЁН. Sleep state-machine `{PENDING, CLOSING, CLOSED}`, close_cb делает wake. Один park на весь lifecycle. |
| **stop_cb returns void** (sync unpark предположение) | → `NovaStopMode {SYNC, ASYNC}` enum. cancel_all_pending различает: SYNC immediate unpark, ASYNC ждёт backend wake. |
| **D93 sync/async semantic не описан** | → D93 расширен правилом 5 + use-cases table (sleep=ASYNC, channel waitlist=SYNC, socket=ASYNC, file=ASYNC) + эволюция Ф.8. |
| **Q-D93-sync-async-stop open** | → ✅ ЗАКРЫТО, ссылка на D93. |
| **Plan 25 G7 «Ф.8 close-cb busy-loop»** | → ✅ ЗАКРЫТО (R7 «no busy-loops anywhere» fully enforced). |

Trade-off:
- Каждый sleep adds ~2-3ms ASYNC close_cb wait latency.
- Concurrent sleeps НЕ affected (close_cb batch в одном uv_run pass).
- Sequential 1000 × sleep(10): ~10s → ~20-25s. sleep_leak_check #2
  budget релакс'нут 15s → 30s.
- Acceptable price за clean architecture готовую для Plan 21/23+.

Verification:
- sleep_real_clock 5/5 PASS (включая cancel-during-sleep).
- cancel_stress_test 3/3 PASS.
- sleep_bench 10k concurrent PASS.
- sleep_leak_check 3/3 PASS.
- Полный 138/138 regression — НЕ verified в сессии (Plan 26 run_tests.ps1
  имеет PS quoting + parallel race issues). Individual filter PASS.

Commit: e94d2bc9 plan-22 Ф.8: close-cb state-machine + D93 sync/async stop_cb contract


═══════════════════════════════════════════════════════════════════
Plan 25 honest pass: default malloc-only обнаружено — 2026-05-11

После Plan 22 hardening + Plan 25 первой версии user задал simple
вопрос: «всё сделано как для прода?». Honest re-read code выявил
самое большое production упрощение которое **не было в Plan 25**:

| Упрощение | Реальность |
|---|---|
| **Default alloc backend** | compiler-codegen/nova_rt/alloc.c — plain malloc, нет GC, nova_release — no-op |
| **Memory mgmt в коде** | Объекты создаются, **никогда не освобождаются** |
| **Use-case "single-host server production-grade"** в Plan 25 матрице | **WRONG.** Server long-lived, leaks накапливаются → OOM |
| **spec/overview.md "паузы <1ms"** | Без disclaimer — звучало factual. На самом деле **дизайн-цель** (decisions/05-memory.md correctly помечает как v1.0+ goal) |

**Что сделано:**

(a) Plan 25 G3 split на G3a/G3b:
    - G3a (новый, **высокий приоритет**): default malloc-only — production blocker.
    - G3b: GC pause measurement — после G3a.

(b) Use-case matrix скорректирован — «Single-host server low traffic»
    ✅ → ❌ blocked G3a.

(c) **Plan 27 (новый): GC switch.** Boehm GC как default.
    - alloc_boehm.c уже готов в репо.
    - vcpkg gc.lib + gc.h уже vendored в
      compiler-codegen/vcpkg_installed/x64-windows-static/.
    - 5 фаз: flag → verify → bench → switch default → cross-platform.

(d) spec/overview.md "<1ms" → "**целевые** <1ms p99" + ссылка на
    Plan 25 G3 для текущего состояния.

(e) nova_tests/concurrency/memory_growth_check.nv — bench PASS под
    malloc (short workloads ok), будет показывать bounded live_count
    под Boehm.

**Lessons:**

- **Hardening pass не покрывает всё.** Plan 22 Ф.7-Ф.10 был scheduler/
  runtime hardening, memory mgmt остался Phase-0.
- **Default settings важны.** Boehm готов в коде, но default остался
  malloc — никто не использовал Boehm в тестах.
- **vcpkg vendored** = clean path к решению. Plan 27 ~1 день работы.

Commit: d2c6a7b3 plan-25 honest pass + plan-27 GC switch


═══════════════════════════════════════════════════════════════════
Plan 22 verification pass — declared vs measured — 2026-05-11 late

User третий раз спросил «без упрощений как для прода?». Перечитал
Plan 22 целиком — обнаружил что **declared target characteristics
никогда не measured**. Это было упущение в hardening retro.

| Plan 22 declared | Реальность (measured) | Status |
|---|---|---|
| Sleep precision ±10ms | **±15-30ms p99 Windows** (OS timer-gran ~15.6ms) | ❌ overclaim для Windows, Linux не measured |
| Cancel <1ms wake | **5-15ms p99** (ASYNC close_cb chain Ф.8) | ❌ overclaim, реалистично 5-15ms |
| 10k fibers CPU <5% | sleep_bench PASS <1500ms wall-clock | 🟡 throughput ✅, CPU **не profiled** |
| sleep(0) zero-overhead | <50µs per yield | ✅ verified |
| sleep(1) minimum useful | 1-15ms Windows | ✅ verified в пределах OS gran |

**Что добавлено:**

- `nova_tests/concurrency/sleep_precision_bench.nv` — 50 × sleep(50ms),
  measures min/max/avg deviation.
- `nova_tests/concurrency/cancel_latency_bench.nv` — single + 100-batch +
  10-nested cancel ops.
- Plan 22 doc обновлён: «Целевой target» теперь honest table declared
  vs measured. «Performance verification» теперь с measured cells.

**Production verdict updated:**

- CLI tools + scripts (короткое время жизни): sleep precision
  acceptable.
- Backend tail-latency p99 <50ms SLO: borderline на Windows.
- Real-time (audio/gaming/trading): **НЕ подходит на Windows** без
  `timeBeginPeriod(1)` либо custom hi-res timer source.
- Linux: ожидается лучше (1-4ms gran), но **never measured** (Ф.11
  deferred TBD).

**Lessons:**

(1) «Test PASS» != «target verified.» Plan 22 closed в Ф.7-Ф.10 на
    основании sleep_real_clock + sleep_bench PASS — но эти тесты
    использовали SLACK_MS=200 и НЕ проверяли declared targets.

(2) Третий вопрос «без упрощений?» = signal что предыдущие ответы
    incomplete. Pass через target table item-by-item — нашёл 3 из 7
    целей never measured.

(3) **Production-grade требует measured numbers**, не declared.
    Plan 22 теперь имеет honest cell в табличке.

Commit: 8a503dc9d plan-22 verification pass

---

## Plan 22 F2 — libuv mandatory (2026-05-11)

**Решение:** User decision «libuv — нормально и правильно, других
вариантов не предусматриваем» → удалить conditional build paths.

**Упрощения закрыты:**

| # | Упрощение | Статус |
|---|---|---|
| busy-yield `#else` ветки | dead code при NOVA_USE_LIBUV=1 | ✅ Удалены |
| `_nova_native_sleep_ms` / `_nova_monotonic_ms` | Windows/POSIX платформенный код | ✅ Удалены, заменены uv_hrtime |
| `libuv: None` graceful fallback | test_runner silent degradation | ✅ Удалён → abort |
| `#ifdef NOVA_USE_LIBUV` в scheduler | условная libuv integration | ✅ Удалены — libuv always-on |

**Diff:** 135 строк удалено, 93 добавлено. Net -42 строки в runtime.

**Regression:** 156 / 156 PASS.

**Trade-off принят:** нет fallback если vcpkg/vcvars сломаны →
setup нужен с первого раза. Mitigation: libuv vendored в репо,
`detect_or_build_libuv` в test_runner строит автоматически.

Commit: efb0248d7 plan-22 F2: libuv mandatory

---

## Plan 28 — nova CLI binary (2026-05-11)

**Решение:** `run_tests.ps1` имел фундаментальные проблемы PowerShell
(output buffering, NativeCommandError stderr-trapping, quoting путей с
пробелами). Bash-обёртка работала лучше, но оба скрипта — костыли.
Мировая практика (go, cargo, zig) — один CLI-бинарь как точка входа.

**Упрощения закрыты:**

| Удалено | Заменено на |
|---|---|
| `run_tests.ps1` (104 строки PowerShell) | `nova test` |
| `run_tests.sh` (43 строки bash) | `nova test` |
| `regen_runtime.ps1` | `nova regen-runtime` |
| `regen_runtime.bat` | `nova regen-runtime` |

**Что создано:**

- `nova-cli/` — новый Rust crate, `nova` binary.
- Субкоманды: `nova test` (все флаги run_tests.*), `nova build`,
  `nova run`, `nova check`, `nova regen-runtime [--check]`.
- `test_runner::compile_c_to_exe()` — pub fn для `nova build`.
- Repo root detection через `nova.toml` walk-up (как cargo ищет Cargo.toml).
- Default results-file: `{repo}/target/last-test-results.json`.

**Invariants:**

- `nova-codegen` CLI сохранён нетронутым — IDE, CI, прямая отладка.
- `--gc malloc` не документируется пользователю — это internal режим
  для runtime-разработки (Plan 27 семантика).

**Regression:** `nova test` даёт те же PASS/FAIL что run_tests.ps1.

Commits: plan-28 nova CLI

---

## 2026-05-11 — Post-Plan-19 C16: Closure mut-capture heap-promotion

### Что закрыто

**C16 (mut-capture codegen)** + смежный парсер-баг D49.

**Упрощение закрыто:** `let mut x = 0; let f = || { x += 1 }` — раньше
closure env копировал значение, не shared reference. Writes из body
не обновляли caller. Теперь — heap-box promotion:

- emit_c.rs: `_box_name = nova_alloc(sizeof(T)); *_box_name = name`
- Caller: ExprKind::Ident проверяет `var_boxed` → `(*_box_name)`
- Lambda body: `var_boxed` сбрасывается перед emit (scope isolation),
  mut-captures регистрируются как `_env->name` → `(*_env->name)`
- Несколько closures над одной mut var → shared box (reuse)

**Парсер-баг D49 закрыт:** `||` после newline поглощался D49-tolerance
как binary OR продолжение. `let x = 0\n|| body` → `let x = (0 || body)`.
Фикс: newline-tolerance убрана для `||`, оставлена только для `or`.

### Новые тесты

- `syntax/closure_mut_capture_escape.nv` — escape counter, shared box,
  HOF regression
- `syntax/closure_unit_return_inference.nv` — unit-return inference
  для side-effect-only callbacks

### Regression

160/160 PASS.

---

## Plan 21 — D91 Channel Capability-Split (2026-05-11)

### Упрощение закрыто

**C17 (single Channel[T] type)** — раньше один `Channel[T]` имел send+recv
методы, receiver мог случайно сделать send. Теперь capability-split:
`Channel.new(cap)` → `(ChanWriter[T], ChanReader[T])`.

### Реализация

- `channels.h`: `Nova_ChanWriter*` / `Nova_ChanReader*` wrappers + WaiterList
  heap-allocated (A1, подготовка M:N) + stop_cb для cancel-during-park
- `emit_c.rs`: dispatch по типу объекта; `type_ref_to_c` fallback через
  `_ => format!("Nova_{}*", name)` покрывает `ChanWriter[T]` без явного case
- Параметры функций: `fn f(tx ChanWriter[int])` → `var_types` в `emit_fn`
- 4 новых теста (Секция 6): `fill_channel`, `drain_channel`, `relay` pipeline

### Новые тесты

- `nova_tests/runtime/channels.nv` — 17 тестов (4 добавлено в Plan 21)
- `nova_tests/negative_capability/channel_sender_no_recv.nv` — EXPECT_CC_ERROR
- `nova_tests/negative_capability/channel_receiver_no_send.nv` — EXPECT_CC_ERROR

### Regression

156/156 PASS.

---

## Plan 30 — Channel Improvements: send→bool + tx.clone() (2026-05-11)

### Упрощения закрыты

**C18 (send-throws-on-closed)** — `send()` бросал panic на закрытый канал.
Теперь возвращает `bool`: `false` = закрыт, не бросает.

**C19 (single-writer-only)** — только один writer на канал; два fiber'а не могли
безопасно делить один `ChanWriter`. Теперь `tx.clone()` + `writer_count` ref-count.

### Реализация

**Ф.1 (send→bool):**
- `channels.h`: `nova_chan_writer_send()` — тип `void` → `nova_bool`; `nova_throw` заменён на `return 0`
- `emit_c.rs`: `infer_expr_c_type("send")` → `"nova_bool"`

**Ф.2 (tx.clone()):**
- `channels.h`: `Nova_ChannelState.writer_count` (int32_t); `nova_chan_writer_clone()`;
  `nova_chan_writer_close()` закрывает только при `writer_count == 0`
- `emit_c.rs`: dispatch `"clone"` → `nova_chan_writer_clone()` + тип `"Nova_ChanWriter*"`

### Новые тесты

- `channels.nv` Секция 7: send-after-close → false; send→true в открытый; let-bind
- `channels.nv` Секция 8: clone/not-close-original; close-порядок; fan-in (sum==66)

### Regression

159/159 PASS.

Commit: 60e226da6

---

## Plan 20 follow-up + Bidirectional HOF inference (2026-05-11)

### Упрощения закрыты

**C6 (bidirectional inference HOF arg → closure)** — `|x| x + 1` при передаче в
HOF с параметром `fn(T) -> R` раньше дефолтил тип `x` в `nova_int`. Теперь:
- `hof_param_fn_sigs: HashMap<(fn_name, param_idx), (inner_param_tys, ret_ty)>` — новое поле
  в `CEmitter`, заполняется при `register_fn` для каждого `fn`-типизированного параметра.
- В `emit_call`: если аргумент — `ClosureLight`, смотрим `hof_param_fn_sigs[(callee, idx)]`
  и передаём `context_param_tys` в `emit_lambda` вместо `None`.
- Тип `x` в `map(arr, |x| x > 0)` теперь `nova_bool` если HOF объявлен `fn(bool) -> bool`.

**Plan 20 gap (multi-marker EXPECT)** — `EXPECT_RUNTIME_PANIC + EXPECT_STDOUT` не работали
вместе: `parse_expect` возвращал `Option<ExpectMarker>` (первый wins). Теперь
возвращает `Vec<ExpectMarker>`. Тест `defer_throw_single.nv` реально проверяет
и panic pattern, и `DEFER_FIRED` в stdout.

**Plan 20 gap (nv_panic fail-frame routing)** — `nv_panic` на main flow не проходил
через fail-frame (guard `nova_in_fiber()`), значит `errdefer` не срабатывал на `panic()`.
Убран guard — теперь `nv_panic` проверяет `_nova_fail_top` первым (как `nova_throw`).
`errdefer_on_panic.nv`: `ERRDEFER_FIRED` в stdout + panic pattern в stderr — PASS.

### Новые тесты

- `nova_tests/syntax/closure_bidir_inference.nv` — 6 тестов: bool/str параметры,
  двухпараметровый HOF, захват + инферированный bool (161/161 PASS после добавления).

### Regression

161/161 PASS (1 флейк `cancel_stress_test` — lld-link file-lock, не код).

Commits: d49111281, 98586c954, 19d6fb79e, e728608a7

### Plan 30 post-close: channel API production review (2026-05-11)

Проведён анализ channels.h vs Rust `mpsc` и Go `chan`. Найдены 2 баги + 2 улучшения.

**Что исправлено:**
- `Nova_ChanWriter.writer_closed bool` — guard двойного close на одном handle.
- `nova_throw()` вместо `abort()` при recv/send вне fiber context.
- `NovaChanTryResult {NOVA_CHAN_TRY_OK, NOVA_CHAN_TRY_EMPTY, NOVA_CHAN_TRY_CLOSED}` —
  трёхвариантный результат для `try_recv`/`try_send` (в Nova API прозрачно).
- `Channel.new(0)` → `nova_throw(...)` вместо silent capacity=1.

**Оставшийся tech debt (намеренно):**
- [T1] `writer_count` — `int32_t`, не atomic. Достаточно для 1:N; M:N (Plan 23) потребует `_Atomic int32_t`.
- [T2] WaiterList — singly-linked, O(n) unlink. Достаточно сейчас; под нагрузкой заменить на doubly-linked.
- [T3] `try_recv().is_none()` не различимо в Nova без `rx.is_closed()` — API неудобен. После generics можно добавить `TryRecvResult` в Nova type system.

### Plan 27 Ф.3-Ф.4: vcvars caching + Boehm default + fiber root fix (2026-05-11)

**vcvars per-test overhead** — `call vcvars64.bat` (~6.2 сек) вызывался внутри
каждой компиляции через `cmd /c`. Упрощение: `capture_vcvars_env()` запускает bat
один раз, парсит `set` → `Vec<(OsString,OsString)>`, хранит в `Toolchain` enum.
Каждая компиляция: `env_clear().envs(&snapshot)`. 16-28 сек/тест → 3 сек/тест.

**GC root set limit** — `GC_add_roots()` per-fiber бил в лимит Boehm (~128 entries)
при 10k файберов ("Too many root sets"). Упрощение: `_nova_gc_add_fiber_roots` /
`_nova_gc_remove_fiber_roots` стали no-op. `NOVA_GC_BOEHM` define вместо `GC_THREADS`
(GC_THREADS включал stop-the-world → конфликт с minicoro context switch → deadlock).

**GcKind default** — `#[default]` перенесён с `Malloc` на `Boehm`. Все CLI дефолты
обновлены. 171/171 PASS с Boehm как production GC.

Commits: 31207daab

Commits: 88504b87c, 106e64c33

### Plan 31 D94: select — channel multiplexing (2026-05-11)

**Nova получила `select { }`** — Go-style channel multiplexing. Архитектурные решения:

**SelectWaiter layout-compatibility** — `SelectWaiter` первые 6 полей совпадают с
`ChannelWaiter`. Это позволяет каналу будить select-waiter через тот же
`nova_sched_wake(w->scope, w->slot)` без runtime type dispatch. Park/wake работает
через существующий channel wake path без изменений.

**Fisher-Yates для fairness** — `nova_select_try_immediate()` перемешивает порядок
проверки армов через xorshift32 с seed от адреса ctx. Честный случайный выбор без
дополнительных структур.

**Time.after как канал** — `Nova_Time_after(ms)` возвращает `Nova_ChanReader*`.
Timeout-арм `Some(_) = Time.after(50)` синтаксически неотличим от recv-арма. Таймер
один раз создаёт канал, отправляет значение, закрывает writer — timeout прозрачен
для select runtime.

**Spawn capture fix** — три функции обхода AST (`collect_idents_expr`,
`collect_free_idents`, `collect_bound_names_expr`) не обходили `ExprKind::Select`.
Переменные внутри select-армов не захватывались spawn-closure. Добавлен обход
`SelectOp::Recv { binding }` как bound name (не free), `chan`/`value` как referenced.

Commits: a5003d6b0, 78743a290, aef65ae9c

---

### Plan 27 Ф.6 + Б.2-Б.8: test-runner production polish (2026-05-11)

**Ф.6 — AllocConstraint / GcKindTag / SkipReason:**

Новый вариант `Outcome::Skipped` — тест пропущен из-за backend-несоответствия,
не считается ни pass ни fail. `GcKindTag` — отдельный tag-enum (без данных)
предотвращает circular dependency между `AllocConstraint` и `GcKind`.

Решение: parse_alloc_constraint читает первые 30 строк; run_one делает ранний
return Skipped до любых build-шагов. Summary: отдельный счётчик `skip`.

**Б.2 — parse_timeout_ms:**

Функция standalone (не расширение parse_expect) — отдельные concerns.
`effective_timeout` локальная переменная в run_one вместо мутирования opts.

**Б.3 — Outcome::Pass расширен:**

Closure `make_pass` в run_one собирает все 5 полей в одном месте.
None при !verbose — не тратим память на discard output больших тестов.

**Б.4 — slowest tests:**

Skipped фильтруются из elapsed-sort — их elapsed ~0 (early return).

**Б.5 — list_only / filter_from:**

list_only: ранний return перед TAP-header и thread::scope.
filter_from_set: HashSet<String> — O(1) lookup, не O(n) scan.

**Б.6 — retries в JUnit:**

Self-closing `<testcase .../>` для pass без retry; wrapped `<testcase>` с
`<system-out>retried N time(s) before pass</system-out>` при retry>0.

**Б.7 — xorshift64 + Fisher-Yates:**

xorshift64 вместо DefaultHasher (нестабилен между Rust версиями). Seed 0
→ system time nanos. Seed → eprintln (reproducibility: сохранить seed для
debug перемешанного прогона).

**Б.8 — nova-cli wiring:**

`--shuffle [SEED]` через `Option<Option<u64>>`: `None`=no shuffle,
`Some(None)`=random (→0 в TestAllOpts), `Some(Some(n))`=fixed. Чистое
кодирование clap num_args=0..=1.

**Regression:** 171/171 PASS.

---

## Plan 31 Ф.6: select all-closed detection (2026-05-12)

**SelectSlot.wildcard вместо паттерна в dispatch:**

`SelectSlot` добавлено поле `wildcard: bool`. Альтернатива — передавать паттерн
(`Some` vs wildcard) в каждый вызов. Выбор: хранить в struct, т.к. `try_immediate`
и `park` оба нуждаются в этом поле, и struct уже передаётся везде.

**pre-check до scope/slot в nova_select_park:**

all-closed detection сделан отдельным первым проходом до проверки `scope==NULL`.
Альтернатива — post-loop check после регистрации. Pre-check лучше потому что
main() не является fiber (scope==NULL), поэтому post-loop был бы недостижим —
`abort()` срабатывал раньше. Pre-check позволяет бросить панику из любого контекста.

**Почему wildcard срабатывает на closed, а Some(v) — нет:**

`_ = rx` — "получить любой результат": данные или EOF (closed). Аналог Rust
`recv()` с explicit `Err(RecvError)` обработкой. `Some(v) = rx` — "получить
только данные, игнорировать closed". Это semantically корректное разделение:
closed канал без данных не является "готовым" для Some-arm.

**Regression:** 173/174 PASS (pre-existing: memory_footprint_test).

---

## D97: fiber_ctx[] — GC root для SpawnCtx (2026-05-12)

**fiber_ctx[] — отдельный массив, не inline в mco_coro:**

Альтернатива — хранить SpawnCtx* в отдельном nova_alloc'd wrapper, регистрировать
как GC root через GC_add_roots. Выбор: параллельный массив в NovaFiberQueue
проще — синхронизирован с fibers[] и fiber_fail_top[] через единый grow/null/swap.
Нет отдельного API для регистрации/дерегистрации.

**NULL на завершении, не lazy-free:**

fiber_ctx[i] = NULL в nova_supervised_step при MCO_DEAD — немедленный release
GC root. Альтернатива — оставить до следующего grow/realloc. Немедленный NULL
предпочтительнее: GC может собрать SpawnCtx раньше, снижает peak heap.

**Regression:** memory_footprint_test восстановлен до 1000 fiber'ов, 91/91 PASS.

---

## Warning routing: CEmitter.warnings Vec<String> (2026-05-12)

**Vec<String> в CEmitter вместо прямого eprintln!:**

In-process codegen работает в том же процессе что test_runner — eprintln!
пишет в общий stderr процесса, минуя pipe. Альтернатива — отдельный subprocess
для codegen (subprocess имеет Stdio::piped). Vec<String> проще: не нужен
subprocess overhead, warnings возвращаются через функцию.

**Prepend в captured_stderr, не отдельное поле:**

Альтернатива — добавить codegen_warnings: Vec<String> в Outcome::Pass.
Выбор: prepend в captured_stderr проще — print_summary уже умеет печатать
captured_stderr в verbose mode, не нужен новый branch. Предупреждения видны
только в --verbose, что правильно (они не являются ошибкой).

**Regression:** 91/91 PASS.


---

## std/testing/handlers.nv — Plan 34 Ф.1+Ф.7 (2026-05-12)

**Где:** std/testing/handlers.nv.

**Что упрощено:** `seeded(seed)` использует xoshiro256++ PRNG —
**не CSPRNG**. Production-Random требует `secure() -> Handler[Random]`
через runtime-hook (CSPRNG из nova_rt или OS-syscall) — не реализован.

**История:**
- Ф.1 (изначально) — Knuth MMIX LCG, 2 строки. Бакоп — плохое
  distribution, короткий period.
- Ф.7 (production-grade, 2026-05-12) — заменён на **xoshiro256++**
  (Sebastiano Vigna, public domain CC0): 4×u64 state, period 2^256-1,
  passes BigCrush/PractRand. State init через splitmix64 для
  non-zero state при seed=0. `bytes(n)` использует 8 байт за advance
  (раньше 1 байт). Чистый go/rust-equivalent quality (Go math/rand v2
  использует PCG, Rust rand crate — ChaCha8; xoshiro — established
  alternative).

**Почему не CSPRNG:** test-handler'ы должны быть deterministic
(тот же seed → та же sequence между запусками). CSPRNG для тестов
контр-продуктивен. Production-handler для real crypto — отдельная
ответственность.

**Как починить (CSPRNG part):** добавить `fn secure() -> Handler[Random]`
с external-binding к runtime-CSPRNG (Windows BCryptGenRandom, Linux
getrandom, macOS SecRandomCopyBytes). Это — часть Plan 18 (P0 stdlib
roadmap), не блокер.

**Приоритет:** P2 — production-cryptography не нужна до v0.5.


---

## fixed_ms.sleep(d) — no-op; mut_clock — advance ✅ (Plan 34 Ф.1+Ф.7, 2026-05-12)

**Где:** std/testing/handlers.nv.

**Что упрощено (изначально Ф.1):** `Time.sleep(d)` под
`fixed_ms`-handler'ом — instant return. Виртуальные часы НЕ
продвигаются. Если тест делает `sleep(1s)`, `Time.now_ms()` после
возвращает то же значение что до.

**Решено в Ф.7:** добавлена `mut_clock(start_ms u64)` — mutable test
clock. `sleep(d)` продвигает `current_ms` на `d.nanos / 1_000_000`
через closure-capture `let mut current_ms`. Используется в
rate_limiter/retry/cron тестах когда нужно «прошёл час» без real
delay.

**Что осталось упрощением:**
- `fixed_ms` сохраняет no-op поведение — это нужно для тестов которые
  явно не хотят advance (uuid v7 with fixed timestamp).
- Sub-millisecond durations в `mut_clock` округляются floor (delta_ms =
  nanos / 1_000_000). Для durations ≥ 1ms работает корректно;
  precision-sensitive тесты должны использовать `1.millis()` минимум.

**Как улучшить (опционально):**
- Sub-millisecond precision — хранить `current_ns u64` вместо
  `current_ms`. Тогда `sleep(d)` точное. Trade-off: u64 ns overflow
  через 584 года — ok.

**Приоритет:** P3 — текущая реализация покрывает 99% use-cases.


---

## import Wildcard `*` и bare-name visibility — Plan 35 Ф.1 (2026-05-12)

**Где:** stdlib (9 файлов) — bcrypt/jwt/ulid/uuid/snowflake/rate_limiter/
retry/property/duration используют `import std.testing.handlers as th`
+ `th.seeded(...)` / `th.fixed_ms(...)`.

**Что упрощено:** хотелось бы написать `seeded(42)` без префикса (как в
docstring property.nv `with Random = seeded(seed)`), но `nova check`
cross-file resolution для bare-name функций не работает. Парсер
принимает `import X.Y.*`, но падает на токене `*`.

**Почему:** wildcard import / bare-name visibility требует:
1. Parser: разрешить `*` после dotted-path в parse_import.
2. Name-resolver: открыть все `export`-сущности модуля по bare имени.
3. Spec-decision: D-блок про import semantics (conflicts, shadowing,
   re-export, alias precedence).

Решение через `import as alias` + `alias.fn()` чище для коротких
вызовов, но многословнее для длинных. После закрытия Plan 35 Ф.1 можно
будет вернуть bare-name в 9 stdlib-файлах (cosmetic).

**Как починить:** Plan 35 Ф.1 (низкий приоритет, ~150 строк).

**Приоритет:** P3 — workaround через alias работает и читается.


---

## json.nv: `mut` параметр не поддерживается (Plan 34 Ф.2.1, 2026-05-12)

**Где:** std/encoding/json.nv:499 — `fn Parser mut @parse_member(fields HashMap[...])`.
Раньше был `(mut fields HashMap[...])`.

**Что упрощено:** парсер Nova не принимает `mut`-modifier для параметра
функции (есть только для self-receiver: `fn X mut @method(...)`). Убрал
`mut`. HashMap — reference-type через GC, мутации фактически работают
(метод `fields.insert(...)` модифицирует тот же объект что caller
держит), но в сигнатуре `mut`-маркер потерян.

**Почему:** добавление `mut`-param в Nova grammar — отдельное spec-решение
(call-site marker? automatic? для всех ref-типов?). Не блокер для type-check.

**Как починить:** D-блок про `mut`-параметры (Rust-style explicit
`&mut T` или Java/Kotlin-style implicit для reference-types). Парсер +
type-checker — ~100 строк.

**Приоритет:** P3 — semantics корректна, только signature lossy.


---

## property.nv: trailing-block closure синтаксис (Plan 34 Ф.2.3, 2026-05-12)

**Где:** std/testing/property.nv — 6 мест с `property(gen, |xs| { ... })`.
Раньше использовался Kotlin/Swift-style `property(gen) { xs => ... }`.

**Что упрощено:** Nova не поддерживает trailing-block-as-closure (Kotlin
`list.forEach { it -> ... }`, Swift `array.map { x in ... }`). По D22
closure-литерал — `|xs| { ... }`. Переписал на explicit-argument форму.
Чуть многословнее, но грамматически однозначно (нет ambiguity со
struct-literal'ом или if/while-body).

**Почему:** trailing-block syntax удобен для DSL'ей и AI-prompts
(`channel.send { msg => ... }` читается естественно), но грамматически
конфликтует с block-as-expression (если `f() { ... }` — то closure
или value-statement?). Нужно D-решение.

**Как починить:** D-блок про trailing-closure синтаксис (когда `{ ... }`
после call'а — closure, когда — separate statement). Требует анализа
ambiguity, ~50 строк парсера + грамматики.

**Приоритет:** P2 — DSL ergonomics, но обходится `|x| { ... }`.



---

## CLI `nova check` / `nova test` — MVP simplifications (Plan 36, 2026-05-12)

### Что упрощено в MVP (Ф.0 + Ф.1 + R7 + R10) vs full Plan 36

**Где:** `nova-cli/src/main.rs` (~455 строк добавлены/изменены).

**Полный план**: 30 requirements (R1-R30) + 12 architecture decisions
(AD1-AD12). **Реализовано в MVP**: R1-R8 base + R10 base + R13 (Ф.0
correctness fix) + R19 (parallel) + R20 (GC backend) + R21 (module-path
hard fail).

**Не реализовано (отложено в sub-plans 36.A-E):**

| Sub-plan | Что упрощено | Sufficient workaround |
|---|---|---|
| 36.A outputs | 1 output format (human). Нет JSON/SARIF/JUnit. | Wrap через `grep`/`awk` или wait for 36.A |
| 36.A diag codes | Нет stable E0001-E9999 registry. Diagnostics только human. | `nova explain` impossible v1, plan 36.D |
| 36.A spec_link | Нет `spec_link` field в diagnostic. | spec ссылка в diagnostic message прямо как plain text |
| 36.B caching | Каждый check полный re-check. <500ms cache miss отсутствует. | Acceptable для CI; локально для разработчика — manual incremental |
| 36.B repro builds | Нет `SOURCE_DATE_EPOCH` / no-timestamps. | Не критично без CI |
| 36.C pre-commit | Нет `.pre-commit-hooks.yaml`. | Manual git hook script если нужно |
| 36.C GHA annotations | `::error file=,line=::` не emit'ится. | CI просто видит exit code + stderr |
| 36.D verbosity | Нет `-q`/`-v`/`-vv`. | --color never для CI достаточен |
| 36.D --explain | Нет `nova explain Exxxx`. | Diagnostic codes пока не emit'ятся, не блокер |
| 36.D --dry-run | Нет `--dry-run` / `--list`. | Скрипт `find ... -name '*.nv'` достаточен |
| 36.E workspace | `find_repo_root` берёт первый parent с nova.toml. 4 nested nova.toml в repo (root + nova_tests + examples + std) — не unified. | В MVP `nova check` от repo root walks-all; для package-scoped check — `nova check std/` явно |

### Почему

Полный Plan 36 — много-сессионная работа (160 gaps в plan v4 после
4-way audit). MVP = focused subset который **shippable in one session**
с реальной production-value (Ф.0 closes silent bug, R7 closes exit code
ambiguity, R10 closes CI no-color requirement).

### Как починить

Sub-plans 36.A-E — отдельные плановые файлы, отдельные сессии. Каждый
закрывает свою группу:
- 36.A — outputs (приоритет: высокий для CI integration)
- 36.B — caching (приоритет: средний, влияет на dev workflow)
- 36.C — CI integration (приоритет: высокий после 36.A)
- 36.D — advanced ergonomics (приоритет: средний)
- 36.E — workspace (приоритет: низкий, current implicit walks-parents
  работает)

### Приоритет

**P1** для 36.A + 36.C (CI integration сценарий критичен).
**P2** для 36.B + 36.D (UX win, не блокер).
**P3** для 36.E (workspace concept — после Plan 03 package ecosystem).


---

## D54 семантические проверки живут только в codegen — Plan 37 (2026-05-12)

**Где:** `compiler-codegen/src/codegen/emit_c.rs`:
- `check_as_cast_allowed` (24 banned-пары: `int as char`, `char as byte`,
  `int as bool`, `str ↔ T`, и др.) — вызывается только из emit-пути
  для `ExprKind::As` (строка 5474).
- `check_bool_condition_at` (strict bool в `if cond` / `while cond`) —
  вызывается только из emit-пути (строки 5269, 7660).

**Что упрощено:** type-checker (`compiler-codegen/src/types/mod.rs`) для
`ExprKind::As` и условий `if`/`while` **только рекурсирует во внутрь**
без D54-валидации. Результат: `nova check std/encoding/hex.nv` → PASS,
`nova test std/encoding/hex.nv` → CODEGEN-FAIL `int as char запрещён`.

**Почему:** проверки добавлялись прицельно в codegen (Plan 08 Ф.5 для
as-cast, Plan 08 Ф.4 для strict bool) и оставались там — type-checker
не переоткрывался под эти классы ошибок. Архитектурно это **отложенная
диагностика**: ошибка на codegen-фазе, а не на check-фазе.

**Влияние на UX:** нарушает контракт `nova check` (D95, Plan 36) —
«полная type+lint валидация модуля без codegen». LLM-агенты и
ide-integrations, которые гоняют `nova check` для feedback'а,
получают «green check + red build» на тех же файлах.

**Как починить:** Plan 37 — перенести (или продублировать через shared
module) проверки в type-checker. Detail в
[docs/plans/37-typecheck-semantic-parity.md](plans/37-typecheck-semantic-parity.md).
Защита defense-in-depth (codegen всё равно держит свой check) на случай
прямого `nova-codegen build` без `check` шага.

**Приоритет:** **P2** — UX win для `nova check` contract, но обходится:
- `nova test foo.nv` (или `nova build foo.nv`) ловит ошибку с тем же
  сообщением, просто позже.
- Workaround не нужен — пользователь чинит код по сообщению codegen.

**Обнаружено:** при правке `std/encoding/hex.nv` под D54 (
`('0' as int + n as int) as char` → нужен `char.try_from(n)?` с `Fail`
в сигнатуре `digit`). type-check файла прошёл, codegen упал.


---

## Cross-file resolve Plan 35 Ф.1 MVP — inline AST expansion (2026-05-12)

### Что упрощено в MVP (commit f481e3950e) vs full Plan 35

**Где:** `nova-cli/src/main.rs::resolve_imports_inline()` (~165 LOC).

**Полный план**: Plan 35 v2 имеет 30 requirements (R1-R30) + 12 AD
после 3-way audit (65 gaps). **Реализовано в MVP**: только R1 (FS
loader, single-root), R2 (in-memory dedup via visited), R3 (topo walk
+ cycle detection через import_stack), R7 (missing import error).

**MVP подход:** **inline AST expansion** — imported `module.items`
просто copy'ятся в текущий `module.items` до typecheck/codegen. Это
дешевле чем правильный `register_imported_module` (требует deep
refactor `CEmitter::emit_module` 600+ LOC) и unblock'ит multi-file
stdlib через `nova build` сегодня.

**Не реализовано (отложено в sub-plans 35.A-E):**

| Sub-plan | Что упрощено в MVP | Workaround / Impact |
|---|---|---|
| 35.A wildcard | `import X.Y.*` не поддерживается | используй `import X.Y` + bare names через AST merge |
| 35.A visibility | `is_export` informational only, не enforced | private items из imported модуля доступны (spec violation) |
| 35.A `export use` | Re-export не поддерживается | каждый файл re-decl'ит для exposure |
| 35.A prelude | Нет автоматического prelude module | bare `Option`/`Result` уже работают через hardcoded baseline |
| 35.B disk cache | Каждый `nova build` re-parsит imports | acceptable для CI; локально медленно при changes |
| 35.B incremental | Нет dependency-based rebuild | full re-parse каждый раз |
| 35.B memory cache invalidation | Простой `HashSet<canonical_path>` per build | каждый процесс начинает с нуля |
| 35.C cross-file generics | Generic bounds не resolve cross-file | inline дублирование bound trait в каждом файле |
| 35.D stable mangling | Нет mangling, items в global namespace | collision если user re-defines stdlib type (unlikely) |
| 35.D DCE | Все imported items emit'ятся (bloat) | bin size больше необходимого |
| 35.E `#[cfg(...)]` | Нет conditional compilation | platform-specific код через if-runtime |
| AD3 sig/body 2-pass | Single-pass typecheck merged AST | mutual recursion через modules может ломаться; flat deps OK |
| FileId propagation | Все Spans в imported items имеют file_id=0 | cross-file diagnostics показывают main file даже для imported errors |
| `nova test` parity | `resolve_imports_inline` только в `cmd_build` | `nova test` с `import` не работает (отдельный pipeline в test_runner) |

### Почему inline expansion вместо register_imported_module

Full register approach (Plan 35 v2 §AD2):
- New `trait ModuleLoader` + 3 impls (~200 LOC).
- New `CEmitter::register_imported_module()` (~150 LOC).
- Visibility filter, collision detection (~50 LOC).
- 2-pass typecheck signature/body split (~300 LOC refactor existing
  check_module).
- Total: ~700+ LOC + invasive refactor.

Inline approach (MVP):
- One helper function `resolve_imports_inline()` (~165 LOC).
- Zero changes в `CEmitter` или `types::check_module`.
- Backward compat: `module.imports.is_empty()` → старое поведение.
- Trade-off: bloat (no DCE), no visibility (informational only).

**Production-grade — sub-plan 35.A**. MVP первое приближение для
разблокировки real blockers (hex.nv) уже сейчас.

### Как починить

Sub-plans 35.A-E:
- **35.A** — visibility enforcement + wildcard `import X.Y.*` + `export use` +
  prelude module + DCE + mangling rules.
- **35.B** — disk cache + incremental rebuild + memory cache invalidation.
- **35.C** — cross-file generic bounds resolution.
- **35.D** — stable v0-style symbol mangling.
- **35.E** — conditional compilation + `internal/` convention + editions.

Также **vertical fix**: extract `resolve_imports_inline` из `nova-cli`
в shared lib (`nova-codegen` или новый crate) и вызывать из
**test_runner pipeline** — закрыть `nova test` parity.

### Приоритет

**P1** для test_runner parity (50 LOC) — без него `nova test` не
работает для multi-file stdlib, разработка stdlib ограничена `nova build`.
**P2** для 35.A visibility/wildcard — production-grade, но обходимо
сегодня.
**P3** для 35.B caching — performance, не correctness.

---

## Plan 33 contracts (bootstrap)

### [V1] TrivialBackend SMT вместо Z3
- **Где:** `compiler-codegen/src/verify/backend/trivial.rs`.
- **Что упрощено:** SMT verification реализована через built-in
  symbolic simplification + pattern matching (constant folding,
  reflexivity, импликация-shortcuts, boolean idempotents). Доказывает
  только тривиальные тавтологии типа `result == x*2` для body `=> x*2`
  (reflexive после substitute) и явные counterexample типа
  `result == 42` для body `=> 100`.
- **Почему:** libz3 не установлен в системе/vcpkg на момент реализации.
  Architecture-grade trait `SmtBackend` готов — Z3-backend подключается
  как отдельная реализация trait'а. Это **engine-agnostic дизайн**
  как требует D24 §148.
- **Как чинить:** Установить libz3 в vcpkg (`vcpkg.json` + Windows
  bindings) или системно (apt/dnf на Linux). Добавить feature-flag
  `z3-backend` в `Cargo.toml`. Реализовать `Z3Backend: SmtBackend`
  через `z3` crate. Activate через `nova test --smt-backend z3`.
- **Приоритет:** H — без linear-arith reasoning (что Z3 даёт) trivial
  backend не доказывает `ensures result > 0` для `=> x + 1` при
  `requires x > 0`. Программисту приходится либо `#unverified`, либо
  переписывать на reflexive form.

### [V2] Loop invariants парсятся, но не сохраняются в AST
- **Где:** `parser::skip_loop_clauses` в `parse_while`/`parse_for`/`parse_loop`.
- **Что упрощено:** `invariant <expr>` и `decreases <expr>` между
  loop-header и body парсятся и игнорируются — программист может писать
  spec, но SMT их не использует.
- **Почему:** trivial backend всё равно не верифицирует loops
  (нужен Z3 для havoc + invariant preservation + decreases check).
- **Как чинить:** Расширить `ExprKind::For`/`While`/`Loop` полями
  `invariants: Vec<Contract>` и `decreases: Option<Expr>` — это
  breaking change для interp/codegen/types match'ей, но **необходимо**
  для Z3 verify pipeline.
- **Приоритет:** M — depends on [V1] (без Z3 не имеет смысла).

### [V3] Composition требует #pure, но purity не выводится автоматически
- **Где:** `types::ContractCtx::pure_fn_names`.
- **Что упрощено:** Composition (вызов user fn в контрактах) разрешён
  только если у fn есть явный `#pure` атрибут. SCC-inference по
  call-graph (как `const fn` в Rust) — НЕ реализован.
- **Почему:** SCC inference потребует mutual-call analysis +
  effect propagation. Это полноценный pass — отложен до Plan 33.3
  full (требуется для composition в SMT тоже).
- **Как чинить:** Добавить `PurityCtx::infer` с fixpoint по
  call-graph через SCC. Атрибут `#pure` остаётся как assertion
  (если выведенный mismatch — compile error).
- **Приоритет:** M — текущее поведение honest (программист обязан
  пометить), но требует boilerplate `#pure` на каждой helper-fn.

### [V4] `old(...)` для mut params — snapshot trivial (значение = current)
- **Где:** `verify/pipeline.rs::substitute_old`.
- **Что упрощено:** `old(x)` подменяется на текущее значение `x`
  (`old(x) → x` в SMT). Это корректно ТОЛЬКО для 33.1/33.2 scope
  где нет `mut` параметров — в этих условиях snapshot тривиален.
- **Почему:** Mut params + frame conditions не закрыты SMT-side
  в bootstrap (parser + type-check есть, SMT verify ждёт Z3).
- **Как чинить:** В Z3 backend каждый mut param получает две версии:
  `x_entry` (snapshot) и `x` (текущее). `old(x)` → `x_entry`.
  Frame-axiom для `modifies M`: всё что не в M — equated с entry.
- **Приоритет:** M — без mut params в 33.1/33.2 это noop;
  обязательно для 33.2 SMT verify + 33.3.

### [V5] Ghost state эмитится в codegen в debug (не «никогда»)
- **Где:** `parser::parse_let_decl` (is_ghost field) + codegen.
- **Что упрощено:** `ghost let x = ...` сейчас компилируется как
  обычный `let` (значение вычисляется и хранится в runtime).
  Dafny semantics — ghost никогда не emit'ится, даже в debug.
- **Почему:** Реализация ghost-aware codegen требует filter'а
  на каждом Stmt + проверки что non-ghost code не reads ghost
  vars. Big change в emit_c.rs.
- **Как чинить:** Plan 36 production hardening: ghost-elimination
  pass в codegen + type-check rule «non-ghost code cannot read
  ghost vars».
- **Приоритет:** L — runtime overhead на пустом месте, но не
  unsoundness. Plan 33.3 описывает это как требование Dafny-parity.

### [V6] pure_view + axiom + #verify_handler — НЕ реализованы
- **Где:** Plan 33.3 spec.
- **Что упрощено:** Контракты на handler-state (`ensures Db.balance(to)
  == old(Db.balance(to)) + amount` из R4) не работают. Effect ops
  объявить как `pure_view` нельзя.
- **Почему:** Требует Z3 + axiom-based encoding + handler verification
  pass. Это значительная работа в Z3 backend.
- **Как чинить:** Plan 33.3 full — после libz3 setup. Полная семантика
  pure_view (см. Plan 33.3 Ф.9): UF + axioms + axiom consistency check +
  обязательный `#verify_handler` для handler'ов с pure_view contracts.
- **Приоритет:** M — основная revolutionary feature D24/R4; без неё
  контракты на handler-state остаются «aspirational», не enforced.

### [V7] Bounded quantifiers (`forall`/`exists`) — НЕ реализованы
- **Где:** Plan 33.3 spec.
- **Что упрощено:** `forall x in xs : P(x)` / `exists x in xs : P(x)` /
  `forall i in lo..hi : P(i)` — парсер не принимает, в encode.rs
  возвращается EncodingError::Unsupported.
- **Почему:** Требует Z3 quantifier support + bounded encoding
  (conjunction для known size; SMT forall с pattern для symbolic).
- **Как чинить:** Plan 33.3 full Ф.10. Парсер расширяется на
  `KwForall`/`KwExists` + range-syntax; encode.rs добавляет конъюнкцию/
  Z3 forall с pattern annotation.
- **Приоритет:** M — нужно для array-based алгоритмов (binary search,
  sorting properties).

### [V8] FP IEEE 754, strings beyond eq, sets/maps — НЕ реализованы
- **Где:** Plan 33.3 Ф.11.
- **Что упрощено:** Контракты с FP-операциями (`f64.is_nan()`),
  string operations (substring, contains), set/map cardinality —
  не verified.
- **Почему:** Каждая теория требует отдельной Z3-кодировки
  (FloatingPoint theory, Seq theory, Arrays + UF).
- **Как чинить:** Plan 33.3 full Ф.11. Включается через
  атрибуты `#verify_fp`/`#verify_strings` (default off, чтобы
  не замедлять обычное reasoning).
- **Приоритет:** L — большинство контрактов работает на int/bool.

### [V9] Incremental SMT cache + parallel verification + Z3↔CVC5 cross-check — НЕ реализованы
- **Где:** Plan 33.3 Ф.12.
- **Что упрощено:** Каждый верификационный запуск — full re-verify.
  Один backend (TrivialBackend). Нет cross-check.
- **Почему:** Все три feature требуют либо libz3 (cache, cross-check
  имеют смысл только с реальным SMT), либо rayon integration (parallel).
- **Как чинить:** Plan 33.3 full Ф.12 — после libz3 setup +
  CVC5 binding crate. Incremental cache: `target/contracts-cache/<hash>.json`.
- **Приоритет:** L — performance, не correctness.

### [V10] #must_verify_module + #trusted external fn — НЕ реализованы
- **Где:** Plan 33.3 Ф.13.
- **Что упрощено:** Module-level strict mode и `#trusted` external
  с контрактами (registered as axioms без proof) — не поддержаны.
- **Почему:** Module-level attribute + extension парсера. External
  fn с контрактами уже rejected как «not supported in Plan 33.1».
- **Как чинить:** Plan 33.3 full Ф.13.
- **Приоритет:** L — workable обходные пути (per-fn `#must_verify`).

### [V11] Dafny-tutorial port (20 примеров) — НЕ выполнено
- **Где:** Plan 33.3 Ф.14.
- **Что упрощено:** Acceptance test «not worse than Dafny» через
  port 20 классических примеров не проведён.
- **Почему:** Требует все вышеперечисленные V6/V7/V8 чтобы пройти.
- **Как чинить:** После Z3 milestone + V6-V10 fixes.
- **Приоритет:** M — критичный gate для production-claim
  «Dafny-parity».


---

## char.try_from с unreachable Err fallback — Plan 34 Ф.5.2 (2026-05-12)

**Где:** 5 stdlib-файлов:
- std/encoding/base64.nv: `encode_char_std`, `encode_char_url`
- std/encoding/hex.nv: `digit`
- std/identifiers/ulid.nv: `encode_char`
- std/identifiers/uuid.nv: `hex_digit`
- std/testing/property.nv: `StrGen @generate` (ASCII char)

**Что упрощено:** D54 запрещает `int as char`, требует
`char.try_from(n)?`. Но в этих случаях `n` всегда в valid диапазоне
(`'0' + value` для `value ∈ [0, 15]` всегда даёт ASCII digit), значит
`Err` невозможен. Refactor:

  let code = '0' as int + value as int
  match char.try_from(code) {
      Ok(c)  => c
      Err(_) => '?'      // unreachable
  }

Fallback `'?'` нужен для exhaustive-match, но недостижим —
**семантически dead branch**.

**Почему:** Альтернативы хуже:
1. `?`-propagation — меняет return type `-> char` → `Fail[CharRangeError] -> char`,
   ломает все callers (3-tier изменение).
2. `panic("unreachable")` — runtime crash вместо degraded output.
3. `unsafe_int_as_char` — нет в spec, добавлять ради 5 callsites не оправдано.

**Как починить:** Plan 37 «type-check semantic parity» (создан агентом)
поднимет D54 проверку в type-checker. После этого type-checker может
validate static-range проверки compile-time (literal + bounded variable
analysis), `char.try_from(IntLit | bounded var)` опускается до direct
cast, fallback ветка элиминируется как dead code.

**Приоритет:** P3 — fallback недостижим, downstream perf не страдает.


---

## stdlib `--skip std/runtime/` обязателен для nova test — Plan 34 Ф.5.1 (2026-05-12)

**Где:** Workflow для CI / dev sweep по stdlib.

**Что упрощено:** `nova test std/` без `--skip std/runtime` даёт **7
false-FAIL'ов** для auto-gen библиотечных модулей std/runtime/* (char/
gc/math/read_buffer/string/string_builder/write_buffer) с linker
error `undefined symbol 'nova_fn_main_impl'`. Эти файлы — *lib-only*,
у них нет main и tests, но `nova test` пытается их собрать как exe.

D95 hard-skip `std/runtime/` есть в `nova check` (через
`should_skip_path`), но **не в `nova test`**. Текущее workaround —
обязать пользователя писать `--skip std/runtime` вручную.

**Почему не auto-skip в walk_nv:** Параллельный агент выбрал
**explicit --skip flag** (commit before f481e3950e), а не зашитую
константу в `walk_nv` (я пробовал, откатил по запросу пользователя).
Преимущество: пользователь видит что skip'ается; не зашиты опциональные
правила в core walker. Минус: friction для типичного use-case.

**Как починить (полное решение):** Один из вариантов:
1. **D95 расширить на nova test** — добавить `runtime` в
   `is_implicit_skip` ИЛИ вызвать `should_skip_path` в test_runner's
   walk-этапе (как уже сделано в check). ~10 строк.
2. **Per-file pragma** `// LIB_ONLY` — runner пропускает файлы без
   main и без test-блоков. Более общее, но больше работы.
3. **Manifest-уровень**: `std/runtime/nova.toml` с
   `kind = "library"` исключает из test sweep'а. Архитектурно
   правильнее, но требует package-system (Plan 03).

**Приоритет:** P2 — `--skip std/runtime` работает, но это lasting
papercut для каждого нового пользователя.


---

## Plan 34 Ф.5.3 — strict-bool fix НЕ применён (D72 блокер) — 2026-05-12

**Где:** 4 файла std/ остались с `if condition must be bool` codegen-fail:
- std/collections/priority_queue.nv:69 `@items[i].lt(@items[parent])`
- std/concurrency/retry.nv:121 `d.gt(max_delay)`
- std/encoding/json.nv:526 `fields.contains(key)`
- std/encoding/url.nv:78 `after_scheme.starts_with("//")`

**Что упрощено:** Изначально Plan 34 Ф.5.3 планировал локальный fix
`if x` → `if x != 0`. После анализа стало ясно — это **не** локальная
правка. Все 4 вызова — generic-method dispatch через protocol-bound
(`Ord.lt`, `Ord.gt`, `Hash.contains`, `Str.starts_with`), который
codegen в generic-context возвращает с return-type `nova_int` вместо
`bool` (D72 erasure).

Plan 14 retrospective прямо называет это «блокер для Plan 15
enforcement». Локальный `!= 0` workaround **не помогает** — codegen
всё равно видит `nova_int` value.

**Почему не fix:** Spec-level work — нужно расширить codegen
`method_overloads` для protocol-bound generics так чтобы они возвращали
правильный bool-type. Это **Plan 15 enforcement** territory +
monomorphization. Не Plan 34 scope.

**Как починить:** Новый план «D72 method-resolution через
protocol-bounds в codegen» — ~200-300 строк в emit_c.rs +
method_overloads expansion. Открывает 4+ stdlib-файла для compile.

**Приоритет:** P1 — блокирует 4 файла, но D72-уровень требует careful
spec-level work.


---

## Plan 34 Ф.5.4 — for-in nova_int НЕ закрыт целиком — 2026-05-12

**Где:** 5 файлов std/ с `for-in: unsupported iterator type 'nova_int'`:
- std/crypto/bcrypt.nv, std/collections/range.nv,
  std/encoding/ini.nv, std/text/diff.nv, std/text/regex.nv

**Что упрощено:** Plan 14 Ф.1 refactor Option[T] раскрыл Iter[T]
erasure для нестандартных iterator expressions. `for i in seq.iter()`
где seq имеет custom Iter — codegen падает.

Параллельный агент сделал commit `e019a47128` "forward-decl user types
+ Nova_Range emit + Range infer для step_by" — это закрывает
**same-file** Range/StepRange. Cross-file и custom Iter ещё открыты.

**Почему не fix в Plan 34:** Iter[T] generic specialization at
monomorphization — Plan 14 «накопленные блокеры» категория.
Architectural work уровня spec.

**Как починить:**
1. Cross-file Range — Plan 35 Ф.2 (cross-file codegen). MVP через
   `f481e3950e` (inline AST expansion) частично решает.
2. Custom Iter (`hashmap.keys() -> Iter[K]`) — Plan 14 «hashmap
   protocol-dispatch» блокер. Требует monomorphization для generic
   methods.

**Приоритет:** P1 — 5 stdlib-файлов и больше, но архитектурный блокер.


---

## Numeric type constants — known limitation (Plan 38, 2026-05-12)

### Где
Codegen (`compiler-codegen/src/codegen/emit_c.rs`) и type-check
(`compiler-codegen/src/types/mod.rs`).

### Что упрощено
D26 prelude декларирует numeric type constants (`int.MAX`, `int.MIN`,
`f64.NAN`, `f64.INFINITY`, `u8.MAX`, etc.) — **ни один** не работает в
codegen. Codegen mangles paths в `<type>_<CONST>` (`int_MAX`,
`f64_NAN`, etc.), которые undefined C identifiers → compile error.

### Почему
Bootstrap codegen не имеет mapping table для type-level constants.
Special-case'итс `int.try_from` / `f64.from_bits` etc. (Plan 08),
но constants — отдельная category (нет params, тип = type-of(prim)).

### Как починить
Plan 38 — full mapping table:
- `int.MAX` → `((nova_int)INT64_MAX)`
- `f64.NAN` → `NAN` (from `<math.h>`)
- `u8.MAX` → `((uint8_t)UINT8_MAX)`
- etc. (см. plan 38 для полной таблицы)

Plus type-check side в `is_known` для primitive type-constants.

### Workaround сегодня
**Inline literal** вместо `int.MAX`:
```nova
// Вместо: if end == int.MAX { ... }
// Используй: if end >= 9223372036854775807 { ... }  // hard-coded INT64_MAX
```

Это **breaks portability** между i32 / i64 builds, но работает для
single-target compile.

### Приоритет
**P2** — system gap, но влияет на ограниченное число файлов сегодня
(основной — `std/collections/range.nv:Range.inclusive`). Plan 38 ~ полдня.

### Real-world impact
- `std/collections/range.nv` — Range.inclusive constructor блокирован
- Любой future numeric stdlib (clamp, bounded, saturating ops)


---

## range.nv blocked — known limitation (Plan 39, 2026-05-12)

### Где
`std/collections/range.nv` — full file compile блокирован.

### Что упрощено
`std/collections/range.nv` объявляет 4 core types (Range, RangeIter,
StepRangeIter, ReverseRangeIter) + ~30 methods + 11 inline tests.
**Не компилируется** через `nova test` / `nova build` из-за:
1. `int.MAX` mangling → Plan 38.
2. `nova test` cross-file resolution отсутствует → Plan 35 Ф.1
   test_runner parity (отложено).
3. Возможные `NovaOpt_<T>` typedef mismatches в pattern match
   ассертах (`r.next() == None`).

### Почему
Cascade блокеров — каждый требует отдельного fix'а в codegen.
Pre-existing, не Plan 35 territory.

### Как починить
Plan 39 = follow-up cleanup после Plan 38 + Plan 35 Ф.1 test_runner.

### Workaround сегодня
**Inline Range/RangeIter/StepRangeIter в user file** — Plan 35 Ф.1
MVP уже доказал что same-file path работает. `for_in_range_iter.nv`
тест: 4 assert PASS на inline declarations.

Cross-file через `import std.collections.range` — works для **`nova
build`** (после Plan 35 Ф.1 MVP), не для **`nova test`** (test_runner
pipeline отдельный).

### Приоритет
**P3** — это **cascade follow-up**, не root cause. После Plan 35 Ф.1
test_runner parity + Plan 38 (~1 день combined) — `range.nv` либо
автоматически проходит, либо требует small fix'и (Plan 39, оцениваем
0-200 LOC).


---

## Iter[T] resolution в codegen — partial D58 implementation (Plan 39 Issue D, 2026-05-12)

### Где
`compiler-codegen/src/codegen/emit_c.rs::emit_for` (Case 3 + fallback).

### Что упрощено
Spec D58 требует 3-step lookup для `for x in c`:
1. Method `next()` direct → primitive iterator loop.
2. Method `iter()` → recursive lookup `next()` на iter type.
3. Иначе → error «type X has neither next nor iter».

**Текущая реализация:** только Case 1 lookup. Если type имеет `iter()`,
но не `next()` — fall through на generic «unsupported iterator type».
**Auto-`iter()` insertion (Case 2) — отсутствует.**

Дополнительно:
- `mut`-receiver enforcement для `next()` — отсутствует. Возможно
  iterator advance без mutable state (immutable next = бесконечный
  loop возвращающий тот же element).
- Generic «unsupported iterator type 'nova_int'» вместо специфичного
  «type 'X' has no `next` or `iter` method».

### Почему
Bootstrap codegen эмитил Case 1 как minimum viable, Case 2 не
добавили потому что core stdlib (Range, []T, RangeIter) — все
exposed через **direct `next()`** или **primitive optimization paths**
(Range = Case 1 primitive int loop, []T = Case 2 array loop).

D58 Case 2 (`iter()` chain) нужен для:
- User-defined collections с `iter()` методом но без direct `next()`.
- Stdlib `HashMap.iter()`, `Vec.iter()` returning отдельный iter type.

### Как починить
Plan 39 Ф.2 Issue D:
1. Переписать emit_for Case 3 по D58 algorithm exactly:
   - method_overloads[(c_ty, "next")] check
   - fallback: method_overloads[(c_ty, "iter")] → recurse next() on
     iter return type
   - clear error if neither
2. Assert `is_mut=true` для `next()`.
3. Improve diagnostic с конкретным type name + method names searched.
4. Test file `nova_tests/syntax/for_in_iter_resolution.nv`.

### Workaround сегодня
**Manual `.iter()` call:** `for x in c.iter()` вместо `for x in c`.
Это эквивалентно D58 Case 2, но не automatic. Стандартный паттерн
сейчас в std/* — почти все file'ы explicit `.iter()`.

### Приоритет
**P2** — нарушение D58 spec, но обходимо через explicit `.iter()`.
Влияет на UX (программист должен помнить `.iter()` где должно быть
automatic), не на correctness (compile error явный).

### Real-world impact
- Cross-file Range/RangeIter сценарии — partial OK через Case 1
  (Range literal) и Case 3 (RangeIter.next direct).
- `for x in some_hashmap` без `.iter()` — error «unsupported iterator
  type». Workaround: `for x in some_hashmap.iter()`.


---

## Numeric type constants FIXED (Plan 38, 2026-05-12)

Plan 38 closed system gap — все int.MAX/f64.NAN/u8.MAX/etc. эмитятся
корректно через numeric_type_constant_mapping helper в emit_c.rs.
~30 mappings: signed/unsigned int 8/16/32/64, char, f32/f64.

Was simplification: all numeric constants undefined.
Now: 30 mappings работают, 18 sub-тестов PASS.

Open:
- int.BITS / i64.BITS (Rust convention) — не в spec D26, отложено.
- Custom type constants на user types (D41 territory) — отдельная фича.


---

## Iter[T] D58 partial FIXED (Plan 39 Issue D, 2026-05-12)

Plan 39 Issue D closed D58 algorithm gaps в emit_for:
- Diagnostic clarity (explicit listing searched methods + hint).
- mut-receiver enforcement (assert next() — instance method).
- Auto-iter() insertion (Case 2) verified working.

Was simplification: generic error, no mut-check, partial D58.
Now: explicit D58, clear errors, 5 тестов PASS.

Open:
- Plan 39 Issue A: handler-flow infer для with Fail[E] = ... interrupt
  None { Some(...) } — r infers nova_int. Requires handler return-type
  inference work.
- Plan 39 Issue B/C — pending after A.
- Cross-file Iter resolution через Plan 35 inline expansion работает
  для nova build, не для nova test (test_runner отдельный pipeline).

Priority P2 для Issue A.


---

## Unified compile pipeline ✅ FIXED (Plan 35 R31, 2026-05-12)

### Был simplification (Plan 35 Ф.1 MVP earlier)

`resolve_imports_inline` только в `cmd_build`. `nova test` cross-file
не работал (отдельный test_runner pipeline). Workaround:
explicit inline-decl типов вместо import.

### Now FIXED

Plan 35 R31 (commit 3a759bfad5): extract resolve_imports_inline в
nova_codegen::imports — shared между cmd_check, cmd_build, test_runner.
Workspace-aware find_repo_root_from (ищет nova.toml с [workspace]
маркером — D78 AD6 fix для 4 nested nova.toml в repo).

Verified: nova test с `import std.collections.range` + `(0..10).step_by(2)` PASS.

### Open

- Workspace-aware finder в **nova-cli::find_repo_root** не sync'нут с
  test_runner version. nova-cli ищет первый nova.toml (legacy). Это
  работает но возможны edge cases. Sub-plan 35.B unified
  ManifestResolver — full AD6 (4 nested nova.toml) cleanup.


---

## NovaInterruptFrame nova_int slot ✅ FIXED (Plan 39 Issue A, 2026-05-12)

### Был simplification (bootstrap MVP)

NovaInterruptFrame использовал единственный `nova_int value` slot.
emit_with объявлял `nova_int result_tmp` всегда. Non-int trail values
(pointers, NovaOpt structs) дискардились через `(void)(trail); result_tmp
= 0LL;`. Из-за этого `let r = with Fail = |e| interrupt None { Some(x) }`
получал `r: nova_int` вместо `Option[X]`.

### Now FIXED

NovaInterruptFrame расширен `value_ptr` slot. codegen emit_with
категоризует trail type (IntLike/Pointer/ValueStruct/UnitVoid),
объявляет result_tmp с правильным C-типом, читает из соответствующего
slot. ExprKind::Interrupt эмитит `nova_interrupt` или
`nova_interrupt_ptr` по категории. handler-walker определяет тип W
когда body=throw (нет trailing).

Auto-gen Option helpers (eq, is_some, is_none, unwrap_or) для всех
NovaOpt_<T> вместо handcrafted в array.h для int/str.

### Open

- **Type-checker bidirectional inference** остаётся pull-based.
  Codegen-side fix достаточен но архитектурно type-checker должен
  иметь expected-type flow. Multi-session work.
- **Multiple effects в одном `with`** с разными IRT — пока не
  enforced (требуют lub). Single-effect case работает.
- **Returned handler** (`let h = make(); with E = h { ... }`) — нужен
  тип `Handler[E, IRT]` в type-checker'е (currently inline only).

---

## Plan 33.3 Ф.9: bootstrap improvements (2026-05-12)

### [ЗАКР] V2 Loop invariants
- **Закрыто:** parse_loop_clauses возвращает invariants caller'у;
  inject_loop_invariants prepend'ит каждый invariant как
  Stmt::AssertStatic в начало body. Runtime check работает в debug.
- **Не закрыто полностью:** pre-entry check (invariant true перед
  first iteration) — invariant injected после первой итерации.
  Полный havoc-based SMT verify ждёт Z3 backend.

### [ЗАКР] V5 Ghost erasure
- **Закрыто:** Stmt::Let с is_ghost=true НЕ emit'ится ни в codegen
  emit_c.rs, ни в interp. Verus/Dafny semantics.
- **Non-ghost код не может читать ghost-vars** — catch'ится на C-level
  (undefined identifier). Proper compile-time check в type-checker —
  отдельная задача (TODO для Plan 33.3 full).


---

## "gc" namespace в builtin list — Plan 34 follow-up (2026-05-12)

**Где:** compiler-codegen/src/types/mod.rs:1470 — builtin name list
содержит `"gc"`.

**Что упрощено:** `gc.heap_size()` / `gc.collect()` и т.д. в любом
.nv файле резолвятся через **builtin shortcut**, а не через cross-file
import из `std.runtime.gc`. Это **двойной source of truth**:
1. std/runtime/gc.nv — `export external fn gc.heap_size() -> int` (etc.)
2. types/mod.rs — builtin name "gc" в name resolver.

**Почему:** Cross-file bare-name resolution не работает (Plan 35 Ф.1
territory). При попытке убрать `"gc"` из builtin list — все callers
(`let h = gc.heap_size()` в user code) падают с `undefined identifier
'gc'`, потому что нужен `import std.runtime.gc as gc` который никто
не пишет (и wildcard `import std.runtime.gc.*` парсер не принимает).

Альтернативы рассматривал:
1. **Заставить пользователя писать `import std.runtime.gc as gc`** —
   удваивает boilerplate для каждого callsite (Go-стиль `runtime.GC()`,
   Java `System.gc()` — там это implicit). Хуже UX.
2. **Plan 35 Ф.1 wildcard `import X.Y.*` + bare-name visibility** —
   правильное решение, но spec-level work (~150 строк parser + name
   resolver + D-блок про import semantics).
3. **Type-form `Gc.heap_size()` (PascalCase)** — отход от Plan 32 spec
   (lowercase `gc` намеренно для consistency с Go/Python/Java
   namespace style).

**Текущий compromise:** double source of truth с явными synchronization
comments в обоих местах. Codegen dispatch (emit_c.rs:7155) — третье
место, тоже синхронизировано вручную.

**Как починить:** Plan 35 Ф.1 — добавить wildcard `import` + open
bare-names при resolve. После этого:
1. Удалить `"gc"` из builtin list в types/mod.rs.
2. Callers пишут `import std.runtime.gc.*` (или implicit prelude
   для `std.runtime.*`).
3. gc.nv `external fn`-declarations становятся **единственным**
   source of truth для type-checker'а.
4. Codegen dispatch остаётся (special-case как panic/exit) — это
   semantic dispatch, не name resolution.

**Приоритет:** P2 — текущий compromise работает корректно, double
source of truth manually synced. Cleanup-приоритет, не функциональный.


---

## Каналы: thread-safety и select-wake race — Plan 40 (2026-05-12)

**Где:** `compiler-codegen/nova_rt/channels.h` (610 строк) —
весь channel-стек после Plan 30/31.

**Что упрощено:**

1. **Нет atomics на shared state.** `Nova_ChannelState` fields
   (`writer_count`, `closed`, `count`, `head`, waiter-lists) — обычные
   `int32_t` / `bool` / pointer. Все операции (send/recv/close/clone)
   non-atomic. Под single-thread runtime'ом работает корректно;
   под M:N (Plan 23) → data race на любой operation.

2. **Select wake — non-race-free retry.** `nova_select_park`
   (channels.h:558) после `nova_sched_park` делает `try_immediate` retry
   для определения winning arm. Channel wake helper (`_nova_channel_wake_recv`)
   уже **извлёк** значение в момент wake. Между wake и retry на M:N
   другой fiber-thread может его выхватить → `ctx->which = -1`,
   silent select dispatch failure. Go использует `selectdone` CAS на
   waiter'е (mark, не commit) — мы упростили до single-thread assumption.

3. **NOVA_SELECT_MAX_ARMS=16 silent skip.** `nova_select_set_recv/send`
   на `n >= 16` делает `return` без diagnostic (channels.h:375, 385).
   Parser и codegen `emit_select` не валидируют. Результат: `select`
   с 17+ arms → silent зависание (17-я arm не зарегистрирована, select
   ждёт ready на не-зарегистрированной arm).

4. **O(n) waiter unlink.** Singly-linked waiter lists
   (`_nova_channel_waiter_unlink`, `_nova_sel_waiter_unlink`). Каждый
   select-park регистрирует N waiter'ов; после wake unlink'ает N — это
   **O(N²)** на каждый dispatch. Go/Rust используют doubly-linked для O(1).

5. **Time.after timer cleanup.** Если select выиграл не по
   `Time.after`-arm'у, timer всё равно сработает и `try_send` в discarded
   канал. NovaAfterState heap-allocated, GC под Boehm видит указатель
   через `timer.data`, под malloc-only fallback — leak до конца программы.

6. **Time.after per-call allocations.** Каждый `Time.after(ms)` =
   ~6 nova_alloc'ов (ChannelPair + state + buf + tx + rx + NovaAfterState).
   Tokio достигает 0-alloc через poll-state без channel — для нас post-1.0.

7. **Channel.new capacity check после allocate.** `nova_channel_new`
   alloc'ает state+buf, **потом** throws на `capacity <= 0`. GC eventually
   соберёт. Косметика.

8. **Нет recv_many batch API.** Tokio 1.32+ recv_many. Не блокер.

**Почему:** Plan 30/31 закрывались под single-thread runtime —
формальная корректность для M:N не закладывалась. Post-close review
(Plan 30 Ф.4) закрыл 4 дефекта (Б1/Б2/Н1/Н2), оставил T1/T2/T3 как
«tech debt», но недооценил критичность T1 (atomics) — это **blocker**,
а не tech debt: Plan 23 не запустится без него.

**Как починить:** Plan 40 разбивает на три фазы:
- Ф.1 (P1, prerequisite для Plan 23): atomics+mutex, race-free wake
  (selectdone CAS), doubly-linked waiters, arm-count diagnostic.
- Ф.2 (P2): Time.after cleanup + pool.
- Ф.3 (P3): capacity check ordering, spec D94 sync, recv_many.

**Mutex vs lock-free:** план рекомендует mutex (Rust mpsc parity).
crossbeam-уровень lock-free — post-1.0 (5+ лет работы над формальной
верификацией, не успеем).

**Приоритет:** **P1 для Ф.1** (без неё Plan 23 не работает), **P2/P3**
для остального.

Detail: [docs/plans/40-channel-hardening.md](plans/40-channel-hardening.md).

### [ЗАКР] V2 Loop invariants (полностью, Ф.9.5)
- **Закрыто полностью:** Ф.9.3 inject invariants как assert_static в начало
  body (per-iteration); Ф.9.5 wrap loop-expr в outer Block с pre-entry
  asserts (catches violation до loop). Pre-entry + per-iteration = full
  bootstrap enforce. SMT havoc-based verify (полный Dafny-grade) ждёт Z3.

### [ЗАКР] V5 Ghost erasure (полностью, Ф.9.1 + Ф.9.7)
- **Закрыто полностью:** Ф.9.1 эрейзит ghost в codegen+interp (Verus/Dafny
  semantics). Ф.9.7 добавил proper type-check `check_ghost_usage` —
  non-ghost reading ghost-var выдаёт compile-error с понятным сообщением
  (раньше — undeclared identifier на C-level).


---

## Selective import filter — syntax only в bootstrap (35.A R26, 2026-05-12)

### Был simplification

`import X.Y.{A, B}` синтаксис принят парсером, но **resolver не enforce'ит**
filter — все items имповрта merge'ятся в текущий module.

### Причина

**Transitive dependency closure issue.** Если user пишет
`import std.collections.range.{Range}`, но Range.@step_by возвращает
StepRangeIter — codegen reference'ит StepRangeIter type even though
filter говорит «только Range». Без полного dep-walking (transitive
closure всех referenced types через methods/fields) filter ломал бы
codegen.

### Now compromise

Filter сохраняется в AST.Import.items (syntax-only documentation
намерения программиста). Полный enforcement через type-checker
visibility (видимые имена в module scope) — post-bootstrap.

### Prelude.nv почти пустой (R27, 2026-05-12)

### Был simplification

`std/prelude.nv` существует но содержит только `PRELUDE_VERSION = 1`.

### Причина

Auto-imported items (Option/Result/Some/None/Ok/Err/Error/Never/print/
println/panic) — все hardcoded в type-checker'е и codegen'е через
special cases. Migration этих items в file-based prelude — отдельная
большая работа (refactor type-checker symbol resolution + codegen
emit для prelude items).

### Now compromise

R27 механизм работает (auto-import std.prelude если файл существует);
user'ы могут расширять prelude добавляя items в std/prelude.nv.
Migration hardcoded → file-based — future work.


---

## Каналы: Plan 40 Ф.1 (M:N safety) отложено с Plan 23 (2026-05-12)

**Где:** `compiler-codegen/nova_rt/channels.h`.

**Что упрощено:** Ф.1 пункты Plan 40 (atomics + selectdone CAS +
doubly-linked waiters + per-call storage) **не реализованы** в
сессии Plan 40 Ф.2/Ф.3 implementation. Текущая реализация остаётся
single-thread корректной.

**Почему не сейчас:** без M:N scheduler'а race-condition'ы
непроверяемы. Plan 30 Ф.4 закрылся с непроверяемыми M:N-claim'ами —
повтор anti-pattern'а запрещён.

**Что сделано вместо:** Ф.2 (B7 Time.after cleanup) + Ф.3
(B5 cap diagnostic, B9 capacity-check ordering, B10 spec D94 sync) —
валидируется на single-thread runtime'е.

**Detailed design Ф.1 сохранён в Plan 40** для immediate implementation
в Plan 23 session. Решения: C11 `mtx_t` + `<stdatomic.h>`,
compound-literal storage в emit'е, Go-style selectdone CAS,
doubly-linked waiter list.

**Приоритет:** P1 prerequisite для Plan 23.


---

## Time.after per-call allocs ~6 — Plan 40 B4 (2026-05-12)

**Где:** `Nova_Time_after` в channels.h.

**Что упрощено:** каждый `Time.after(ms)` = Nova_ChannelPair
(state+buf+tx+rx, 4 allocs) + NovaAfterState (1) + libuv timer
heap (1) = ~6 nova_alloc'ов. Tokio = 0-alloc через inline timer
без backing channel'а.

**Почему:** bootstrap channel-based интегрируется с select как
просто recv arm. 0-alloc требует выделенного timeout-syntax (special
casing), что D94 намеренно избежал.

**Влияние:** GC pressure под нагрузкой (HTTP client pool с timeout'ами).
Под Boehm — minor; под malloc-only — leak.

**Как починить:** timer pool в eventloop.h (Plan 22 follow-up).

**Приоритет:** P2.


---

## NOVA_SELECT_MAX_ARMS = 32 hard cap — Plan 40 Ф.3 (2026-05-12)

**Где:** `compiler-codegen/nova_rt/channels.h::NOVA_SELECT_MAX_ARMS` +
`emit_c.rs::emit_select`.

**Что упрощено:** select ограничен 32 channel arms. Overflow →
compile-error «select: too many channel arms (N); maximum is 32».

Раньше cap=16 без diagnostic'а; overflow = silent zero-fill →
select висел вечно. Это bug, не «упрощение». Plan 40 Ф.3 это починил.

**Почему 32:** stack-allocated `SelectCtx.arms[MAX]` требует
compile-time-known cap. Per-call adaptive storage (Plan 40 Ф.1) уберёт
cap полностью. VLA отвергнут (MSVC не поддерживает). Heap-alloc
отвергнут (GC pressure).

**Влияние:** идиоматический Go-код = 3-8 arms; 32 = 4× запас.
Workaround на overflow: nested selects.

**Как починить:** Plan 40 Ф.1 per-call storage.

**Приоритет:** P3 — limit не достигается в нормальном коде; bug
(silent hang) уже исправлен.
