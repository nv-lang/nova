# Spec request: D80 — Handler scoping per-fiber

Документ — запрос компиляторного агента на добавление архитектурного
правила в spec'у: handler-binding'и от `with X = handler { body }`
**scoped per-fiber, не shared между fibers**.

---

## Контекст

Я нашёл и исправил критический bug в bootstrap-runtime: `with`-handler'ы
до этого хранились в `__declspec(thread)` global'ах, что **нарушало
изоляцию между fibers** на одном OS-thread (D71 single-threaded
cooperative).

### Voids invariant

```nova
fn helper_with_clock_100() -> int {
    with Time = handler Time { sleep(_) {return ()} now() {return 100} } {
        Time.now()  // должен вернуть 100
    }
}

fn helper_with_clock_200() -> int {
    with Time = handler Time { sleep(_) {return ()} now() {return 200} } {
        Time.now()  // должен вернуть 200
    }
}

supervised {
    spawn { let a = helper_with_clock_100() }
    spawn { let b = helper_with_clock_200() }
}
```

До fix — fiber A's `Time.now()` мог вернуть **200** если scheduler
переключился между его `with`-push и `Time.now()` call. Это **тихий
data corruption**.

### Реализация

Добавлено в `compiler-codegen/nova_rt/effects.h`:

```c
/* Registry of TLS handler-storage addresses */
typedef struct {
    void** slots[NOVA_MAX_EFFECT_STORAGES];   /* registered TLS addresses */
    int    count;
} NovaEffectRegistry;
extern NovaEffectRegistry _nova_effect_registry;

void nova_register_effect_storage(void** slot_addr);

typedef struct {
    void* values[NOVA_MAX_EFFECT_STORAGES];
} NovaEffectSnapshot;

void nova_effect_snapshot_save(NovaEffectSnapshot* snap);
void nova_effect_snapshot_restore(const NovaEffectSnapshot* snap);
```

В `fibers.h` `nova_supervised_step` save/restore snapshot вокруг
`mco_resume`. Каждый fiber имеет heap-allocated `NovaEffectSnapshot*`
в `NovaFiberQueue::fiber_effect_snapshot[]`. Inheritance: при
`nova_fiber_spawn_into` snapshot **копируется from current TLS**
(structured-concurrency наследование).

Тесты: `tests-nova/concurrency/per_fiber_handlers.nv` — 4 теста
(per-fiber different handlers, outer→inner inheritance, inner
override outer, outer survives spawn-завершения).

62/62 tests-nova passing.

---

## Запрос: добавить D80 в spec/decisions/06-concurrency.md

### D80 — Handler scoping per-fiber

#### Что
`with X = handler { body }` устанавливает binding `X = handler`
**только** для текущего fiber'а (D14). Другие fibers, выполняющиеся
concurrent на том же OS-thread (D71 cooperative scheduler) или
разных OS-threads (D14 production multithreaded), **не видят** этот
binding.

При `spawn`/`parallel for`/`supervised`-spawn новый fiber **наследует**
текущий handler-stack (snapshot of all TLS handler-pointers). Изменения
handler'ов внутри fiber'а (через дополнительные `with`-блоки) видны
только этому fiber'у.

#### Правило

**Семантика:**
1. Каждый fiber имеет собственный snapshot handler-pointers для всех
   эффектов.
2. При `nova_supervised_step` resume fiber'а: TLS restored из
   fiber's snapshot.
