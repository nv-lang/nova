# Каналы и `select` в Nova

[English](channels.md) | **Русский**

`Channel[T]` — основной примитив межфибровой коммуникации. Модель —
**capability-split** (Rust mpsc-style): `Channel.new(cap)` возвращает
**пару** объектов с разделёнными правами — `ChanWriter[T]` («только
слать») и `ChanReader[T]` («только получать»).

`select { ... }` — multiplexed channel operations: ожидает несколько
recv/send одновременно, просыпается по первому готовому arm'у.

Spec: [D91](../spec/decisions/06-concurrency.md#d91) (channel revision)
+ [D94](../spec/decisions/06-concurrency.md#d94) (select).

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
    let { tx, rx } = Channel.new(4)
    tx.send(10)
    tx.send(20)
    tx.send(30)
    let a = rx.recv()
    let b = rx.recv()
    let c = rx.recv()
    assert(a.unwrap_or(-1) == 10)
    assert(b.unwrap_or(-1) == 20)
    assert(c.unwrap_or(-1) == 30)
    tx.close()
}
```

```nova
test "select: data wins over timeout" {
    let ch = Channel.new(1)
    let tx = ch.tx
    let rx = ch.rx
    let mut branch = 0
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

Возвращает **пару** — record с полями `tx` (writer-capability) и `rx`
(reader-capability). Поддерживает три формы извлечения:

```nova
// 1. Record-destructure (Plan 53, наиболее идиоматично)
let { tx, rx } = Channel.new(4)

// 2. Record-destructure с переименованием
let { tx: sender, rx: receiver } = Channel.new(4)

// 3. Tuple destructure (compat с D91 spec примерами)
let (tx, rx) = Channel.new(4)

// 4. Record-access (когда нужны разные lifetimes)
let ch = Channel.new(4)
let tx = ch.tx
let rx = ch.rx
```

**Capacity ≥ 1.** `Channel.new(0)` сейчас панкует с
`"capacity must be >= 1"` ([Plan 44.1](plans/44.1-channel-hardening.md)
Ф.3) — zero-capacity rendezvous каналы пока не реализованы.

**Тип передачи (`T`)** выводится из первого `send`/`recv`:

```nova
let { tx, rx } = Channel.new(8)
tx.send(42)         // T = int
let v = rx.recv()   // Option[int]
```

Явная аннотация — turbofish: `Channel[str].new(8)`.

---

## `ChanWriter[T]` API

| Метод | Сигнатура | Семантика |
|---|---|---|
| `send` | `(v T) -> bool` | Blocking send. Возвращает `true` если отправил; `false` если канал закрыт (не panic — [Plan 30](plans/30-channel-improvements.md)) |
| `try_send` | `(v T) -> bool` | Non-blocking. `true` если поместилось; `false` если буфер полон или канал закрыт |
| `close` | `() -> ()` | Закрывает writer-capability. Idempotent. С multi-writer (`clone`) — ref-counted: канал реально закрывается только когда все writers закрылись |
| `clone` | `() -> ChanWriter[T]` | Создаёт дополнительный writer на тот же буфер. `writer_count++` |
| `is_closed` | `() -> bool` | `true` если буфер закрыт *и* у этого writer'а нет capability слать |

### `send` возвращает `bool`

```nova
test "channel: send после close возвращает false, не паникует" {
    let { tx, rx: _rx } = Channel.new(2)
    assert(tx.send(1))
    tx.close()
    assert(!tx.send(99))    // false: канал закрыт
}
```

Полезно для graceful shutdown без обёртки в `try/catch`:

```nova
fn produce(tx ChanWriter[Job], jobs []Job) {
    let mut i = 0
    while i < jobs.len() {
        if !tx.send(jobs[i]) {
            break               // consumer закрылся — выходим тихо
        }
        i = i + 1
    }
}
```

### `try_send` — non-blocking

```nova
test "channel: try_send full buffer" {
    let { tx, rx } = Channel.new(2)
    assert(tx.try_send(10))
    assert(tx.try_send(20))
    assert(!tx.try_send(30))            // буфер полон
    assert(rx.recv().unwrap_or(-1) == 10)
    assert(tx.try_send(30))             // место освободилось
    tx.close()
}
```

### `clone` — multi-writer

```nova
test "channel: fan-in — два writer'а, один reader" {
    let { tx, rx } = Channel.new(8)
    let tx2 = tx.clone()                // writer_count = 2
    let mut sum = 0
    supervised {
        spawn { tx.send(1);  tx.send(2);  tx.send(3);  tx.close() }
        spawn { tx2.send(10); tx2.send(20); tx2.send(30); tx2.close() }
        spawn {
            while let Some(v) = rx.recv() { sum = sum + v }
        }
    }
    assert(sum == 66)
}
```

Канал закрывается **только когда все writers вызвали `close()`**.
Внутри — ref-count (`writer_count`): `Channel.new` инициализирует в 1,
`clone()` инкрементирует, `close()` декрементирует. Когда достигает 0
— канал реально закрывается, `rx.recv()` начинает возвращать `None`.

---

## `ChanReader[T]` API

| Метод | Сигнатура | Семантика |
|---|---|---|
| `recv` | `() -> Option[T]` | Blocking recv. `Some(v)` пока есть данные или канал открыт; `None` когда канал closed *и* буфер пуст |
| `try_recv` | `() -> Option[T]` | Non-blocking. `None` если буфер пуст (НЕ означает что канал закрыт — проверяй `is_closed()` отдельно) |
| `len` | `() -> int` | Количество элементов в буфере *сейчас* |
| `capacity` | `() -> int` | Capacity, заданная в `Channel.new` |
| `is_closed` | `() -> bool` | `true` если все writers закрылись |

### `recv` → `Option[T]`

Closed-channel — **не ошибка**, валидный исход «источник закончился».
`Option[T]` композируется с `match`, `?`, `??`, и идиоматичным
`while let`-loop'ом.

```nova
test "channel: close + recv drain" {
    let { tx, rx } = Channel.new(4)
    tx.send(1)
    tx.send(2)
    tx.close()
    assert(rx.recv().unwrap_or(-1) == 1)
    assert(rx.recv().unwrap_or(-1) == 2)
    assert(rx.recv().is_none())             // drain'нули — None
    assert(rx.recv().is_none())             // повторно — тоже None
}
```

### `try_recv` различает empty-open vs empty-closed

```nova
test "channel: try_recv различает empty-open от empty-closed через is_closed" {
    let { tx, rx } = Channel.new(4)
    assert(rx.try_recv().is_none())     // пустой открытый
    assert(!rx.is_closed())
    tx.close()
    assert(rx.try_recv().is_none())     // пустой закрытый — то же None
    assert(rx.is_closed())              // отличает через is_closed
}
```

### `len` / `capacity`

```nova
test "channel: len и capacity" {
    let { tx, rx } = Channel.new(8)
    assert(rx.capacity() == 8)
    assert(rx.len() == 0)
    tx.send(1)
    tx.send(2)
    assert(rx.len() == 2)
    let _ = rx.recv()
    assert(rx.len() == 1)
    tx.close()
}
```

---

## Идиомы

### Drain через `while let`

```nova
test "channel: while-let drain pattern" {
    let { tx, rx } = Channel.new(4)
    tx.send(10)
    tx.send(20)
    tx.send(30)
    tx.close()
    let mut sum = 0
    while let Some(v) = rx.recv() {
        sum = sum + v
    }
    assert(sum == 60)
}
```

Это **самый идиоматичный** receiver-pattern. Цикл завершается
автоматически, когда канал закрылся и буфер пуст — `recv()` вернёт
`None`.

### Producer/consumer

```nova
test "channel: producer-consumer pipeline" {
    let { tx, rx } = Channel.new(4)
    let mut sum = 0
    supervised {
        spawn {
            tx.send(1)
            tx.send(2)
            tx.send(3)
            tx.send(4)
            tx.send(5)
            tx.close()                  // важно: producer закрывает после finish
        }
        spawn {
            while let Some(v) = rx.recv() {
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
    let { tx: tx1, rx: rx1 } = Channel.new(1)
    let { tx: tx2, rx: rx2 } = Channel.new(1)
    let mut result = 0
    supervised {
        spawn {
            tx1.send(10)
            let reply = rx2.recv()
            result = reply.unwrap_or(-1)
            tx1.close()
        }
        spawn {
            let msg = rx1.recv()
            tx2.send(msg.unwrap_or(0) * 2)
            tx2.close()
        }
    }
    assert(result == 20)
}
```

### Fan-in (multi-writer)

Несколько spawn'ов производят, один потребляет.

```nova
let { tx, rx } = Channel.new(8)
supervised {
    for item in work_items {
        let worker_tx = tx.clone()      // каждому spawn'у — свой capability
        spawn {
            worker_tx.send(process(item))
            worker_tx.close()
        }
    }
    tx.close()                          // close корневого writer'а
    spawn {
        while let Some(v) = rx.recv() {
            collect(v)
        }
    }
}
```

**Почему `clone()` обязателен:** без него все spawn'ы захватили бы один
`tx` через managed reference; `close()` первого закрыл бы канал для
всех. С `clone()` каждый spawn держит свою capability и закрывает её
независимо — канал закрывается только когда все `worker_count + 1`
writers вызвали `close()`.

### Relay (cross-channel pipeline)

```nova
fn relay(rx ChanReader[int], tx ChanWriter[int]) {
    while let Some(v) = rx.recv() {
        tx.send(v * 2)
    }
    tx.close()
}

test "channel: relay — Receiver → Sender pipeline через функцию" {
    let { tx: tx1, rx: rx1 } = Channel.new(4)
    let { tx: tx2, rx: rx2 } = Channel.new(4)
    tx1.send(1)
    tx1.send(2)
    tx1.send(3)
    tx1.close()
    relay(rx1, tx2)
    let mut s = 0
    while let Some(v) = rx2.recv() { s = s + v }
    assert(s == 12)
}
```

### Передача в функции

Capability-types в сигнатурах делают API явным.

```nova
fn fill_channel(tx ChanWriter[int], values []int) {
    let mut i = 0
    while i < values.len() {
        tx.send(values[i])
        i = i + 1
    }
    tx.close()
}

fn drain_channel(rx ChanReader[int]) -> int {
    let mut sum = 0
    while let Some(v) = rx.recv() {
        sum = sum + v
    }
    sum
}

test "channel: Sender и Receiver передаются независимо" {
    let { tx, rx } = Channel.new(8)
    fill_channel(tx, [100, 200, 300])
    let s = drain_channel(rx)
    assert(s == 600)
}
```

Передать `tx` куда не нужно `recv` — type system гарантирует, что
получатель не сможет прочитать (и наоборот).

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

> **Bootstrap-форма recv**: `Some(v) = rx => { ... }` — bare `rx` без
> `.recv()`. Spec упоминает также `pattern = rx.recv()` форму; в
> текущем компиляторе работает только bare-форма.

**Семантика** ([D94](../spec/decisions/06-concurrency.md#d94)):

1. **Guard evaluation** — `if <expr>` перед стрелкой делает arm
   disabled когда false.
2. **Immediate check** — все enabled arms проверяются в
   псевдослучайном порядке (Fisher-Yates). Если ≥1 готов — выполняется
   без park'а.
3. **Park** — если ни один не готов и нет default: регистрирует waiter
   на каждый arm, паркует fiber.
4. **Wake** — первый готовый arm будит fiber; остальные waiters
   unlinked. `done`-флаг предотвращает double-wake.
5. **Fairness** — Fisher-Yates shuffle на каждой итерации (нет
   starvation).
6. **`_ => ...` (default)** — если присутствует: шаг 2 всегда
   succeeds, fiber не паркуется.
7. **Все каналы закрыты + нет default** → panic `"select: all channels closed"`.
8. **Cancel** (`tok.cancel()` от `supervised(cancel:)`) — отменяет все
   pending waiters; fiber просыпается, проверяет `cancel_requested`.

### Recv arm

```nova
test "select single recv: value from channel" {
    let ch = Channel.new(1)
    let tx = ch.tx
    let rx = ch.rx
    supervised {
        spawn { tx.send(42) }
        spawn {
            let mut got = 0
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
    let ch = Channel.new(1)
    let tx = ch.tx
    let rx = ch.rx
    let mut sent = 0
    select {
        tx.send(77) => { sent = 1 }
        _           => { sent = -1 }
    }
    assert(sent == 1)
    let opt = rx.recv()
    let mut got = 0
    match opt {
        Some(v) => { got = v }
        None    => { got = -1 }
    }
    assert(got == 77)
}
```

### Guard arms

```nova
test "select guard: disabled arm skips to default" {
    let ch = Channel.new(1)
    ch.tx.send(10)
    let rx = ch.rx
    let enabled = false
    let mut branch = 0
    select {
        Some(v) = rx if enabled => { branch = v }
        _                       => { branch = -1 }
    }
    assert(branch == -1)         // arm disabled — default сработал
}
```

Guard — pre-condition. Если `false`, arm выключен ещё до проверки
ready-state канала. Аналог `if` в Tokio `select!`. Go guard'ы не
поддерживает.

### Default arm

`_ => { ... }` — выполняется если ни один channel-arm не готов
*сейчас*. Превращает `select` в non-blocking.

```nova
test "select recv with default: default when channel empty" {
    let ch = Channel.new(1)
    let rx = ch.rx
    let mut branch = 0
    select {
        Some(_) = rx => { branch = 1 }
        _            => { branch = 2 }     // ← default
    }
    assert(branch == 2)
}
```

### Wildcard `_ = rx`

Wildcard в recv-target срабатывает на **оба** состояния: `Some(v)` и
`None` (closed). `Some(v) = rx` срабатывает только на реальное
значение.

```nova
test "Some arm skips closed+empty, picks open channel with data" {
    let ch1 = Channel.new(1)
    let ch2 = Channel.new(1)
    let tx1 = ch1.tx
    let tx2 = ch2.tx
    let rx1 = ch1.rx
    let rx2 = ch2.rx

    tx1.close()                  // ch1 closed+empty
    tx2.send(42)                 // ch2 has data

    let mut result = 0
    select {
        Some(v) = rx1 => { result = -1 }     // Some НЕ срабатывает на closed
        Some(v) = rx2 => { result = v  }     // ← выполнится
    }
    assert(result == 42)
}

test "wildcard fires immediately on closed+empty channel" {
    let ch = Channel.new(1)
    let tx = ch.tx
    let rx = ch.rx
    tx.close()

    let mut fired = false
    select {
        _ = rx => { fired = true }           // ← wildcard ловит closed
    }
    assert(fired)
}
```

**Правило:**
- `Some(v) = rx` — нужно реальное значение из канала
- `_ = rx` — нужен **любой** ready-state (значение или closed)

`None = rx` отдельным arm пока не реализован (Plan 31 §«Отличия от
spec»); для дифференциации используйте `_ = rx` + `match` внутри тела
arm'а или `rx.is_closed()` после `recv`-а.

### Timeout через `ChanReader.close_after`

Специального `timeout =>` arm'а нет — timeout это обычный
recv-канал, создаваемый `ChanReader.close_after(Duration)`.

```nova
import std.time.duration

test "select timeout: fires when channel stays empty" {
    let ch = Channel.new(1)
    let rx = ch.rx
    let mut branch = 0
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
    let ch = Channel.new(1)
    let tx = ch.tx
    let rx = ch.rx
    let mut branch = 0
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
[`std/concurrency/timer.nv`](../std/concurrency/timer.nv) как
compiler-builtin (под капотом `nova_chan_reader_close_after_ns(d.nanos)`).
Канал закрывается через `d`; первый `recv()` возвращает `Some(())`
после firing'а, потом `None`.

**Type safety** (Plan 65 revision 2026-05-18): ранее API назывался
`Time.after(int ms)` — bare int (мс/мкс/сек?). Теперь — типизированный
`Duration`. Migration: `cargo run --bin migrate_plan65 -- --apply` —
переписывает literal-аргументы автоматически
(см. [docs/nova-cli.ru.md](nova-cli.ru.md#migrate_plan65)).

**Edge-cases:**
- `Duration.ZERO` или `Duration.from_*(0)` — канал создаётся
  *уже* закрытым; первый `recv()` вернёт `None` без yield (fast path,
  без libuv timer)
- Sub-millisecond `Duration` (`from_nanos(500_000)`) — округляется
  **вверх** до 1 ms (libuv granularity)
- Негативный `Duration` — runtime panic с nanosecond-значением

**Performance:** сейчас каждый вызов аллоцирует свежий `uv_timer_t`
(~120 байт + syscall). Адекватно для idiomatic 10-100 concurrent
timers. Custom timer-wheel для high-throughput (10k+ HTTP timeouts)
— [Plan 66](plans/66-timer-wheel-and-tick-every.md).

### Multi-arm fairness

```nova
test "select multi-arm: fairness — both channels get served" {
    let n = 50
    let ch1 = Channel.new(n)
    let ch2 = Channel.new(n)
    let tx1 = ch1.tx
    let tx2 = ch2.tx
    let rx1 = ch1.rx
    let rx2 = ch2.rx

    let mut from1 = 0
    let mut from2 = 0

    supervised {
        spawn {
            let mut i = 0
            while i < n {
                tx1.send(1)
                tx2.send(2)
                i += 1
            }
        }
        spawn {
            let mut total = 0
            while total < n * 2 {
                select {
                    Some(v) = rx1 => { from1 += 1; let _ = v }
                    Some(v) = rx2 => { from2 += 1; let _ = v }
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

Fisher-Yates shuffle на каждой итерации обеспечивает, что оба канала
получают свою долю (Go использует тот же подход — `select` в Nova
семантически совместим).

---

## `supervised(cancel:)` + `select`

```nova
test "select: data wins supervised(cancel:) race" {
    let ch = Channel.new(1)
    let tx = ch.tx
    let rx = ch.rx
    let mut branch = 0
    let mut error_seen = false

    let tok = CancelToken.new()
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

`tok.cancel()` отменяет **все** pending waiters в любом `select`-блоке
внутри `supervised(cancel: tok)`. Fiber просыпается, проверяет
`cancel_requested`, и выходит из supervised-блока через структурную
отмену (D75 / [Plan 49](plans/49-cancel-throw-routing.md)).

Cancellation **не ошибка** — она не превращается в `throw`, не
вызывает Fail-handler. Поведение симметрично Go `context.Done()`, но
с типизированным `CancelToken` (D75) вместо `error`-канала.

---

## Закрытие канала

### Идиома: `defer tx.close()`

**Spec preference** — `defer` гарантирует close при выходе из scope:

```nova
fn run_pipeline() Net -> () {
    let { tx, rx } = Channel[Job].new(10)
    defer tx.close()

    supervised {
        spawn { for j in jobs { tx.send(j) } }
        spawn { while let Some(j) = rx.recv() { process(j) } }
    }
}   // <-- tx.close() сработает гарантированно; rx.recv() в spawn'е получит None и завершится
```

### Bootstrap-ограничение: `defer` + tuple-destructure

> ⚠️ **Известная проблема:** `defer tx.close()` **не** работает в
> сочетании с `let (tx, rx) = Channel.new(N)` или
> `let { tx, rx } = Channel.new(N)` — `defer` эмитит setjmp-frame
> *до* объявления переменных, что ломает scope (Plan 25 G8, будет
> устранено когда внедрят open-coded defer).
>
> **Workaround:** explicit `tx.close()` в конце функции, либо
> разделить destructure:
>
> ```nova
> let ch = Channel.new(N)
> let tx = ch.tx
> let rx = ch.rx
> defer tx.close()    // OK — tx объявлен напрямую
> // ...
> ```

### Auto-close на drop — нет

В отличие от Rust mpsc, Nova не имеет deterministic destructor'ов
(managed heap, [D6](../spec/decisions/05-memory.md#d6)). GC соберёт
sender «когда-нибудь» — это **недетерминированно** и сделало бы тесты
flaky. Поэтому `close()` всегда explicit.

### Idempotent

```nova
test "channel: close idempotent" {
    let { tx, rx } = Channel.new(2)
    tx.close()
    tx.close()                  // не error
    assert(rx.is_closed())
}
```

С multi-writer (`clone`) повторный `close()` *одного* writer'а не
декрементирует `writer_count` повторно (idempotent per-instance).

---

## Panic-сценарии

| Условие | Сообщение |
|---|---|
| `Channel.new(0)` | `"capacity must be >= 1"` (Plan 44.1 Ф.3) |
| `select` со всеми каналами closed + без default | `"select: all channels closed"` (Plan 31 Ф.6) |
| `ChanReader.close_after(<negative Duration>)` | panic с nanosecond-значением |
| `select` с `arm_count > stack` | overflow ловится до allocate'а — explicit panic |

`tx.send` на closed-канал — **не panic**, возвращает `false`
(Plan 30). `rx.recv` на closed+drained — **не panic**, возвращает
`None`.

---

## Bootstrap-ограничения

| Что не работает / отложено | План |
|---|---|
| `None = rx` отдельный arm (только `_ = rx` wildcard) | Plan 31 followup |
| `Channel.new(0)` zero-capacity rendezvous | Plan 44.2+ |
| `defer tx.close()` + tuple/record destructure | [Plan 25](plans/25-production-readiness-roadmap.md) G8 |
| `pattern = rx.recv()` (с `.recv()`) форма в select | работает только bare `pattern = rx` |
| `oneshot::channel<T>` / `watch::channel<T>` / `broadcast::channel<T>` (Tokio variants) | Plan 44.2 |
| `recv_many` batch API | Plan 44.1 Ф.4 follow-up |
| Lock-free SPSC flavor | Plan 50+ (Loom-verified) |
| `tick_every(Duration)` periodic ticker | [Plan 66](plans/66-timer-wheel-and-tick-every.md) |
| `close_at(Monotonic)` absolute deadline | [Plan 65](plans/65-chanreader-close-after.md) Ф.13 (✅ реализовано) |
| Time-effect mock для deterministic timer-тестов | [Plan 65](plans/65-chanreader-close-after.md) Ф.10 (✅ реализовано) |

---

## Связанные документы

- [`spec/decisions/06-concurrency.md`](../spec/decisions/06-concurrency.md) —
  D79 / D91 / D94 / D75 / D97 (channels, select, cancel, fiber stacks)
- [`docs/plans/21-channel-revision-implementation.md`](plans/21-channel-revision-implementation.md)
  — D91 implementation (capability-split)
- [`docs/plans/30-channel-improvements.md`](plans/30-channel-improvements.md)
  — `send → bool` + `tx.clone()`
- [`docs/plans/31-channel-select.md`](plans/31-channel-select.md) —
  `select { ... }` (D94)
- [`docs/plans/44.1-channel-hardening.md`](plans/44.1-channel-hardening.md)
  — production-grade M:N safety (atomics, doubly-linked, cache padding)
- [`docs/plans/49-cancel-throw-routing.md`](plans/49-cancel-throw-routing.md)
  — cancel semantics (typed `CancelToken[T]`)
- [`docs/plans/65-chanreader-close-after.md`](plans/65-chanreader-close-after.md)
  — `ChanReader.close_after(Duration)` (rename от `Time.after`)
- [`docs/plans/66-timer-wheel-and-tick-every.md`](plans/66-timer-wheel-and-tick-every.md)
  — periodic ticker + custom timer-wheel (P2)
- [`std/concurrency/timer.nv`](../std/concurrency/timer.nv) —
  `ChanReader.close_after` doc-surface
- [`std/time/duration.nv`](../std/time/duration.nv) — `Duration` type
- [`nova_tests/runtime/channels.nv`](../nova_tests/runtime/channels.nv)
  — 22 теста channel API
- [`nova_tests/concurrency/`](../nova_tests/concurrency/) —
  `select_*.nv` тесты (7 файлов)
