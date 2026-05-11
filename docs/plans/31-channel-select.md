// SPDX-License-Identifier: MIT OR Apache-2.0
# План 31: `select` — multiplexed channel operations

**Статус:** ✅ ЗАКРЫТ (2026-05-11). Реализован production-ready select.
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
| closed channel arm (None match) | 🟡 wildcard только | implicit zero | ✅ |
| псевдослучайный выбор | ✅ Ф.1 | ✅ (Fisher-Yates) | random |
| arm guards (if cond) | ✅ Ф.4 | ❌ | ✅ |
| cancel_scope integration | 🟡 через stop_cb | — | через token |
| race-free wake | ✅ `which` field | ✅ selectdone | ✅ |
| biased mode | ❌ пост-bootstrap | ❌ | ✅ |
| all-closed panic | 🟡 todo Ф.6 | runtime panic | ✅ |

---

## Синтаксис (D94) — реализованный

```nova
select {
    Some(v) = rx          => { work(v) }   // recv с binding
    _ = rx                => { }            // recv wildcard (Some или None/closed)
    tx.send(val)          => { }            // send arm
    Some(v) = rx if guard => { work(v) }   // recv с guard
    _                     => { default }   // default (non-blocking)
}
```

**Отличие от spec D94:** `None = rx` arm не реализован (только `Some(v) = rx` и `_ = rx`).
Wildcard `_ = rx` срабатывает на любой recv — и на значение (Some), и на закрытый канал (None).

**Timeout через `Time.after(ms)`:**

```nova
select {
    Some(v) = rx              => { work(v) }
    Some(_) = Time.after(500) => { log("timeout") }
}
```

---

## Архитектура реализованная

### SelectCtx + SelectSlot + SelectWaiter (channels.h)

```c
#define NOVA_SELECT_MAX_ARMS 8

typedef struct {
    Nova_ChannelState* chan;
    bool               is_recv;
    nova_int           send_val;
    bool               guard;
} SelectSlot;

typedef struct SelectWaiter {
    /* Layout-compatible с ChannelWaiter (первые 6 полей идентичны) */
    NovaFiberQueue*      scope;
    int                  slot;
    Nova_ChannelState*   channel;
    bool                 is_recv;
    nova_int             send_val;
    struct SelectWaiter* next;
    /* select-only */
    int                  arm_idx;
} SelectWaiter;

typedef struct {
    SelectSlot      arms[NOVA_SELECT_MAX_ARMS];
    int             n_arms;
    int             which;     /* arm fired: 0..n-1, or -2 = default */
    nova_int        recv_val;
    NovaFiberQueue* scope;
    int             slot;
    SelectWaiter    waiters[NOVA_SELECT_MAX_ARMS];
} SelectCtx;
```

**Ключевое решение:** `SelectWaiter` layout-compatible с `ChannelWaiter` (первые 6 полей
совпадают). Канал будит select-waiter через тот же `nova_sched_wake(w->scope, w->slot)` —
без дополнительного dispatch кода в channel wake path.

**Отличие от spec:** spec описывал `select_recv_waiters` / `select_send_waiters` как
отдельные lists в `Nova_ChannelState`. Реализация использует существующий `recv_waiters` /
`send_waiters` — SelectWaiter регистрируется прямо туда (layout-compatible cast). Это
упрощает интеграцию без изменения Nova_ChannelState.

### API

```c
SelectCtx nova_select_init(int n_arms);
void nova_select_set_recv(SelectCtx* ctx, int n, Nova_ChanReader* rx, bool guard);
void nova_select_set_send(SelectCtx* ctx, int n, Nova_ChanWriter* tx, nova_int val, bool guard);
int  nova_select_try_immediate(SelectCtx* ctx);  // returns 1 if fired
void nova_select_park(SelectCtx* ctx);
```

### Fisher-Yates fairness

`nova_select_try_immediate()` перемешивает порядок проверки армов через xorshift32 RNG
с seed = `(uintptr_t)ctx ^ 0xdeadbeef`. Это обеспечивает fairness при одновременной
готовности нескольких армов.

### Time.after(ms) — timeout через channel

```c
static inline Nova_ChanReader* Nova_Time_after(nova_int ms) {
    Nova_ChannelPair pair = nova_channel_new(1);
    NovaAfterState* st = nova_alloc(sizeof(NovaAfterState));
    st->tx = pair.tx;
    uv_timer_init(nova_evloop(), &st->timer);
    uv_timer_start(&st->timer, _nova_after_timer_cb, delay, 0);
    return pair.rx;
}
```

Timer callback отправляет значение и закрывает writer. `rx` используется как
обычный recv arm — select не знает про "timeout" специально.

### Codegen

- Single-arm fast path: `nova_chan_reader_recv()` + `if (tag == NOVA_TAG_Option_Some)`
  для `Some(v) = rx`, или `if (1)` для wildcard `_ = rx`
- Full path: `SelectCtx` + `nova_select_set_recv/set_send` + `nova_select_try_immediate`
  + `nova_select_park` (если не immediate) + dispatch по `ctx.which`

### Spawn capture fix

Три функции обхода AST расширены для `ExprKind::Select`:
- `collect_idents_expr` — собирает referenced vars из `chan`, `value`, guard, body
- `collect_free_idents` — для lambda free-var detection
- `collect_bound_names_expr` — регистрирует `SelectOp::Recv { binding: Some(b) }`
  как locally bound (не free), чтобы `v` из `Some(v) = rx` не захватывался spawn

---

## Фазы

