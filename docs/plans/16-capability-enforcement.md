# План 16: Capability enforcement — `forbid` и `realtime` compile-time checks

**Статус:** активный, не начат.
**Дата создания:** 2026-05-08.
**Зависимости:** [D63](../../spec/decisions/04-effects.md#d63),
[D64](../../spec/decisions/04-effects.md#d64) уже описывают синтаксис.

---

## Проблема

Spec-то говорит «sandbox в типах, не в рантайме» (R5/R6 в
revolutionary.md), а codegen эмитит body как plain block без проверок:

```nova
// текущее поведение compiler'а:
fn run_user_script(code str) Fail -> Result =>
    forbid Net, Fs, Db {
        eval(code)               // ← Net.* call здесь = НЕ ловится
    }

fn realtime_audio(buf []f32) -> () =>
    realtime nogc {
        let arr = []int.new()    // ← managed alloc здесь = НЕ ловится
    }
```

`emit_c.rs:4139` (`forbid`) и `:4143` (`realtime`) — оба эмитят
содержимое как обычный block. Семантические гарантии **не
выполняются**. Это major spec-vs-impl drift, который может **скрыть
ошибки до production**.

---

## Что нужно

### Forbid (D63)

При `forbid X1, X2 { body }`:

1. На вход — список запрещённых эффектов `{X1, X2}`.
2. Внутри body — для каждого вызова функции `f(...)`: посмотреть
   эффекты в её сигнатуре. Если **прямые** эффекты пересекаются с
   запрещёнными — compile error на месте вызова:
   ```
   error E0144: function `http.get` requires effect `Net`,
     forbidden by enclosing `forbid Net` block
       at src/main.nv:42
     │
     │   forbid Net, Fs {
     │       http.get(url)        // ← вот тут
     │       ^^^^^^^^^^^^^
     ```
3. Транзитивные эффекты (по D62 — частичный contract) — **warning**,
   не error. С опцией `--strict-forbid` поднимать до error.
4. Forbid внутри forbid — union эффектов.

### Realtime (D64)

При `realtime { body }` или `realtime nogc { body }`:

1. Запретить **suspend-операции** — channel.recv (без `try_recv`),
   `Time.sleep`, `Net.*`, `Db.*`, `Fs.*` — всё что может
   приостановить fiber.
2. Для `realtime nogc` — также запретить **managed-heap аллокации**:
   `[]T.new()`, `[]T.with_capacity()`, `Type.new()` (если new-конструктор
   требует alloc), `str.from()` если конкатенация может alloc'ить.
3. Внутри realtime разрешён `region { ... }` — arena-allocations
   (D6).

### Сообщения

Структурированные ошибки по [R5.3](../../spec/revolutionary.md#r5-3) —
показать enclosing-scope, причину, патч.

---

## Фазы

### Ф.1 — Effect-context tracking в codegen

**Файлы:** `compiler-codegen/src/codegen/emit_c.rs`.

Добавить в `EmitContext`:
```rust
forbidden_effects: Vec<HashSet<String>>,   // stack — forbid-блоки
realtime_active: bool,
realtime_nogc: bool,
```

Push/pop при входе/выходе из forbid/realtime блока.

**Объём:** ~30 строк.

### Ф.2 — Check на каждом call-site

При эмите `ExprKind::Call`:

1. Резолвнуть callee → его прямые эффекты (из `fn_effects` map'а).
2. Для каждого активного forbidden-set'а — пересечение.
3. Если пересечение непустое — emit `Err(...)` с структурированным
   сообщением (showing source span, forbidden set, callee effect).
4. Realtime: если callee имеет один из suspend-effects — error.
5. Realtime nogc: если callee — alloc'ирующий (по списку:
   `[]T_new`, `Nova_X_new` где X — record, `nova_str_concat`, etc.) —
   error.

**Объём:** ~120 строк включая эффект-список и error-formatting.

### Ф.3 — Допустимые исключения

Внутри forbid/realtime разрешено вызывать:

- Функции **без эффектов** (полностью pure) — всегда ok.
- Функции эффектов которые **не в forbidden set'е**.
- Для realtime — `region.alloc()` (arena-allocations).
- Для realtime — `try_recv`/`try_send` (non-blocking channel ops).

Вынести в whitelist по mind-model spec'а.

**Объём:** ~40 строк.

### Ф.4 — Тесты

`nova_tests/effects/forbid_realtime.nv` уже существует (PASS), но
проверяет только parser. Расширить:

- Negative test: `forbid Net { http_get(url) }` — должна быть
  compile-error.
- Negative test: `realtime nogc { let xs = []int.new() }` — error.
- Positive test: `forbid Db { compute_pure(x) }` — ok.
- Positive test: `realtime { region { let xs = arena.alloc(...) } }` — ok.

Учитывая что existing-test PASS — добавить как `_negative_test "..." {
EXPECT_COMPILE_ERROR ... }` или отдельный набор `nova_tests/effects/forbid_negative.nv`
с phantom-функциями (тестировать через codegen-driver, не через runtime).

**Объём:** ~10 тестов; нужен инфра для negative-tests (если ещё нет).

### Ф.5 — Spec уточнение

После реализации возможно нужно дописать в [D62](../../spec/decisions/04-effects.md#d62)
точную семантику transitive vs direct effects в forbid-scope. Сейчас
spec говорит «warning для транзитивных» — закрепить как
configurable.

---

## Что НЕ делаем

- **Async-effect blocking detection** — D62 говорит Async — ambient,
  а forbid Async запрещён. Не трогаем.
- **Closure capture эффектов** — если closure захватывает handler
  через `with`, эффект "уносится" в lambda. Полное отслеживание —
  отдельный план (нужен полноценный effect-row inference, что не
  bootstrap-scope).
- **Runtime sentinel-frame для transitive effects** — D63 упоминает,
  это runtime-mechanism для plug-in scenarios, не AOT. Не сейчас.

---

## Оценка

~200 строк + 10 negative-тестов = **2-3 дня**.

Главный challenging momento — **negative-test infrastructure**. Если
её нет, потратить день на её создание (driver принимает `// expect-error
E0144` маркер в .nv-файле, гоняет codegen, сверяет error-code и span).

---

## Связь

- [Plan 14](14-stdlib-codegen-gaps.md) — параллельный, не зависит.
- [Plan 15](15-generic-bounds-enforcement.md) — параллельный.
- [spec/revolutionary.md → R6](../../spec/revolutionary.md) — capability
  security, главное обоснование плана.

---

## Ссылки

- [spec/decisions/04-effects.md → D63](../../spec/decisions/04-effects.md#d63) — forbid.
- [spec/decisions/04-effects.md → D64](../../spec/decisions/04-effects.md#d64) — realtime.
- [spec/decisions/04-effects.md → D62](../../spec/decisions/04-effects.md#d62) —
  прямые vs транзитивные эффекты.
- `compiler-codegen/src/codegen/emit_c.rs:4139` — текущий forbid emit.
- `compiler-codegen/src/codegen/emit_c.rs:4143` — текущий realtime emit.
