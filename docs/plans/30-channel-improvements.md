// SPDX-License-Identifier: MIT OR Apache-2.0
# План 30: Channel improvements — send→bool + multi-writer

**Статус:** ✅ закрыт (2026-05-11).
**Дата создания:** 2026-05-11.
**Зависимости:**
- [Plan 21](21-channel-revision-implementation.md) — ✅ закрыт; D91 capability-split реализован.

---

## Цель

Два улучшения channel API после Plan 21:

1. **`send` → `bool`**: вместо `throw` на closed канал возвращает `false`.
   Устраняет неожиданный panic при гонке close/send.
2. **Multi-writer**: `writer_count` ref-count в `Nova_ChannelState` +
   `tx.clone()` + закрытие канала только когда все writers закрыты.

---

## Improvement 1: `send` → `bool`

### Мотивация

Сейчас `nova_chan_writer_send()` делает `nova_throw()` если канал закрыт.
Это неожиданный panic — вызывающий код не может его предотвратить без
`try/catch` (errdefer). В Rust `send()` возвращает `Result<(), SendError<T>>`.
У нас `Result` — тяжелее чем нужно; достаточно `bool`.

Семантика после изменения:
- канал открыт, буфер не полон → отправляет, возвращает `true`
- канал открыт, буфер полон → паркуется, ждёт места, возвращает `true`
- канал закрыт → возвращает `false`, не throw

`try_send` семантика не меняется (non-blocking, `false` = полный ИЛИ закрыт).

### Не ломает существующий код

В C возврат функции можно игнорировать. Весь существующий код `tx.send(42)`
без использования возврата компилируется без изменений. Nova-codegen
эмитирует statement-вызов — `nova_chan_writer_send(tx, v);` без присвоения,
если Nova-код не использует результат.

### Изменения

**`compiler-codegen/nova_rt/channels.h`:**
- `nova_chan_writer_send()`: тип возврата `void` → `nova_bool`
- убрать `nova_throw(...)` на `st->closed`, заменить на `return 0`
- в parking loop: если после wake `st->closed` → `return 0`

**`compiler-codegen/src/codegen/emit_c.rs`:**
- dispatch `"send"` на `Nova_ChanWriter*`:
  - если результат используется (let binding) → emit как expression
  - если statement → emit как statement (без изменений в codegen,
    C сам игнорирует возврат)
- `infer_expr_c_type` для `ChanWriter*.send` → `"nova_bool"` (было `"nova_unit"`)

**`nova_tests/runtime/channels.nv`:**
- добавить тесты: `assert(!tx.send(42))` после `tx.close()`
- добавить тест: send после close возвращает false, не паникует

---

## Improvement 2: Multi-writer (tx.clone())

### Мотивация

Сейчас `Channel.new()` создаёт один `Nova_ChanWriter*` и один
`Nova_ChanReader*`. Если два fiber'а должны писать в один канал, нет
безопасного способа — оба держат указатель на один `Nova_ChanWriter`,
и `close()` одного сразу закрывает канал для второго.

В Rust `mpsc::Sender` реализует `Clone` — ref-counted. Канал закрывается
когда все senders дропнуты. Это нужно для паттерна fan-in:

```nova
ro (tx, rx) = Channel.new(8)
ro tx2 = tx.clone()
supervised {
    spawn { tx.send(1); tx.close() }
    spawn { tx2.send(2); tx2.close() }
    spawn {
        while Some(v) = rx.recv() { ... }
    }
}
```

### Архитектура

**`writer_count: int32_t`** в `Nova_ChannelState` (atomic в будущем M:N,
сейчас single-threaded — обычный int достаточно).

- `nova_channel_new()` → `state->writer_count = 1`
- `nova_chan_writer_clone(tx)` → создаёт новый `Nova_ChanWriter*` с тем же
  `state`, инкрементирует `state->writer_count`
- `nova_chan_writer_close(tx)` → декрементирует `writer_count`; если
  `writer_count == 0` → помечает `closed = true` и будит всех waiters

### Nova API

