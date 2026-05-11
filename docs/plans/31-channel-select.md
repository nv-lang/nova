// SPDX-License-Identifier: MIT OR Apache-2.0
# План 31: `select` — multiplexed channel receive

**Статус:** 🟡 план, не начат.
**Дата создания:** 2026-05-11.
**Зависимости:**
- [Plan 21](21-channel-revision-implementation.md) — ✅ закрыт; D91 capability-split реализован.
- [Plan 30](30-channel-improvements.md) — желательно завершить до (park/wake стабилен).

---

## Цель

Реализовать `select` — ожидание на нескольких каналах одновременно,
пробуждение по первому готовому. Это ключевой примитив для:
- fan-in (merge нескольких producers)
- timeout на recv (`select { rx.recv() | Time.sleep(1.0) }`)
- non-blocking multi-channel опрос

---

## Синтаксис (предлагаемый)

```nova
select {
    Some(v) = rx1.recv() => {
        // обработка v из rx1
    }
    Some(v) = rx2.recv() => {
        // обработка v из rx2
    }
    _ = Time.sleep(1.0) => {
        // timeout
    }
}
```

Альтернатива (Go-style с `case`):

```nova
select {
    case Some(v) = rx1.recv() => { ... }
    case Some(v) = rx2.recv() => { ... }
    case _ = Time.sleep(1.0)  => { ... }
}
```

**Решение принять при реализации парсера** — зафиксировать в
`spec/decisions/06-concurrency.md` как D94.

---

## Архитектура

### Runtime: multi-park

`select` на N каналах = регистрация N `ChannelWaiter` одновременно,
парковка fiber'а. Когда любой из каналов готов:
1. Будит fiber через `nova_sched_wake`
2. Остальные waiters отменяются через stop_cb (unlink из списков)

Структура:

```c
typedef struct SelectWaiter {
    ChannelWaiter waiters[N];  // N = число arm'ов
    int           which;       // индекс сработавшего arm'а
    NovaFiberQueue* scope;
    int             slot;
} SelectWaiter;
```

Stop_cb при wake одного arm'а: итерирует остальные waiters, unlink'ает их.

### Codegen

`select { arm1 => body1; arm2 => body2 }` →

```c
{
    SelectWaiter _sw;
    _sw.scope = _nova_active_scope;
    _sw.slot  = _nova_active_slot;
    _sw.which = -1;
    // регистрация arm'ов
    nova_chan_reader_select_arm(&_sw, 0, rx1);
    nova_chan_reader_select_arm(&_sw, 1, rx2);
    // park
    nova_sched_park(_sw.scope, _sw.slot);
    // dispatch по _sw.which
    if (_sw.which == 0) { ... }
    else if (_sw.which == 1) { ... }
}
```

### Парсер

Новый keyword `select` (или re-use `match`-like структуры). Парсер строит
`ExprKind::Select { arms: Vec<SelectArm> }` где:

```rust
struct SelectArm {
    pat:  Pattern,   // Some(v), _
    recv: Expr,      // rx.recv() или Time.sleep(n)
    body: Block,
}
```

---

## Ограничения scope

- **Plan 31 scope**: только `rx.recv()` arms. `Time.sleep` arm требует
  интеграции с `uv_timer_t` (отдельный arm-type).
- **default arm** (`_ => { ... }` без блокировки) = non-blocking select.
  Реализовать в Ф.2.
- **Вложенный select** — не поддерживается в bootstrap (сложная отмена).

---

## Фазы

### Ф.1: Runtime — SelectWaiter + multi-park
- [ ] `channels.h`: `SelectWaiter`, `nova_chan_reader_select_arm()`,
  `nova_select_wait()`, `nova_select_cancel_others()`
- [ ] `sched.h`: если нужны изменения в park/wake API

### Ф.2: Парсер — `select { ... }`
- [ ] Новый keyword `select` в лексере
- [ ] `parse_select()` в parser/mod.rs
- [ ] AST: `ExprKind::Select { arms }`
- [ ] type-checker: walk select arms

### Ф.3: Codegen — emit_select
- [ ] `emit_select()` в emit_c.rs
- [ ] Dispatch по `_sw.which`
- [ ] Pattern binding из recv result

### Ф.4: `default` arm (non-blocking)
- [ ] `select { ... default => { ... } }` — если ни один не готов сразу

### Ф.5: Тесты + spec
- [ ] `nova_tests/concurrency/select.nv`: fan-in, priority, default arm
- [ ] D94 в `spec/decisions/06-concurrency.md`
- [ ] regression

---

## Definition of Done

- `select { Some(v) = rx1.recv() => {...} Some(v) = rx2.recv() => {...} }`
  компилируется и корректно работает
- Пробуждается ровно по одному arm'у, остальные отменяются
- `default` arm работает как non-blocking try
- Все существующие тесты зелёные
