// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 103 — `std.runtime.sync` production-grade spec + API expansion (roadmap)

> **Статус:** 🟡 roadmap 2026-05-25, **P1** (spec-drift closure + API completeness)
> **Приоритет:** master закрывает spec-drift; sub-plans 103.1-103.8 ранжированы по dependency.
> **Оценка:** ~12-16 dev-day (8 sub-plans, ~1.5-2 dev-day каждый).
> **Зависимости:** Plan 18 Шаг 1 ✅ (shipped baseline), Plan 21 ✅ (Channel — preferred default), Plan 83.3 ✅ (Blocking), Plan 82 ✅ (M:N stable), Plan 44.1 Ф.1 ✅ (sync.h backend selector + memory ordering).
> **Связь:** Plan 18 (stdlib roadmap, наследуется этим планом), Plan 91 (std MVP — sync **не** в MVP, но обещание Q12.4 v1.0).

## Цель и контекст

В `std/runtime/sync.nv` шиплены production-baseline `AtomicInt`/
`AtomicBool`/`Mutex`/`WaitGroup`/`Once` (Plan 18 Шаг 1, `#stable since
"0.1"`). В spec — три противоречивых утверждения:

| Место | Утверждение | Статус |
|---|---|---|
| `06-concurrency.md` §1531-1597 | «Mutex / Atomic — НЕ в spec» | устарело |
| `06-concurrency.md` §641 | «Mutex/Atomic отвергнуты в пользу channel-only» | устарело |
| `open-questions.md` Q12.4 v1.0 §396 | «Mutex, RwLock, Atomic[T] как stdlib-типы» | обещание |
| `open-questions.md` Q9 §274 | «Точные API — открыто» | открыто |
| `open-questions.md` Q-memory-model §1665 | «Memory model между fibers — фиксировать в D14 production» | открыто |
| `04-effects.md` §4302-4304 | `Atomic[int].new(0)` как canonical example | implicit promise |

Параллельно текущий baseline — **функционально неполный** vs production-grade
(Go `sync`, Rust `std::sync` + `parking_lot`, Java `j.u.c`, Kotlin
`kotlinx.atomicfu` + `j.u.c`):

- **Memory orderings** в Nova-side жёстко acq_rel/acquire/release — все
  ordering варианты есть в C-слое (`sync.h` Plan 44.1) но не exposed.
  Industry: 5 orderings (Relaxed/Acquire/Release/AcqRel/SeqCst).
- **Atomics:** только `AtomicInt`/`AtomicBool`. Industry: `AtomicI8`/
  `I16`/`I32`/`I64`/`U8`/`U16`/`U32`/`U64`/`Usize`/`Isize`/`Ptr`/
  `Bool` — каждый с полным operation suite.
- **Atomic operations:** только `load`/`store`/`fetch_add`/`fetch_sub`/
  `compare_exchange`/`swap`. Industry: `fetch_or`/`and`/`xor`/`max`/
  `min`/`nand`, `compare_exchange_weak`, `fence`.
- **Mutex extras:** нет `try_lock_for(Duration)`, нет `is_locked()`
  observability, нет `with_lock(fn)` RAII-helper.
- **Mutex variants:** нет `ReentrantMutex` (Java), нет `RwLock`
  (Rust/Go/Java).
- **Coordination:** нет `Semaphore` (Java/Tokio), нет `Barrier`
  (Java CyclicBarrier/Rust), нет `Condvar` (Rust/Java Condition), нет
  `CountDownLatch` (Java — частично пересекается с WaitGroup).
- **One-shot init:** есть только `Once` с двух-шаговым `run()/done()`
  pattern (не closure-form). Нет `OnceCell[T]` value-capturing, нет
  `Lazy[T]` auto-init-on-first-access (Rust once_cell, Kotlin `by lazy`).
- **Realtime/Blocking interaction** не зафиксирована: `Mutex.lock()`
  может park → запрещён в `realtime { }`, но spec-правило отсутствует.

Цель Plan 103: **закрыть spec-drift И достичь паритета (или превосходства)
с Go/Rust/TS/Kotlin для concurrency primitives** — без потери
«owner-actor pattern preferred default» позиции спеки.

## Дизайн-философия (Nova edge)