```nova
ro (tx, rx) = Channel.new(8)
ro tx2 = tx.clone()   // новый ChanWriter на тот же буфер
```

### Изменения

**`compiler-codegen/nova_rt/channels.h`:**
- `Nova_ChannelState`: добавить поле `int32_t writer_count`
- `nova_channel_new()`: `state->writer_count = 1`
- `nova_chan_writer_clone(tx)`: `nova_alloc` нового `Nova_ChanWriter`,
  копирует `state`, инкрементирует `state->writer_count`
- `nova_chan_writer_close(tx)`: декрементирует; закрывает только если 0

**`compiler-codegen/src/codegen/emit_c.rs`:**
- dispatch `"clone"` на `Nova_ChanWriter*` → `nova_chan_writer_clone(obj)`
- return type `"Nova_ChanWriter*"`

**`nova_tests/runtime/channels.nv`:**
- тест fan-in: два spawned writers, один reader, assert sum correct
- тест clone+close порядок: clone, close original → канал НЕ закрыт;
  close clone → канал закрыт

---

## Фазы

### Ф.1: `send` → `bool`
- [x] channels.h: возврат `nova_bool`, убрать throw
- [x] emit_c.rs: type inference `send` → `"nova_bool"`
- [x] channels.nv: новые тесты (Секция 7, 3 теста)
- [x] regression: 159/159 PASS

### Ф.2: Multi-writer (tx.clone())
- [x] channels.h: `writer_count` + `nova_chan_writer_clone` + новый `close`
- [x] emit_c.rs: dispatch `clone` на ChanWriter + infer_expr_c_type → `"Nova_ChanWriter*"`
- [x] channels.nv: fan-in тест, clone/close тесты (Секция 8, 3 теста)
- [x] regression: зелёные

### Ф.3: Spec uplift + retro
- [x] D91 Bootstrap-status: пометить Improvement 1+2 ✅
- [x] docs/project-creation.txt + docs/simplifications.md retro
- [x] commit

---

## Definition of Done

- `tx.send(v)` возвращает `bool` — false если закрыт, не throw
- `tx.clone()` создаёт дополнительный writer на тот же буфер
- Канал закрывается только когда все writers вызвали `close()`
- Все существующие тесты продолжают проходить
- Новые тесты: send-after-close → false, fan-in pipeline

---

## Ф.4: Post-close review (2026-05-11)

После закрытия плана проведён анализ channels API относительно Rust/Go.
Найдены и исправлены реальные дефекты (коммит `88504b87c`).

### Исправлено

**Б1 — double-close одного writer портил writer_count.**
Guard `if (st->closed)` не защищал per-writer — второй вызов `close(tx)` на
том же handle декрементировал `writer_count` повторно, закрывая канал раньше
времени для других clones. Исправление: поле `writer_closed bool` в
`Nova_ChanWriter`, guard по нему.

**Б2 — recv/send вне fiber context вызывали `abort()`.**
В Go это паника (recoverable), в Rust — паника через unwind. Заменено на
`nova_throw()`. Убраны `<stdio.h>` / `<stdlib.h>` из channels.h.

**Н1 — `try_recv`/`try_send` не различали "пусто" от "закрыт".**
Оба случая возвращали `false`/`None`. Caller не мог понять, ждать ли данных
или канал уже закрыт. Добавлен `NovaChanTryResult {OK, EMPTY, CLOSED}`.
Nova API не меняется (emit_c.rs конвертирует в `bool`/`Option`);
caller использует `rx.is_closed()` для различения после `None`.

**Н2 — `Channel.new(0)` тихо создавал capacity=1.**
Теперь `nova_throw("Channel.new: capacity must be >= 1")`. Nova каналы
всегда buffered; capacity=0 (rendezvous) не поддерживается.

### Оставшийся tech debt

| # | Проблема | Когда чинить |
|---|----------|-------------|
| T1 | `writer_count` — `int32_t`, не atomic | Plan 23 (M:N threading) |
| T2 | WaiterList — singly-linked, O(n) unlink при cancel | при нагрузочных тестах |
| T3 | `try_recv` None не различим без `is_closed()` в Nova коде | после generics/type system |
