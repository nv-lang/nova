# Plan 73: `consume` qualifier — D131

> **Создан 2026-05-19.**
>
> **✅ ЗАКРЫТ 2026-05-21.** Все 6 фаз: Ф.1 spec D131 (`05-memory.md`),
> Ф.2 AST (`Receiver.consume` + `Param.consume`), Ф.3 parser (`KwConsume`
> токен + receiver-квалификатор `mut`/`consume` взаимоисключающие +
> `consume name Type` параметр), Ф.4 семантика — flow-sensitive
> `check_consume` pass в `types/mod.rs` (`VarState` Live/Consumed/
> MaybeConsumed + `ConsumeRegistry` + branch-join if/match/??/select +
> пессимистичные циклы + изолированный walk closure/handler/trailing),
> Ф.5 stdlib+runtime (`runtime_registry` `is_consume` → auto-gen
> `string_builder.nv` `consume @into`; `string_builder.h` — убран
> runtime-флаг `consumed`, `@into()` зануляет поля + repurposed
> `_check_live` как `data != NULL` defense-in-depth assert), Ф.6 тесты
> `nova_tests/plan73/` (фикстуры: positive — basic / branches /
> user-defined метод+параметр; negative — use-after / maybe-consumed /
> loop / consume-param / mut+consume parse-conflict). Existing
> `f15_stringbuilder_consumed_negative` мигрирован RUNTIME_PANIC →
> COMPILE_ERROR.
>
> **Followup (2026-05-21, тот же план):** consume-checker усилен —
> (1) canonical-name **alias-tracking**: `let a = b` → consume любого
> имени инвалидирует весь alias-класс (sound); (2) тип переменной
> выводится также из **return-типа свободной функции** (`let x =
> factory()`). +3 фикстуры (`consume_err_alias`, `consume_err_factory`,
> `consume_ok_alias`).
>
> **Границы (bootstrap):** alias через **результат метода**
> (`let sb2 = sb.append(...)`, builder-chain) не отслеживается — требует
> точного «возвращает receiver» ([Plan 77](77-fluent-return.md)); резолв
> типа receiver'а best-effort (return-типы method-call'ов не покрыты →
> Plan 37). Оба — sound (false-negative, не false-positive). См.
> `simplifications.md` `[M-consume-method-result-alias]` /
> `[M-consume-receiver-type-best-effort]`.
>
> **Расширение «противоположной стороны»:** D131 = *affine* (≤1 раз;
> забыть OK). [Plan 100](100-linear-must-consume.md) (D133, design
> finalized 2026-05-23) добавляет type-level `consume`-обязательство
> (`type Transaction consume { ... }`) — instance такого типа **обязан**
> быть consumed до scope exit'а (Transaction.commit/rollback паттерн).
> Opt-in per-type; reuse того же `check_consume` pass'а + alias-
> tracking + field-aware flow внутри методов record'а.
>
> **Цель:** добавить в Nova compile-time проверку логической линейности.
> Некоторые типы (`StringBuilder`) после определённых вызовов (`into()`)
> инвалидируются. Сейчас это защищается только runtime-флагом — нужна
> ошибка компилятора при use-after-consume и maybe-consumed.

---

## Контекст

Nova использует GC, borrow checker не нужен. Но `consume` — это не про
memory safety, а про **логический инвариант**: после `sb.into()` буфер
отдан, дальнейшее использование `sb` — семантическая ошибка.

Синтаксис симметричен `mut`:

```nova
fn StringBuilder mut @append(s str) -> Self   // mutable self
fn StringBuilder consume @into() -> str        // consuming self

fn foo(mut a int) { ... }                      // mutable param
fn foo(consume sb StringBuilder) -> str { ... } // consuming param
```

Callsite **неявный** — просто `sb.into()` / `foo(sb)`.
(`consume:` занято синтаксисом named params с default-значениями.)

---

## Решение D131

Добавить в `spec/decisions/05-memory.md` решение D131:

- `consume` — квалификатор receiver/param, не ownership в смысле Rust
- После `consume`-вызова переменная переходит в состояние `Consumed`
- Use-after-consume → **compile error**
- Maybe-consumed (consume только на части веток) → **compile error**
- Runtime-флаг `consumed` в C-рантайме остаётся как defense-in-depth

---

## Фазы

### Ф.1 — Спек: D131 в `spec/decisions/05-memory.md`

Добавить после D6. Следующий свободный D-номер: **D131**.

---

### Ф.2 — AST: `consume: bool`

**`compiler-codegen/src/ast/mod.rs`**

`Receiver` (lines 469-475) — рядом с `mutable`:
```rust
pub struct Receiver {
    pub type_name: String,
    pub generics: Vec<TypeRef>,
    pub kind: ReceiverKind,
    pub mutable: bool,   // fn Type mut @method
    pub consume: bool,   // fn Type consume @method  ← NEW
    pub span: Span,
}
```

`Param` (lines 484-499) — новое поле (сейчас `mut` на params не хранится):
```rust
pub struct Param {
    pub name: String,
    pub ty: TypeRef,
    pub is_variadic: bool,
    pub default: Option<Expr>,
    pub consume: bool,   // consume name Type  ← NEW
    pub span: Span,
}
```

---

### Ф.3 — Парсер: `consume`

**`compiler-codegen/src/parser/mod.rs`**

**Лексер** — добавить `KwConsume` токен для ключевого слова `consume`.