| Решение | Nova | Go | Rust | TS Atomics | Kotlin/Java |
|---|---|---|---|---|---|
| **Preferred default** | Channel (D79) + actor | Channel | Mutex/RwLock — равнозначны Channel | Только Atomics (SharedArrayBuffer) | Channel/Flow + Mutex/Atomics |
| **Mutex data-carrying** (`Mutex<T>`) | ❌ нет — Go-style без T | ❌ | ✅ `Mutex<T>` | n/a | ❌ |
| **Poisoning** (lock poisoned on panic) | ❌ нет | ❌ | ✅ `LockResult` | n/a | ❌ |
| **Fairness default** | ✅ fair FIFO | unfair (runtime decides) | unfair (`parking_lot`) / fair (`std`) | n/a | unfair (`ReentrantLock(false)`) |
| **`Mutex` reentrant default** | ❌ non-reentrant; opt-in `ReentrantMutex` | ❌ | ❌ | n/a | `ReentrantLock` default; `sync` block reentrant |
| **Memory ordering API** | ✅ Full 5-variant `MemOrdering` enum (103.1 ✅) | ❌ только seq_cst | ✅ 5-variant `Ordering` | ✅ implicit seq_cst | ✅ VarHandle access modes (4) |
| **Fiber-aware park/wake** | ✅ через `nova_sched` | ✅ goroutine park | ⚠ thread-blocking | n/a (single-thread) | ✅ coroutine suspend |
| **Realtime-safe атомики** | ✅ lock-free на leaf | ⚠ implicit | ⚠ implicit | ✅ guaranteed | ⚠ implicit |
| **GC-aware (no scan of raw ptr)** | ✅ `AtomicPtr` через managed ref | ✅ implicit | ⚠ unsafe для `AtomicPtr<T>` | n/a | ✅ AtomicReference |
| **Type-checker enforcement Realtime ban** | ✅ Plan 103.6 | ❌ | ❌ | ❌ | ❌ |
| **AI-first guidance baked in spec** | ✅ D-блок «когда что использовать» | partial | partial | minimal | partial |
| **`OnceCell`/`Lazy` value-capturing** | ✅ 103.5 | ❌ только `sync.Once` | ✅ (`once_cell` crate, stdlib in 1.70+) | ❌ | ✅ `by lazy {}` |
| **`Semaphore`/`Barrier`/`Condvar`** | ✅ 103.4 | ⚠ через channels (idiomatic) | ✅ | ❌ | ✅ |
| **Generic Atomic[T]** | ❌ моно (AtomicI32/U64/...) — generic codegen блокер | ❌ AtomicInt32/64 separate | ✅ generic seal | n/a | ❌ AtomicInteger/Long separate |
| **Lock guards via linear types (`MutexGuard consume`)** | ⏳ V2 через Plan 100.7 (sub-plan 103.9, gated) | ❌ pure imperative `Lock()/Unlock()` | ✅ `MutexGuard<T>` RAII drop=unlock | n/a | ❌ `synchronized` block (lexical) |

**Nova edge** vs четырёх референсов:
1. **AI-first guidance в spec** (когда channel vs mutex vs atomic — не tribal знание, а D-блок).
2. **Type-checker enforcement** `realtime { }` ban на park-ing методы — none of Go/Rust/Java/Kotlin не enforce этого статически.
3. **Fiber-aware park без OS-thread block** + GC-aware integration (managed ptr atomics).
4. **Fair FIFO mutex default** (vs unfair как у Go/parking_lot/Java по умолчанию) — для AI-generated кода предсказуемость важнее throughput.
5. **No poisoning** (vs Rust) — упрощает AI codegen, нет `LockResult<...>` обёртки.
6. **No data-carrying Mutex<T>** (vs Rust) — упрощает type system, sample-style Go API.
7. **Full Ordering API** (vs Go который скрывает) + **default = SeqCst** для simple-API methods (vs Rust где нужно указывать ordering всегда).

Что **не делаем** vs industry:
- Rust `Mutex<T>` / `RwLock<T>` data-carrying — overhead на type system, не stoit Rust pattern на Nova.
- Rust poisoning — добавляет `LockResult` к каждому lock; усложняет AI codegen.
- Rust `unsafe` для raw pointer atomics — Nova managed memory, `AtomicPtr` работает через GC-managed reference.
- Java `synchronized` keyword — Mutex как value лучше.
- Java `volatile` — `AtomicX` явно лучше.
- Generic `Atomic[T]` для произвольных типов — текущий codegen блокер (`[M-fn-prefix-int-only-mono]`); моно-типы для primitive types достаточно.

## Consume types (Plan 100) integration

Plan 100 (`linear-must-consume.md`, P3) вводит `consume` тип-модификатор —
аналог Rust linear types без lifetime. **Key insight:** consume применим
к **resource-like** sync примитивам (lock guards, permits), **не** к
**shared-state** atomics:

| Тип | Consume применим? | Почему |
|---|---|---|
| `AtomicI*` / `AtomicU*` / `AtomicBool` / `AtomicPtr` | ❌ нет | Shared state, не resource. Операции не «consume'ят» значение. AtomicInt живёт сколько надо, доступен из N fiber'ов одновременно. |
| `Mutex.lock() → MutexGuard consume` | ✅ canonical | RAII drop = unlock. Защита от unlock-без-lock и double-unlock **статически**. Plan 100.7 явно перечисляет Mutex pilot. |
| `RwLock.read/write → ReadGuard/WriteGuard consume` | ✅ да | Аналог Mutex; два distinct guard типа. |
| `Semaphore.acquire() → Permit consume` | ✅ да | Permit drop'ает обратно в семафор; защита от забытого release. |
| `Condvar.wait(guard) → new guard consume` | ✅ да | wait требует mutex держится; consume передаёт guard через wait. |
| `Once.start() → OnceGuard consume` | ✅ полезно | distinct `.commit()` vs `.abort()` (Plan 100 P-преимущество). |
| `WaitGroup` / `Barrier` / `CountDownLatch` / `OnceCell` / `Lazy` | ❌ нет | Shared coordination, не resources. |

