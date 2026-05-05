# Упрощения и отложенные доработки

Живой список осознанных упрощений, сделанных в ходе разработки.
Каждое упрощение попадает сюда в момент принятия решения — чтобы не потерять контекст.

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

### [C6] Generics — не реализованы (mangle как Nova_Name)
- **Где:** `emit_c.rs` → `type_ref_to_c`
- **Что упрощено:** Generic параметры игнорируются, тип генерируется как `Nova_Name*` без специализации.
- **Почему:** Монаморфизация требует отдельного прохода и отслеживания специализаций.
- **Как чинить:** Прохождение по всем call sites, сбор конкретных типов, повторная генерация типа и функций для каждой специализации.
- **Приоритет:** H

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

### [R1] Аллокатор — malloc без free (по умолчанию)
- **Где:** `nova_rt/alloc.c`
- **Что упрощено:** `nova_alloc` → malloc, `nova_release` → no-op. Нет GC. Память течёт.
- **Почему:** Для прототипирования достаточно. Boehm GC доступен через `gc=boehm`.
- **Как чинить:** Включить RC (`gc=rc`) или Boehm GC (`gc=boehm`) через build_c.bat.
- **Приоритет:** L (Boehm GC уже есть как опция)

### [R2] Fibers — нет structured concurrency (supervised/race/parallel)
- **Где:** `nova_rt/fibers.c` / `emit_c.rs`
- **Что упрощено:** `spawn { }` создаёт fiber и сразу запускает до завершения. Нет параллельного выполнения, нет `supervised`, `race`, `parallel for`.
- **Почему:** Structured concurrency требует scheduler. Миникоро — stackful coroutines, не OS threads.
- **Как чинить:** Добавить scheduler loop, async I/O integration, или перейти на OS threads.
- **Приоритет:** M

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
