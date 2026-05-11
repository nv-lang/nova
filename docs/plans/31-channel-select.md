// SPDX-License-Identifier: MIT OR Apache-2.0
# План 31: `select` — multiplexed channel operations

**Статус:** 🟡 план, не начат (обновлён 2026-05-11 — production-grade revision v2).
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
| race-free wake (CAS-like) | ✅ `done` flag | ✅ selectdone | ✅ |
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

**Грамматика:**

```
select-expr     = 'select' '{' NL* select-arm+ '}'
select-arm      = channel-arm | default-arm
channel-arm     = pattern '=' (recv-op | send-op) guard? '=>' arm-body NL*
recv-op         = expr '.' 'recv' '(' ')'
send-op         = expr '.' 'send' '(' expr ')'
guard           = 'if' expr
default-arm     = 'default' '=>' arm-body NL*
arm-body        = block | stmt
```

Синтаксис `pattern = rx.recv()` / `pattern = tx.send(val)` — обычное
присваивание-матч (аналог `let pat = expr`). Это не новый оператор `<-`,
что согласуется с `while let Some(v) = rx.recv()` (уже в языке).

**ВАЖНО:** Spec D79/D91 содержит устаревший синтаксис `msg <- ch` — он
**ЗАМЕНЯЕТСЯ** этим. D94 фиксирует финальный синтаксис.

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

Это Go-style (`time.After(d)` возвращает `<-chan time.Time`). Select не знает
про "timeout" специально — он просто recv на обычном канале. Это устраняет
специальный `timeout(expr) =>` синтаксис из старого spec D79 (который требовал
особой грамматики и runtime-интеграции).

### Arm guards

```nova
select {
    Some(v) = rx1.recv() if v > 0 => { process(v) }
    Some(v) = rx1.recv()           => { skip(v) }
}
```

Guard — опциональное `if <expr>` после pattern. Arm пропускается
(treated as not-ready) если guard false. Guard вычисляется **после** получения
значения, но **до** commit'а — если guard false, значение возвращается в буфер.

**Различие от Go:** Go не имеет guards в select. Rust Tokio `select!` имеет
`if <precond>` до операции (pre-condition, не post-receive guard). Nova следует
Rust: guard вычисляется как pre-condition, arm считается disabled.

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

1. **Guard evaluation** — до immediate-check вычисляем guard для каждого arm.
   Disabled arms (guard=false) пропускаются полностью.