**Method receiver** (lines 1798-1822) — по аналогии с `mut`:
```rust
if matches!(self.peek().kind, TokenKind::KwConsume)
    && matches!(self.peek_at(1).kind, TokenKind::At | TokenKind::Dot)
{
    self.bump();
    receiver_consume = true;
}
// Receiver { ..., consume: receiver_consume }
```
`consume` и `mut` взаимоисключающие — если оба → parse error.

**Parameter** (`parse_param`, lines 1992-2045):
```rust
let is_consume = if matches!(self.peek().kind, TokenKind::KwConsume) {
    self.bump();
    true
} else { false };
// Param { ..., consume: is_consume }
```

---

### Ф.4 — Семантика: VarState tracking

**`compiler-codegen/src/types/mod.rs`**

#### 4.1 Тип состояния

```rust
#[derive(Clone)]
enum VarState {
    Live,
    Consumed(Span),       // потреблено здесь
    MaybeConsumed(Span),  // потреблено на части путей
}
```

#### 4.2 ConsumeCtx

Отдельная структура (не смешивать с name-resolution scope stack):

```rust
struct ConsumeCtx {
    states: HashMap<String, VarState>,
}
```

Передаётся рядом с `scope` в `walk_*` методах.

#### 4.3 Логика

**`Stmt::Let`** — добавить `states[name] = VarState::Live`.

**Вызов `consume`-метода** (`expr.@method()`):
- Проверить состояние receiver → если `Consumed`/`MaybeConsumed` → error
- Установить `Consumed(call_span)`

**Передача в `consume`-параметр** (`f(arg)`):
- Аналогично для соответствующего аргумента

**`Ident(name)`** — любое использование:
- Если `Consumed(at)` или `MaybeConsumed(at)` → error:
  ```
  error: use of consumed variable `sb`
    note: consumed at <at>
  ```

**Ветвление** (if/match):
```
saved = consume_ctx.clone()
walk then-branch  → states_then
restore consume_ctx = saved
walk else-branch  → states_else  (или saved если нет else)

join по каждой переменной:
  (Live, Live)         → Live
  (Consumed, Consumed) → Consumed
  (Live, Consumed)     → MaybeConsumed
  (Consumed, Live)     → MaybeConsumed
  (MaybeConsumed, _)   → MaybeConsumed
```

**Цикл** (loop/while/for) — pessimistic:
- Определить какие переменные consumed внутри тела
- Пометить их `MaybeConsumed` перед входом в тело
- Re-walk; ошибки → обычные

---

### Ф.5 — Обновить stdlib и C-рантайм

**`std/runtime/string_builder.nv`**:
- Добавить квалификатор `consume`
- Обновить комментарий: убрать "runtime panic", добавить "compile error (D131)"

```nova
// Финализировать в str. После вызова sb недоступна — compile error (D131).
export external fn StringBuilder consume @into() -> str
```

**`compiler-codegen/nova_rt/string_builder.h`**:
- Удалить поле `nova_bool consumed` из `Nova_StringBuilder`
- Удалить макрос/функцию `_nova_string_builder_check_live`
- В `Nova_StringBuilder_method_into` после построения `nova_str` занулить
  внутренние поля — defense-in-depth: если компилятор пропустит,
  следующий доступ даёт null pointer dereference → panic, не тихая порча:

```c
static inline nova_str Nova_StringBuilder_method_into(Nova_StringBuilder* b) {
    nova_str s = (nova_str){
        .ptr = (const char*)b->data,
        .len = (size_t)b->len,
    };
    b->data = NULL;   // defense-in-depth: use-after-consume → null deref
    b->len  = 0;
    b->cap  = 0;
    return s;
}
```

---

### Ф.6 — Тесты

Новые fixture-файлы:

**Позитивные:**
```nova
// consume-ok-basic.nv
let sb = StringBuilder.new()
sb.@append("hi")
let s = sb.@into()
// sb не используется — ок
```

```nova
// consume-ok-if-both.nv
let sb = StringBuilder.new()
let s = if cond { sb.@into() } else { sb.@into() }
// оба пути consume — ок
```

**Негативные:**
```nova
// consume-err-use-after.nv
let sb = StringBuilder.new()
let s = sb.@into()
sb.@append("oops")  // error: use of consumed variable `sb`
```

```nova
// consume-err-maybe.nv
let sb = StringBuilder.new()
if cond { let _ = sb.@into() }
sb.@append("oops")  // error: maybe-consumed
```

```nova
// consume-err-loop.nv
let sb = StringBuilder.new()
loop { let _ = sb.@into() }  // error: sb maybe-consumed on 2nd iteration
```

---

## Критические файлы

| Файл | Изменение |
|------|-----------|
| `spec/decisions/05-memory.md` | D131 |
| `compiler-codegen/src/ast/mod.rs` | `Receiver.consume`, `Param.consume` |
| `compiler-codegen/src/parser/mod.rs` | парсинг `consume` |
| `compiler-codegen/src/lexer/mod.rs` | `KwConsume` токен |
| `compiler-codegen/src/types/mod.rs` | `VarState`, `ConsumeCtx`, walk-логика |
| `std/runtime/string_builder.nv` | `consume @into()` + обновить комментарий |
| `compiler-codegen/nova_rt/string_builder.h` | удалить `consumed` поле + `check_live`, занулять поля в `into()` |
| `tests/consume-*.nv` | fixture-тесты |

## Верификация

```
nova test tests/consume-ok-*.nv    # все проходят
nova test tests/consume-err-*.nv   # все дают ожидаемые ошибки
nova test std/                     # нет регрессий
```