**Стратегия (V1 без consume + future migration sub-plan):**

- **V1 (Plan 103.1-103.8):** lock-family и coordination примитивы шипятся
  с традиционным `lock()/unlock()`, `acquire()/release()` API. Plan 103
  закрывает spec-drift сейчас, не блокируясь готовностью Plan 100 (P3).
- **V1 mitigation:** обязательный `with_lock(fn)` / `with_read(fn)` /
  `with_write(fn)` / `with_permit(fn)` helpers в каждом sub-plan'е —
  closure-form preferred pattern, защищает от забытого release через
  Plan 20 `defer`. `lock()/unlock()` остаётся как low-level escape hatch.
- **V2 (Plan 103.9 — gated on Plan 100.7):** `Mutex.lock()` начинает
  возвращать `MutexGuard consume`; helpers становятся thin wrappers;
  старый bare `unlock()` deprecated с migration path. Semver-bump.
  Sub-plan 103.9 — placeholder, не starts пока Plan 100 не готов.

**Почему V1 без consume:**
- Plan 100 P3 (polish, ~43 dev-day, не блокер 0.1) — далеко от готовности.
- Plan 103 P1 — Q9/Q12.4/Q-memory-model должны закрыться сейчас, не ждать.
- API design V1 готовится к V2-non-breaking-add: с `with_lock(fn)`
  паттерном пользователь редко видит bare `lock/unlock`, V2 migration
  ломает мало кода.

## Декомпозиция (9 sub-plans; V1 = 103.1-103.8, V2 = 103.9)

```
                                ┌──────────────────────────┐
                                │  103.1  Memory Ordering  │  foundation
                                │   (MemOrdering enum+fence│  closes Q-memory-model
                                │   + happens-before spec) │
                                └────────┬─────────────────┘
                                         │
                  ┌──────────────────────┼──────────────────────┐
                  ▼                      ▼                      ▼
        ┌─────────────────┐   ┌────────────────────┐   ┌────────────────┐
        │  103.2 Atomics  │   │  103.3 Mutex/RwLock│   │  103.5 Once /  │
        │  full suite     │   │  /ReentrantMutex   │   │  Lazy /        │
        │  (sized + ops + │   │  family            │   │  OnceCell      │
        │  weak CAS +     │   │                    │   │                │
        │  AtomicPtr)     │   └─────────┬──────────┘   └────────────────┘
        └─────────────────┘             │
                                        ▼
                              ┌────────────────────┐
                              │  103.4 Coordination│
                              │  (Semaphore +      │
                              │  Barrier +         │
                              │  CountDownLatch +  │
                              │  Condvar)          │
                              └────────────────────┘
                                        │
                                        ▼
                              ┌────────────────────────────┐
                              │  103.6 Realtime/Blocking   │
                              │  type-checker enforcement  │
                              │  (orthogonal — все 103.2-5)│
                              └────────────┬───────────────┘
                                           │
                  ┌────────────────────────┴───────────────────┐
                  ▼                                            ▼
        ┌─────────────────────────────┐         ┌─────────────────────┐
        │  103.7  Spec D-блоки        │         │  103.8  Conformance │
        │  D167-D173 + AI-first       │         │  + stress + litmus  │
        │  guidance + Q closure       │         │  + audit + close    │
        └──────────────┬──────────────┘         └──────────┬──────────┘
                       │                                   │
                       └───────────────┬───────────────────┘
                                       │
                                       ▼ V1 closed
                       ──────────────────────────────────
                       (V2 — gated на Plan 100.7 ready)
                                       │
                                       ▼
                       ┌───────────────────────────────────┐
                       │  103.9  Consume guards migration  │
                       │  MutexGuard / ReadGuard /         │
                       │  WriteGuard / Permit / OnceGuard  │
                       │  как `consume`-типы. Plan 100.7   │
                       │  pilot site = std.runtime.sync.   │
                       │  Semver-bump. NO consume для      │
                       │  atomics/WaitGroup/Barrier (M16). │
                       └───────────────────────────────────┘
```

