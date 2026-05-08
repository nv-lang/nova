# Plan 02 — Nova C Backend (compiler-codegen)

Компиляция Nova в нативный бинарь через C как промежуточное представление.
Рабочая директория: `compiler-codegen/` — единственный компилятор Nova
(parser/type-checker/interp/codegen в одном crate'е). До 2026-05-08
существовал отдельный `compiler-bootstrap/` (treewalk-only); удалён
после того как codegen поглотил его функциональность.

## Архитектурный принцип: GC за интерфейсом

Весь runtime — отдельная C-библиотека `nova_rt/`. Кодогенератор никогда
не вызывает `malloc` напрямую — только через эти пять функций:

```c
// nova_rt/alloc.h
void* nova_alloc(size_t size);   // выделить объект
void  nova_retain(void* ptr);    // +1 ref (для будущего RC)
void  nova_release(void* ptr);   // -1 ref / уведомить GC
void  nova_gc_init(void);        // инициализация при старте
void  nova_gc_shutdown(void);    // финализация
```

Чтобы сменить GC — меняется только `nova_rt/alloc.c`. Кодогенератор
не меняется вообще.

## Фазы

### Фаза 0 — скаффолдинг
- Добавить модуль `codegen/` в `compiler-codegen/src/` рядом с `interp/`
- Файлы: `codegen/mod.rs`, `codegen/emit_c.rs`, `codegen/runtime.rs`
- Создать `compiler-codegen/nova_rt/`: `alloc.h`, `alloc.c` (первая реализация — `malloc`, без GC)
- CLI флаг `--emit=c` или subcommand `compile`

### Фаза 1 — синхронный subset
Компилировать в C ровно то, что уже работает в интерпретаторе:
- `fn`, `let`, `let mut`, арифметика, `bool`, `str`
- `if/else`, `return`, рекурсия
- `println` → `printf`

**Цель:** `hello.nv`, `arithmetic.nv` компилируются и запускаются.

### Фаза 2 — типы данных
- Records (`type Point { x f64 }`) → C struct + `nova_alloc`
- Sum types (`type Shape | Circle(f64) | ...`) → tagged union
- `match` → switch + if-chain

**Цель:** `records.nv`, `match_demo.nv` компилируются и запускаются.

### Фаза 3 — замена GC *(можно в любой момент после Фазы 1)*
- Заменить `nova_rt/alloc.c`: `malloc` → Boehm `GC_malloc`, или RC со счётчиком в заголовке объекта
- Кодогенератор не меняется

### Фаза 4 — эффекты и `with`
- `Fail` / `?` оператор → `setjmp`/`longjmp` в C
- `with Handler { ... }` → thread-local указатель на handler struct

### Фаза 5 — fiber'ы
- Подключить `minicoro.h` в `nova_rt/` (header-only, кросс-платформенная)
- `spawn` / `parallel` → `mco_create` + scheduler loop

### Фаза 5b — тест-блоки
- `test "name" { body }` → статическая функция + setjmp-runner в nova_fn_main_impl
- `assert(expr)` → `nova_assert(cond, "expr_text")` с текстом условия
- NovaTestFrame в effects.h, _nova_test_frame TLS

### Фаза 6 — массивы
- `NovaArray_T` через NOVA_ARRAY_DECL/IMPL макросы (nova_rt/array.h)
- ArrayLit, spread, `.len`, `.get(i)` → NovaOpt_T
- Паттерны `[]`, `[x, ..]`, `[head, ..rest]` на массивах
- Some(n)/None паттерны на NovaOpt_T (value struct)

## Статус

| Фаза | Статус |
|---|---|
| 0 — скаффолдинг | ✅ готово |
| 1 — синхронный subset | ✅ готово |
| 2 — типы данных | ✅ готово |
| 3 — замена GC | ✅ готово (alloc.c / alloc_rc.c / alloc_boehm.c) |
| 4 — эффекты | ✅ готово (with/handler → thread-local vtable + ctx struct, D61: interrupt, прямой vtable call) |
| 5 — fiber'ы | ✅ готово (minicoro stackful coroutines, spawn { } → nova_fiber_run) |
| 5b — тест-блоки | ✅ готово (test/assert → setjmp-runner, with_tests + gc_coroutines_test) |
| 6 — массивы | ✅ готово (NovaArray_T макросы, spread, .len/.get, array patterns, demo.nv) |

## Следующие шаги

- Generics / монаморфизация (сейчас тип элемента массива = nova_int по умолчанию)
- for-in по массивам (сейчас только Range)
- f-string / строковая интерполяция
- stdlib скелет
