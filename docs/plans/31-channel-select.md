// SPDX-License-Identifier: MIT OR Apache-2.0
# План 31: `select` — multiplexed channel operations

**Статус:** 🟡 план, не начат (обновлён 2026-05-11 — production-grade revision).
**Дата создания:** 2026-05-11.
**Зависимости:**
- [Plan 21](21-channel-revision-implementation.md) — ✅ закрыт; D91 capability-split реализован.
- [Plan 30](30-channel-improvements.md) — ✅ закрыт; send→bool + tx.clone().

---

## Цель

Реализовать `select` — ожидание на нескольких channel-операциях одновременно,
пробуждение по первому готовому. Это ключевой примитив для:
- fan-in (merge нескольких producers)
- non-blocking send/recv с `default` arm
- timeout на recv (`Time.after(d)` → ChanReader — см. ниже)
- worker-pool graceful shutdown

---

## Сравнение с Go / Rust

| Фича | Plan 31 | Go | Rust Tokio |
|---|---|---|---|
| recv arm | ✅ | ✅ | ✅ |
| send arm | ✅ Ф.2 | ✅ | ✅ |
| default arm (non-blocking) | ✅ Ф.3 | ✅ | via else |
| timeout через channel | ✅ Time.after(d) | time.After(d) | tokio::time::sleep |
| closed channel arm (None match) | ✅ | implicit zero | ✅ |
| псевдослучайный выбор | ✅ Ф.1 | ✅ (Fisher-Yates) | random |
| arm guards (if cond) | ✅ Ф.4 | ❌ | ✅ |
| cancel_scope integration | ✅ через stop_cb | — | через token |
| biased mode | ❌ пост-bootstrap | ❌ | ✅ |

---

## Синтаксис (D94)

```nova
select {
    Some(v) = rx1.recv()    => { process(v) }
    Some(v) = rx2.recv()    => { process(v) }
    None    = rx1.recv()    => { break }           // rx1 закрылся
    _       = tx.send(val)  => { /* sent */ }      // send arm
    _       = tx.send(val) if ready => { ... }     // send arm с guard
    default                 => { /* non-blocking */ }
}
```

### Timeout через `Time.after(d)`

Специального `timeout` arm'а **нет** — timeout реализуется через `Time.after(d)`,
которая возвращает `ChanReader[()]` закрывающийся через `d` секунд:

```nova
let timeout = Time.after(1.0)
select {
    Some(v) = rx.recv()      => { process(v) }
    None    = timeout.recv() => { log_idle() }
}
```

Это Go-style (`time.After(d)` возвращает `<-chan time.Time`) — элегантнее
чем специальный синтаксис, select не знает про "timeout" специально.

### Arm guards

```nova
select {
    Some(v) = rx1.recv() if v > 0 => { process(v) }
    Some(v) = rx1.recv()           => { skip(v) }
}
```

Guard — опциональное `if <expr>` после pattern. Arm пропускается
(treated as not-ready) если guard false.

### Closed channel

Если канал закрыт, `rx.recv()` немедленно возвращает `None` — arm
считается **ready**. Программист матчит `None` явно или через `_`:

```nova
select {
    Some(v) = rx.recv() => { work(v) }
    None    = rx.recv() => { break }      // канал закрылся
}
```

Без `None`-arm — если канал закрыт и recv возвращает None, `Some(v)` не
матчится, arm считается not-matched. Select ждёт других arm'ов. Если
все каналы закрыты и нет default — panic ("select: all channels closed").

---

## Семантика