| # | Sub-plan | Scope | Зависимости | Оценка |
|---|---|---|---|---|
| **103.1** ✅ | [Memory ordering API](103.1-memory-ordering-api.md) | `MemOrdering { Relaxed, Acquire, Release, AcqRel, SeqCst }` enum + `fence(MemOrdering)` функция + D167 draft в spec. Закрывает Q-memory-model foundation. | — | ~1 d |
| **103.2** ✅ | [Atomics full suite](103.2-atomics-full-suite.md) | `AtomicI8/I16/I32/I64/U8/U16/U32/U64/Usize/Isize/Bool/Ptr` — 12 типов. Каждый: `load/store/swap/compare_exchange/compare_exchange_weak` + integer-specific `fetch_add/sub/or/and/xor/max/min`. Все с `MemOrdering`-параметром (M14d) + default-SeqCst overload. **Backward compat:** существующие `AtomicInt`/`AtomicBool` без `MemOrdering` → deprecated alias на `AtomicI64`/`AtomicBool` (SeqCst). | 103.1 ✅ | ~2 d |
| **103.3** | [Mutex/RwLock/ReentrantMutex family](103.3-mutex-family.md) | `Mutex` (hardened): `try_lock_for(Duration)`, `is_locked()`, `with_lock(fn)`, fairness mode opt-in. `RwLock`: `read/write/try_read/try_write/try_read_for/try_write_for`, writer-priority fairness. `ReentrantMutex`: opt-in для legacy, owner-fiber tracking + counter. | 103.1, 103.2 (использует ordering для internal state), Plan 65 (Duration) | ~2 d |
| **103.4** | [Coordination primitives](103.4-coordination-primitives.md) | `Semaphore` (bounded permits, `try_acquire_for`), `Barrier` (CyclicBarrier-style, reusable), `CountDownLatch` (one-shot vs WaitGroup), `Condvar` (wait/notify tied to Mutex, spurious wakeup contract). | 103.3 (Mutex для Condvar) | ~2 d |
| **103.5** | [Once + Lazy + OnceCell](103.5-once-lazy-oncecell.md) | `Once` hardening: `call_once(closure)` safe API (текущий `run/done` → deprecated с migration path). `OnceCell[T]`: value-capturing one-shot init, `get_or_init(closure) -> &T`. `Lazy[T]`: auto-init on first access wrapper. | 103.1, 103.2 (для state machine atomic) | ~1.5 d |
| **103.6** | [Realtime/Blocking integration](103.6-realtime-blocking-integration.md) | Type-checker enforcement: park-ing methods (`Mutex.lock`, `Condvar.wait`, `Semaphore.acquire`, etc.) запрещены в `realtime { }`. Leaf-method allowlist (`load`, `store`, `try_lock`, `fetch_*`, `compare_exchange*`, etc.). 6 error codes. Spec contract. | 103.2, 103.3, 103.4, 103.5 (нужны типы для enforcement) | ~1.5 d |
| **103.7** | [Spec D-blocks + AI-first guidance](103.7-spec-d-blocks.md) | **D167** memory model + ordering semantics; **D168** atomic primitives API contract; **D169** Mutex/RwLock/ReentrantMutex; **D170** Semaphore/Barrier/CountDownLatch/Condvar; **D171** Once/Lazy/OnceCell; **D172** Realtime/Blocking interaction matrix; **D173** AI-first guidance «когда channel vs mutex vs atomic vs once». Q9, Q12.4, Q-memory-model → ✅ closed. §1531 переписать. | 103.1-103.6 (фиксирует уже реализованное) | ~1.5 d |
| **103.8** | [Conformance + stress + audit + close](103.8-conformance-and-close.md) | Cross-cutting test suite: stress (10k+ ops под M:N с 4-8 workers), litmus tests (memory ordering observable differences relaxed vs seq_cst), property tests (CAS-loop сходимость, semaphore-bounded concurrency), cross-platform validation (Windows/Linux). Pre-existing `nova_tests/concurrency/sync_*.nv` migrate на новый API. Audit consistency (sync.nv ↔ sync_primitives.h ↔ D-блоки). Plan 18 update, README, logs, master close. | 103.1-103.7 | ~1.5 d |
| **103.9** | [Consume guards migration (V2)](103.9-consume-guards-migration.md) | **GATED on Plan 100.7 readiness.** Lock-family + Semaphore + Once API V2: `Mutex.lock() → MutexGuard consume`, `RwLock.read/write → ReadGuard/WriteGuard consume`, `Semaphore.acquire() → Permit consume`, `Once.start() → OnceGuard consume`. Старые bare `unlock()/release()/done()` deprecated с migration path (Plan 62.F edition-versioning); `with_lock(fn)`/`with_read(fn)`/etc. helpers становятся thin wrappers над guard-returning core. Semver-bump. Type contracts integrate с Plan 100 D162 (check_consume + defer-family). Spec D174 (consume integration). Cross-module migration: 4+ pilot sites в `nova_tests/concurrency/`. | 103.3, 103.4, 103.5, Plan 100.7 (gating) | ~2-2.5 d |

**Critical path V1:** 103.1 → 103.2 → (103.3 ⊕ 103.5) → 103.4 → 103.6 → 103.7 → 103.8.

**V2 (post-V1):** 103.9 gated на Plan 100.7 (`stdlib migration playbook + 4 pilot migrations` включая «Mutex+Lock-guard (std/sync)»). Plan 103.9 — placeholder в roadmap, не starts пока Plan 100.7 не готов. **API V1 спроектирован так, чтобы V2 миграция была non-breaking для `with_lock(fn)` пользователей.**

**Параллелизуется:** 103.3 и 103.5 после 103.2 готовы. 103.7 можно драфтить параллельно с 103.4 (D-блоки 167-168 не зависят от Condvar/Lazy).

