---
source_rev: 07df7d2c9
source_date: 2026-08-02
---

# Каналы и `select` в Nova

[English](channels.md) | **Русский**

`Channel[T]` — основной примитив межфибровой коммуникации. Модель —
**разделение прав** (Rust mpsc-style): `Channel.new(cap)` возвращает
**пару** объектов с разделёнными правами — `ChanWriter[T]` («только
слать») и `ChanReader[T]` («только получать»).

`select { ... }` — мультиплексированные операции канала: ожидает
несколько `recv`/`send` одновременно, просыпается по первой готовой
ветви.

Спецификация: [D91](../../spec/decisions/06-concurrency.md#d91) (ревизия
каналов) + [D94](../../spec/decisions/06-concurrency.md#d94) (select).

---

## Содержание

- [Quickstart](#quickstart)
- [`Channel.new`](#channelnew)
- [`ChanWriter[T]` API](#chanwritert-api)
- [`ChanReader[T]` API](#chanreadert-api)
- [Идиомы](#идиомы)
  - [Drain через `while let`](#drain-через-while-let)
  - [Producer/consumer](#producerconsumer)
  - [Ping-pong](#ping-pong)
  - [Fan-in (multi-writer)](#fan-in-multi-writer)
  - [Relay (cross-channel pipeline)](#relay-cross-channel-pipeline)
  - [Передача в функции](#передача-в-функции)
- [`select { ... }`](#select--)
  - [Синтаксис и семантика](#синтаксис-и-семантика)
  - [Recv arm](#recv-arm)
  - [Send arm](#send-arm)
  - [Guard arms](#guard-arms)
  - [Default arm](#default-arm)
  - [Wildcard `_ = rx`](#wildcard-_--rx)
  - [Timeout через `ChanReader.close_after`](#timeout-через-chanreaderclose_after)
  - [Multi-arm fairness](#multi-arm-fairness)
- [`supervised(cancel:)` + `select`](#supervisedcancel--select)
- [Закрытие канала](#закрытие-канала)
- [Panic-сценарии](#panic-сценарии)
- [Bootstrap-ограничения](#bootstrap-ограничения)
- [Связанные документы](#связанные-документы)

---

## Quickstart

```nova
test "channel: send + recv FIFO" {
    ro { tx, rx } = Channel.new(4)
    tx.send(10)
    tx.send(20)
    tx.send(30)
    ro a = rx.recv()
    ro b = rx.recv()
    ro c = rx.recv()
    assert(a ?? -1 == 10)
    assert(b ?? -1 == 20)
    assert(c ?? -1 == 30)
    tx.close()
}
```

```nova
test "select: data wins over timeout" {
    ro ch = Channel.new(1)
    ro tx = ch.tx
    ro rx = ch.rx
    mut branch = 0
    supervised {
        spawn {
            tx.send(99)
            select {
                Some(v) = rx                                          => { branch = v }
                Some(_) = ChanReader.close_after(Duration.from_millis(200)) => { branch = -1 }
            }
        }
    }
    assert(branch == 99)
}
```

---

## `Channel.new`

```nova
fn Channel[T].new(capacity int) -> { tx ChanWriter[T], rx ChanReader[T] }
```

Возвращает **пару** — запись с полями `tx` (право отправки) и `rx`
(право получения). Поддерживает три формы извлечения:

```nova
// 1. Record destructure (Plan 53, most idiomatic)
ro { tx, rx } = Channel.new(4)

// 2. Record destructure with renaming
ro { tx: sender, rx: receiver } = Channel.new(4)

// 3. Tuple destructure (compat with D91 spec examples)
ro (tx, rx) = Channel.new(4)

// 4. Record access (when distinct lifetimes are needed)
ro ch = Channel.new(4)
ro tx = ch.tx
ro rx = ch.rx
```

**Ёмкость ≥ 1.** `Channel.new(0)` сейчас паникует с
`"capacity must be >= 1"` ([Plan 44.1](../plans/44.1-channel-hardening.md)
Ф.3) — каналы rendezvous с нулевой ёмкостью пока не реализованы.

**Тип передачи (`T`)** выводится из первого `send`/`recv`:

```nova
ro { tx, rx } = Channel.new(8)
tx.send(42)         // T = int
ro v = rx.recv()   // Option[int]
```

Явная аннотация — turbofish: `Channel[int].new(8)`.

**`T` должен помещаться в слово ([M-channel-generic-elem-type]).** Vela
(M:N-рантайм) хранит каждый элемент в одном слоте размером со слово,
поэтому `T` должен без потерь укладываться в него: `int`, `bool`, `char`,
целые фиксированной ширины и любой тип размером с указатель (`[]T`,
записи, `HashMap`, суммы, …) — работают. `T`, не влезающий в слово —
`str`, `f32`/`f64`, кортежи, value-записи — отвергается на этапе
компиляции (`E_CHANNEL_UNSOUND_ELEM_TYPE`), а не тихо
усекается/переинтерпретируется.

---

## `ChanWriter[T]` API

| Метод | Сигнатура | Семантика |
|---|---|---|
| `send` | `(v T) -> bool` | Блокирующий `send`. Возвращает `true`, если отправил; `false`, если канал закрыт (не паника — [Plan 30](../plans/30-channel-improvements.md)) |
| `try_send` | `(v T) -> bool` | Неблокирующий `try_send`. `true`, если поместилось; `false`, если буфер полон или канал закрыт |
| `close` | `() -> ()` | Закрывает право записи. Идемпотентный. С несколькими отправителями (`share`) — со счётчиком ссылок: канал реально закрывается только когда закрылись все отправители |
| `share` | `() -> ChanWriter[T]` | Создаёт дополнительного отправителя поверх того же буфера. `writer_count++` |
| `is_closed` | `() -> bool` | `true`, если буфер закрыт *и* у этого отправителя нет права отправлять |

### `send` возвращает `bool`

```nova
test "channel: send after close returns false, does not panic" {
    ro { tx, rx: _rx } = Channel.new(2)
    assert(tx.send(1))
    tx.close()
    assert(!tx.send(99))    // false: channel closed
}
```

Полезно для корректного завершения без обёртки в `try/catch`:

```nova
fn produce(tx ChanWriter[Job], jobs []Job) {
    mut i = 0
    while i < jobs.len() {
        if !tx.send(jobs[i]) {
            break               // consumer closed — exit silently
        }
        i = i + 1
    }
}
```

### `try_send` — non-blocking

```nova
test "channel: try_send full buffer" {
    ro { tx, rx } = Channel.new(2)
    assert(tx.try_send(10))
    assert(tx.try_send(20))
    assert(!tx.try_send(30))            // buffer full
    assert(rx.recv() ?? -1 == 10)
    assert(tx.try_send(30))             // slot freed
    tx.close()
}
```

### `share` — multi-writer

> **Именование (Plan 201, 2026-07-13):** метод называется `share()`, НЕ
> `clone()` — протокол `Clone` в Nova означает независимую глубокую копию,
> а здесь **псевдоним (alias)** того же канала (второе право поверх того
> же буфера; канал закрывается, только когда закроется последний
> отправитель). То же правило именует `TcpStream.share()`. Семантика
> псевдонима = `share`, глубокая копия = `clone` — везде в std.

```nova
test "channel: fan-in — two writers, one reader" {
    ro { tx, rx } = Channel.new(8)
    ro tx2 = tx.share()                // writer_count = 2
    mut sum = 0
    supervised {
        spawn { tx.send(1);  tx.send(2);  tx.send(3);  tx.close() }
        spawn { tx2.send(10); tx2.send(20); tx2.send(30); tx2.close() }
        spawn {
            while Some(v) = rx.recv() { sum = sum + v }
        }
    }
    assert(sum == 66)
}
```

Канал закрывается **только когда все отправители вызвали `close()`**.
Внутри — счётчик ссылок (`writer_count`): `Channel.new` инициализирует
в 1, `share()` инкрементирует, `close()` декрементирует. Когда достигает
0 — канал реально закрывается, `rx.recv()` начинает возвращать `None`.

---

## `ChanReader[T]` API

| Метод | Сигнатура | Семантика |
|---|---|---|
| `recv` | `() -> Option[T]` | Блокирующий `recv`. `Some(v)`, пока есть данные или канал открыт; `None`, когда канал закрыт *и* буфер пуст |
| `try_recv` | `() -> Option[T]` | Неблокирующий `try_recv`. `None`, если буфер пуст (НЕ означает, что канал закрыт — проверяй `is_closed()` отдельно) |
| `len` | `() -> int` | Количество элементов в буфере *сейчас* |
| `capacity` | `() -> int` | Ёмкость, заданная в `Channel.new` |
| `is_closed` | `() -> bool` | `true`, если все отправители закрылись |

### `recv` → `Option[T]`

Закрытый канал — **не ошибка**, валидный исход «источник закончился».
`Option[T]` композируется с `match`, `?`, `??` и идиоматичным циклом
`while let`.

```nova
test "channel: close + recv drain" {
    ro { tx, rx } = Channel.new(4)
    tx.send(1)
    tx.send(2)
    tx.close()
    assert(rx.recv() ?? -1 == 1)
    assert(rx.recv() ?? -1 == 2)
    assert(rx.recv().is_none())             // drained — None
    assert(rx.recv().is_none())             // repeated — still None
}
```

### `try_recv` различает empty-open vs empty-closed

```nova
test "channel: try_recv distinguishes empty-open from empty-closed via is_closed" {
    ro { tx, rx } = Channel.new(4)
    assert(rx.try_recv().is_none())     // empty, open
    assert(!rx.is_closed())
    tx.close()
    assert(rx.try_recv().is_none())     // empty, closed — same None
    assert(rx.is_closed())              // distinguish via is_closed
}
```

### `len` / `capacity`

```nova
test "channel: len and capacity" {
    ro { tx, rx } = Channel.new(8)
    assert(rx.capacity() == 8)
    assert(rx.len() == 0)
    tx.send(1)
    tx.send(2)
    assert(rx.len() == 2)
    ro _ = rx.recv()
    assert(rx.len() == 1)
    tx.close()
}
```

---

## Идиомы

### Drain через `while let`

```nova
test "channel: while-let drain pattern" {
    ro { tx, rx } = Channel.new(4)
    tx.send(10)
    tx.send(20)
    tx.send(30)
    tx.close()
    mut sum = 0
    while Some(v) = rx.recv() {
        sum = sum + v
    }
    assert(sum == 60)
}
```

Это **самый идиоматичный** шаблон получателя. Цикл завершается
автоматически, когда канал закрылся и буфер пуст — `recv()` вернёт
`None`.

### Producer/consumer

```nova
test "channel: producer-consumer pipeline" {
    ro { tx, rx } = Channel.new(4)
    mut sum = 0
    supervised {
        spawn {
            tx.send(1)
            tx.send(2)
            tx.send(3)
            tx.send(4)
            tx.send(5)
            tx.close()                  // important: producer closes after finishing
        }
        spawn {
            while Some(v) = rx.recv() {
                sum = sum + v
            }
        }
    }
    assert(sum == 15)
}
```

### Ping-pong

```nova
test "channel: ping-pong" {
    ro { tx: tx1, rx: rx1 } = Channel.new(1)
    ro { tx: tx2, rx: rx2 } = Channel.new(1)
    mut result = 0
    supervised {
        spawn {
            tx1.send(10)
            ro reply = rx2.recv()
            result = reply ?? -1
            tx1.close()
        }
        spawn {
            ro msg = rx1.recv()
            tx2.send((msg ?? 0) * 2)
            tx2.close()
        }
    }
    assert(result == 20)
}
```

### Fan-in (multi-writer)

Несколько файберов производят, один потребляет.

```nova
ro { tx, rx } = Channel.new(8)
supervised {
    for item in work_items {
        ro worker_tx = tx.share()      // each spawn gets its own capability
        spawn {
            worker_tx.send(process(item))
            worker_tx.close()
        }
    }
    tx.close()                          // close the root writer
    spawn {
        while Some(v) = rx.recv() {
            collect(v)
        }
    }
}
```

**Почему `share()` обязателен:** без него все файберы захватили бы один
`tx` через управляемую ссылку; `close()` первого закрыл бы канал для
всех. С `share()` каждый файбер держит своё право и закрывает его
независимо — канал закрывается только когда все `worker_count + 1`
отправителей вызвали `close()`.

### Relay (cross-channel pipeline)

```nova
fn relay(rx ChanReader[int], tx ChanWriter[int]) {
    while Some(v) = rx.recv() {
        tx.send(v * 2)
    }
    tx.close()
}

test "channel: relay — Receiver → Sender pipeline through a function" {
    ro { tx: tx1, rx: rx1 } = Channel.new(4)
    ro { tx: tx2, rx: rx2 } = Channel.new(4)
    tx1.send(1)
    tx1.send(2)
    tx1.send(3)
    tx1.close()
    relay(rx1, tx2)
    mut s = 0
    while Some(v) = rx2.recv() { s = s + v }
    assert(s == 12)
}
```

### Передача в функции

Права в сигнатурах делают API явным.

```nova
fn fill_channel(tx ChanWriter[int], values []int) {
    mut i = 0
    while i < values.len() {
        tx.send(values[i])
        i = i + 1
    }
    tx.close()
}

fn drain_channel(rx ChanReader[int]) -> int {
    mut sum = 0
    while Some(v) = rx.recv() {
        sum = sum + v
    }
    sum
}

test "channel: Sender and Receiver passed independently" {
    ro { tx, rx } = Channel.new(8)
    fill_channel(tx, [100, 200, 300])
    ro s = drain_channel(rx)
    assert(s == 600)
}
```

Передать `tx` в функцию, которая не должна уметь `recv` — система типов
гарантирует, что вызываемая сторона не сможет прочитать (и наоборот).

---

## `select { ... }`

### Синтаксис и семантика

```
select-expr  = 'select' '{' NL* select-arm+ '}'
select-arm   = channel-arm | default-arm
channel-arm  = pattern '=' (recv-target | send-op) guard? '=>' arm-body NL*
recv-target  = expr                                 // bare rx
send-op      = expr '.' 'send' '(' expr ')'
guard        = 'if' expr
default-arm  = '_' '=>' arm-body NL*
arm-body     = block | stmt
```

> **Bootstrap-форма recv**: `Some(v) = rx => { ... }` — `rx` напрямую,
> без `.recv()`. Спецификация описывает также форму `pattern = rx.recv()`;
> текущий компилятор принимает только форму без `.recv()`.

**Семантика** ([D94](../../spec/decisions/06-concurrency.md#d94)):

1. **Вычисление охранного условия** — `if <expr>` перед стрелкой
   отключает ветвь, когда false.
2. **Немедленная проверка** — все включённые ветви проверяются в
   псевдослучайном порядке (Fisher-Yates). Если готова хотя бы одна —
   ветвь выполняется без приостановки.
3. **Приостановка** — если ни одна не готова и нет default:
   зарегистрировать ожидающего на каждой ветви, приостановить файбер.
4. **Пробуждение** — первая готовая ветвь будит файбер; остальные
   ожидающие отвязываются. Флаг `done` предотвращает двойное
   пробуждение.
5. **Справедливость** — перемешивание Fisher-Yates на каждой итерации
   (нет голодания).
6. **`_ => ...` (default)** — если присутствует: шаг 2 всегда успешен;
   файбер никогда не приостанавливается.
7. **Все каналы закрыты + нет default** → паника
   `"select: all channels closed"`.
8. **Отмена** (`tok.cancel()` из `supervised(cancel:)`) — отменяет всех
   ожидающих; файбер просыпается, проверяет `cancel_requested`.

### Recv arm

```nova
test "select single recv: value from channel" {
    ro ch = Channel.new(1)
    ro tx = ch.tx
    ro rx = ch.rx
    supervised {
        spawn { tx.send(42) }
        spawn {
            mut got = 0
            select {
                Some(v) = rx => { got = v }
            }
            assert(got == 42)
        }
    }
}
```

### Send arm

```nova
test "select send arm: sends to channel with space" {
    ro ch = Channel.new(1)
    ro tx = ch.tx
    ro rx = ch.rx
    mut sent = 0
    select {
        tx.send(77) => { sent = 1 }
        _           => { sent = -1 }
    }
    assert(sent == 1)
    ro opt = rx.recv()
    mut got = 0
    match opt {
        Some(v) => { got = v }
        None    => { got = -1 }
    }
    assert(got == 77)
}
```

### Guard arms

```nova
test "select guard: disabled arm falls through to default" {
    ro ch = Channel.new(1)
    ch.tx.send(10)
    ro rx = ch.rx
    ro enabled = false
    mut branch = 0
    select {
        Some(v) = rx if enabled => { branch = v }
        _                       => { branch = -1 }
    }
    assert(branch == -1)         // arm disabled — default ran
}
```

Охранное условие — предусловие. Если `false`, ветвь выключена ещё до
проверки готовности канала. Аналог `if` в Tokio `select!`. Go охранные
условия не поддерживает.

### Default arm

`_ => { ... }` — выполняется, если ни одна ветвь канала не готова
*сейчас*. Превращает `select` в неблокирующий зонд.

```nova
test "select recv with default: default when channel empty" {
    ro ch = Channel.new(1)
    ro rx = ch.rx
    mut branch = 0
    select {
        Some(_) = rx => { branch = 1 }
        _            => { branch = 2 }     // ← default
    }
    assert(branch == 2)
}
```

### Wildcard `_ = rx`

Подстановочный знак в цели приёма срабатывает на **оба** состояния:
`Some(v)` и `None` (закрытый канал). `Some(v) = rx` срабатывает только на
реальное значение.

```nova
test "Some arm skips closed+empty, picks open channel with data" {
    ro ch1 = Channel.new(1)
    ro ch2 = Channel.new(1)
    ro tx1 = ch1.tx
    ro tx2 = ch2.tx
    ro rx1 = ch1.rx
    ro rx2 = ch2.rx

    tx1.close()                  // ch1 closed+empty
    tx2.send(42)                 // ch2 has data

    mut result = 0
    select {
        Some(v) = rx1 => { result = -1 }     // Some does NOT fire on closed
        Some(v) = rx2 => { result = v  }     // ← runs
    }
    assert(result == 42)
}

test "wildcard fires immediately on closed+empty channel" {
    ro ch = Channel.new(1)
    ro tx = ch.tx
    ro rx = ch.rx
    tx.close()

    mut fired = false
    select {
        _ = rx => { fired = true }           // ← wildcard catches closed
    }
    assert(fired)
}
```

**Правило:**
- `Some(v) = rx` — нужно реальное значение из канала
- `_ = rx` — нужно **любое** готовое состояние (значение или закрытие)

`None = rx` отдельной ветвью пока не реализован (Plan 31 §«Отличия от
спецификации»); для дифференциации используйте `_ = rx` + `match` внутри
тела ветви или `rx.is_closed()` после `recv`.

### Timeout через `ChanReader.close_after`

Специальной ветви `timeout =>` нет — таймаут это обычный канал приёма,
создаваемый `ChanReader.close_after(Duration)`.

```nova
import std.time.duration

test "select timeout: fires when channel stays empty" {
    ro ch = Channel.new(1)
    ro rx = ch.rx
    mut branch = 0
    supervised {
        spawn {
            select {
                Some(_) = rx                                          => { branch = 1 }
                Some(_) = ChanReader.close_after(Duration.from_millis(50)) => { branch = 2 }
            }
        }
    }
    assert(branch == 2)
}

test "select timeout: data wins over timeout" {
    ro ch = Channel.new(1)
    ro tx = ch.tx
    ro rx = ch.rx
    mut branch = 0
    supervised {
        spawn {
            tx.send(99)
            select {
                Some(v) = rx                                           => { branch = v }
                Some(_) = ChanReader.close_after(Duration.from_millis(200)) => { branch = -1 }
            }
        }
    }
    assert(branch == 99)
}
```

`ChanReader.close_after(d Duration) -> ChanReader[()]` — реализован в
[`std/concurrency/timer.nv`](../std/concurrency/timer.nv) как встроенная
функция компилятора (под капотом — `nova_chan_reader_close_after_ns(d.nanos)`).
Канал закрывается через `d`; первый `recv()` возвращает `Some(())` после
срабатывания, потом `None`.

**Типобезопасность** (Plan 65 revision 2026-05-18): ранее API назывался
`Time.after(int ms)` — голый `int` (мс/мкс/сек?). Теперь — типизированный
`Duration`. Миграция: `cargo run --bin migrate_plan65 -- --apply` —
переписывает литеральные аргументы автоматически
(см. [docs/guide/nova-cli.ru.md](nova-cli.ru.md#migrate_plan65)).

**Крайние случаи:**
- `Duration.ZERO` или `Duration.from_*(0)` — канал создаётся
  *уже* закрытым; первый `recv()` вернёт `None` без приостановки (быстрый
  путь, без таймера libuv)
- `Duration` короче миллисекунды (`from_nanos(500_000)`) — округляется
  **вверх** до 1 мс (гранулярность libuv)
- Отрицательный `Duration` — паника во время выполнения со значением
  в наносекундах

**Производительность:** сейчас каждый вызов выделяет свежий `uv_timer_t`
(~120 байт + системный вызов). Адекватно для идиоматичного использования
с 10–100 параллельными таймерами. Своё таймерное колесо для высокой
пропускной способности (10k+ HTTP-таймаутов) —
[Plan 66](../plans/66-timer-wheel-and-tick-every.md).

### Multi-arm fairness

```nova
test "select multi-arm: fairness — both channels get served" {
    ro n = 50
    ro ch1 = Channel.new(n)
    ro ch2 = Channel.new(n)
    ro tx1 = ch1.tx
    ro tx2 = ch2.tx
    ro rx1 = ch1.rx
    ro rx2 = ch2.rx

    mut from1 = 0
    mut from2 = 0

    supervised {
        spawn {
            mut i = 0
            while i < n {
                tx1.send(1)
                tx2.send(2)
                i += 1
            }
        }
        spawn {
            mut total = 0
            while total < n * 2 {
                select {
                    Some(v) = rx1 => { from1 += 1; ro _ = v }
                    Some(v) = rx2 => { from2 += 1; ro _ = v }
                }
                total += 1
            }
        }
    }
    assert(from1 > 0)
    assert(from2 > 0)
    assert(from1 + from2 == n * 2)
}
```

Перемешивание Fisher-Yates на каждой итерации обеспечивает, что оба канала
получают свою долю (Go использует тот же подход — `select` в Nova
семантически совместим).

---

## `supervised(cancel:)` + `select`

```nova
test "select: data wins supervised(cancel:) race" {
    ro ch = Channel.new(1)
    ro tx = ch.tx
    ro rx = ch.rx
    mut branch = 0
    mut error_seen = false

    ro tok = CancelToken.new()
    with Fail = handler Fail {
        fail(_msg) {
            error_seen = true
            interrupt ()
        }
    } {
        supervised(cancel: tok) {
            spawn {
                tx.send(77)
                Time.sleep(500)
                tok.cancel()
            }
            spawn {
                select {
                    Some(v) = rx                                           => { branch = v }
                    Some(_) = ChanReader.close_after(Duration.from_millis(200)) => { branch = -1 }
                }
            }
        }
    }
    assert(!error_seen)
    assert(branch == 77)
}
```

`tok.cancel()` отменяет **всех** ожидающих в любом блоке `select` внутри
`supervised(cancel: tok)`. Файбер просыпается, проверяет `cancel_requested`
и выходит из блока `supervised` через структурную отмену
(D75 / [Plan 49](../plans/49-cancel-throw-routing.md)).

Отмена **не ошибка** — она не превращается в `throw` и не вызывает
обработчик `Fail`. Поведение симметрично Go `context.Done()`, но с
типизированным `CancelToken` (D75) вместо канала ошибок.

---

## Закрытие канала

### Идиома: `defer tx.close()`

**Предпочтение спецификации** — `defer` гарантирует `close` при выходе из
области видимости:

```nova
fn run_pipeline() Net -> () {
    ro { tx, rx } = Channel[Job].new(10)
    defer tx.close()

    supervised {
        spawn { for j in jobs { tx.send(j) } }
        spawn { while Some(j) = rx.recv() { process(j) } }
    }
}   // <-- tx.close() always runs; rx.recv() in the spawn gets None and terminates
```

### Bootstrap-ограничение: `defer` + tuple-destructure

> ⚠️ **Известная проблема:** `defer tx.close()` **не** работает в
> сочетании с `let (tx, rx) = Channel.new(N)` или
> `let { tx, rx } = Channel.new(N)` — `defer` порождает фрейм setjmp *до*
> объявления переменных, что ломает область видимости (Plan 25 G8, будет
> устранено после внедрения встроенного `defer`).
>
> **Обход:** явный `tx.close()` в конце функции, либо разделить
> деструктуризацию:
>
> ```nova
> let ch = Channel.new(N)
> let tx = ch.tx
> let rx = ch.rx
> defer tx.close()    // OK — tx is declared directly
> // ...
> ```

### Auto-close на drop — нет

В отличие от Rust mpsc, Nova не имеет детерминированных деструкторов
(управляемая куча, [D6](../../spec/decisions/05-memory.md#d6)). GC соберёт
отправителя «когда-нибудь» — это **недетерминированно** и сделало бы тесты
нестабильными. Поэтому `close()` всегда явный.

### Idempotent

```nova
test "channel: close idempotent" {
    ro { tx, rx } = Channel.new(2)
    tx.close()
    tx.close()                  // not an error
    assert(rx.is_closed())
}
```

С несколькими отправителями (`share`) повторный `close()` *одного*
отправителя не декрементирует `writer_count` повторно (идемпотентно для
экземпляра).

---

## Panic-сценарии

| Условие | Сообщение |
|---|---|
| `Channel.new(0)` | `"capacity must be >= 1"` (Plan 44.1 Ф.3) |
| `select` со всеми закрытыми каналами и без default | `"select: all channels closed"` (Plan 31 Ф.6) |
| `ChanReader.close_after(<negative Duration>)` | паника со значением в наносекундах |
| `select` с `arm_count > stack` | переполнение ловится до выделения памяти — явная паника |

`tx.send` на закрытый канал — **не паника**, возвращает `false`
(Plan 30). `rx.recv` на закрытый и опустошённый — **не паника**, возвращает
`None`.

---

## Bootstrap-ограничения

| Что не работает / отложено | План |
|---|---|
| Отдельная ветвь `None = rx` (только подстановочный знак `_ = rx`) | Plan 31 followup |
| `Channel.new(0)` zero-capacity rendezvous | Plan 44.2+ |
| `defer tx.close()` + tuple/record destructure | [Plan 25](../plans/25-production-readiness-roadmap.md) G8 |
| `pattern = rx.recv()` (с `.recv()`) форма в select | работает только bare `pattern = rx` |
| `oneshot::channel<T>` / `watch::channel<T>` / `broadcast::channel<T>` (Tokio variants) | Plan 44.2 |
| `recv_many` batch API | Plan 44.1 Ф.4 follow-up |
| Разновидность SPSC без блокировок | Plan 50+ (Loom-verified) |
| `tick_every(Duration)` периодический тикер | [Plan 66](../plans/66-timer-wheel-and-tick-every.md) |
| `close_at(Monotonic)` абсолютный дедлайн | [Plan 65](../plans/65-chanreader-close-after.md) Ф.13 (✅ реализовано) |
| Имитация эффекта времени для детерминированных тестов таймеров | [Plan 65](../plans/65-chanreader-close-after.md) Ф.10 (✅ реализовано) |

---

## Связанные документы

- [`spec/decisions/06-concurrency.md`](../../spec/decisions/06-concurrency.md) —
  D79 / D91 / D94 / D75 / D97 (каналы, `select`, отмена, стеки файберов)
- [`docs/plans/21-channel-revision-implementation.md`](../plans/21-channel-revision-implementation.md)
  — реализация D91 (разделение прав)
- [`docs/plans/30-channel-improvements.md`](../plans/30-channel-improvements.md)
  — `send → bool` + `tx.share()`
- [`docs/plans/31-channel-select.md`](../plans/31-channel-select.md) —
  `select { ... }` (D94)
- [`docs/plans/44.1-channel-hardening.md`](../plans/44.1-channel-hardening.md)
  — промышленная безопасность M:N (атомарные операции, двусвязный список,
  выравнивание кэша)
- [`docs/plans/49-cancel-throw-routing.md`](../plans/49-cancel-throw-routing.md)
  — семантика отмены (типизированный `CancelToken[T]`)
- [`docs/plans/65-chanreader-close-after.md`](../plans/65-chanreader-close-after.md)
  — `ChanReader.close_after(Duration)` (переименование `Time.after`)
- [`docs/plans/66-timer-wheel-and-tick-every.md`](../plans/66-timer-wheel-and-tick-every.md)
  — периодический тикер + собственное таймерное колесо (P2)
- [`std/concurrency/timer.nv`](../std/concurrency/timer.nv) —
  `ChanReader.close_after` doc-surface
- [`std/time/duration.nv`](../std/time/duration.nv) — тип `Duration`
- [`nova_tests/runtime/channels.nv`](../nova_tests/runtime/channels.nv)
  — 22 теста API каналов
- [`nova_tests/concurrency/`](../../nova_tests/concurrency/) —
  `select_*.nv` тесты (7 файлов)