2. **Immediate availability check** — проверяем каждый enabled arm
   в псевдослучайном порядке (Fisher-Yates shuffle). Если ≥1 ready —
   выполняем первый найденный (без park'а).

3. **All-closed check** — если все enabled arms — recv на closed channel
   (count==0, closed=true) и нет default → panic "select: all channels closed".

4. **Park** — если ни один не ready: устанавливаем `done=false`, регистрируем
   SelectWaiter для каждого enabled arm, паркуем fiber.

5. **Wake** — когда любой arm готов: вызывает wake-callback, который атомарно
   (CAS `done`: false→true) записывает `which` и будит fiber. Остальные
   SelectWaiter'ы unlinkуются из channel-waiter-list'ов через
   `nova_select_cancel_others()`.

6. **Fairness** — псевдослучайный порядок проверки (Fisher-Yates) на
   каждой итерации. При нескольких ready одновременно — один из них
   выбирается случайно, без starvation.

7. **cancel_scope** — если scope отменяется во время park'а:
   `nova_sched_cancel_all_pending` вызывает stop_cb для каждого
   SelectWaiter'а, все unlinkуются, fiber просыпается и проверяет
   `cancel_requested`.

8. **default arm** — если присутствует: шаг 2 (immediate check) всегда
   succeeds (default ready если все остальные not-ready). Никогда
   не park'аем.

---

## Архитектура

### Runtime: SelectCtx + SelectWaiter (channels.h)

```c
/* Один arm select: описывает channel + тип операции. */
typedef struct SelectArm {
    Nova_ChannelState* channel;
    bool               is_recv;        /* true = recv, false = send */
    nova_int           send_val;       /* для send arm */
    bool               guard;          /* arm active? (guard evaluation result) */
} SelectArm;

/* Контекст всего select-выражения.
 * Heap-alloc arms/waiters: разные select имеют разное N, известное
 * в compile time для конкретного select, но struct универсальный. */
typedef struct SelectCtx {
    SelectArm*      arms;              /* heap-alloc, n элементов */
    int             n;                 /* число arm'ов */
    volatile int    which;             /* индекс сработавшего arm (-1 = none yet) */
    volatile bool   done;              /* CAS-флаг: true = arm уже выбран */
    nova_int        recv_val;          /* принятое значение (recv arm) */
    bool            recv_is_some;      /* true = Some(v), false = None */
    NovaFiberQueue* scope;
    int             slot;
    SelectWaiter**  waiters;           /* heap-alloc, n элементов; NULL если arm неактивен */
} SelectCtx;

/* Waiter для одного arm в select. Расширяет ChannelWaiter back-pointer'ом
 * на SelectCtx — нужен для wake: один arm будит весь select. */
typedef struct SelectWaiter {
    /* Поля совместимые с ChannelWaiter (первые N полей идентичны
     * для safe cast в channel-waiter-list). */
    NovaFiberQueue*    scope;
    int                slot;
    Nova_ChannelState* channel;   /* NULL = unlinked */
    bool               is_recv;
    nova_int           send_val;
    struct SelectWaiter* next;    /* для channel-waiter-list */
    /* Select-specific: */
    SelectCtx*         ctx;       /* back-pointer на owner SelectCtx */
    int                arm_idx;   /* который arm этот waiter представляет */
} SelectWaiter;
```

**Почему `volatile bool done` а не атомик:**
Runtime — single-threaded cooperative. CAS не нужен. `done` флаг предотвращает
double-wake при одновременной готовности нескольких arm'ов (между immediate-check
и park нет yielding, но в send/recv wake два arm'а могут fire между yield-cycles).
`volatile` чтобы компилятор не оптимизировал read в `nova_select_wake`.

**Несовместимость типов SelectWaiter/ChannelWaiter:**
`SelectWaiter` НЕ является `ChannelWaiter` — это отдельный тип.
Channel-wake API (`_nova_channel_wake_recv`) работает с `ChannelWaiter*`.
Поэтому в channels.h нужен отдельный `SelectWaiter` list в `Nova_ChannelState`:

```c
struct Nova_ChannelState {
    /* ... существующие поля ... */
    ChannelWaiter*  recv_waiters;
    ChannelWaiter*  send_waiters;
    SelectWaiter*   select_recv_waiters;  /* Ф.1: select-specific */
    SelectWaiter*   select_send_waiters;  /* Ф.2: select-specific */
};
```

Альтернатива (union-waiter) сложнее — отдельные lists проще и безопаснее.

### API (channels.h)

```c
/* Инициализация SelectCtx. Аллоцирует arms[] и waiters[] через nova_alloc. */
void nova_select_init(SelectCtx* ctx, int n,
                      NovaFiberQueue* scope, int slot);

/* Настройка arm'а до немедленной проверки. guard=false → arm disabled. */
void nova_select_set_recv(SelectCtx* ctx, int i,
                          Nova_ChanReader* rx, bool guard);
void nova_select_set_send(SelectCtx* ctx, int i,
                          Nova_ChanWriter* tx, nova_int val, bool guard);

/* Попытка немедленного выполнения (без park). Fisher-Yates shuffle порядок.
 * Returns true если arm сработал (ctx->which, ctx->recv_val заполнены). */
bool nova_select_try_immediate(SelectCtx* ctx);

/* Регистрация SelectWaiter'ов для всех enabled arms + park.
 * После возврата — ctx->which ≥ 0 (или -1 если cancel).
 * Вызывает nova_select_cancel_others() до возврата. */
void nova_select_park(SelectCtx* ctx);

/* Wake-callback: атомарно (done: false→true) записывает which, recv_val,
 * recv_is_some; будит fiber. Idempotent — второй вызов игнорируется.
 * Вызывается из channel's send/recv/close path при обнаружении SelectWaiter. */
void nova_select_wake(SelectCtx* ctx, int which,
                      nova_int recv_val, bool recv_is_some);

/* Отменяет все SelectWaiter'ы кроме which.
 * Unlinkует из channel's select_recv/send_waiters lists.
 * Вызывается автоматически внутри nova_select_park после wake. */
void nova_select_cancel_others(SelectCtx* ctx, int which);

/* stop_cb для cancel_scope integration.
 * Вызывается из nova_sched_cancel_all_pending для каждого SelectWaiter.
 * Unlinkует waiter, если ctx->done — no-op (другой arm уже fired). */
NovaStopMode nova_select_waiter_stop_cb(void* handle);
```

### Интеграция wake в channel-path

Когда `nova_chan_writer_send` или `nova_chan_reader_recv` завершает операцию,
они проверяют не только обычный `recv_waiters`/`send_waiters`, но и
`select_recv_waiters`/`select_send_waiters`:

```c
/* В _nova_channel_wake_recv (вызывается после push в буфер): */
static inline void _nova_channel_wake_recv(Nova_ChannelState* st) {
    /* Сначала обычные waiters: */
    if (st->recv_waiters) { ... existing code ... return; }
    /* Затем select-waiters: */
    if (st->select_recv_waiters) {
        SelectWaiter* sw = st->select_recv_waiters;
        /* Unlink: */
        st->select_recv_waiters = sw->next;
        sw->channel = NULL;
        /* Передать значение из буфера: */
        nova_int v = st->buf[st->head];
        st->head = (st->head + 1) % st->cap;
        st->count--;
        nova_select_wake(sw->ctx, sw->arm_idx, v, /*is_some=*/true);
    }
}
```

Аналогично в `nova_chan_writer_close` — будит select-recv-waiters с `is_some=false`.

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
    if (!nova_select_try_immediate(&_sc) && !_has_default) {
        nova_select_park(&_sc);
    }

    int _which = _sc.which;
    if (_sc.done && _which == 0 && _sc.recv_is_some) {
        nova_int v = _sc.recv_val;
        /* body1 */
    } else if (_sc.done && _which == 1 && _sc.recv_is_some) {
        nova_int v = _sc.recv_val;
        /* body2 */
    } else {
        /* default — body3 */
    }
}
```

**"All channels closed" panic:**

```c
    /* После try_immediate: если all arms closed и нет default → panic. */
    if (!_sc.done && !_has_default) {
        /* check: все enabled recv arms закрыты и пусты */
        bool _all_closed = true;
        for (int _i = 0; _i < _sc.n; _i++) {
            if (!_sc.arms[_i].guard) continue;
            if (_sc.arms[_i].is_recv &&
                (_sc.arms[_i].channel->count > 0 || !_sc.arms[_i].channel->closed))
                { _all_closed = false; break; }
        }
        if (_all_closed) nv_panic("select: all channels closed");
        nova_select_park(&_sc);
    }
