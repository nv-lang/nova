# Ренейм интринсика __cas_raw → cmpxchg + схлопывание перегрузок

## Статус: выполнено (тестирование)

### Что сделано

#### 1. std/src/runtime/sync.nv
- **Ренейм интринсика (13 типов):**
  - Все `extern "nova" fn AtomicX mut @__cas_raw(...)` → `@cmpxchg(...)`
  - Типы: AtomicI64, AtomicI32, AtomicI16, AtomicI8, AtomicU64, AtomicU32, AtomicU16, AtomicU8, AtomicIsize, AtomicUsize, AtomicInt, AtomicBool, AtomicPtr
  - Суммарно: 13 декларций интринсиков переименовано

- **Замена вызовов интринсика:**
  - Все `@__cas_raw(...)` → `@cmpxchg(...)` в методах compare_exchange/compare_exchange_weak
  - Суммарно: 26 вызовов (по 2 на каждый тип, кроме AtomicInt у которого 1)

- **Схлопывание перегрузок (11 типов с MemOrdering):**
  - Удалены 2-арг перегрузки (передавшие SeqCst)
  - Добавлены дефолт-параметры к 4-арг версиям: `success MemOrdering = MemOrdering.SeqCst, failure MemOrdering = MemOrdering.SeqCst`
  - Типы с полным MemOrdering: AtomicI64, AtomicI32, AtomicI16, AtomicI8, AtomicU64, AtomicU32, AtomicU16, AtomicU8, AtomicIsize, AtomicUsize, AtomicBool, AtomicPtr (12 типов)
  - Специальный случай AtomicInt: 1 перегрузка (без MemOrdering), оставлена как есть

- **Обновлены комментарии:**
  - Упоминания `@__cas_raw` → `@cmpxchg` в сор-документации (линия 96)

#### 2. compiler-codegen/nova_rt/sync_primitives.h
- **Ренейм C-функций (13 штук):**
  - Все `Nova_AtomicX_method___cas_raw` → `Nova_AtomicX_method_cmpxchg`
  - Все упоминания `__cas_raw` → `cmpxchg` в комментариях
  - Суммарно: ~15 замен (функции + комментарии)

#### 3. compiler-codegen/src/codegen/emit_c.rs
- **Обновлены комментарии:**
  - `@__cas_raw` → `@cmpxchg` в комментарии рядом с RUNTIME_DEFINED_TYPES

#### 4. Документация (spec + plans)
- **spec/decisions/06-concurrency.md (D425):**
  - Обновлены упоминания `@__cas_raw` → `@cmpxchg` в секции "Лоуэринг (codegen)"
  
- **docs/plans/207-atomic-cas-witnessed-value.md:**
  - Обновлены упоминания `@__cas_raw` → `@cmpxchg` в секции "Итог"

- **docs/plans/207-progress.md:**
  - Обновлены все упоминания `__cas_raw` → `cmpxchg`

- **docs/plans/backlog-followups.md:**
  - Обновлены упоминания `__cas_raw`/`@__cas_raw` → `cmpxchg`

- **docs/plans/README.md:**
  - Обновлены упоминания в описании План 207

### Проверки

✅ Нет хардкодов в компиляторе, требующих пересборки (строковый поиск в emit_c.rs не обнаружил спец-обработки имени метода)

✅ Нет прямых вызовов `__cas_raw` в тестах (`spec_tests`, `sync_test.nv`)

✅ Отсутствие остатков `__cas_raw` — проверено полным поиском по кодовой базе

### Файлы, затронутые правками

1. d:/Sources/nv-lang/nova-cmpxchg/std/src/runtime/sync.nv (13 ренейм интринсиков + 26 замен вызовов + 12 схлопываний перегрузок)
2. d:/Sources/nv-lang/nova-cmpxchg/compiler-codegen/nova_rt/sync_primitives.h (~15 замен функций и комментариев)
3. d:/Sources/nv-lang/nova-cmpxchg/compiler-codegen/src/codegen/emit_c.rs (1 обновление комментария)
4. d:/Sources/nv-lang/nova-cmpxchg/spec/decisions/06-concurrency.md (1 обновление)
5. d:/Sources/nv-lang/nova-cmpxchg/docs/plans/207-atomic-cas-witnessed-value.md (1 обновление)
6. d:/Sources/nv-lang/nova-cmpxchg/docs/plans/207-progress.md (3+ обновления)
7. d:/Sources/nv-lang/nova-cmpxchg/docs/plans/backlog-followups.md (обновления в строке Plan 207)
8. d:/Sources/nv-lang/nova-cmpxchg/docs/plans/README.md (обновление в описании План 207)

### Следующий шаг: верификация

Нужна компиляция и прогон тестов для подтверждения работоспособности.
