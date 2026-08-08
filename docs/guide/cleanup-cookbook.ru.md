---
source_rev: d006f7f9d
source_date: 2026-08-08
---

[English](cleanup-cookbook.md) | **Русский**

// SPDX-License-Identifier: MIT OR Apache-2.0
# Cleanup Cookbook — production-рецепты для `consume X = expr { body }`

> **План 110.** Книга production-рецептов для cleanup-семейства
> Nova V3 — паттерны миграции из Go/Rust/TS/Java/Kotlin, общие
> resource-паттерны (пулы соединений, файловые хендлы, транзакции,
> блокировки), анти-паттерны + отладка, советы по производительности.

## Раздел 1 — Паттерны миграции

### 1.1 Из Rust `Drop` trait

```rust
// Rust:
struct File { fd: i32 }
impl Drop for File {
    fn drop(&mut self) { unsafe { close(self.fd); } }
}

fn read(path: &str) -> Result<String, IoError> {
    let f = File::open(path)?;
    f.read_all()  // drop fires implicitly
}
```

```nova
// Nova:
type File { fd int }

fn File consume @cleanup(_outcome ScopeOutcome) -> () => @do_close()

fn read(path str) Fail[IoError] -> str {
    consume f = File.open(path)? {
        f.read_all()!!    // @cleanup fires explicitly
    }
}
```

**Разница:** `consume {}` в Nova делает cleanup **видимым** в точке вызова
(нет магии неявного drop). Асинхронный cleanup через `suspend` в `@cleanup`
(D191) работает «из коробки» — async-Drop в Rust не решён.

### 1.2 Из Go `defer`

```go
// Go:
func process(db *DB) error {
    tx, err := db.Begin()
    if err != nil { return err }
    defer func() {
        if r := recover(); r != nil {
            tx.Rollback()
            panic(r)
        }
    }()
    if err := doWork(tx); err != nil {
        tx.Rollback()  // manual rollback on error
        return err
    }
    return tx.Commit()
}
```

```nova
// Nova:
fn Transaction consume @cleanup(outcome ScopeOutcome) Fail[DbError] -> () {
    match outcome {
        Success   => @commit()!!
        Failure(_) => @rollback()!!
        Panic(_)  => @rollback_emergency()
    }
}

fn process(db Db) Fail[DbError] -> () {
    consume tx = db.begin()? {
        do_work()!!
    }
    // commit/rollback based on outcome — automatic.
}
```

**Разница:** Go-программист вручную различает success/error-пути, повторяя
rollback-логику. Nova маршрутизирует автоматически через `outcome`.

### 1.3 Из Java try-with-resources

```java
// Java:
try (Transaction tx = db.begin()) {
    doWork();
}  // tx.close() called; commit/rollback in the close() impl manually
```

```nova
// Nova:
consume tx = db.begin()? {
    do_work()!!
}
// outcome routing is built into Cleanup.@cleanup
```

**Разница:** `AutoCloseable.close()` в Java не различает success от error
(программист должен закодировать это в теле `close()`). В Nova outcome —
first-class.

### 1.4 Из TypeScript `using`

```typescript
// TS (ES2024):
{
    using tx = await db.begin();
    await doWork();
    // tx[Symbol.asyncDispose]() called
}
```

```nova
// Nova:
consume tx = await db.begin()? {
    await do_work()!!
}
```

**Разница:** TS `using` не имеет cancel-shield-by-default; доставка cancel
во время `Symbol.asyncDispose` может разломать cleanup.

### 1.5 Из Kotlin `.use{}`

```kotlin
// Kotlin:
file.use { f ->
    f.readText()
}  // f.close() called
```

```nova
// Nova:
consume f = file {
    f.read_text()!!
}
```

**Разница:** `.use{}` в Kotlin — extension-функция на `Closeable`. В Nova —
первоклассная language feature с типизированным error-dispatch.

## Раздел 2 — Resource-паттерны

### 2.1 Database Transaction