```

Фактически этот детект встроен в `nova_select_park` — если после регистрации
всех waiters обнаруживается что каналы уже закрыты (между immediate-check и
park) → `nova_select_wake` уже был вызван с правильным which.

### Fisher-Yates shuffle для fairness

```c
/* Внутри nova_select_try_immediate: */
int order[n];  /* VLA: n известен в compile time, но функция принимает ctx->n */
for (int i = 0; i < n; i++) order[i] = i;
for (int i = n - 1; i > 0; i--) {
    int j = (int)(nova_rand_u32() % (uint32_t)(i + 1));
    int tmp = order[i]; order[i] = order[j]; order[j] = tmp;
}
for (int k = 0; k < n; k++) {
    int i = order[k];
    if (!ctx->arms[i].guard) continue;
    /* check if ready... */
}
```

**`nova_rand_u32()` — LCG, глобальный seed:**

```c
/* channels.h (или nova_rt.h): */
static uint32_t _nova_rand_state = 0;

/* Инициализируется один раз при первом call или явно из eventloop init.
 * Не thread-safe — OK для single-threaded cooperative runtime (Plan 23 заменит). */
static inline void nova_rand_seed(uint64_t seed) {
    _nova_rand_state = (uint32_t)(seed ^ (seed >> 32));
    if (!_nova_rand_state) _nova_rand_state = 1;
}
static inline uint32_t nova_rand_u32(void) {
    if (!_nova_rand_state) nova_rand_seed((uint64_t)uv_hrtime());
    _nova_rand_state = _nova_rand_state * 1664525u + 1013904223u; /* Numerical Recipes LCG */
    return _nova_rand_state;
}
```

Инициализация seed из `uv_hrtime()` — lazy при первом вызове. Достаточно
для fairness; под M:N (Plan 23) заменяется на per-thread xorshift64.

### Time.after(d) — timeout через channel

```c
/* eventloop.h: */
Nova_ChanReader* nova_time_after(double seconds);
```

Создаёт `ChanReader[()]` + `uv_timer_t` с one-shot. Через `seconds` секунд:
1. timer callback: `nova_chan_writer_send(internal_tx, 0)`
2. `nova_chan_writer_close(internal_tx)`

Результат: `rx.recv()` возвращает `Some(0)` через d секунд, потом `None`.
Select использует это как обычный recv arm — никакой специальной интеграции.

**Lifetime:** `Nova_ChanReader*` возвращается вызывающему. `uv_timer_t`
держится в `uv_default_loop()`. Если receiver дропнут до срабатывания — timer
всё равно сработает (benign: send в closed channel → false, no-op). GC соберёт
ChanReader и underlying ChannelState после последней ссылки.

---

## Парсер

Новый keyword `select` в лексере (`TokenKind::KwSelect`). `parse_select()`
строит `ExprKind::Select { arms }`:

```rust
pub struct SelectArm {
    pub pat:   Pattern,          // Some(v), None, _, или pattern для send result
    pub op:    SelectOp,         // Recv(expr) | Send(expr, expr) | Default
    pub guard: Option<Box<Expr>>,
    pub body:  Block,
    pub span:  Span,
}

