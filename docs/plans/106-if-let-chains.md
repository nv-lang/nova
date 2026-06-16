\ SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 106 — Guard-условие `&&` в `if`/`while` pattern-bind

> **Создан:** 2026-06-17. **Статус:** ✅ CLOSED 2026-06-17.
> **Spec:** [03-syntax.md D34](../../spec/decisions/03-syntax.md#d34-pattern-bind-в-ifwhile-conditions--unified-grammar-с-match-arms) — D34 amend 2026-06-17.

## Что реализовано

`&&` guard в `if`/`while` pattern-bind:

```nova
if Some(x) = expr && x > 5 { use(x) }
if ro Some(user) = db.find(id) && user.is_active { process(user) }
while Some(item) = queue.pop() && item.valid { handle(item) }
```

- Биндинги паттерна видны в guard-выражении
- Guard type-checked как `bool`
- else-ветка выполняется при провале паттерна **или** провале guard
- Codegen: goto-based структура (pattern-cond → bind → guard-cond → then → done; else → done)
- WhileLet with guard: `if (!guard) break;` после unpack паттерна
- 13 тестов: 7 pos + 3 neg + 3 CC (nova_tests/plan106/)

## Acceptance criteria

- [x] `if Some(x) = expr && guard { body }` парсится и компилируется корректно
- [x] Биндинги паттерна видны в guard-выражении (`x > 5` — `x` из паттерна)
- [x] Guard type-checked как `bool` (не-bool → E_TYPE_MISMATCH)
- [x] `while Some(x) = expr && guard { body }` — симметрично `if`
- [x] else-ветка срабатывает при провале паттерна ИЛИ guard
- [x] Short-circuit: guard не вычисляется если паттерн не совпал
- [x] Биндинги паттерна НЕ видны после `if` блока (корректный scope)
- [x] Биндинги паттерна НЕ видны в else-ветке
- [x] Переменная из guard, не объявленная в паттерне → E (undefined)
- [x] `ro`-форма: `if ro Some(x) = ...` с guard работает корректно
- [x] Существующие if/while pattern-bind без guard — без регрессий
- [x] **Без упрощений как для прода**: полный scope-pipelining, корректный codegen, type-check guard как bool

## Фазы выполнения

| Фаза | Что | Статус |
|---|---|---|
| AST | `guard: Option<Box<Expr>>` в `ExprKind::IfLet` и `ExprKind::WhileLet` | ✅ |
| Parser | После scrutinee: `if AmpAmp { bump; parse_expr() }` в `parse_if()` и `parse_while()` | ✅ |
| Type-checker (name resolution) | Guard ходится с биндингами паттерна в scope; else-branch — без них | ✅ |
| Type-checker (consume_walk) | Guard посещается после pattern-bind declarations | ✅ |
| Codegen IfLet | goto-based: pattern-cond → bind → guard → then → done_label | ✅ |
| Codegen WhileLet | `if (!guard) break;` после pattern unpack | ✅ |
| Interpreter | Guard в env с биндингами паттерна; break при guard-fail | ✅ |
| may_gc.rs | Walk guard в scope биндингов паттерна | ✅ |
| Тесты | 13/13 PASS | ✅ |
| Spec | D34 amend 2026-06-17 | ✅ |

## Изменённые файлы

| Файл | Изменение |
|---|---|
| `compiler-codegen/src/ast/mod.rs` | +`guard: Option<Box<Expr>>` в IfLet + WhileLet |
| `compiler-codegen/src/parser/mod.rs` | `&&` guard parsing в parse_if() + parse_while() (+61 строк) |
| `compiler-codegen/src/types/mod.rs` | Type-check guard как bool (+11 строк) |
| `compiler-codegen/src/codegen/emit_c.rs` | Guard codegen: goto-structure IfLet, break WhileLet (+198/-65) |
| `compiler-codegen/src/codegen/may_gc.rs` | Walk guard в pattern scope (+10/-2) |
| `compiler-codegen/src/interp/mod.rs` | Interpreter guard support (+22/-4) |
| `spec/decisions/03-syntax.md` | D34 amend: grammar + примеры + «что отвергнуто» |
| `nova_tests/plan106/` | 13 фикстур |

## Дизайн-решения

**`&&` вместо запятой (Swift):**
- `&&` — очевидная логическая семантика (AND между паттерном и guard)
- запятая перегружена в Nova (tuple, args, enum variants)
- Rust-аудитория знакома с `&&` в boolean context (не с comma-chains)
- Rust сам перешёл на `&&` в let-chains (RFC 2497, stable 1.64)

**Что guard НЕ делает (вне scope):**
- Множественные let-паттерны через `&&` (`if Some(x) = e1 && Some(y) = e2 {}`) — это Rust let-chains V2; слишком сложный codegen и scope semantics; откладывается
- match guards через запятую — отдельный вопрос
- let-else (`let Some(x) = e else { return }`) — отдельный конструкт

## Связь

- [D34 §«Guard-выражение»](../../spec/decisions/03-syntax.md#d34) — формальная грамматика + примеры
- [M-106-if-guard] ✅ CLOSED 2026-06-17
- [docs/plans/backlog-followups.md](../plans/backlog-followups.md) — ссылка на Plan 106 секцию