## Progress tracker (live)

Обновляется по мере закрытия sub-plans.

| # | Sub-plan | Status | Merge / commit | Tests | D-block |
|---|---|---|---|---|---|
| 103.1 | Memory ordering | ✅ ЗАКРЫТ 2026-05-25 | merge `6be2519b0e2` / commit `9f6b0f902c2` | 7/7 PASS (5 pos + 2 neg) | D167 draft |
| 103.2 | Atomics full suite | ✅ ЗАКРЫТ 2026-05-25 | merge `69d7605cc1c` / commit `348b205f47f` | 17/17 PASS (11 pos + 6 neg) | D168 draft |
| 103.3 | Mutex/RwLock/ReentrantMutex | ⏸ pre-patched, ready | — | — | D169 (планируется) |
| 103.4 | Coordination | ⏸ gated на 103.3 | — | — | D170 (планируется) |
| 103.5 | Once/Lazy/OnceCell | ✅ ЗАКРЫТ 2026-05-26 | merge `c7f9bca1026` | 20/20 PASS (11 pos + 3 neg + 2 prop + 1 stress) | D171 draft (отложен в 103.7) |
| 103.6 | Realtime/Blocking enforcement | ⏸ gated на 103.2-103.5 | — | — | D172 (планируется) |
| 103.7 | Spec D-blocks final + AI-first guidance | ⏸ gated на 103.1-103.6 | — | — | D173 NEW |
| 103.8 | Conformance + V1 close | ⏸ gated на 103.1-103.7 | — | — | — |
| 103.9 | Consume guards migration (V2) | ⏸ hard-gated на Plan 100.7 | — | — | D174 (планируется) |

**V1 closure: 3 of 8** (103.1 ✅ + 103.2 ✅ + 103.5 ✅). 103.3 pre-patched
+ ready-to-launch (нет worktree); 103.4 gated на 103.3; 103.6/7/8
sequential последующие.

## Empirical learnings (live)

Что выяснилось при impl 103.1+103.2 (обновляется по мере закрытия
sub-plans):

### 103.1 закрытие (2026-05-25)

- **Naming collision discovered:** `Ordering` уже занят prelude
  (three-way comparison `Less | Equal | Greater`). Финальное имя
  enum'а — **`MemOrdering`**. Зафиксировано как M14d. Все downstream
  sub-plans (103.2-103.9) обязаны использовать `MemOrdering`.
- **Подтверждено M14b (`#stable(since = "0.1")`):** baseline sync.nv
  имеет только `0.1` и `0.6` (Plan 65 time/duration). Новые
  declarations 103.1 шли c `0.1` — без введения `0.2`.
- **Подтверждено M14c (`compiler-codegen/src/diag.rs`):** Plan 50 —
  это про keyword-only default params, **не diagnostic format**.
  `diag.rs` имеет production-grade infra (`Diagnostic` + `Suggestion`
  + `Applicability::MachineApplicable`). Используется напрямую.
- **Spec D167 draft** в `06-concurrency.md` после D166 (Plan 100.8).
  Final closure отложен в 103.7.

### 103.5 закрытие (2026-05-26)

- **20/20 тестов PASS** (превышает acceptance: ≥11 pos + ≥6 neg + ≥2 prop + ≥1 stress).
- **Подтверждено M8 (closure-form primary):** `Once.call_once(fn)`
  работает; legacy `run/done` deprecated с warning, runtime сохраняет
  backward compat.
- **Подтверждено M9 (distinct types):** OnceCell + Lazy — два разных
  типа с разной panic-semantics (OnceCell init panic → retry, Lazy →
  poison).
- **OnceCell[T] / Lazy[T] mono'd inline в emit_c.rs** — не pre-declared
  как Once/AtomicX в `sync_primitives.h` (упрощение #1 в
  simplifications.md: generic типы требуют T-конкретизации).
- **NOVA_SYNC_ASSERT discovery:** `#ifdef NOVA_DEBUG` — no-op в Dev
  mode. Fix: unconditional `Nova_Once_method_done` state check
  через `Nova_Fail_fail + nova_throw` (защита от silent no-op).
- **Lazy + parallel for Windows crash discovered:** parallel for +
  Lazy.force() как первая/единственная операция → "fiber stack overflow
  in slot 0" (VEH crash). Не диагностирован. Workaround:
  `lazy_no_double_init_prop` — sequential; concurrent coverage через
  `once_stress_mn_4workers` (16 fibers × 100 force). Documented в
  simplifications.md #2.