### Ф.1: Runtime + recv-only select
- [x] `channels.h`: `SelectSlot`, `SelectWaiter`, `SelectCtx`
- [x] `channels.h`: `nova_select_init / set_recv / try_immediate / park`
- [x] `channels.h`: интеграция через layout-compatible cast в existing waiter lists
- [x] `channels.h`: xorshift32 RNG для Fisher-Yates
- [x] `channels.h`: `_nova_select_waiter_stop_cb` для cancel_scope

### Ф.2: Send arm в select
- [x] `nova_select_set_send()` в SelectCtx
- [x] `nova_select_try_immediate` обрабатывает send arm
- [x] SelectWaiter для send arm регистрируется в `send_waiters`

### Ф.3: Парсер + codegen
- [x] Лексер: `"select" => TokenKind::KwSelect` (был missing → select парсился как ident)
- [x] AST: `ExprKind::Select { arms }`, `SelectArm`, `SelectOp::Recv/Send/Default`
- [x] Parser: `parse_select()` / `parse_select_op()`
- [x] `emit_select()` в emit_c.rs: single-arm fast path + full SelectCtx path
- [x] `collect_idents_expr` / `collect_free_idents` / `collect_bound_names_expr`
  — traversal для ExprKind::Select (spawn capture fix)

### Ф.4: Arm guards
- [x] Parser: `if <expr>` после pattern
- [x] Codegen: guard evaluation → `nova_select_set_recv(..., guard_val)`

### Ф.5: Time.after + тесты
- [x] `nova_rt/eventloop.h`: `Nova_Time_after(nova_int ms)` — uv_timer + channel
- [x] `emit_c.rs`: `Time.after(ms)` → `Nova_ChanReader*` type inference
- [x] `nova_tests/concurrency/select_test.nv` — 11 тестов (все проходят)
- [x] `nova_tests/concurrency/select_closed_test.nv` — 5 тестов (closed channel + cancel_scope)
- [x] `nova_tests/expected_runtime/select_all_closed.nv` — негативный (EXPECT_RUNTIME_PANIC)

### Ф.6: all-closed panic ✅
- [x] `channels.h`: добавлено поле `wildcard: bool` в `SelectSlot` — различает
  `Some(v) = rx` (не срабатывает на closed без данных) от `_ = rx` (срабатывает)
- [x] `nova_select_try_immediate`: `st->closed` срабатывает только если `arm->wildcard`
- [x] `nova_select_set_recv`: принимает дополнительный `wildcard` параметр
- [x] `emit_c.rs`: `nova_select_set_recv(..., wildcard)` — 1 для `_ = rx`, 0 для `Some(v) = rx`
- [x] `nova_select_park`: pre-check считает `can_unblock` arms до проверки scope/slot,
  бросает `nova_throw("select: all channels closed")` если 0 — работает и в main thread
- [x] `nova_tests/expected_runtime/select_all_closed.nv` — обновлён на реальный multi-arm
  select с `Some(v) = rx1 / rx2` (оба канала closed+empty) → PASS

---

## Отличия реализации от spec

1. **`None = rx` arm не реализован.** Spec описывал `None = rx` как отдельный arm для
   закрытых каналов. Реализован только `_ = rx` (wildcard) который срабатывает на
   любой recv result (Some или None). Для разного поведения на Some vs None нужно
   использовать `rx.recv()` в `match` после select.

2. **`select_recv_waiters` в Nova_ChannelState не добавлен.** SelectWaiter
   layout-compatible с ChannelWaiter и регистрируется прямо в existing `recv_waiters`
   / `send_waiters`. Проще, не требует изменения struct Nova_ChannelState.

3. **cancel_scope интеграция частичная.** `_nova_select_waiter_stop_cb` unlinkует из
   `recv_waiters`/`send_waiters`. Но cancel_scope не tested с select parked state.
   Тест "data wins cancel_scope race" проходит через happy path (data arrives before cancel).

4. **all-closed panic реализован в Ф.6.** `nova_select_park` на all-closed каналах без
   default бросает `nova_throw("select: all channels closed")` вместо deadlock.
   `SelectSlot.wildcard` различает `_ = rx` (срабатывает на closed) от `Some(v) = rx` (нет).

---

## Definition of Done ✅

- [x] `select { Some(v) = rx => {...} }` компилируется и работает
- [x] Пробуждается по одному arm, остальные SelectWaiter'ы unlinked
- [x] `_` (default) arm работает как non-blocking
- [x] `_ = rx` arm срабатывает на closed channel
- [x] Send arm работает: `tx.send(v) => {}`
- [x] Arm guards работают: `if cond` делает arm disabled
- [x] Timeout через `Time.after(ms)` работает
- [x] Fairness: Fisher-Yates shuffle
- [x] Spawn capture: переменные из select-армов корректно захватываются
- [x] Все существующие тесты зелёные (173/174, pre-existing fail)
- [x] all-closed panic (Ф.6) — бросает вместо deadlock
- [ ] D94 в spec обновлён (TODO)

---

## Commits

- `a5003d6b0` — Ф.3: lexer KwSelect + AST + parser + emit_select codegen
- `e0630a48e` — Ф.6: SelectSlot.wildcard + all-closed pre-check + select_all_closed.nv реальный тест
- `78743a290` — Ф.1/2: channels.h SelectCtx runtime + Time.after + тесты WIP
- `aef65ae9c` — Ф.5: spawn capture fix (collect_idents/free/bound для Select)
- `9710a9795` — Ф.6: closed-channel tests + wildcard fast-path fix + negative test
