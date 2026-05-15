// SPDX-License-Identifier: MIT OR Apache-2.0
# План 21: D91 implementation — Channel revision (capability-split)

**Статус:** ✅ **ЗАКРЫТ** — D91 capability-split реализован
(`ChanWriter`/`ChanReader`, negative-тесты sender-no-recv /
receiver-no-send проходят; подтверждено Plan 30 §«Связь»).
**Дата создания:** 2026-05-10.
**Обновлён:** 2026-05-11 (production-grade rewrite после Plan 22 completion).
**Зависимости:**
- [D91](../../spec/decisions/06-concurrency.md#d91-channel-revision--capability-split-на-sender--receiver) — нормативная спецификация.
- [Plan 20](20-defer-implementation.md) — ✅ закрыт; `defer` доступен.
- [Plan 22](22-sleep-libuv-integration.md) — ✅ закрыт; park/wake API
  (`nova_rt/sched.h`) доступен. `_nova_active_scope` / `_nova_active_slot`
  thread-locals задокументированы и используются.

**Без упрощений.** Plan 22 закрыт → реализуем `send`/`recv` сразу через
production park/wake API. Busy-yield fallback не допускается (R7 Plan 22:
«no busy-loops anywhere»).

**select — не в Plan 21.** Парсер `select` не существует; его реализация
требует отдельного grammar + codegen (~400-600 строк). Вынесен в Plan 28
(TBD). Plan 21 реализует Ф.1-Ф.6 полностью; `select` после revision
будет в отдельном плане.

---

## Цель

Реализовать D91 — переписать Channel API с Go-style (один объект с
`send`/`recv`) на Rust mpsc-style (capability-split:
`Channel.new(cap) -> (Sender[T], Receiver[T])`). Production-grade через
park/wake: blocking `send`/`recv` не busy-loop'ят. Cancel-during-recv
работает автоматически через D93 stop_cb.

---

## Архитектурные решения (зафиксированы до реализации)

### A1. WaiterList — heap-allocated, не stack

В псевдокоде Plan 22 draft'а использовался stack-allocated `ChannelWaiter`:
```c
ChannelWaiter w = { .scope = sc, .slot = sl, .next = st->recv_waiters };
st->recv_waiters = &w;
nova_sched_park(sc, sl);  // <-- yield! w на стеке уже недействителен?
```

**Проблема:** `mco_yield` переключает coroutine-стек. После yield
stack-frame fiber'а suspended, `&w` формально валиден (minicoro
сохраняет весь стек), но это хрупко и UB-prone под M:N (Plan 23).

**Решение A1 (production):** `ChannelWaiter` аллоцируется через
`nova_alloc` (heap). GC соберёт когда никто не держит. После wake —
явный unlink из waitlist (idempotent). Это паттерн Plan 22 eventloop.c
(uv_timer_t через heap).

```c
typedef struct ChannelWaiter {
    NovaFiberQueue*      scope;
    int                  slot;
    Nova_ChannelState*   channel;  /* обратная ссылка для unlink в stop_cb */
    bool                 is_recv;  /* recv-waiter или send-waiter */
    nova_int             send_val; /* если send-waiter: значение для отправки */
    struct ChannelWaiter* next;
} ChannelWaiter;
```

### A2. stop_cb — SYNC для channel (не ASYNC)

Sleep stop_cb (Plan 22 Ф.8) — ASYNC, потому что `uv_close` асинхронен.
Channel stop_cb — **SYNC**: убираем waiter из list (O(n) но list короток)
и сразу wake. Нет async backend.

```c
static NovaStopMode _nova_channel_waiter_stop_cb(void* handle) {
    ChannelWaiter* w = (ChannelWaiter*)handle;
    /* unlink из recv_waiters или send_waiters: */
    _nova_channel_waiter_unlink(w);
    /* wake вызовет nova_sched_cancel_all_pending → SYNC → unpark immediate */
    return NOVA_STOP_SYNC;
}
```

### A3. _nova_active_scope / _nova_active_slot — готовые thread-locals

`fibers.h` экспортирует:
```c
__declspec(thread) extern NovaFiberQueue* _nova_active_scope;
__declspec(thread) extern int             _nova_active_slot;
```

`nova_receiver_recv` и `nova_sender_send` используют их напрямую.
Вызов из не-fiber контекста → abort с FATAL message.

