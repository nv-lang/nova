# План 21: D91 implementation — Channel revision (capability-split)

**Статус:** 🟡 **DRAFT — implementation pending**.
**Дата создания:** 2026-05-10.
**Зависимости:**
- [D91](../../spec/decisions/06-concurrency.md#d91-channel-revision--capability-split-на-sender--receiver) — нормативная спецификация.
- [Plan 20](20-defer-implementation.md) — `defer` для idiomatic `defer tx.close()`.
- [D79](../../spec/decisions/06-concurrency.md#d79-channels--coordination-между-fiberами) — текущая Go-style модель, которую пересматриваем.

**Связь с Plan 22:** формально независим, но архитектурно опирается
на **park/wake primitive** (`nova_rt/sched.h`), который вводит Plan 22.
Если Plan 22 завершён — `send`/`recv` сразу реализуются через API
(`nova_sched_park`/`nova_sched_wake`/`nova_sched_register_pending`),
cancel-during-recv работает автоматически через stop_cb mechanism.
Если Plan 22 ещё не сделан — Plan 21 реализует `send`/`recv` через
busy-yield D79-семантики; рефакторинг на API-driven park/wake —
follow-up после Plan 22. **Рекомендуемый порядок: Plan 22 → Plan 21.**
См. [«Интеграция с Plan 22»](#интеграция-с-plan-22) ниже.

---

## Цель

Реализовать D91 — переписать Channel API на Rust mpsc-style
(capability-split: `Channel.new(cap) -> (Sender, Receiver)`).
Breaking change для существующих тестов и nova_rt. Production-grade:
без упрощений, полное покрытие capabilities, миграция всех use-site'ов.

---

## Декомпозиция

### Ф.1. nova_rt — split state vs wrappers

**Что:** Переделать `nova_rt/channels.h`:

```c
// Hidden state (бывший Nova_Channel):
typedef struct { ... } Nova_ChannelState;

// Capability wrappers — публичные:
typedef struct { Nova_ChannelState* state; } Nova_Sender;
typedef struct { Nova_ChannelState* state; } Nova_Receiver;

// Factory возвращает tuple (через struct-by-value или через 2-out-pointer):
typedef struct { Nova_Sender* tx; Nova_Receiver* rx; } Nova_ChannelPair;
Nova_ChannelPair nova_channel_new(int64_t capacity);

// Capability-методы:
void                   nova_sender_send(Nova_Sender*, nova_int);
nova_bool              nova_sender_try_send(Nova_Sender*, nova_int);
void                   nova_sender_close(Nova_Sender*);
NovaOpt_nova_int       nova_receiver_recv(Nova_Receiver*);
NovaOpt_nova_int       nova_receiver_try_recv(Nova_Receiver*);
```

State разделена от capabilities. Sender и Receiver — wrapper struct'ы,
каждый share'ит `Nova_ChannelState*`. Это эквивалентно Rust `Arc<Inner>`
паттерну, но через managed heap (без RC, GC соберёт когда оба
wrapper'а unreachable).

**Park/wake реализация — два варианта:**

- **Если Plan 22 завершён** (рекомендуемо): `send`/`recv` блокировка
  через `nova_sched_park`/`nova_sched_wake` API из `nova_rt/sched.h`,
  waitlist'ы recv/send-waiter'ов в `Nova_ChannelState`, cancel-integration
  через `nova_sched_register_pending` + stop_cb. См.
  [«Интеграция с Plan 22»](#интеграция-с-plan-22) для полного псевдокода.

- **Если Plan 22 не сделан**: bootstrap-fallback на busy-yield D79-
  семантику (`while (empty) nova_fiber_yield()`). Менее эффективно,
  cancel виден на следующем yield-pass'е, но семантически корректно.
  Рефакторинг на API — follow-up после Plan 22.

**Файлы:**
- `compiler-codegen/nova_rt/channels.h` — переписать.
- Возможно `nova_rt/channels.c` если part-functions требуют out-of-header.

**Объём:** ~250 строк (под Plan 22 API) либо ~200 строк (без, проще
реализация без waitlist'ов).

### Ф.2. Codegen — Channel literals и method dispatch

**Что:** В `emit_c.rs`:

1. **`Channel[T].new(N)`** — special-case вызова. Возвращает tuple
   `(Sender[T], Receiver[T])`. Tuple-destructuring через
   `let (tx, rx) = Channel.new(...)` — уже работает (D17 tuple).

2. **Метод-dispatch** на Sender/Receiver — через тот же mechanism
   что для других типов (Plan 11). `tx.send(v)` →
   `Nova_Sender_method_send(tx, v)`.

3. **Type-инференция** для tuple-destructuring — `tx` имеет тип
   `Sender[T]`, `rx` — `Receiver[T]`. Должны корректно
   propagate'нуться в bound checks.

**Файлы:**
- `compiler-codegen/src/codegen/emit_c.rs` — special-case для
  `Channel.new`, dispatch на Sender/Receiver.

**Объём:** ~200 строк.

### Ф.3. Type-checker — protocol Sender / Receiver registration

**Что:** Зарегистрировать `Sender[T]` и `Receiver[T]` как built-in
protocols (как `Iter[T]`, `Hashable`, `From[T]`).

`Channel[T]` сам по себе — это **factory namespace**, не type.
`Channel.new(N)` — единственный API. Type `Channel[T]` в expression-
position теперь **запрещён** (compile error).

**Файлы:**
- `compiler-codegen/src/types/mod.rs` — добавить built-in protocols
  Sender/Receiver. Снять `Channel[T]` из value-types.

**Объём:** ~80 строк.

### Ф.4. `select` revision — через Receiver

**Что:** Парсер и codegen для `select`:

```nova
select {
    msg <- rx_a  => process_a(msg)
    msg <- rx_b  => process_b(msg)
    timeout(5.seconds()) => default
}
```

`<-` оператор в pattern-position читает из `Receiver[T]`. Раньше
читал из `Channel[T]` — обновить grammar и dispatch.

**Файлы:**
- `compiler-codegen/src/parser/mod.rs` — парсер select.
- `compiler-codegen/src/codegen/emit_c.rs` — emit select.

**Объём:** ~150 строк.

### Ф.5. Миграция nova_tests/runtime/channels.nv

**Что:** Переписать все тесты под новый API:

```nova
// Было:
let ch = Channel.new(4)
ch.send(10)
let v = ch.recv()

// Стало:
let (tx, rx) = Channel.new(4)
defer tx.close()
tx.send(10)
let v = rx.recv()
```

Объём тестов в `channels.nv` — ~200 строк. Большая часть переписки
mechanical.

**Зависимость от Plan 20:** Без `defer`'а пришлось бы вызывать
`tx.close()` явно в конце каждого теста — не катастрофа, но `defer`
делает миграцию идиоматичной.

**Файлы:**
- `nova_tests/runtime/channels.nv`.

**Объём:** ~200 строк edits.

### Ф.6. Тесты на capability-isolation

**Что:** Новые negative-тесты что:
- `tx.recv()` — compile error (`Sender` не имеет recv).
- `rx.send(v)` — compile error.
- Использование `Channel[T]` как value — compile error.

**Файлы:**
- `nova_tests/negative_capability/channel_sender_no_recv.nv`.
- `nova_tests/negative_capability/channel_receiver_no_send.nv`.
- `nova_tests/negative_capability/channel_type_as_value.nv`.

**Объём:** ~60 строк.

### Ф.7. Spec uplift + docs

**Что:**
- D91 Bootstrap-status: 🟡 → ✅.
- D79 — пометка «полностью пересмотрено D91» (вместо «частично»).
- Cross-refs в `effects.md`, `syntax.md`, `revolutionary.md` если
  есть Channel-примеры по старому API.

**Объём:** ~30 строк.

---

## Интеграция с Plan 22

Plan 22 (`Time.sleep` через libuv) вводит **нормативный park/wake API**
в `nova_rt/sched.h` (см. [Plan 22 «Park/wake API»](22-sleep-libuv-integration.md#parkwake-api---нормативный-primitive)):

```c
void      nova_sched_park(NovaFiberQueue* scope, int slot);
void      nova_sched_wake(NovaFiberQueue* scope, int slot);
nova_bool nova_sched_is_parked(NovaFiberQueue* scope, int slot);
void      nova_sched_register_pending(NovaFiberQueue* scope, int slot,
                                       void* handle, NovaCancelStopCb stop_cb);
void      nova_sched_unregister_pending(NovaFiberQueue* scope, int slot);
```

`Channel.send`/`recv` — блокирующая операция, fiber ждёт внешнего
события (приход данных / освобождение буфера / close). Опирается
на API напрямую:

```c
/* Channel state расширяется waitlist'ами. */
typedef struct ChannelWaiter {
    NovaFiberQueue* scope;
    int             slot;
    struct ChannelWaiter* next;
} ChannelWaiter;

typedef struct {
    /* ...buffer fields... */
    ChannelWaiter* recv_waiters;     /* fiber'ы parked в recv */
    ChannelWaiter* send_waiters;     /* fiber'ы parked в send (full buffer) */
    nova_bool      closed;
} Nova_ChannelState;

/* stop_cb для cancel-wake: убрать waiter из chain. */
static void _nova_channel_recv_stop_cb(void* handle) {
    ChannelWaiter* w = (ChannelWaiter*)handle;
    /* unlink w from its channel's recv_waiters chain (idempotent) */
    /* ...impl... */
}

NovaOpt_nova_int nova_receiver_recv(Nova_Receiver* rx) {
    Nova_ChannelState* st = rx->state;
    NovaFiberQueue*    sc = _nova_active_scope;
    int                sl = _nova_active_slot;

    while (st->count == 0 && !st->closed) {
        ChannelWaiter w = { .scope = sc, .slot = sl, .next = st->recv_waiters };
        st->recv_waiters = &w;

        nova_sched_register_pending(sc, sl, &w, _nova_channel_recv_stop_cb);
        nova_sched_park(sc, sl);
        nova_sched_unregister_pending(sc, sl);

        if (sc && sc->cancel_requested) {
            nova_throw(nova_str_from_cstr("scope cancelled"));
        }
    }
    if (st->count == 0 && st->closed) return NOVA_OPT_NONE;
    return NOVA_OPT_SOME(buffer_pop(st));
}

/* Sender side — будит первого recv-waiter'а после push'а. */
void nova_sender_send(Nova_Sender* tx, nova_int v) {
    Nova_ChannelState* st = tx->state;
    /* ...handle full-buffer parking аналогично... */
    buffer_push(st, v);
    if (st->recv_waiters) {
        ChannelWaiter* w = st->recv_waiters;
        st->recv_waiters = w->next;
        nova_sched_wake(w->scope, w->slot);
    }
}

/* close — будит всех recv-waiter'ов (они увидят closed + empty → None). */
void nova_sender_close(Nova_Sender* tx) {
    Nova_ChannelState* st = tx->state;
    if (st->closed) return;
    st->closed = true;
    while (st->recv_waiters) {
        ChannelWaiter* w = st->recv_waiters;
        st->recv_waiters = w->next;
        nova_sched_wake(w->scope, w->slot);
    }
}
```

**Cancel-during-recv** работает автоматически — `cancel()` Plan 22 R4
итерируется по `pending_stop_cb` всех slot'ов scope'а и вызывает
зарегистрированный stop_cb (наш `_nova_channel_recv_stop_cb`), потом
wake. Никакой Channel-специфичной интеграции с CancelToken не нужно.

**Решение по порядку:**

1. **Plan 22 завершён до Plan 21** (рекомендуемо): Plan 21 Ф.1
   реализует `send`/`recv` сразу через API из `sched.h`. Production-
   grade с самого начала, cancel-during-recv работает immediate.

2. **Plan 21 идёт первым** или параллельно: Plan 21 Ф.1 реализует
   `send`/`recv` через busy-yield (текущая D79-семантика); cancel
   виден на следующем yield-pass. Рефакторинг на API-driven park/wake
   — follow-up после Plan 22. Это **два прохода** через channels.h,
   но user-семантика совпадает.

3. **Cancel-during-recv** — без Plan 22 "best-effort" (cancel на
   следующем yield); с Plan 22 — immediate. Spec D91 явно cancel-
   семантику не описывает, оба варианта формально валидны.

**Рекомендуемый порядок:** Plan 22 → Plan 21. Plan 22 ставит
архитектурный фундамент (park/wake API + libuv-driven scheduler),
Plan 21 опирается на стабильный contract без переделок.

---

## ⚠️ Атомарность фаз

**Ф.1-Ф.6 — атомарный PR.** Промежуточные состояния нелегальны:
- nova_rt новый, codegen старый: linker fail.
- Codegen на split, тесты на Go-style: 0 PASS.
- select на старом ch: не парсится против Receiver.

**Зависит от Plan 20** — defer должен быть реализован до Plan 21
(или одновременно), иначе миграция channels.nv требует `tx.close()`
руками — будет noise.

Ф.7 — отдельный коммит после.

---

## Порядок исполнения

| # | Фаза | Зависимости | Атом? |
|---|---|---|---|
| Ф.1 | nova_rt — split state/wrappers | — | **A** |
| Ф.2 | Codegen — Channel.new + dispatch | Ф.1 | **A** |
| Ф.3 | Type-checker — register protocols | Ф.2 | **A** |
| Ф.4 | select revision | Ф.3 | **A** |
| Ф.5 | Migration of nova_tests | Ф.2-Ф.4, Plan 20 | **A** |
| Ф.6 | Negative тесты | Ф.3, Ф.5 | **A** |
| Ф.7 | Spec uplift | Ф.6 | post-A |

---

## Риски

1. **Tuple-return for Channel.new.** Codegen для функций возвращающих
   tuple — есть в Nova (D17). Но `Channel.new` возвращает tuple
   из **pointer'ов на разные типы** (`Sender*`, `Receiver*`). Что
   требует careful struct-emit. **Mitigation:** test с простым
   `fn f() -> (A*, B*)` отдельно, потом распространить.

2. **Backward compat — нет.** Старые `nova_rt/channels.h` symbols
   (`nova_channel_send` etc.) удаляются. Если есть пользовательский
   код, использующий их — сломается. **Mitigation:** breaking change,
   bootstrap-этап позволяет (D-policy «не бойся переделок»).

3. **State sharing через managed heap.** Sender и Receiver share
   `Nova_ChannelState*`. В Nova GC collect'ит, когда оба unreachable.
   Что если один live, другой собран? **Mitigation:** GC видит через
   `state`-pointer что state ещё referenced — collected только когда
   **оба** wrapper'а unreachable. Это стандартная GC-семантика для
   shared sub-objects.

4. **`close()` после close.** Idempotent — повторный close OK.
   **Mitigation:** в nova_sender_close проверка `state->closed`
   уже стоит (D79).

5. **`send` после close.** Spec D91 — panic. **Mitigation:** в emit
   `nova_sender_send` — assert + nv_panic.

6. **`select` после revision сложнее.** Раньше `select` работал с
   одним типом (`Channel[T]`), теперь — с `Receiver[T]` который
   protocol. Может потребовать dynamic dispatch. **Mitigation:**
   bootstrap может ограничить `select` arms до конкретных Receiver
   реализаций (через runtime); полный generic-select — отдельный
   риск.

---

## Definition of Done

- [ ] Ф.1-Ф.6 атомарный PR замерджен; полный test-suite **0
  regressions** на migrated channels.nv.
- [ ] 3 negative-теста проходят (sender/receiver capability isolation
  + Channel-as-value запрет).
- [ ] D91 Bootstrap-status ✅.
- [ ] D79 переоформлен как полностью устаревший.
- [ ] Запись в `docs/project-creation.txt` + `docs/simplifications.md`.
- [ ] discussion-log в private обновлён.

---

## Связь с другими планами

- [Plan 20](20-defer-implementation.md) — **должен быть завершён до
  Plan 21** (defer необходим для idiomatic close).
- [D91 spec](../../spec/decisions/06-concurrency.md#d91-channel-revision--capability-split-на-sender--receiver).
- [D79](../../spec/decisions/06-concurrency.md#d79-channels--coordination-между-fiberами)
  — старая Go-style модель, пересматриваемая.
- [D90](../../spec/decisions/03-syntax.md#d90-defer-и-errdefer--scope-level-cleanup-statement)
  — defer для close.
- [D6](../../spec/decisions/05-memory.md#d6) — managed heap, GC
  для shared state.
- [Q-keyword-symmetry](../../spec/open-questions.md#q-keyword-symmetry)
  — capability-split factory как use-case для anon protocol-impl
  (Plan 21 может выявить реальную боль если named-types путь будет
  громоздкий).