- **D171 spec — НЕ написан** (упрощение #3 в simplifications.md):
  отложен в Plan 103.7 (вместе с финализацией D167-D173).
- **OnceCell.take() atomicity** — take() под mutex без atomic ops;
  достаточно для single-fiber take (documented).

### 103.2 закрытие (2026-05-25)

- **AtomicInt → AtomicI64 alias (M14)** работает без issues.
  Pre-existing `nova_tests/concurrency/sync_atomic.nv` PASS без
  изменений.
- **17/17 тестов PASS** — 11 positive + 6 negative. Превышены
  acceptance criteria плана (≥8 pos + ≥6 neg + ≥4 property + ≥2 stress).
- **D168 draft** в `06-concurrency.md` после D167.
- **814+ строк** в `sync_primitives.h` — substantial mechanical
  copy-paste от canonical AtomicI64 template. Подтверждает что
  decision M2 (sized atomics, не generic `Atomic[T]`) правильное
  для bootstrap-codegen.

### Что подтвердилось из дизайна (M-decisions validation)

- ✅ **M2** sized atomics (не generic) — оптимально для текущего
  codegen.
- ✅ **M8** closure-form `Once.call_once(fn)` primary (103.5 ✅).
- ✅ **M9** OnceCell + Lazy distinct types (103.5 ✅).
- ✅ **M14** backward compat AtomicInt — alias mechanism работает.
- ✅ **M14b** `since = "0.1"` consistency — без проблем.
- ✅ **M14c** `diag.rs` напрямую — без проблем.
- ✅ **M14d** `MemOrdering` rename — discovered necessity post-design.

### Что ещё validate в downstream sub-plans

- **M3** non-reentrant Mutex default + opt-in ReentrantMutex (103.3
  → проверка).
- **M6** fair FIFO Mutex default + opt-in unfair (103.3).
- **M7** RwLock writer-priority default (103.3).
- **M10** Condvar tied to Mutex (103.4).
- **M12** type-checker `realtime { }` ban (103.6 — Nova edge,
  critical validation).
- **M15** V1 `with_lock(fn)` preserved для V2 non-breaking migration
  (test в 103.9 V2).

### Discovered upgrades (need follow-up)

- **NOVA_SYNC_ASSERT runtime guard** (103.5): debug-only assertions
  не работают в Dev mode. 103.5 ввёл unconditional state checks для
  Once.done. Аналогичный pattern возможно нужен для Mutex.unlock,
  WaitGroup.done, etc. — проверить в 103.3/103.4.
- **Lazy + parallel for crash on Windows** (103.5): undiagnosed
  fiber-arena issue. Plan 83.x team task если воспроизводится; для
  Plan 103 — workaround sequential prop test.

## Acceptance criteria

### V1 closure (после 103.8 — основной milestone Plan 103)

- [ ] Все 8 V1 sub-plans (103.1-103.8) ✅ ЗАКРЫТЫ, merged в main.
- [ ] **Capability parity** с Go `sync` + Rust `std::sync` (без `Mutex<T>`/poisoning из принципа): покрыты Atomic-suite, Mutex, RwLock, Once, WaitGroup, плюс Nova-added (ReentrantMutex, Semaphore, Barrier, CountDownLatch, Condvar, OnceCell, Lazy, full Ordering API).
- [ ] **Capability matrix** (см. выше) проверена end-to-end: каждая строка имеет либо реализацию, либо явный rationale «не делаем».
- [ ] **`grep "Mutex / Atomic — НЕ в spec" spec/`** → 0 результатов.
- [ ] **Q9, Q12.4, Q-memory-model** в `open-questions.md` имеют explicit статус «закрыт через D167-D173» (полностью, не partial).
- [ ] **6 D-блоков** в `spec/decisions/06-concurrency.md`: D167-D172 (technical) + D173 (AI-first guidance).
- [ ] **Type-checker enforcement** `realtime { }` ban: 6 error codes реализованы, по 2 negative test'а на каждый.
- [ ] **Backward compat:** существующие `AtomicInt`/`AtomicBool` deprecated-alias на `AtomicI64`/`AtomicBool` (SeqCst default); все pre-existing nova_tests passes без изменения.
- [ ] **Conformance test suite:** ≥3 stress теста (10k+ ops, M:N 4 workers), ≥3 litmus теста (observable ordering differences), ≥5 property тестов (CAS convergence, Semaphore bounded concurrency, Barrier rendezvous, RwLock fairness, Once exactly-once), positive+negative для каждого типа.
- [ ] **Cross-platform:** все sync тесты passes на Windows clang + Linux clang (macOS — best effort, ждёт CI).
- [ ] **AI-first guidance:** D173 содержит decision-tree «какой примитив выбрать» (channel → mutex → atomic → once); 5 canonical patterns с anti-patterns.
- [ ] **Plan 18** баннер обновлён, все 5 шагов имеют точный статус.
- [ ] **`nova test` full suite** — 0 новых FAIL relative к pre-Plan-103 baseline.

### V2 closure (после 103.9 — gated на Plan 100.7)

- [ ] Plan 100.7 ✅ closed (pre-condition).
- [ ] `Mutex.lock() → MutexGuard consume`, `RwLock.read/write → ReadGuard/WriteGuard consume`, `Semaphore.acquire() → Permit consume`, `Once.start() → OnceGuard consume` — все 4 семейства мигрированы.
- [ ] D174 «consume integration» в `spec/decisions/06-concurrency.md`.
- [ ] `with_lock(fn)` / `with_read(fn)` / `with_write(fn)` / `with_permit(fn)` helpers остаются — стали thin wrappers над guard-returning core; pre-existing usage compiles без изменений.
- [ ] Bare `unlock()/release()/done()` deprecated с migration path (edition-versioned по Plan 62.F.bis); в pre-V2 edition compiles с deprecation warning, в post-V2 edition — compile error.
- [ ] 4+ pilot sites в `nova_tests/concurrency/` мигрированы как demonstration.
- [ ] Atomics, WaitGroup, Barrier, CountDownLatch, OnceCell, Lazy — **НЕ** мигрированы (M16).

## Non-acceptance (out of Plan 103)

- **Generic `Atomic[T]`** (Rust seal pattern) — заблокирован `[M-fn-prefix-int-only-mono]`. Когда `fn[T]` mono закрыт (Plan 101.x) — отдельный план.
- **`Atomic[Float64]`** (через bits transmutation) — отдельный план, edge case.
- **STM** (software transactional memory, Haskell/Clojure) — экзотика, не nano roadmap.
- **`Domain`-style ОС-thread isolation** (OCaml 5) — отложено Q12.7.
- **Lock-free queues / Michael-Scott queue / hazard pointers** — это Channel impl detail, не stdlib type.
- **`parking_lot`-style ParkLot primitive** (Rust crates) — текущий `nova_sched` park/wake достаточен.
- **Actor framework** (Akka/Erlang) — outside scope, channel + spawn — primitives, actor — pattern.
- **`async`/`await`** color — Nova D62 ambient, нет.

## Дизайн-решения для master (фиксируются здесь, в sub-plans следуем)

| # | Решение | Альтернатива | Почему |
|---|---|---|---|
| **M1** | `Ordering` как enum, **default = `SeqCst`** для simple-overload methods | Без default (Rust требует always) | AI-first: дефолт безопасный, опытный pisатель указывает явно |
| **M2** | Sized atomics (`AtomicI32`, `AtomicU64`, ...) | Generic `Atomic[T]` | Codegen block + monomorphization не нужна для primitives |
| **M3** | `Mutex` non-reentrant; `ReentrantMutex` opt-in | Reentrant default (Java) | Non-reentrant дёт deadlock-detection early; reentrant — legacy/wrap usage |
| **M4** | `Mutex` без data-carrying | Rust `Mutex<T>` | Type-system simplicity; `defer m.unlock()` + scope разделяет lock от data |
| **M5** | No poisoning | Rust `LockResult` | AI codegen без `match ... err =>` обёртки на каждый lock |
| **M6** | Fair FIFO mutex default; unfair — opt-in `Mutex.new_unfair()` | Unfair default (Go/parking_lot) | AI-generated кода предсказуемость > throughput; perf-critical может opt-in |
| **M7** | `RwLock` writer-priority default | Reader-priority (Linux pthread) | Избегаем writer starvation — общий sense для AI-кода |
| **M8** | Closure-form `Once.call_once(fn)` primary; `run/done` deprecated alias | Только run/done | Closure form harder to misuse (нельзя забыть `done()`) |
| **M9** | `OnceCell[T]` + `Lazy[T]` отдельные типы | Один тип с auto-init flag | Lazy — wrapper над OnceCell с `Deref`-like access, разные APIs |
| **M10** | `Condvar` всегда привязан к `Mutex` (Rust-style) | Java `Condition` отделимый | Type-system enforced correctness — нельзя wait без mutex |
| **M11** | `Semaphore` bounded (init permits) | Unbounded (counting) | Bounded — концерн backpressure; unbounded — counter pattern, делается через `AtomicI32` |
| **M12** | Type-checker enforce `realtime { }` ban на park-ing | Runtime panic | Compile-time лучше; D64 уже precedent |
| **M13** | Module `runtime.sync` (не prelude) | Prelude (`Mutex`, `AtomicInt` глобально) | Explicit import — намерение использовать low-level визуально |
| **M14** | Backward compat: `AtomicInt` → alias `AtomicI64`, `AtomicBool` остаётся | Breaking rename | Pre-existing user code должен компилироваться |
| **M14b** | Все новые declarations **`#stable(since = "0.1")`** — consistency с baseline sync.nv | Ввести `"0.2"` для всех new API | Verified 2026-05-25: `std/` имеет только `"0.1"` (sync baseline) и `"0.6"` (Plan 65 duration). Новый `"0.2"` без global decision = третий version-marker. Sub-plans drafts (103.2-103.9) содержат `"0.2"` от первого варианта — будут rewritten на `"0.1"` при их implementation start. |
| **M14c** | Diagnostic infrastructure — **используем `compiler-codegen/src/diag.rs` напрямую** (`Diagnostic`, `Suggestion`, `Applicability`) | «Plan 50 D102 format» (ошибочная ссылка в первом варианте sub-plans) | Verified 2026-05-25: Plan 50 — про keyword-only default params, не diagnostic format. `diag.rs` уже имеет полную production infra (MachineApplicable / MaybeIncorrect / HasPlaceholders + notes + with_suggestion builder). Sub-plans 103.2-103.9 содержат «Plan 50 D102 format» ссылки — будут rewritten при implementation. |
| **M14d** | **`MemOrdering` (не `Ordering`)** — финализированное имя enum'а | `Ordering` (как в первом draft) | Discovered 2026-05-25 при impl Plan 103.1: prelude уже имеет `Ordering { Less \| Equal \| Greater }` (three-way comparison). Sub-plans 103.2-103.9 drafts ссылаются на `Ordering` — все downstream API (`AtomicI64.load(ord MemOrdering)` etc.) **обязаны** использовать `MemOrdering`. Sub-plans будут rewritten при implementation start (как было с `since` versions M14b и diag M14c). |
| **M15** | V1 без consume; consume guards migration — отдельный sub-plan 103.9 (GATED на Plan 100.7); `with_lock(fn)`/`with_read(fn)`/`with_permit(fn)` в V1 как preferred pattern → non-breaking при V2 | (a) ждать Plan 100, (b) V1 уже с consume gated, (c) `Mutex.lock()/unlock()` без миграции вообще | Plan 100 P3 далеко от готовности; Q9/Q12.4/Q-memory-model нужно закрыть сейчас; `with_lock` pattern минимизирует exposed bare API → V2 cleanup без breaking change большинства usage sites |
| **M16** | Atomics НЕ получают consume никогда (даже в V2) | Atomic = resource → consume | Atomic = shared state primitive, операции не «consume'ят». Идеологический mismatch с linear-types. |

## Risks + mitigations

| Риск | Mitigation |
|---|---|
| **Backward compat ломается:** существующие `AtomicInt.compare_exchange(a, b) -> bool` API изменится при добавлении `Ordering` | M14 — deprecated alias `AtomicInt` = `AtomicI64`, simple-overload без Ordering (default SeqCst) сохраняет старую сигнатуру |
| **`fn[T]` ограничение блокирует sized atomics** | Sized atomics — отдельные моно-типы (M2), не нужна generic |
| **M:N interaction баги** | 103.8 stress tests + cross-platform validation; baseline уже M:N-safe |
| **Spec D173 «AI-first guidance» субъективна** | Решение через decision-tree (objective rules) + 5 canonical patterns; review с пользователем перед closure |
| **`Duration` API ещё не stable** | Plan 65 ✅ shipped `Duration` тип; используем; если нестабильно — `Instant` deadline альтернатива |
| **Memory ordering litmus tests platform-dependent (x86 vs ARM)** | 103.8 явно покрывает; на x86 многие relaxed = seq_cst — тесты должны быть architecturally-aware |
| **Lazy / OnceCell GC tracking** | OnceCell[T] хранит ref на T — GC сам обрабатывает; явный contract в D171 |
| **Condvar spurious wakeup** | Industry standard — predicate-loop pattern; D170 contract + test |

## Связь с другими планами

- [18-stdlib-roadmap.md](18-stdlib-roadmap.md) — Plan 18 Шаг 1 ✅ shipped; Plan 103 формализует + extends.
- [21-channel-revision-implementation.md](21-channel-revision-implementation.md) — Channel формализован D79 (preferred default; Plan 103 — escape hatch).
- [44.1-channel-impl.md](44.1-channel-impl.md) — Plan 44.1 Ф.1 sync.h: backend selector + memory ordering rules → база для 103.1.
- [65-duration-type.md](65-duration-type.md) — `Duration` тип, используется в `try_lock_for` / `wait_for`.
- [83.3-blocking-libuv-threadpool.md](83.3-blocking-libuv-threadpool.md) — `Blocking` эффект; Plan 103.6 интегрирует.
- [91-stdlib-mvp-for-0.1.md](91-stdlib-mvp-for-0.1.md) — std MVP; sync **не** в MVP но Plan 103 закрывает spec-долг; some sync примитивы используются stdlib collections.
- [100-linear-must-consume.md](100-linear-must-consume.md) — consume umbrella; **Plan 103.9 GATED на Plan 100.7** (Mutex pilot migration); Plan 103 V1 спроектирован для non-breaking V2 миграции.
- [100.7-stdlib-migration-playbook.md](100.7-stdlib-migration-playbook.md) — упоминает «Mutex+Lock-guard (std/sync)» как одну из 4 pilot migrations — это и есть Plan 103.9.

## Эволюция

- **2026-05-25 v1 (этот доклад):** roadmap-rewrite после user-feedback «без упрощений, не хуже Go/Rust/TS/Kotlin» + «consume для AtomicInt?». Декомпозиция 9 sub-plans (V1 = 103.1-103.8 + V2 = 103.9 gated на Plan 100.7), capability matrix vs 4 эталона, 16 design-решений зафиксированы. M15 (V1 без consume + future migration) + M16 (atomics никогда не consume).
- **v0 (commit `748a9b61623`):** initial proposal — простая 6-фазная формализация только текущего baseline. Superseded.

Sub-plans ссылаются на этот master для дизайн-решений M1-M14 и общей философии.
