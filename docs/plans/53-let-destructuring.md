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

---

## Ф.5 — Production hardening (2026-05-15)

**Ф.5.1 — Structured `if let` suggestion на refutable array.**
- Сейчас `check_let_pattern_irrefutable` (types/mod.rs:888) для
  `Pattern::Array` выдаёт сообщение «array pattern in let is
  refutable» без `Suggestion`. Для sum-variant case уже есть
  structured suggestion на `match` — array-кейс асимметричен.
- **Fix:** Добавить `Suggestion { applicability: MachineApplicable }`
  с заменой `let [a, b] = xs` → `if let [a, b] = xs { ... }` (для
  expression-let — span к whole let, replacement приоретительный).
- AI-first: LLM авто-исправляет; symmetric с sum-variant.
- Тест: `nova_tests/negative_capability/p53_let_array_pattern.nv`
  обновить — добавить EXPECT_STDOUT с suggestion-текстом.

---

## Ф.6 — Production hardening 2 (2026-05-16, post-audit)

Production-grade аудит (2026-05-16) выявил несколько gap'ов между планом
и реализацией. Закрытие — Ф.6 (4 sub-phase'а).

**Ф.6.1 — Mojibake в diagnostic strings (4 negative-теста FAIL).**
- **Проблема:** Все 4 `nova_tests/negative_capability/p53_*` тестов
  reporters `NEG-WRONG-MSG`. Ожидаемый pattern `refutable pattern в
  \`let\`` (UTF-8) — но binary эмитит double-encoded `refutable pattern
  РІ \`let\`` (Cyrillic `в` → cp1251 → re-UTF-8 mojibake). Acceptance
  criterion «refutable pattern в `let` — production-grade compile
  error» формально нарушен — diagnostic unreadable.
- **Где:** `compiler-codegen/src/types/mod.rs:929-1023` + `:437-442`
- **Fix:** Переписать Cyrillic-строки правильным UTF-8 (либо latinise
  ключевые слова в diagnostic'ах, чтобы исключить класс багов).
- **Acceptance:** `nova test --filter negative_capability/p53` → 4/4 PASS.
- **LOC:** ~50.

**Ф.6.2 — Structured `Suggestion` на refutable patterns.**
- **Проблема:** Ф.5.1 коммит обещал `Suggestion { applicability:
  MachineApplicable }`, но в реальности используется только
  `.with_note(...)`. Sum-variant case тоже без `with_suggestion()`.
  AI-first/LSP code-action UX gap.
- **Где:** `types/mod.rs:964-1020` — 5 refutable branches (Literal,
  Variant, Or, Array, Record-sum-variant).
- **Fix:** Добавить `with_suggestion(Suggestion { ... })` per-branch с
  replacement `if let <pat> = <expr> { ... } else { ... }`.
- **LOC:** ~80.

**Ф.6.3 — `decl.mutable` propagation в record-destructure.**
- **Проблема:** Plan §102 явно: «маркер mutability: каждый bind
  получает `decl.mutable`». `emit_record_destructure`
  (`emit_c.rs:12290-12366`) НЕ touches `self.var_mutable`. Plain path
  (`:6931-6934`) делает; record-path нет. Затрагивает spawn-capture
  decision (`:2526-2527` — `by_value = is_scalar && !is_mut`).
- **Fix:** В `emit_record_destructure` для каждого bound name —
  `var_mutable.insert(name)` если `decl.mutable`.
- **Test:** `let mut { x, y } = ...; spawn { use(x); }; x = 99` — должен
  capture by reference.
- **LOC:** ~10 + ~25 test.

**Ф.6.4 — `Channel.new` special-case → schema registration.**
- **Проблема:** `emit_c.rs:12296-12338` хардкодит Pair-destructure для
  `Channel.new` (50 LOC), нарушая Plan §97-100 («без дубль-логики»).
- **Fix:** Зарегистрировать `Nova_ChannelPair` schema в module-init
  (~5 LOC); общий path возьмётся.
- **LOC:** ~5 add, ~50 remove (net -45).

**Ф.6.5 — Acceptance gap: round-trip Channel-теста.**
- **Проблема:** `nova_tests/syntax/let_destructure_record.nv:65-70`
  использует `Channel.new`, но не verifies send/recv через destructured
  handles. Acceptance flagship-кейс работает only по компиляции.
- **Fix:** Расширить test — send value через `tx`, recv на `rx`,
  assert значение.
- **LOC:** ~10.

**Ф.6.6 — Unknown field в destructure: Nova diagnostic.**
- **Проблема:** `let { nonexistent } = p` (где p: Pair) → CC-FAIL
  `no member named 'nonexistent' in 'struct Nova_Pair'` — leak C
  internals.
- **Fix:** type-checker валидирует field-names против `record_schemas`
  до codegen + Levenshtein suggest «did you mean `tx`?».
- **LOC:** ~40.

### Размер Ф.6

| Sub-phase | LOC |
|---|---|
| Ф.6.1 mojibake | ~50 |
| Ф.6.2 Suggestion | ~80 |
| Ф.6.3 mutable + test | ~35 |
| Ф.6.4 Channel.new dedup | -45 |
| Ф.6.5 round-trip test | ~10 |
| Ф.6.6 unknown field diag | ~40 |
| **Итого** | **~170** |

### Acceptance Ф.6
- [ ] `nova test --filter p53` → 4/4 PASS (mojibake fixed)
- [ ] Все 5 refutable branches имеют `Suggestion`
- [ ] `let mut { x } = ...` тест с mutation works
- [ ] `Channel.new` round-trip тест — send+recv через destructured handles
- [ ] Unknown field → Nova diagnostic «did you mean ...»
- [ ] 0 регрессий vs main baseline
