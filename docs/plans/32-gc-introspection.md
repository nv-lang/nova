# Plan 32: GC introspection API (`gc.heap_size()` / `gc.collect()`)

> **Статус:** Ф.1 ✅. Ф.2 ✅. Ф.4 ✅. Ф.3 (std/runtime/gc.nv auto-gen) и
> Ф.5 (spec D26 prelude) — добавлены 2026-05-12 audit fix.
> **Создан:** 2026-05-12. **Обновлён:** 2026-05-12.

## Контекст

Текущие GC-тесты (`gc_correctness.nv`, `gc_bench.nv`) проверяют только
**корректность** — что live-объекты не collected преждевременно (sentinels
сохраняются). Они не проверяют **факт сбора мусора**: под `--gc malloc`
память тоже не должна "ломаться" в этих тестах (там просто всё leaks).

Без runtime API нельзя написать differential-тест:
- under `malloc`: heap growth = O(allocations) (линейный leak)
- under `boehm`:  heap growth = O(live set) (bounded)

Plan 27 закрыл выбор backend'а (`--gc malloc|boehm` flag, vcpkg gc.lib
vendored, тесты переключаются), но не дал способа **наблюдать** GC из Nova.

Также пользу даёт: диагностика memory pressure в long-running CLI, явный
flush перед blocking I/O, smoke-тесты для CI ("heap не растёт за N итераций").

Прецеденты:
- Go: `runtime.GC()`, `runtime.ReadMemStats(&ms); ms.HeapAlloc`.
- Java: `System.gc()`, `Runtime.getRuntime().totalMemory()`.
- Python: `gc.collect()`, `gc.get_stats()`.
- .NET: `GC.Collect()`, `GC.GetTotalMemory(false)`.

Все mainstream-runtime'ы экспонируют introspection. Nova следует прецеденту.

---

## API

Namespace: `std.runtime.gc` (не prelude — это runtime introspection, не
core-language). Импорт: `use std.runtime.gc`.

```nova
external fn gc.heap_size() -> int        // bytes; 0 если backend не поддерживает
external fn gc.live_count() -> int       // приблизительное число live-объектов
external fn gc.alloc_count() -> int      // монотонный счётчик аллокаций с старта
external fn gc.collect()                 // принудительный сбор (no-op под malloc)
external fn gc.reset_stats()             // сброс счётчиков (для per-test isolation)
```

**Semantics per backend:**

| API | malloc | boehm |
|---|---|---|
| `heap_size()` | `0` (нет introspection — honest unsupported) | `GC_get_heap_size()` |
| `live_count()` | `alloc_count - free_count` (free_count всегда 0 ⇒ = alloc_count) | `GC_get_heap_size() - GC_get_free_bytes()` приблизительно, или просто `alloc_count` upper bound |
| `alloc_count()` | существующий `_alloc_count` | существующий `_alloc_count` |
| `collect()` | no-op | `GC_gcollect()` |
| `reset_stats()` | zero counters | zero counters (GC stats не сбрасываются — Boehm не поддерживает) |

`heap_size() == 0` под malloc — это **honest sentinel** «не поддерживается».
Тесты должны проверять `if heap_size() > 0 { ... }` или использовать
`ALLOC_REQUIRES boehm`.

---

## Фазы

### Ф.1 — Runtime API (alloc.h + alloc.c + alloc_boehm.c)

`alloc.h`: добавить declarations:
```c
size_t nova_gc_heap_size(void);
void   nova_gc_collect(void);
```

`alloc.c` (malloc backend):
```c
size_t nova_gc_heap_size(void) { return 0; }  /* honest: no introspection */
void   nova_gc_collect(void)   { /* no-op */ }
```

`alloc_boehm.c`:
```c
size_t nova_gc_heap_size(void) { return GC_get_heap_size(); }
void   nova_gc_collect(void)   { GC_gcollect(); }
```

Существующие функции (`alloc_count`, `free_count`, `live_count`, `reset_stats`)
уже экспортированы — реюзаем как есть.

### Ф.2 — Codegen registry

В `compiler-codegen/src/codegen/runtime_registry/` зарегистрировать
функции в namespace `gc` (через новый модуль `gc.rs` или extension к
существующему `runtime.rs` — посмотреть как сделаны `time.sleep`, `random.*`).

Mapping:
- `gc.heap_size()` → `(nova_int)nova_gc_heap_size()`
- `gc.live_count()` → `(nova_int)nova_gc_live_count()`
- `gc.alloc_count()` → `(nova_int)nova_gc_alloc_count()`
- `gc.collect()` → `nova_gc_collect();` (statement, unit-return)
- `gc.reset_stats()` → `nova_gc_reset_stats();` (statement)

### Ф.3 — std/runtime/gc.nv (auto-gen)

Через generator (Plan 13 pattern) сгенерировать `std/runtime/gc.nv` с
external declarations. Получится автоматически из registry.

### Ф.4 — Тесты

`nova_tests/concurrency/gc_introspection.nv`:

1. **Positive под Boehm** (`ALLOC_REQUIRES boehm`):
   - `heap_size()` возвращает > 0 после аллокаций.
   - `collect()` уменьшает (или хотя бы не увеличивает) `heap_size()`
     после массовых короткоживущих аллокаций.
   - `alloc_count()` монотонно растёт.
   - `reset_stats()` обнуляет alloc_count.

2. **Differential test** (любой backend):
   - 100k короткоживущих объектов, `collect()`, проверка:
     - под `boehm`: `heap_size_after < alloc_count * avg_obj_size`
       (sublinear — GC собрал большинство).
     - под `malloc`: тест skipped (heap_size==0).
   - Через runtime check `if heap_size() == 0 { skip via early return }`.

3. **Negative** (`ALLOC_EXCLUDES boehm`):
   - Под malloc `heap_size() == 0` (sentinel).
   - Под malloc `collect()` no-op (не падает).

### Ф.5 — Docs

- `docs/project-creation.txt` — запись об API.
- `docs/simplifications.md` — отметить что `heap_size==0` под malloc это
  intentional sentinel.
- `spec/decisions/08-runtime.md` D26 prelude — добавить упоминание
  `std.runtime.gc` namespace.

---

## Acceptance criteria

- `nova_tests/concurrency/gc_introspection.nv` PASS под `--gc boehm`.
- Differential-тест демонстрирует bounded heap под boehm vs unbounded под malloc.
- `gc.collect()` вызов из Nova-кода реально вызывает Boehm sweep
  (measurable через `heap_size()` до/после).
- Существующие 177/177 тестов не сломаны.

---

## Что НЕ входит

- Finalizers (`runtime.SetFinalizer` style) — отдельная задача, требует
  weak-references-инфраструктуру.
- Per-allocation tracking (heap profiler) — отдельный план, нужен
  Boehm `GC_register_finalizer` hookup.
- `GC.MemoryInfo` rich struct (gen-0/1/2 counts) — у нас нет generational
  GC, single-gen Boehm.

---

## Риски

- Boehm `GC_get_heap_size()` под Windows static link может требовать
  `GC_THREADS` macro — проверить. Сейчас single-threaded, должно работать
  без.
- `gc.collect()` тяжёлая операция (full mark-sweep) — может ломать sleep
  precision тесты если кто-то добавит в них. Документировать в spec'е что
  это manual API, не использовать в hot path.
