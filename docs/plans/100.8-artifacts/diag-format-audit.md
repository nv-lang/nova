# Plan 100.8 — Diagnostic Format Consistency Audit (Ф.7)

> **Date:** 2026-05-26  
> **D5 format spec:** `spec/decisions/09-tooling.md` §D166  
> **Auditor:** Plan 100.8 implementation

## D5 Diagnostic Format Spec (D166 §D3)

All Plan 100 errors must follow:
```
error[ERROR-CODE]: <human-readable message>
   ╭─[file.nv:LINE:COL]
 L │   <source line>
   │   ━━━━━━━━━━━━━ <label for this span>
   ╰─
   note: <additional context>
   help: <consume methods list if available>
   suggestion (<applicability>):
     <replacement text>
```

Applicability values: `machine-applicable` | `maybe-incorrect` | `has-placeholders`

---

## Audit Checklist

### D133-not-consumed

| Property | Spec | Implemented | Status |
|---|---|---|---|
| Error code in message | `[D133-not-consumed]` | ✅ `check_obligations_at_exit` | ✅ |
| Span points to binding | binding declaration span | ✅ `exit_span` used | ✅ |
| Note: type declared here | `note: type X declared consume at file:LINE` | ✅ `with_note` call | ✅ |
| Help: consume methods | lists `.method()` per method in LinearityRegistry | ✅ `methods` vec in suggestion | ✅ |
| Suggestion `MaybeIncorrect` | `suggestion (maybe-incorrect): name.method()` | ✅ `Applicability::MaybeIncorrect` | ✅ |
| Multi-path suggestion | `errdefer { x.cl() }\nokdefer { x.primary() }` for MaybeConsumed | ✅ | ✅ |
| No-methods fallback | `// TODO: add consume-method for X` | ✅ `suggestion_text` fallback | ✅ |

### D162-uncovered-error-path

| Property | Spec | Implemented | Status |
|---|---|---|---|
| Error code in message | `[D162-uncovered-error-path]` | ✅ `check_d162_coverage` | ✅ |
| Only failable fns | gated on `fn_is_failable(effects)` | ✅ | ✅ |
| Only when consume obligations | checked `ctx.consume_obligations.is_empty()` | ✅ | ✅ |
| Suggestion: add errdefer | `errdefer { x.method() }` | ✅ `MachineApplicable` | ✅ |
| D162-uncovered-success-path | warning when errdefer but no okdefer | ✅ | ✅ |

### Message consistency rules

| Rule | Requirement | Audit result |
|---|---|---|
| Code prefix | All codes have `[D1NN-...]` prefix | ✅ All Plan 100 codes have prefix |
| Backtick quoting | Names in backticks: `` `tx` ``, `` `Transaction` `` | ✅ Consistent |
| Applicability accuracy | `MachineApplicable` only when no context needed | ✅ `MaybeIncorrect` used for consume |
| Note vs help separation | `note:` for context; `help:` for actions | ✅ Correct per implementation |
| No trailing whitespace | Diagnostic messages stripped | ✅ format! macros use trim |

---

## Verdict

All Plan 100.8 D133/D162 diagnostics **follow D5 format** per D166 spec. No format violations found.

**Machine-applicable applicability ladder:**
- `MachineApplicable` → D162 `add errdefer` (unambiguous — only one correct edit)
- `MaybeIncorrect` → D133 `consume via method` (user must pick which method)
- `HasPlaceholders` → not used in Plan 100.8 (no unknown names)

---

## References

- `compiler-codegen/src/types/mod.rs` — `check_obligations_at_exit` (D133)
- `compiler-codegen/src/types/mod.rs` — `check_d162_coverage` (D162)
- `compiler-codegen/src/diag.rs` — `Suggestion`, `Applicability` enum
- `spec/decisions/09-tooling.md` §D166 — D5 format spec
