// SPDX-License-Identifier: MIT OR Apache-2.0
# План 37: type-checker semantic parity with codegen

> **Статус:** план, не начат. Средний приоритет (UX: ошибки D54
> ловятся на ранней фазе, не на codegen). Обнаружен 2026-05-12 при
> работе над `std/encoding/hex.nv`.

**Цель.** Перенести семантические проверки, которые сейчас выполняются
только в codegen (`emit_c.rs`), в type-checker. Сегодня `nova check`
для модуля проходит, а `nova test` / `nova build` падает с
codegen-error — это плохой UX (отложенная диагностика) и ломает
обещание `nova check` как «полная type-валидация».

---

## Контекст / триггер

`std/encoding/hex.nv` использует D54-запрещённый каст:

```nova
fn digit(n u8, upper bool) -> char =>
    if n < 10 {
        ('0' as int + n as int) as char    // <-- запрещено D54
    } ...
```

- `nova check std/encoding/hex.nv` → **PASS** (type-check молчит).
- `nova test std/encoding/hex.nv` → **CODEGEN-FAIL** с сообщением
  «`as`-cast `int as char` запрещён: use `char.try_from(n)?`».

Аналогичная история случилась бы для `int as bool`, `char as byte`,
`str ↔ T` и других пар.

Источник: `check_as_cast_allowed`
([emit_c.rs:10063](../../compiler-codegen/src/codegen/emit_c.rs#L10063))
вызывается только из `emit_c.rs:5474`. Type-checker для
`ExprKind::As(e, _)` лишь рекурсирует во внутрь
([types/mod.rs:434,1047,1614,2499](../../compiler-codegen/src/types/mod.rs))
— никаких D54-проверок.

То же самое с **strict bool condition**: `check_bool_condition_at`
([emit_c.rs:10165](../../compiler-codegen/src/codegen/emit_c.rs#L10165))
вызывается только из codegen для `if`/`while`. Type-checker `if n { ... }`
с numeric `n` пропускает.

---

## Scope

Перенести в type-checker (или **продублировать** — см. «Вариант реализации»)
семантические проверки, которые сейчас живут в `CEmitter`:

### S1 — D54 запрещённые `as`-касты

24 пары из `check_as_cast_allowed`. Включает:

- `int|i32|i64|u32|u64 as char` → use `char.try_from(n)?`
- `char as byte` → use `byte.try_from(c)?`
- `int|i*|u*|byte|f* as bool` → use `n != 0`
- `str → numeric/bool` → use `T.try_from(s)?`
- `numeric/bool/char → str` → use `str.from(x)`

Special-cases (должны сохраниться):
- `CharLit as numeric` всегда OK.
- `IntLit as char` для compile-time-known литералов в Unicode-диапазоне
  (Plan 14 Ф.7) — статический range-check.

### S2 — strict bool condition

`if cond` / `while cond` где `cond` — definitely-non-bool
(`nova_int|f64|f32|str|byte|i*|u*`).

### S3 — потенциально другие проверки

Audit `emit_c.rs` на функции `check_*` / `Err(...)` в emit-путях, не
обёрнутые в type-checker. Кандидаты:
- `current_fn_return_ty` mismatch на `return`/последнее-выражение.
- Match exhaustiveness (если живёт только в codegen).
- Variadic/turbofish ariety.
- effect handler arity.

Audit — **первая задача Ф.1**, не догадки.

---

## Вариант реализации

Два пути:

**A. Чистый перенос** — переместить `check_as_cast_allowed` и
`check_bool_condition` из `emit_c.rs` в `types/mod.rs`. Codegen
полагается на то, что type-checker уже валидировал. Проблема:
`nova-codegen build` без `nova check` теряет проверку — нужна
гарантия, что codegen всегда runs **после** type-check'а в edge-кейсах
(REPL, partial recompilation).

**B. Дублирование с shared module** — вынести функции в
`compiler-codegen/src/semantic_checks.rs` (новый модуль), вызывать из
обоих фаз. Type-checker даёт ранний error, codegen — defense-in-depth.
Стоимость: +1 проход AST в check-фазе.

**Рекомендация:** **B** — defense-in-depth важнее экономии прохода
(который для as-cast'ов крошечный). Codegen не должен предполагать
«type-checker всегда был запущен» — это хрупкая инвариантa.

---

## Файлы

- `compiler-codegen/src/semantic_checks.rs` — новый модуль (или
  встроить в `types/mod.rs` как pub fn'ы).
- `compiler-codegen/src/types/mod.rs` — вызовы из walk_expr на
  `ExprKind::As` и `ExprKind::If`/`ExprKind::While` (где cond есть).
- `compiler-codegen/src/codegen/emit_c.rs` — переключить
  `check_as_cast_allowed` / `check_bool_condition_at` на shared.
- ~150-300 строк (без audit Ф.0).

---

## Acceptance criteria

**Ф.0 — audit:**
- Список всех `Err(...)` в `emit_c.rs`, которые **семантические**
  (не «implementation gap», не «runtime helper missing»).
- Каждая отмечена: «должно быть в type-check» / «codegen-only по
  природе» / «уже в type-check, дублирование ОК».

**Ф.1 — D54 as-cast в type-check:**
- `nova check std/encoding/hex.nv` (текущий файл, без правок) → FAIL
  с тем же сообщением что сейчас даёт codegen.
- 24 banned-пары покрыты positive+negative тестами в
  `nova_tests/typecheck/d54_as_cast/`.
- Special-cases (CharLit / IntLit в Unicode range) — positive tests
  с PASS.

**Ф.2 — strict bool в type-check:**
- `nova_tests/typecheck/d54_strict_bool/` — negative `if int_var {}`,
  `while str_var {}` ловятся в check.

**Ф.3 — sweep + retrospective:**
- 191/191 nova_tests PASS, 45/45 std type-check (без регрессий).
- `std/encoding/hex.nv` починен под D54 (`char.try_from(n)?` с
  `Fail` в сигнатуре `digit`), либо отдельным мини-планом, либо
  внутри этого.
- Документация: snippet в [project-creation.txt] про parity
  type-check ↔ codegen.

---

## Не входит

- **Полная переработка type-checker'а** под Hindley-Milner / inference
  расширения. Этот план — **только перенос уже работающих проверок** на
  раннюю фазу.
- **Сами правила D54** (что разрешено, что нет) — фиксированы в
  [spec/decisions/05-types.md](../../spec/decisions/05-types.md) (D54).
  План их не меняет.
- Improvements diagnostic'ов (rich error spans, did-you-mean) — может
  пойти отдельно.

---

## Связанные планы

- [Plan 34](34-stdlib-typecheck-fix.md) — закрыт; этот gap обнаружен при
  починке `std/encoding/hex.nv` уже после закрытия Plan 34.
- [Plan 36 / D95](36-cli-production-hardening.md) — `nova check`
  contract: «полная type+lint валидация без codegen». Текущий gap
  нарушает этот контракт.
- [spec/decisions/05-types.md](../../spec/decisions/05-types.md) — D54
  правила конверсий (источник запретов).

---

## Риски

- **Регрессии в test-suite** при добавлении строгости. 191/191 prevent —
  если что-то падает, это уже было ошибкой, прятавшейся за «codegen
  поймает».
- **Type inference completeness** — для as-cast проверка использует
  `infer_expr_c_type` ([emit_c.rs:5470](../../compiler-codegen/src/codegen/emit_c.rs#L5470)),
  который видит `nova_int` etc. Type-checker должен иметь эквивалентный
  inference на эти выражения. Audit Ф.0 это закроет.
- **str ↔ T** в banned-list: type-checker должен знать строки.
  Сейчас знает (есть `str` через `nova_str` mapping) — риск минимальный.