```nova
type Transaction { conn Connection, id int }

fn Transaction consume @cleanup(outcome ScopeOutcome) Fail[DbError] -> () {
    match outcome {
        Success      => @conn.commit(@id)!!
        Failure(err) => {
            if err is DbError.Deadlock {
                @conn.rollback(@id)!!           // graceful, retry-friendly
            } else {
                @conn.rollback_force(@id)!!     // hard rollback
            }
        }
        Panic(_) => @conn.rollback_force(@id)
    }
}

// Optional: per-instance timeout
fn Transaction @exit_timeout_ms() -> int => @conn.config.tx_timeout_ms

fn process_order(db Db, order Order) Fail[OrderError] Db -> Receipt {
    consume tx = db.begin()? {
        ro id = db.insert_order(order)!!
        db.notify_warehouse(id)!!
        return Receipt { order_id: id }
    }
}
```

### 2.2 Файловый хендл

```nova
type File { fd int }

fn File consume @cleanup(_outcome ScopeOutcome) Fail[IoError] -> () =>
    @do_close()!!

fn read_config(path str) Fail[IoError] -> Config {
    consume f = File.open(path, mode: ReadOnly)? {
        ro raw = f.read_all()!!
        Config.parse(raw)!!
    }
}
```

### 2.3 Mutex / блокировки — hot-path `Cleanup[never]`

```nova
// stdlib:
type MutexGuard { /* runtime opaque */ }
fn MutexGuard consume @cleanup(_outcome ScopeOutcome) -> () => @release()

// usage:
fn increment_counter(state State) -> () {        // no Fail[E]!
    consume _l = state.mutex.acquire() {         // Cleanup[never]
        state.value += 1
    }
}
```

**Hot-path оптимизация** (D194 §perf): codegen элидирует shield/timeout/
outcome для `Cleanup[never]` без `WithExitTimeout` — компилируется в
`state.value += 1; state.mutex.release()`. Нулевой оверхед против сырой пары
lock+release.

### 2.4 TCP socket с graceful-close

```nova
type TcpStream { /* opaque */ }

fn TcpStream consume @cleanup(outcome ScopeOutcome) Fail[IoError] -> () {
    match outcome {
        Success => {
            @send_eof()!!
            @wait_for_ack(timeout_ms: 1000)!!
            @close()!!
        }
        Failure(_) => @close()!!         // abort cleanup, no graceful
        Panic(_)   => @close()
    }
}

fn TcpStream @exit_timeout_ms() -> int => 5000   // a grace close can take time

fn handle_request(addr str) Fail[IoError] Net -> () {
    consume sock = TcpStream.connect(addr)? {
        sock.write_all(request)!!
        sock.read_all()!!
    }
}
```

### 2.5 Пул соединений

```nova
type PooledConn { pool ConnPool, conn Conn }

fn PooledConn consume @cleanup(_outcome ScopeOutcome) -> () => {
    @pool.release(@conn)             // return to pool, not close
}

fn query(pool ConnPool, sql str) Fail[DbError] -> Rows {
    consume conn = pool.acquire()? {
        conn.execute(sql)!!
    }
}
```

`Cleanup[never]` — release в пул никогда не падает (атомарная операция пула).

### 2.6 Builder-паттерн (сырой `consume`, не scope-block)

```nova
type StringBuilder consume {
    mut buf []u8
}

fn StringBuilder consume @as_str() -> str => str.from_bytes_unchecked_steal(@buf)

fn build_url(parts []str) -> str {
    mut sb = StringBuilder.new()
    for p in parts {
        sb.append(p)
    }
    sb.as_str()    // consume — a final conversion, not cleanup
}
```

Не используйте `consume X = ... { }` для transfer-паттернов — cleanup'а нет.

## Раздел 3 — Паттерн жизненного цикла приложения

```nova
fn main() Io Fail[IoError] -> () {
    with Application = Application.handler(default_exit_timeout_ms: 10_000) {
        // setup phase
        ro server = HttpServer.bind(":8080")!!

        // deep inside some constructor:
        // Application.register_finalizer(|| metrics.flush())

        server.serve()!!
    }
    // handler.on_exit fires finalizers in reverse-order (LIFO topo)
}
```

`default_exit_timeout_ms: 10_000` поднимает default для **всех** блоков
`consume{}` (которые не имеют своего `WithExitTimeout` impl) до 10 секунд.
Уровень-2 в 3-уровневом разрешении D192.

### Тестовая изоляция

```nova
fn test_user_creation() Io -> () {
    with Application = Application.handler() {
        Application.register_finalizer(|| reset_test_db())
        run_test_scenario()
    }
    // finalizers fire here, not shared with other tests
}
```

