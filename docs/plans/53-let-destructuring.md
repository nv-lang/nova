# Plan 53: Record-destructuring в `let`-биндингах

> **Создан 2026-05-15, ЗАКРЫТ 2026-05-15.** Production-grade, без упрощений.
>
> **СТАТУС:** ✅ Реализован (Ф.0-Ф.5). Codegen `emit_record_destructure`
> делегирует биндинг полей в общий `pattern_bind_typed` (plain-record-aware);
> nested case закрыт регистрацией field_access в var_types перед
> recurse. Interp работал ИЗ КОРОБКИ — `match_pattern` уже умел
> `Pattern::Record`. Type-checker `check_let_pattern_irrefutable`
> отсеивает refutable patterns (Literal/Variant/Or/Array, Record к
> sum-variant) с production-grade diagnostic + note + suggestion на
> `if let`/`match`. Special-case для `Channel.new` — mirror tuple-special
> case. 7 positive + 4 negative тестов.
>
> **Реализует:** irrefutable record-pattern в `let` — `let { tx, rx } = ch`
> вместо `let tx = ch.tx; let rx = ch.rx`. AST/parser/match-codegen
> уже поддерживают `Pattern::Record`; не реализована последняя миля в
> `emit_c.rs::emit_let_stmt` и `interp::exec_let`.
>
> **Зависит от:** —. Не блокирует другие планы.
>
> **Приоритет:** P2 — ergonomics, не блокер; быстро + значительно
> улучшает чтение «pair-объектов» (Channel, Result, Pair-возврата).

---

## Зачем

`Pattern::Record` уже работает в `match`-armах (`match ch { Pair { tx, rx } => ... }`)
и в codegen-helper'ах `pattern_cond`/`pattern_bind_typed`. В `let`-statement он
парсится (через `parse_pattern`), но `emit_let_stmt` падает с
`complex pattern in let binding not yet supported`. Полная фича уже
почти есть — закрываем последнюю милю.

Канонический use-case — `Channel.new()`:

```nova
// До:
let pair = Channel.new(0)
let tx = pair.tx
let rx = pair.rx

// После:
let { tx, rx } = Channel.new(0)
```

Аналогично — любой record-возврат (Result-like структуры, точки, размеры,
пары координат), плюс распаковка полей объекта внутри тела функции.

---

## Сравнение с Go / Rust / TS

| | Record-destructuring в let | Renaming | Rest-pattern (..) |
|---|---|---|---|
| **Go** | нет (`a, b := struct.A, struct.B`) | нет | нет |
| **Rust** | да (`let Point { x, y } = p`) | `let Point { x: a, y: b } = p` | `..` |
| **TypeScript** | да (`const { x, y } = obj`) | `const { x: a } = obj` | `...rest` (новый объект) |
| **Nova (Plan 53)** | да | через `field: pat` (Rust-style, не TS-style: `pat` это под-pattern) | `..` (без bind) |

Refutable patterns (Sum-variants, литералы, `Or`) в `let` — **compile error**.
Используется `if let` / `match` — как Rust. TS позволяет refutable
destructuring (`const { a } = maybeUndef` → runtime throw) — мы безопаснее.

---

## Архитектурное решение

### Какие patterns допустимы в `let` — только irrefutable

- ✓ `Ident` — `let x = e`
- ✓ `Wildcard` — `let _ = e` (eval, discard)
- ✓ `Tuple` — `let (a, b) = pair` (уже работает)
- ✓ `Record` без type_path — `let { tx, rx } = ch` (любой record)
- ✓ `Record` с type_path к **record-типу** — `let Pair { tx, rx } = ch`
  (Pair-record, не sum-variant)
- ✓ `Record` со shorthand: `{ tx }` = `{ tx: tx }` (привязывает к
  переменной с тем же именем)
- ✓ `Record` с под-pattern: `{ pair: { tx, rx } }` — рекурсивно
  irrefutable (под-pattern тоже irrefutable)
- ✓ `Record` с rest `..` — `let { tx, .. } = ch` (без bind)
- ✓ `Record` с renaming через под-pattern: `{ tx: sender, rx: receiver }`
- ✗ `Record` с type_path к **sum-variant** — `let Foo.Variant { x } = obj`
  — REFUTABLE → compile error «use `if let` / `match`»
- ✗ `Variant` — `let Some(x) = opt` — REFUTABLE
- ✗ `Literal` — `let 42 = n` — REFUTABLE (и бессмысленно)
- ✗ `Or` — refutable
- ✗ `Array` — refutable (длина не гарантирована)
- ✗ `Binding` — пока не реализован, не трогаем

### Реализация — три места

