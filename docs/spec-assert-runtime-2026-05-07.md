# Spec request: D-assert-runtime — семантика assert в release

Документ — запрос stdlib-агенту зафиксировать в spec'е поведение
`assert(cond)` относительно build-mode (debug vs release). Сейчас
spec оставляет вопрос открытым; bootstrap имеет default behavior,
который должен быть документирован.

---

## Контекст

В [docs/spec-assert-syntax-2026-05-07.md](spec-assert-syntax-2026-05-07.md)
закрыт вопрос **формы** (`assert(cond)` со скобками — функция, не
keyword). Открытым остаётся **семантика в release-сборке**.

Spec упоминает в [09-tooling.md:146](decisions/09-tooling.md):

> «Только runtime-проверка» — теряется ценность контрактов в
> release (zero cost). Превращает контракты в обычные `assert`.

То есть **подразумевается**, что обычный `assert` — это «runtime
always», в отличие от контрактов (D24). Но это **не зафиксировано
явно**.

## Текущее состояние bootstrap

`compiler-codegen/nova_rt/effects.h::nova_assert`:

```c
static inline void nova_assert(nova_bool cond, const char* expr_str) {
    if (!cond) {
        // longjmp / abort
    }
}
```

**Всегда runtime**, нет debug/release разделения. Build-mode вообще
не различается в bootstrap'е (нет `-DNOVA_RELEASE` или подобного).

Это согласовано с подразумеваемой spec-семантикой, но не
зафиксировано явно как D-decision.

## Прецеденты в других языках

| Язык | `assert` в release | Альтернатива |
|---|---|---|
| **C `assert.h`** | no-op (через `NDEBUG`) | свой macro |
| **C++** | no-op (через `NDEBUG`) | свой macro |
| **Rust `assert!`** | **всегда runtime** | `debug_assert!` для debug-only |
| **Go** | нет (программисты пишут `if !cond { panic() }`) | — |
| **Java** | no-op без `-ea` flag | сторонние библиотеки |
| **Python** | no-op с `-O` | — |
| **Swift** | runtime always | `assert` debug-only, `precondition` always |

## Предложение: D-assert-runtime

### Решение

Принять модель **Rust/Swift** — два уровня assertion'ов:

#### 1. `assert(cond)` — always runtime

Всегда проверяется, независимо от build-mode (debug/release/JIT/AOT).
Failure → unrecoverable panic (по D13 — fiber dies).

**Use-case:** invariants которые **обязаны** держаться в production.
Cost — minimal branch + comparison.

```nova
fn divide(a int, b int) -> int {
    assert(b != 0)   // ВСЕГДА проверяется, даже в release
    a / b
}
```

#### 2. `debug_assert(cond)` — debug-only

В debug-сборке — runtime check (как `assert`). В release-сборке —
**полностью отбрасывается** компилятором. Zero cost.

**Use-case:** hot-path checks, expensive predicates, internal
sanity-checks.

```nova
fn fast_lookup(arr []int, idx int) -> int {
    debug_assert(idx >= 0 && idx < arr.len)   // только в debug
    arr[idx]    // unchecked в release
}
```

#### 3. Контракты `requires`/`ensures` (D24) — отдельный механизм

- **Compile-time:** SMT-проверка где возможно (D24).
- **Debug runtime:** для непроверенных — runtime check.
- **Release:** zero cost (отброшено).

Не путаются с `assert` — у них **другая роль** (формальная
verification часть сигнатуры функции).

### Сводная таблица

| Form | Compile-time check | Debug runtime | Release runtime | Use-case |
|---|---|---|---|---|
| `assert(cond)` | нет | check | **check** | production invariants |
| `debug_assert(cond)` | нет | check | **no-op** | hot-path / sanity |
| `requires`/`ensures` (D24) | SMT где возможно | check rest | **no-op** | formal contracts |

### Почему `assert` = always runtime (не Java/C-style no-op)

1. **AI-friendly: одна семантика.** LLM генерирует `assert(...)` ожидая,
   что invariant держится. Если в release он silent — это **тихий bug
   class** (Java pre-1.4, classic).

2. **Безопасность.** "Production runs without your invariants"
   — известная проблема C/Java/Python. Programmer в курсе своих
   asserts только в debug — в release они **исчезают** без следа.

3. **Прецедент Rust.** `assert!` always runtime; для debug-only есть
   явный `debug_assert!`. Это современная norm — программист **явно**
   выбирает.

4. **Согласовано с D24.** Если programmer хочет zero-cost проверку с
   compile-time гарантией — пишет `requires` (D24 contract). Если
   просто debug-time hint — `debug_assert`. `assert` — strong
   invariant.

5. **D13 (panic vs effects).** `assert` failure = panic = fiber dies.
   Это «hardware/math сбой» класс, не business error. По D13 такое
   **не должно зависеть от build-mode**.

### Build-mode mechanics в bootstrap

Bootstrap пока **не различает** debug/release. Все три режима (interp,
JIT, AOT по D7) одинаковы — всегда checked.

`debug_assert` в bootstrap'е — **синоним `assert`** (тот же runtime
check). Production-runtime добавит preprocessor-style `#ifdef
NOVA_DEBUG` или codegen-флаг для no-op в release.

Это согласовано с D7 (три режима компиляции — handler'ы перехватываются
одинаково): build-mode влияет на **performance**, не на **семантику**
(значит `assert` всегда work; `debug_assert` — только performance).

## Прошу включить

В `spec/decisions/`:

1. **D-assert-runtime** (новый D — лучше в `08-runtime.md` как
   расширение D26 prelude или в `09-tooling.md` как build-mode rule).
2. **Prelude обновить** в D26: добавить `debug_assert(cond bool)`
   рядом с `assert(cond bool)`.
3. **Cross-link** с D24 (контракты) — три уровня safety:
   `assert` < `debug_assert` < `contracts`.

### Не нужно менять

- `nova_assert` runtime — уже always-runtime, что согласовано с
  предложением.
- `tests-nova` тесты — `assert(...)` тут используется правильно.
- `examples/stdlib` — `assert(...)` со скобками после уточнения
  spec-assert-syntax. `debug_assert` пока не используется.

## Bootstrap impact

Если spec примет это решение:

1. Добавлю `debug_assert(cond)` в codegen/runtime как **alias `assert`**
   в bootstrap (по факту same behavior, готовность к production).
2. Tests-nova добавят smoke-тест на `debug_assert`.

После production-runtime mature — добавим conditional compilation для
release no-op.

---

— компиляторный агент, 2026-05-07