D195 R2/R3: вложенный Application имеет свой пустой registry + свой default
timeout (5s hardcoded, если не задан).

## Раздел 4 — FFI cleanup-обёртки

### 4.1 SQLite Connection (cross-ref План 100.5)

```nova
external type SqliteConn

extern "C" fn sqlite_open(path str) -> SqliteConn
extern "C" fn sqlite_close(conn SqliteConn) Fail[IoError] -> ()

// Wrap external resource in Cleanup:
fn SqliteConn consume @cleanup(_outcome ScopeOutcome) Fail[IoError] -> () =>
    sqlite_close(@)!!

fn query_users(db_path str) Fail[IoError] -> []User {
    consume conn = sqlite_open(db_path) {
        conn.query("SELECT * FROM users")!!
    }
}
```

Аттестация `cancellation-safety` для C-side: см. Plan 100.5 + Plan 110.7.

### 4.2 libcurl хендл

```nova
external type CurlHandle
extern "C" fn curl_init() -> CurlHandle
extern "C" fn curl_perform(h CurlHandle) Fail[NetError] -> []byte
extern "C" fn curl_cleanup(h CurlHandle) -> ()

fn CurlHandle consume @cleanup(_outcome ScopeOutcome) -> () => curl_cleanup(@)

fn fetch(url str) Fail[NetError] -> []byte {
    consume h = curl_init() {
        h.set_url(url)
        h.perform()!!
    }
}
```

## Раздел 5 — Анти-паттерны

### 5.1 Забытый `Cleanup` impl на новом resource-типе

```nova
type MyResource { handle int }

// ❌ DON'T:
fn use_it() -> () {
    consume r = MyResource.new() { ... }   // → D188-not-consumable
}
```

Предложение: реализуйте `Cleanup[E]` для resource-типа. Quick-fix
LSP code-action «implement Cleanup» (План 110.6).

### 5.2 Обёрнутый init без unwrap

```nova
// ❌ DON'T:
consume tx = db.maybe_begin() { ... }    // maybe_begin() : Option[Tx]
                                          // → D196-wrapped-init-needs-unwrap
```

Предложение: `consume tx = db.maybe_begin()!! { ... }` или проверьте сначала.

### 5.3 Расходящиеся Cleanup-типы в conditional

```nova
// ❌ DON'T:
consume r = if cond { File.open(path)? } else { TcpStream.connect(addr)? } {
    ...
}
// → D196-divergent-consumable
```

Предложение: вынесите в полиморфный wrapper-тип или используйте
`Box[Cleanup[E]]`.

### 5.4 spawn / parallel / supervised в `@cleanup`

```nova
// ❌ DON'T:
fn Resource consume @cleanup(_o ScopeOutcome) -> () {
    spawn { @async_flush() }         // → E_CLEANUP_FORBIDDEN_OPERATION
}
```

Правило D159/D191. Используйте последовательный `await @async_flush()?` или
off-thread очередь с persistent worker-fiber.

### 5.5 Попытки отключить cancel-shield

Cancel-shield всегда включён в теле `@cleanup`. Отключить невозможно — это
намеренно (уроки Rust scopeguard / C++23 показывают: opt-in shield
большинство забывает).

## Раздел 6 — Отладка cleanup-цепочек

### 6.1 Чтение MultiError

```nova
match process() {
    Ok(_) => println("done")
    Err(e) => {
        // e — Error / MultiError
        if e is MultiError {
            println("primary: ${e.primary()}")
            for sup in e.suppressed() {
                println("  suppressed: ${sup}")
            }
            if Some(panic_msg) = e.find_first_panic() {
                println("  PANIC IN CHAIN: ${panic_msg}")
            }
        } else {
            println("error: ${e.msg}")
        }
    }
}
```

### 6.2 OpenTelemetry-трейсинг

```nova
fn main() Io -> () {
    with ResourceTrace = OtelCleanupHandler.new(exporter: otel_exporter) {
        with Application = Application.handler() {
            run_app()
        }
    }
}
```

Каждый enter/exit `consume {}` генерирует OTel-спан:
- атрибуты: `cleanup.label`, `cleanup.timeout_ms`, `cleanup.start_time_ns`.
- статус: OK / ERROR_failed / ERROR_panic.
- Родительские-дочерние спаны LIFO-стек корректно.