3. После yield/return: TLS saved обратно в fiber's snapshot.
4. Restore TLS к outer-flow state (state до step'а).
5. `spawn`/новый fiber inherits current TLS как initial snapshot —
   structured concurrency.

**Грамматика без изменений** — это runtime-инвариант, не язык.

#### Пример (corrected behavior)

```nova
fn use_clock_100() -> int {
    with Time = handler Time { sleep(_) => () now() => 100 } {
        Time.now()  // ВСЕГДА 100, независимо от других fibers
    }
}

fn use_clock_200() -> int {
    with Time = handler Time { sleep(_) => () now() => 200 } {
        Time.now()  // ВСЕГДА 200
    }
}

supervised {
    spawn { let a = use_clock_100() }   // a = 100, гарантированно
    spawn { let b = use_clock_200() }   // b = 200, гарантированно
}
```

**Inheritance example:**

```nova
with Time = handler Time { ... now() => 42 } {
    supervised {
        spawn {
            // inherits outer Time handler
            assert(Time.now() == 42)

            with Time = handler Time { ... now() => 999 } {
                // inner override visible only here
                assert(Time.now() == 999)
            }

            // outer restored
            assert(Time.now() == 42)
        }
    }
}
```

#### Почему

1. **Корректность.** Без per-fiber scoping fiber A's handler может
   быть перезаписан fiber B на shared TLS-globals. Тихий data
   corruption — наихудший класс багов в concurrent коде.

2. **D14 invariant.** «Невидимая infra-инфраструктура fiber-runtime'а»
   подразумевает, что fibers логически независимы. Shared mutable
   state — нарушение.

3. **AI-friendly.** LLM генерирует код по логической модели «каждый
   spawn — независимый поток вычисления». Без per-fiber scoping
   эта модель ломается на handler'ах — confusing.

4. **Прецеденты:**
   - **OCaml 5 effect handlers** — handler scope follows fiber-tree.
   - **Koka effect handlers** — same.
   - **Rust `tokio::task_local!`** — explicit per-task storage с
     parent inheritance.

#### Что отвергнуто

- **Shared TLS handlers** (старая bootstrap-семантика). Causes data
  corruption.
- **Explicit handler passing** (через параметры). Нарушает D62
  «handler — implicit через with-scope».
- **Copy-on-write snapshot.** Premature optimization; bootstrap
  использует eager save/restore, ~µs overhead per resume.

#### Цена

- **Memory:** sizeof(NovaEffectSnapshot) ≈ NOVA_MAX_EFFECT_STORAGES
  pointers per fiber. В bootstrap'е 32 × 8 = 256 байт. Heap-allocated
  чтобы не overflow'ить fiber stack.
- **CPU:** save/restore — N memcpy-equivalent (N = registered effects).
  В bootstrap'е N ≤ 5 (Fail, Time, + user-defined). Production может
  использовать lazy/COW snapshots.
- **Registration:** caller (codegen) должен registrировать каждый
  handler-storage в `nova_register_effect_storage` при старте.

#### Implementation invariant: handler-storage **не static**

**Все** handler-storage переменные (`_nova_handler_X` для каждого
эффекта `X`) **должны** быть emit'нуты с external linkage:

```c
__declspec(thread) NovaVtable_X* _nova_handler_X = NULL;   // ✓ correct
```

**НЕ** так:

```c
__declspec(thread) static NovaVtable_X* _nova_handler_X = NULL;   // ✗ WRONG
```

`static` ограничивает visibility одним translation unit (TU). Это
ломает D80, потому что:

1. **Registry в другом TU.** `nova_register_effect_storage(&_nova_handler_X)`
   вызывается из main wrapper. Если storage `static` в module-TU, а
   registry в effects.c — registry не сможет хранить адрес (формально
   адрес можно взять, но это implementation-defined и неportable).

2. **Production multi-module compilation.** При разделении проекта на
   multiple `.c` файлов user-defined effect объявленный в module A
   может использоваться в module B (через `import`). Storage должен
   быть extern-видимым.

3. **Snapshot save/restore через `void**`.** Registry хранит `void**`
   (адрес slot'а). Доступ через TLS-pointer работает корректно если
   storage extern; со `static` strict aliasing rules могут быть
   нарушены при cross-TU access.

**Правило:** codegen эмитит handler-storage **без `static`** —
default external linkage с TLS-storage class. Built-in эффекты
(Fail, Time, Mem) в runtime'е (`effects.c`) уже следуют этому
правилу.

#### Test protection

`tests-nova/concurrency/per_fiber_handlers.nv` — 4 теста на per-fiber
scoping. Они защищают invariant: если storage возвратится в `static`
(или snapshot save/restore сломается), эти тесты упадут с data
corruption (handler одного fiber'а вернёт значение от другого).

#### Связь
- D14 (Fiber runtime — невидимая infra) — D80 уточняет, что **handlers
  входят в "невидимую инфра"** (per-fiber state).
- D50 (spawn/detach/Blocking) — naturally extends к handler scoping.
- D61 (effect handlers) — D80 — runtime-impl invariant; semantics
  уже подразумевалась в D61.
- D75 (cancel_scope) — same per-scope state pattern.

---

## Просьба

Включить D80 в `spec/decisions/06-concurrency.md`. Это **архитектурное
правило** (как cancel_scope — D75 — runtime-инвариант, выраженный в
spec'е). Без явного D-decision программист/LLM может ожидать
shared-handler семантику (как Java ThreadLocal) и получить разрыв
между моделью и реализацией.

Bootstrap уже реализован, тесты проходят (62/62). Spec догоняет.

—

— компиляторный агент, 2026-05-07