### A4. Nova_ChannelPair — struct для tuple-return из Channel.new

Codegen не поддерживает multiple-return напрямую в C ABI. Используем
struct-by-value (оба поля — pointer'ы, помещаются в 2 регистра):

```c
typedef struct { Nova_Sender* tx; Nova_Receiver* rx; } Nova_ChannelPair;
Nova_ChannelPair nova_channel_new(int64_t capacity);
```

В emit_c.rs: `Channel.new(cap)` → `nova_channel_new(cap)`. Tuple-
destructuring `let (tx, rx) = Channel.new(cap)` раскрывается в:
```c
Nova_ChannelPair _ch_pair_N = nova_channel_new(cap);
Nova_Sender* tx = _ch_pair_N.tx;
Nova_Receiver* rx = _ch_pair_N.rx;
```

### A5. Full-buffer send — park (не busy-yield)

`nova_sender_send` на полном буфере park'ает через sched API.
`nova_receiver_recv` при wake проверяет буфер и снимает send-waiter
(аналогично Go chan).

### A6. select — Plan 28 (не Plan 21)

`select` требует парсер и codegen которых не существует. Plan 21
не включает select. Существующие тесты не используют select.

---

## Фазы

### Ф.1 — nova_rt/channels.h: capability-split + park/wake

**Файл:** `compiler-codegen/nova_rt/channels.h` — полная замена.

**Структуры:**

```c
/* ChannelWaiter — heap-allocated (A1). */
typedef struct ChannelWaiter {
    NovaFiberQueue*      scope;
    int                  slot;
    Nova_ChannelState*   channel;
    bool                 is_recv;
    nova_int             send_val;
    struct ChannelWaiter* next;
} ChannelWaiter;

typedef struct Nova_ChannelState {
    nova_int*     buf;
    int64_t       cap;
    int64_t       head;
    int64_t       count;
    bool          closed;
    ChannelWaiter* recv_waiters;   /* fibers waiting for data */
    ChannelWaiter* send_waiters;   /* fibers waiting for space (full buffer) */
} Nova_ChannelState;

typedef struct { Nova_ChannelState* state; } Nova_Sender;
typedef struct { Nova_ChannelState* state; } Nova_Receiver;
typedef struct { Nova_Sender* tx; Nova_Receiver* rx; } Nova_ChannelPair;
```

**nova_channel_new:**
```c
static inline Nova_ChannelPair nova_channel_new(int64_t capacity) {
    Nova_ChannelState* st = nova_alloc(sizeof(Nova_ChannelState));
    int64_t actual = capacity > 0 ? capacity : 1;  /* rendezvous = cap 1 */
    st->buf          = nova_alloc(actual * sizeof(nova_int));
    st->cap          = actual;
    st->head         = 0;
    st->count        = 0;
    st->closed       = false;
    st->recv_waiters = NULL;
    st->send_waiters = NULL;
    Nova_Sender*   tx = nova_alloc(sizeof(Nova_Sender));
    Nova_Receiver* rx = nova_alloc(sizeof(Nova_Receiver));
    tx->state = st;
    rx->state = st;
    return (Nova_ChannelPair){ .tx = tx, .rx = rx };
}
```

**stop_cb + unlink:**
```c
static inline void _nova_channel_waiter_unlink(ChannelWaiter* w) {
    if (!w->channel) return;
    Nova_ChannelState* st = w->channel;
    ChannelWaiter** head = w->is_recv ? &st->recv_waiters : &st->send_waiters;
    ChannelWaiter* prev = NULL;
    ChannelWaiter* cur  = *head;
    while (cur) {
        if (cur == w) {
            if (prev) prev->next = cur->next;
            else      *head      = cur->next;
            w->channel = NULL;  /* sentinel: unlinked */
            return;
        }
        prev = cur; cur = cur->next;
    }
}

static NovaStopMode _nova_channel_waiter_stop_cb(void* handle) {
    ChannelWaiter* w = (ChannelWaiter*)handle;
    _nova_channel_waiter_unlink(w);
    return NOVA_STOP_SYNC;  /* A2: channel cancel — synchronous */
}
```

**nova_receiver_recv:**
```c
static inline NovaOpt_nova_int nova_receiver_recv(Nova_Receiver* rx) {
    Nova_ChannelState* st = rx->state;
    /* Fast path: data already available */
    if (st->count > 0) goto _take;
    if (st->closed) goto _closed;

    /* Slow path: park until data or close */
    {
        NovaFiberQueue* sc = _nova_active_scope;
        int             sl = _nova_active_slot;
        if (!sc || sl < 0) {
            fprintf(stderr, "nova: recv outside fiber context\n"); abort();
        }
        while (st->count == 0 && !st->closed) {
            ChannelWaiter* w = nova_alloc(sizeof(ChannelWaiter));
            w->scope   = sc; w->slot = sl;
            w->channel = st; w->is_recv = true;
            w->send_val= 0;  w->next = st->recv_waiters;
            st->recv_waiters = w;

            nova_sched_register_pending(sc, sl, w, _nova_channel_waiter_stop_cb);
            nova_sched_park(sc, sl);
            nova_sched_unregister_pending(sc, sl);

            if (sc->cancel_requested) {
                nova_throw(nova_str_from_cstr("scope cancelled"));
            }
        }
    }
    if (st->count == 0) goto _closed;

_take: {
        nova_int v = st->buf[st->head];
        st->head = (st->head + 1) % st->cap;
        st->count--;
        /* Wake first send-waiter if any */
        if (st->send_waiters) {
            ChannelWaiter* w = st->send_waiters;
            st->send_waiters = w->next;
            w->channel = NULL;
            /* push waiter's value into buffer */
            int64_t tail = (st->head + st->count) % st->cap;
            st->buf[tail] = w->send_val;
            st->count++;
            nova_sched_wake(w->scope, w->slot);
        }
        return (NovaOpt_nova_int){ .tag = NOVA_TAG_Option_Some, .value = v };
    }
_closed:
    return (NovaOpt_nova_int){ .tag = NOVA_TAG_Option_None, .value = 0 };
}
```

**nova_sender_send:**
```c
static inline void nova_sender_send(Nova_Sender* tx, nova_int v) {
    Nova_ChannelState* st = tx->state;
    if (st->closed) {
        nova_throw(nova_str_from_cstr("send on closed channel"));
    }
    /* Fast path: space available */
    if (st->count < st->cap) goto _push;

    /* Slow path: park until space */
    {
        NovaFiberQueue* sc = _nova_active_scope;
        int             sl = _nova_active_slot;
        if (!sc || sl < 0) {
            fprintf(stderr, "nova: send outside fiber context\n"); abort();
        }
        while (st->count >= st->cap && !st->closed) {
            ChannelWaiter* w = nova_alloc(sizeof(ChannelWaiter));
            w->scope    = sc; w->slot = sl;
            w->channel  = st; w->is_recv = false;
            w->send_val = v;  w->next = st->send_waiters;
            st->send_waiters = w;

            nova_sched_register_pending(sc, sl, w, _nova_channel_waiter_stop_cb);
            nova_sched_park(sc, sl);
            nova_sched_unregister_pending(sc, sl);

            if (sc->cancel_requested) {
                nova_throw(nova_str_from_cstr("scope cancelled"));
            }
        }
        if (st->closed) {
            nova_throw(nova_str_from_cstr("send on closed channel"));
        }
        /* recv уже push'нул наш v в буфер (A5: recv-side commit) */
        return;
    }

_push: {
        int64_t tail = (st->head + st->count) % st->cap;
        st->buf[tail] = v;
        st->count++;
        /* Wake first recv-waiter if any */
        if (st->recv_waiters) {
            ChannelWaiter* w = st->recv_waiters;
            st->recv_waiters = w->next;
            w->channel = NULL;
            nova_sched_wake(w->scope, w->slot);
        }
    }
}
```

**try_send / try_recv / close — без park:**
```c
static inline nova_bool nova_sender_try_send(Nova_Sender* tx, nova_int v) {
    Nova_ChannelState* st = tx->state;
    if (st->closed || st->count >= st->cap) return 0;
    int64_t tail = (st->head + st->count) % st->cap;
    st->buf[tail] = v; st->count++;
    if (st->recv_waiters) {
        ChannelWaiter* w = st->recv_waiters;
        st->recv_waiters = w->next;
        w->channel = NULL;
        nova_sched_wake(w->scope, w->slot);
    }
    return 1;
}

static inline NovaOpt_nova_int nova_sender_try_recv_forbidden(void) {
    fprintf(stderr, "nova: Sender.recv() — capability error\n"); abort();
}

static inline NovaOpt_nova_int nova_receiver_try_recv(Nova_Receiver* rx) {
    Nova_ChannelState* st = rx->state;
    if (st->count == 0) {
        return (NovaOpt_nova_int){ .tag = NOVA_TAG_Option_None, .value = 0 };
    }
    nova_int v = st->buf[st->head];
    st->head = (st->head + 1) % st->cap;
    st->count--;
    if (st->send_waiters) {
        ChannelWaiter* w = st->send_waiters;
        st->send_waiters = w->next;
        w->channel = NULL;
        int64_t tail = (st->head + st->count) % st->cap;
        st->buf[tail] = w->send_val;
        st->count++;
        nova_sched_wake(w->scope, w->slot);
    }
    return (NovaOpt_nova_int){ .tag = NOVA_TAG_Option_Some, .value = v };
}

static inline void nova_sender_close(Nova_Sender* tx) {
    Nova_ChannelState* st = tx->state;
    if (st->closed) return;
    st->closed = true;
    /* Wake all recv-waiters — они увидят closed+empty → None */
    while (st->recv_waiters) {
        ChannelWaiter* w = st->recv_waiters;
        st->recv_waiters = w->next;
        w->channel = NULL;
        nova_sched_wake(w->scope, w->slot);
    }
    /* Wake all send-waiters — они увидят closed → throw */
    while (st->send_waiters) {
        ChannelWaiter* w = st->send_waiters;
        st->send_waiters = w->next;
        w->channel = NULL;
        nova_sched_wake(w->scope, w->slot);
    }
}

static inline nova_int nova_channel_state_len(Nova_ChannelState* st) {
    return (nova_int)st->count;
}
static inline nova_int nova_channel_state_capacity(Nova_ChannelState* st) {
    return (nova_int)st->cap;
}
static inline nova_bool nova_channel_state_is_closed(Nova_ChannelState* st) {
    return (nova_bool)st->closed;
}
```

**Объём:** ~220 строк.

---

### Ф.2 — emit_c.rs: Channel.new + Sender/Receiver dispatch

**Что изменить:**

1. **`Channel.new(cap)`** — возвращает `Nova_ChannelPair`. Tuple-
   destructuring `let (tx, rx) = Channel.new(cap)` разворачивается:
   ```c
   Nova_ChannelPair _ch_N = nova_channel_new(cap);
   Nova_Sender* tx = _ch_N.tx;
   Nova_Receiver* rx = _ch_N.rx;
   ```
   Здесь `N` — уникальный счётчик (как `_nova_clos_N`).

2. **type inference** — `tx` → `Nova_Sender*`, `rx` → `Nova_Receiver*`.
   В `infer_c_type`: специальный case для `Channel.new`.

3. **Method dispatch Sender:**
   - `tx.send(v)` → `nova_sender_send(tx, (nova_int)(v))`
   - `tx.try_send(v)` → `nova_sender_try_send(tx, (nova_int)(v))`
   - `tx.close()` → `nova_sender_close(tx)`

4. **Method dispatch Receiver:**
   - `rx.recv()` → `nova_receiver_recv(rx)`
   - `rx.try_recv()` → `nova_receiver_try_recv(rx)`

5. **Auxiliary Receiver methods через state pointer:**
   - `rx.len()` → `nova_channel_state_len(rx->state)`
   - `rx.capacity()` → `nova_channel_state_capacity(rx->state)`
   - `rx.is_closed()` → `nova_channel_state_is_closed(rx->state)`

   (len/capacity/is_closed — через Receiver, не через Sender, потому
   что они read-only introspection. В тестах использовались на `ch`
   — теперь на `rx`. Если нужны на `tx` — добавим симметрично.)

6. **Старый `Nova_Channel*` dispatch — удалить** (D79 compat убираем
   полностью; тесты мигрируют в Ф.5).

**Ключевые места в emit_c.rs:**

- ~L6299: блок `if obj_ty == "Nova_Channel*"` → заменить на
  `Nova_Sender*` / `Nova_Receiver*` ветки.
- ~L6379: `Channel.new` special case → emit `nova_channel_new`.
- ~L6800: Path-form `Channel::new` → то же.
- ~L9952: type inference для Channel → Sender*/Receiver*.
- ~L10211: Path-form type → `Nova_ChannelPair`.

**Новая логика tuple-destructuring для Channel.new:**

Если LHS — tuple pattern `(a, b)` и RHS — `Channel.new(cap)`:
```rust
// emit_assign для let (tx, rx) = Channel.new(cap):
let tmp = format!("_ch_pair_{}", self.unique_id());
out.push_str(&format!("Nova_ChannelPair {} = nova_channel_new({});\n", tmp, cap_c));
out.push_str(&format!("Nova_Sender* {} = {}.tx;\n", tx_name, tmp));
out.push_str(&format!("Nova_Receiver* {} = {}.rx;\n", rx_name, tmp));
```

**Объём:** ~180 строк изменений в emit_c.rs.

---

### Ф.3 — types/mod.rs: Sender/Receiver как built-in types

**Что:**

1. Добавить `"Sender"` и `"Receiver"` в список known built-in types
   (рядом с `"Channel"`, `"Iter"`, etc.).

2. `Channel[T]` в value-position — compile error: «Channel — factory
   namespace, не type. Используй Sender[T] или Receiver[T]».

3. Method validation для Sender/Receiver:
   - `Sender`: разрешены `send`, `try_send`, `close`.
   - `Receiver`: разрешены `recv`, `try_recv`, `len`, `capacity`,
     `is_closed`.
   - Если вызван `tx.recv()` или `rx.send()` → compile error
     (capability violation).

**Объём:** ~80 строк в types/mod.rs.

---

### Ф.4 — nova_tests/runtime/channels.nv: миграция на D91

**Все тесты переписываются с Go-style на capability-split.**

Паттерн замены:
```nova
// Было:
let ch = Channel.new(4)
ch.send(10)
let v = ch.recv()
ch.close()

// Стало:
let (tx, rx) = Channel.new(4)
defer tx.close()
tx.send(10)
let v = rx.recv()
```

**Специфика:**
- `ch.len()` / `ch.capacity()` / `ch.is_closed()` → `rx.len()` /
  `rx.capacity()` / `rx.is_closed()`.
- `ch.try_send(v)` → `tx.try_send(v)`.
- `ch.try_recv()` → `rx.try_recv()`.
- `while let Some(v) = ch.recv()` → `while let Some(v) = rx.recv()`.
- `ch.close()` → `tx.close()` (через defer).

**Добавить concurrent-тесты** (Plan 22 дал park/wake — теперь можем):

```nova
test "channel: concurrent send+recv via spawn" {
    let (tx, rx) = Channel.new(1)
    defer tx.close()
    let mut received = 0
    supervised {
        spawn {
            tx.send(42)
        }
        spawn {
            let v = rx.recv()
            received = v.unwrap_or(-1)
        }
    }
    assert(received == 42)
}

test "channel: producer-consumer pipeline" {
    let (tx, rx) = Channel.new(4)
    defer tx.close()
    let mut sum = 0
    supervised {
        spawn {
            for i in 1..=5 {
                tx.send(i)
            }
        }
        spawn {
            while let Some(v) = rx.recv() {
                sum = sum + v
            }
        }
    }
    assert(sum == 15)
}

test "channel: cancel during recv" {
    let (tx, rx) = Channel.new(1)
    defer tx.close()
    let mut cancelled = false
    supervised {
        spawn {
            // recv на пустом канале — park'ается
            let _ = rx.recv()  // должен проснуться от cancel
        }
    }
    // supervised закрывается после cancel → rx.recv() бросает
    // (тест проверяет что нет hang'а)
}
```

**Объём:** ~220 строк (200 миграция + 60 новые тесты).

---

### Ф.5 — negative тесты capability-isolation

**Новые файлы** (compile-must-fail тесты):

`nova_tests/negative/channel_sender_no_recv.nv`:
```nova
// EXPECT: error
module nova_tests.negative.channel_sender_no_recv
test "sender cannot recv" {
    let (tx, _rx) = Channel.new(1)
    let _ = tx.recv()  // Sender не имеет метода recv
}
```

`nova_tests/negative/channel_receiver_no_send.nv`:
```nova
// EXPECT: error
module nova_tests.negative.channel_receiver_no_send
test "receiver cannot send" {
    let (_tx, rx) = Channel.new(1)
    rx.send(1)  // Receiver не имеет метода send
}
```

`nova_tests/negative/channel_type_as_value.nv`:
```nova
// EXPECT: error
module nova_tests.negative.channel_type_as_value
test "Channel[T] as value is forbidden" {
    let ch: Channel[int] = Channel.new(1)  // Channel — не тип
}
```

**Объём:** ~30 строк.

---

### Ф.6 — spec uplift

**D91 Bootstrap-status:** 🟡 → ✅.

**D79** — обновить статус: «Полностью пересмотрено D91. Старый API
(nova_channel_send/recv) удалён из runtime. Миграция завершена.»

**Plan 21 статус:** 🟡 → ✅.

**Объём:** ~20 строк в spec.

---

## Порядок исполнения

| # | Фаза | Файл | Атом? |
|---|---|---|---|
| Ф.1 | channels.h — новый runtime | `nova_rt/channels.h` | **A** |
| Ф.2 | emit_c.rs — dispatch + tuple-destructuring | `emit_c.rs` | **A** |
| Ф.3 | types/mod.rs — Sender/Receiver built-ins | `types/mod.rs` | **A** |
| Ф.4 | channels.nv migration + concurrent tests | `nova_tests/runtime/channels.nv` | **A** |
| Ф.5 | negative capability тесты | `nova_tests/negative/*.nv` | **A** |
| Ф.6 | spec uplift | `spec/decisions/06-concurrency.md`, этот план | post-A |

Ф.1-Ф.5 — атомарный набор (broken intermediate states). Ф.6 — после PASS.

---

## Риски и mitigation

| # | Риск | Mitigation |
|---|---|---|
| R1 | Tuple-destructuring `let (tx, rx) = Channel.new(...)` не поддержан в emit_c | Проверить emit_assign для tuple LHS + ChannelPair RHS; special-case если нужно |
| R2 | `_nova_active_scope` NULL при recv из не-fiber (main-flow) | abort с FATAL — не silent; тесты запускаются в supervised → всегда в fiber context |
| R3 | send-waiter commit in recv (A5 race) | Single-threaded cooperative → нет race; в M:N (Plan 23) потребует atomic |
| R4 | WaiterList unlink O(n) при cancel с 1000 waiters | List короткий в реальных программах; O(n) acceptable до Plan 23 |
| R5 | Concurrent тесты hang если producer/consumer в одном supervised | Тестировать с ненулевым буфером; unbuffered (cap=0) требует interleave — добавить Time.sleep(0) yield |
| R6 | len/capacity/is_closed на tx не нужны? | По D91 — только Receiver имеет introspection; если тест требует на tx — добавить симметрично |

---

## Definition of Done

- [ ] Ф.1-Ф.5 PASS: 156 базовых + все channel тесты зелёные.
- [ ] 3 negative-теста PASS (EXPECT: error — compile fail).
- [ ] Concurrent тесты (producer-consumer, cancel-during-recv) PASS.
- [ ] R7 Plan 22 «no busy-loops» — channels.h не содержит `while ... nova_fiber_yield()`.
- [ ] D91 Bootstrap-status ✅.
- [ ] Retro в `docs/project-creation.txt` + `docs/simplifications.md`.

---

## Связь с другими планами

- [Plan 20](20-defer-implementation.md) — ✅; `defer tx.close()` работает.
- [Plan 22](22-sleep-libuv-integration.md) — ✅; park/wake API стабилен.
- [Plan 23](23-mn-runtime-roadmap.md) — M:N; channels.h использует
  `_nova_active_scope` (thread-local) → под M:N нужен per-worker scope
  context. WaiterList unlink должен стать lock-free или mutex-guarded.
- **Plan 28 (TBD)** — `select { msg <- rx => ... }` после D91 migration.
  Требует: парсер `select`, codegen multi-arm recv, timeout arm integration
  с libuv timer.
- [D91 spec](../../spec/decisions/06-concurrency.md#d91-channel-revision--capability-split-на-sender--receiver) — нормативная семантика.
- [D93](../../spec/decisions/06-concurrency.md) — park/wake contract.

---

## История

- **2026-05-10** — draft создан (pre-Plan 22).
- **2026-05-11** — production rewrite: Plan 22 закрыт, busy-yield fallback
  удалён, реальный park/wake API, WaiterList heap-allocated (A1),
  stop_cb SYNC (A2), _nova_active_scope (A3), Nova_ChannelPair (A4),
  select вынесен в Plan 28, concurrent тесты добавлены в Ф.4.