1. **Проверка immediate availability** — до park'а проверяем каждый arm
   в псевдослучайном порядке (Fisher-Yates shuffle). Если ≥1 ready —
   выполняем первый найденный (без park'а).

2. **Park** — если ни один не ready: регистрируем все waiters, паркуем
   fiber.

3. **Wake** — когда любой arm готов: вызывает `nova_select_wake()`,
   который записывает `which` и будит fiber. Остальные waiters unlinked
   через `nova_select_cancel_others()`.

4. **Fairness** — псевдослучайный порядок проверки (Fisher-Yates) на
   каждой итерации. При нескольких ready одновременно — один из них
   выбирается случайно, без starvation.

5. **cancel_scope** — если scope отменяется во время park'а:
   `nova_sched_cancel_all_pending` вызывает stop_cb для каждого
   SelectWaiter arm'а, все unlinkуются, fiber просыпается и проверяет
   `cancel_requested`.

6. **default arm** — если присутствует: шаг 1 (immediate check) всегда
   succeeds (default ready если все остальные not-ready). Никогда
   не park'аем.

---

## Архитектура

### Runtime: SelectCtx (channels.h)

```c
/* Один arm в select. */
typedef struct SelectArm {
    Nova_ChannelState* channel;
    bool               is_recv;        /* true = recv, false = send */
    nova_int           send_val;       /* для send arm */
    bool               guard;          /* arm active? (guard evaluation result) */
} SelectArm;

/* Контекст всего select-выражения. */
typedef struct SelectCtx {
    SelectArm*      arms;              /* heap-alloc, n элементов */
    int             n;                 /* число arm'ов */
    int             which;             /* индекс сработавшего arm (-1 = default/none) */
    nova_int        recv_val;          /* принятое значение (recv arm) */
    bool            recv_is_some;      /* true = Some(v), false = None */
    NovaFiberQueue* scope;
    int             slot;
    /* Per-arm waiter, heap-alloc: */
    ChannelWaiter** waiters;           /* waiters[i] = NULL если arm неактивен */
} SelectCtx;
```

**Почему heap-alloc arms, не фиксированный N:**
Разные `select` имеют разное число arm'ов — N известен в compile time
для каждого конкретного select, но struct должен быть универсальным.
Codegen аллоцирует `arms` и `waiters` через `nova_alloc`.

### API (channels.h)

```c
/* Инициализация. */
void nova_select_init(SelectCtx* ctx, int n,
                      NovaFiberQueue* scope, int slot);

/* Настройка arm'а до select. */
void nova_select_set_recv(SelectCtx* ctx, int i,
                          Nova_ChanReader* rx, bool guard);
void nova_select_set_send(SelectCtx* ctx, int i,
                          Nova_ChanWriter* tx, nova_int val, bool guard);

/* Попытка немедленного выполнения (без park). Shuffle порядок.
 * Returns true если arm сработал (ctx->which, ctx->recv_val заполнены). */
bool nova_select_try_immediate(SelectCtx* ctx);

/* Регистрация всех waiters + park. После возврата — ctx->which готов. */
void nova_select_park(SelectCtx* ctx);

/* Вызывается из wake-callback arm'а: записывает which, будит fiber. */
void nova_select_wake(SelectCtx* ctx, int which,
                      nova_int recv_val, bool recv_is_some);

/* Отменяет все waiters кроме which. */
void nova_select_cancel_others(SelectCtx* ctx, int which);
```

### Codegen pattern

```nova
select {
    Some(v) = rx1.recv() => { body1 }
    Some(v) = rx2.recv() => { body2 }
    default              => { body3 }
}
```

→ C:

```c
{
    SelectCtx _sc;
    nova_select_init(&_sc, 2, _nova_active_scope, _nova_active_slot);
    nova_select_set_recv(&_sc, 0, rx1, /*guard=*/true);
    nova_select_set_recv(&_sc, 1, rx2, /*guard=*/true);

    bool _has_default = true;
    bool _done = nova_select_try_immediate(&_sc);
    if (!_done && !_has_default) {
        nova_select_park(&_sc);
        _done = true;
    }

    if (_sc.which == 0 && _sc.recv_is_some) {
        nova_int v = _sc.recv_val;
        /* body1 */
    } else if (_sc.which == 1 && _sc.recv_is_some) {
        nova_int v = _sc.recv_val;
        /* body2 */
    } else {
        /* default — body3 */
    }
}
```

### Fisher-Yates shuffle для fairness

```c
/* nova_select_try_immediate внутри: */
int order[n];
for (int i = 0; i < n; i++) order[i] = i;
/* Fisher-Yates: */
for (int i = n - 1; i > 0; i--) {
    int j = nova_rand_u32() % (i + 1);
    int tmp = order[i]; order[i] = order[j]; order[j] = tmp;
}
for (int k = 0; k < n; k++) {
    int i = order[k];
    if (!ctx->arms[i].guard) continue;
    /* check if ready: count > 0 (recv) or space (send) or closed */
    ...
}
```

`nova_rand_u32()` — простой LCG (seed из `uv_hrtime()`), достаточен
для fairness в bootstrap.

### Time.after(d) — timeout через channel

```c
/* nova_rt/time.h или eventloop.h: */
Nova_ChanReader* nova_time_after(double seconds);
```

Создаёт `ChanReader[()]` + `uv_timer_t`. Через `seconds` секунд:
- timer callback делает `nova_chan_writer_send` в internal tx
- затем `nova_chan_writer_close(tx)`

Результат: `rx.recv()` возвращает `Some(())` через d секунд, потом `None`.
Select использует это как обычный recv arm — никакой специальной интеграции.

---

## Парсер

Новый keyword `select` в лексере. `parse_select()` строит:

```rust
pub struct SelectArm {
    pub pat:   Pattern,          // Some(v), None, _, или pattern для send result
    pub op:    SelectOp,         // Recv(expr) | Send(expr, expr) | Default
    pub guard: Option<Box<Expr>>,
    pub body:  Block,
}

pub enum SelectOp {
    Recv(Box<Expr>),             // rx.recv()
    Send(Box<Expr>, Box<Expr>),  // tx.send(val)
    Default,
}

pub struct ExprSelect {
    pub arms: Vec<SelectArm>,
    pub span: Span,
}
```

---

## Type-checker

- Каждый `op` в arm проверяется на тип объекта: `Recv` — только на
  `ChanReader[T]`, `Send` — только на `ChanWriter[T]`.
- Pattern проверяется против `Option[T]` для recv, `bool` для send.
- Не более одного `default` arm — compile error.
- `Default` arm должен быть последним — compile error иначе.
- guard expression — тип `bool`.

---

## Ограничения bootstrap

- **send arm в select**: Ф.2 — реализуем, но требует `send_waiters`
  в SelectCtx. Полезно отложить на Ф.2 если Ф.1 (recv-only) уже даёт
  ценность.
- **arm guards**: Ф.4 — опциональны, не блокируют core select.
- **biased mode** — не реализуем в bootstrap. Tokio-style детерминизм
  для тестов достигается через `--jobs 1` + фиксированный seed.
- **вложенный select** — не поддерживается в bootstrap (сложная отмена
  вложенных SelectCtx). Compile error.
- **Time.after(d)** — Ф.5; требует новой функции в eventloop.h.

---

## Фазы

### Ф.1: Runtime + recv-only select
- [ ] `channels.h`: `SelectCtx`, `SelectArm`, `nova_select_init/set_recv/
  try_immediate/park/wake/cancel_others`
- [ ] Fisher-Yates shuffle + `nova_rand_u32()` (LCG, seed из uv_hrtime)
- [ ] stop_cb для cancel_scope integration
- [ ] `sched.h`: изменений нет — park/wake API используется как есть

### Ф.2: Send arm в select
- [ ] `nova_select_set_send()` + send-waiter логика в SelectCtx
- [ ] Codegen: send arm dispatch

### Ф.3: Парсер + codegen (recv-only + default)
- [ ] Новый keyword `select` в лексере
- [ ] `parse_select()` в parser/mod.rs
- [ ] AST: `ExprKind::Select { arms: Vec<SelectArm> }`
- [ ] type-checker: проверки arm-типов, один default, last default
- [ ] `emit_select()` в emit_c.rs
- [ ] Dispatch по `_sc.which` + pattern binding

### Ф.4: Arm guards
- [ ] Parser: `if <expr>` после pattern в select arm
- [ ] Codegen: guard evaluation → `nova_select_set_recv(..., guard_val)`

### Ф.5: Time.after(d) + тесты + spec
- [ ] `nova_rt/eventloop.h` или `time.h`: `nova_time_after(double)`
- [ ] emit_c.rs: dispatch `Time.after(d)` → `Nova_ChanReader*`
- [ ] `nova_tests/concurrency/select.nv`:
  - fan-in: два rx, один select, assert LIFO-free sum
  - closed channel arm: None-match triggers break
  - default arm: non-blocking poll
  - timeout: Time.after(0.1) + assert fired
  - send arm: select между tx.send и rx.recv
  - guards: arm с guard=false пропускается
  - cancel_scope: select отменяется при cancel
- [ ] D94 в `spec/decisions/06-concurrency.md`
- [ ] Regression: все тесты зелёные

---

## Definition of Done

- `select { Some(v) = rx1.recv() => {...} Some(v) = rx2.recv() => {...} }`
  компилируется и корректно работает
- Пробуждается ровно по одному arm'у, остальные unlinked
- `default` arm работает как non-blocking try
- `None = rx.recv()` arm срабатывает на closed channel
- Send arm работает: `_ = tx.send(v) => {}`
- Arm guards работают: `if cond` пропускает arm
- Timeout через `Time.after(d)` работает как обычный recv arm
- Fairness: Fisher-Yates shuffle (нет starvation на многочисленных тестах)
- cancel_scope отменяет все pending select waiters
- Все существующие тесты зелёные (0 regressions)
- D94 зафиксирован в spec