pub enum SelectOp {
    Recv(Box<Expr>),             // rx — выражение типа ChanReader[T]
    Send(Box<Expr>, Box<Expr>),  // tx, val — tx типа ChanWriter[T]
    Default,
}

// ExprKind::Select добавляется в ast/mod.rs:
Select {
    arms: Vec<SelectArm>,
},
```

**parse_select() логика:**
1. Expect `KwSelect`, `LBrace`
2. Loop: peek `RBrace` → done; peek `KwDefault` → parse default arm
3. Иначе: parse pattern (`parse_pattern()`), expect `Eq`, parse recv/send op
   (lookahead: ident `.` `recv` `(` `)` vs ident `.` `send` `(` expr `)`),
   optional guard (`KwIf` expr), `FatArrow` (`=>`), parse block

---

## Type-checker

- `Recv(rx_expr)`: `rx_expr` должен иметь тип `ChanReader[T]` → паттерн проверяется против `Option[T]`
- `Send(tx_expr, val_expr)`: `tx_expr` — `ChanWriter[T]`, `val_expr` — `T` → паттерн против `bool`
- `Default`: нет channel-операции, паттерн игнорируется
- Не более одного `Default` arm — compile error
- `Default` arm должен быть последним — compile error
- `if guard`: тип `bool`
- Arm body: все arms должны иметь одинаковый тип (как match)
- Нет send И recv на одном канале в одном select — разрешено (разные arms)

---

## Ограничения bootstrap

- **biased mode** — не реализуем. Tokio-style детерминизм для тестов
  достигается через `--jobs 1` + фиксированный seed.
- **вложенный select** — разрешён (SelectCtx stack-allocated,
  вложение безопасно). Нет ограничений на глубину.
- **select в realtime блоке** — compile error (park не разрешён в realtime).
- **нулевой select** (`select {}`) — compile error "empty select".
- **единственный arm без default** — корректно, блокирует до готовности.

---

## Фазы

### Ф.1: Runtime + recv-only select (без guards, без send arm)
- [ ] `channels.h`: добавить `SelectArm`, `SelectWaiter`, `SelectCtx`
- [ ] `channels.h`: добавить `select_recv_waiters` / `select_send_waiters` в `Nova_ChannelState`
- [ ] `channels.h`: `nova_select_init / set_recv / try_immediate / park / wake / cancel_others`
- [ ] `channels.h`: интеграция wake в `_nova_channel_wake_recv` + `nova_chan_writer_close`
- [ ] `channels.h`: `nova_rand_u32()` LCG + lazy seed из uv_hrtime()
- [ ] `channels.h`: `nova_select_waiter_stop_cb` для cancel_scope
- [ ] Тест runtime (C-level unit test в channels.c или inline): recv select работает

### Ф.2: Send arm в select
- [ ] `nova_select_set_send()` + SelectWaiter для send arm
- [ ] Интеграция wake в `_nova_channel_wake_send` (проверка select_send_waiters)
- [ ] `nova_select_cancel_others`: unlink из обоих lists (recv + send)
- [ ] Тест: select между tx.send и rx.recv

### Ф.3: Парсер + codegen (recv-only + default)
- [ ] Лексер: `TokenKind::KwSelect`, mapping `"select"` → `KwSelect`
- [ ] AST: `ExprKind::Select { arms: Vec<SelectArm> }`, `SelectOp`, `SelectArm`
- [ ] Parser: `parse_select()` — pattern, `=`, recv-op, guard?, `=>`, block
- [ ] Type-checker: arm-type checks, один default, last default
- [ ] `emit_select()` в emit_c.rs: init, set_recv, try_immediate, park, dispatch
- [ ] Dispatch по `_sc.which` + pattern binding (Some/None/wildcard)
- [ ] Codegen send arm: `nova_select_set_send()` в emit

### Ф.4: Arm guards
- [ ] Parser: `if <expr>` после pattern (перед `=>`)
- [ ] Codegen: guard evaluation → `nova_select_set_recv(..., guard_val)`

### Ф.5: Time.after(d) + тесты + spec
- [ ] `nova_rt/eventloop.h`: `nova_time_after(double seconds)` — uv_timer_t + internal channel
- [ ] `emit_c.rs`: dispatch `Time.after(d)` → `Nova_ChanReader*` (static method)
- [ ] `nova_tests/concurrency/select.nv`:
  - fan-in: два rx, один select, assert LIFO-free (оба arm'а срабатывают)
  - closed channel arm: None-match triggers break
  - default arm: non-blocking poll (channel empty → default fires)
  - timeout: Time.after(0.05) + assert fired перед данными
  - send arm: select между tx.send и rx.recv
  - guards: arm с guard=false пропускается
  - cancel_scope: select отменяется при cancel
- [ ] D94 в `spec/decisions/06-concurrency.md`:
  - Заменить устаревший `msg <- ch` синтаксис на `Some(v) = rx.recv()`
  - Заменить `timeout(expr) =>` на `Time.after(d)` idiom
  - Добавить таблицу Grammar, семантику, примеры
- [ ] Regression: все тесты зелёные

---

## Definition of Done

- `select { Some(v) = rx1.recv() => {...} Some(v) = rx2.recv() => {...} }`
  компилируется и корректно работает
- Пробуждается ровно по одному arm'у (`done` флаг предотвращает double-wake),
  остальные SelectWaiter'ы unlinked
- `default` arm работает как non-blocking try
- `None = rx.recv()` arm срабатывает на closed channel
- Send arm работает: `_ = tx.send(v) => {}`
- Arm guards работают: `if cond` делает arm disabled
- Timeout через `Time.after(d)` работает как обычный recv arm
- Fairness: Fisher-Yates shuffle (нет starvation на многочисленных тестах)
- cancel_scope отменяет все pending SelectWaiter'ы
- Все существующие тесты зелёные (0 regressions)
- D94 зафиксирован в spec; устаревший синтаксис `<-` удалён из D79/D91
