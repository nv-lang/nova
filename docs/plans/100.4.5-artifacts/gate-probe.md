// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 100.4.5 — Ф.0 GATE: design probe + Plan 100.8 D162 leverage

> Дата: 2026-05-26. Worktree: `nova-p100-4-5`. Branch: `plan-100-4-5-consume-final`.
> HEAD baseline: `4c04c5dfa02` (post-100.4.2 closure).

## 1. КЛЮЧЕВОЕ ОТКРЫТИЕ
Plan 100.8 (D166 tooling layer) **уже частично реализовал D162** через
`check_d162_coverage` в compiler-codegen/src/types/mod.rs (line 7920+):

- `D162-uncovered-error-path` — failable function (`Fail[E]`) + consume
  binding + НЕТ errdefer → compile error с quick-fix suggestion.
- `D162-uncovered-success-path` — failable function + consume + errdefer
  есть, но НЕТ okdefer/explicit consume на success path → warning-style error.

То есть **D1 (defer-family cover), D2 (multi-defer exhaustive), D4 (partial
cover error)** — уже работают через Plan 100.8 D166 base.

## 2. Что осталось (для bootstrap MVP, Option B)

✅ **Уже работает (Plan 100.8 D166 leverage):**
- D1: errdefer/okdefer cover recognition.
- D2: exhaustive cover (has_errdefer && has_okdefer).
- D4: partial cover → D162-uncovered-error-path / D162-uncovered-success-path.

📋 **Не реализовано — followup markers:**
- `[M-100.4.5-double-cover-check]` (D3) — `tx.commit()` + `okdefer { tx.commit() }`
  double-consume. Требует tracking explicit-consume против defer-cover.
- `[M-100.4.5-conditional-cover-warning]` (D5) — conditional `if { commit() }`
  в defer body — warning W (D162-conditional-cover).
- `[M-100.4.5-d90-§7-interrupt-errdefer]` (D6, **P2 BREAKING CHANGE**) —
  D90 §7 amend: `interrupt` triggers errdefer (текущее: НЕ triggers).
  Требует runtime change + audit existing handler-flow tests.
- `[M-100.4.5-supervised-spawn-cancel]` (D7) — depends on D6 + Plan 47.
- `[M-100.4.5-strict-consume-mode]` (D5) — `--strict-consume` CLI flag.

## 3. Scope decision (Option B)

**MVP:** spec D162 + amend D90 §7 (status updates), fixtures verify
existing D162 behavior + mark followup markers. Закрывает Plan 100.4
umbrella с honest bootstrap-границей: D1/D2/D4 ✅; D3/D5/D6/D7 — followup.

**Полная реализация D6 (interrupt → errdefer)** — потенциально breaking
для existing handler-flow user code; требует Ф.0 audit existing fixtures
+ runtime change. **Отложено на dedicated сессию** (P2).

## 4. Acceptance Ф.0
- [x] Plan 100.8 D166 leverage discovered.
- [x] Scope decided (Option B): leverage existing + spec + fixtures + markers.
- [x] D3/D5/D6/D7 followup'ы enumerated.

**GATE Ф.0: PASS.**