1. **Codegen** (`emit_c.rs::emit_let_stmt`):
   - Перед общим путём проверить `Pattern::Record` → ветка
     `emit_record_destructure`.
   - Eval `value` ровно один раз в fresh tmp (порядок side-эффектов).
   - Определить C-тип tmp (из `decl.ty` либо `infer_expr_c_type(value)`).
   - Через `var_types.insert(tmp, ty_c)` зарегистрировать тип tmp.
   - Делегировать в существующий `pattern_bind_typed(&pattern, &tmp)` —
     он уже умеет plain-record case через `record_schemas`. Никакой
     дубль-логики.
   - Маркер mutability: каждый bind получает `decl.mutable`. Хотя
     destructuring обычно immutable, разрешаем — D27.

2. **Interp** (`interp::exec_stmt` / `bind_let_pattern`):
   - Аналогично: eval value, делегировать в `bind_pattern` (которая
     уже работает в match'ах). Refutability там обрабатывается
     рантайм-ошибкой; для `let` мы её гарантируем compile-time.

3. **Type-checker** (`types::check_module` walker для `Stmt::Let`):
   - Refutability check: если `Pattern::Record { type_path: Some(path), .. }`
     и `path` указывает на sum-variant — diagnostic.
   - Аналогично для `Variant`, `Literal`, `Or`, `Array` — clear error
     с подсказкой на `if let` / `match`.
   - Production-grade diagnostic: `note: declared here` + structured
     suggestion где применимо.

### Что НЕ меняем

- `parse_pattern` — уже работает.
- AST — `Pattern::Record`, `RecordPatternField` уже есть.
- Match arm логика — не трогаем; переиспользуем helper'ы.
- `Pattern::Tuple` в let — уже работает через `emit_tuple_destructure`.

---

## Фазы

### Ф.0 — Codegen `emit_record_destructure`

`emit_let_stmt`:
```rust
if let Pattern::Record { .. } = &decl.pattern {
    return self.emit_record_destructure(decl);
}
```

`emit_record_destructure(decl)`:
- fresh tmp; emit value into tmp с правильным C-типом
- `var_types.insert(tmp, ty_c)`
- delegate `pattern_bind_typed(&decl.pattern, &tmp)`

### Ф.1 — Interp record-destructuring в let

`interp::exec_stmt` Stmt::Let arm: если `Pattern::Record` → eval value,
для каждого pattern-field прочитать поле объекта (Value::Record) и
bind. Под-pattern → recurse.

### Ф.2 — Type-checker refutability

`walk_stmt` для `Stmt::Let` → проверка `is_irrefutable_let_pattern(pat)`.
Diagnostic с note + suggestion `if let <pat> = <expr> { ... }`.

### Ф.3 — Тесты

`nova_tests/syntax/let_destructure_record/` — positive:
- basic shorthand `{ tx, rx }`
- type-prefix `Pair { tx, rx }`
- renaming через под-pattern `{ tx: sender, rx: receiver }`
- nested `{ outer: { x, y } }`
- rest `{ tx, .. }`
- Channel.new (канонический use-case)
- mutable destructure `let mut { x, y } = ...` если поддержим

`nova_tests/negative_capability/p53_*` — negative:
- variant pattern `let Some(x) = opt`
- literal `let 42 = n`
- Or pattern
- Array pattern (refutable)
- Sum-variant record `let Foo.Var { x } = obj`

### Ф.4 — Spec + docs

D-decision (новый, например D109) либо расширение D-pattern-семантики.
README плана, simplifications.md если есть `[M-*]`, project-creation.txt.

### Ф.5 — Регрессия

Полный `nova test`, без новых FAIL.

---

## Acceptance criteria

- [ ] `let { tx, rx } = ch` для plain record-типа компилируется в
      `T tx = ch.tx; T rx = ch.rx` с одним eval `ch`.
- [ ] Type-prefix `let Pair { tx, rx } = ch` работает.
- [ ] Shorthand, renaming, rest, nested — все работают.
- [ ] Channel.new use-case: `let { tx, rx } = Channel.new(0)` работает
      (получает `Nova_ChanWriter*` + `Nova_ChanReader*`).
- [ ] Refutable pattern в `let` — production-grade compile error с
      подсказкой `if let` / `match`.
- [ ] `nova run` (interp) — то же поведение.
- [ ] Все тесты PASS; полная регрессия без новых FAIL.
- [ ] Каждая фаза — отдельный commit.

---

## Size estimate

| Компонент | LOC |
|---|---|
| Codegen `emit_record_destructure` (делегирует) | ~50 |
| Interp Pattern::Record в exec_let | ~60 |
| Type-checker refutability | ~80 |
| Тесты positive + negative | ~250 |
| Spec + docs | ~80 |
| **Итого** | **~520** |