### 6.3 `nova consume-analyze` (План 100.8 + План 110.8 update)

```bash
nova consume-analyze src/db.nv
```

Показывает:
- Какие типы реализуют Cleanup[E];
- Coverage (всё ли cleanup-path покрыт);
- Применена ли hot-path оптимизация (Cleanup[never] + без WithExitTimeout);
- Потенциальные `D198-realtime-application-override` warnings.

## Раздел 7 — Соображения о производительности

### 7.1 Когда использовать `Cleanup[never]`

Используйте, когда cleanup действительно не может упасть:
- Освобождение блокировки (без I/O).
- Возврат permit (атомарная операция).
- Возврат в пул (атомарная операция).
- Cancel-scope cancel (изменение состояния в памяти).

**Не** используйте для:
- Закрытия файла — может упасть (disk full, EBADF).
- Закрытия TCP-сокета — может упасть (broken pipe).
- DB commit — может упасть.

### 7.2 Проверка элизии в hot-path

```bash
nova build --release --asm-dump src/lock_path.nv
```

Грепните `nv_consume_enter` / `nv_resolve_exit_timeout` — для `Cleanup[never]`
+ без `WithExitTimeout` они должны **отсутствовать** в hot-path asm.

### 7.3 Оверхед cancel-shield

Per benchmark (цель Плана 110.6): оверхед cancel-shield +
3-уровневого разрешения ≤ baseline Плана 100.4 + 5%. Типично: < 100ns на
cleanup-entry.

Если профиль показывает cleanup-оверхед > 5%:
- Проверьте возможность `Cleanup[never]` (элизия hot-path).
- Вынесите `consume{}` за пределы hot-loop (acquire lock один раз против
  на каждую итерацию).
- Профилируйте реальное узкое место — cleanup редко доминирует.

### 7.4 Стоимость композиции MultiError

Глубина 1 (одиночная ошибка, без suppression): нулевой оверхед.
Глубина 10: ~ 200 ns (аллокация + звено цепочки).
Глубина 100: ~ 2 µs.
Глубина 256: ограничена — sentinel `MultiErrorTruncated` (D193).

Если cleanup-каскад глубже 256 — обычно сигнал бага (рекурсия в
cleanup-path).

## Раздел 8 — Частые ловушки

### 8.1 Порядок boot

```nova
// ❌ DON'T:
fn Application.handler(...) -> ApplicationHandler {
    register_finalizer(|| cleanup())   // can't — handler isn't active yet (D195 R7)
    ApplicationHandler { ... }
}
```

Конструктор должен полностью завершиться до входа в `with`-блок.
Регистрация finalizers — только из body.

### 8.2 abort/SIGKILL не запускает finalizers

Документировано в D195 R8 как ограничение всех языков (Java/Go/Rust/etc).
Cleanup на `panic()` — fires. На `exit(code)` — fires. На abort/SIGKILL/
SIGSEGV — НЕТ (OS убивает процесс напрямую).

Для критичного состояния при abort:
- Используйте OS-уровневые механизмы (flush файла, транзакционная БД);
- Или атрибут `#[run_on_abort]` из Плана 110.4 (follow-up
  `[M-110-run-on-abort]`).

### 8.3 Сюрприз семантики вложенных Application

```nova
with Application = Application.handler(default_exit_timeout_ms: 30_000) {
    with Application = Application.handler() {   // inherits nothing!
        // default_exit_timeout_ms == 5_000 (hardcoded), NOT 30_000
    }
}
```

D195 R3: намеренное отсутствие наследования для тестовой изоляции. Если
нужно наследование — передайте явно: `Application.handler(default_exit_timeout_ms:
parent.default_exit_timeout_ms())`.

## См. также

- [Plan 110](../plans/110-scoped-resources-radical-simplification.md) — зонтичный план.
- [Plan 100.5](../plans/100.5-ffi-external-integration.md) — FFI bridge.
- [Plan 100.8](../plans/100.8-performance-ide-tooling.md) — perf + tooling.
- [idiom/consume-scope-cleanup.md](../dev/idioms/consume-scope-cleanup.md) — Q-блоки
  (обзор семантики).
- [D188](../../spec/decisions/03-syntax.md#d188)–[D198](../../spec/decisions/03-syntax.md#d198) — spec.
