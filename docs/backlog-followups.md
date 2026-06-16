# Backlog Followups

This file tracks deferred items, known limitations, and future improvement tickets
referenced from plan docs and simplifications.md.

---

## Single-letter type names — E_TYPE_NAME_TOO_SHORT

- **[M-single-letter-type-ban]** CLOSED Plan 167. Запретить `type X { ... }` где имя типа длиной 1 символ.
  Мотивация: однобуквенные имена конфликтуют с generic-параметрами (`fn[S Iter[T]]` vs `type S`),
  вызывая E_PREFIX_SHADOWS_NAMED_TYPE. Haskell решает регистром (type vars строчные), Nova
  решает запретом однобуквенных типов — generic-параметры остаются однобуквенными по конвенции.
  Реализация: новый error E_TYPE_NAME_TOO_SHORT в checker (name.len() == 1 для TypeDecl).
  Sweep: grep `^type [A-Z] ` по nova_tests/ и std/ — исправить (~10 в nova_tests/plan118_1_addr_chains/).
  Priority: M.

---

## Plan 118.6 — Safe &x model

- **[M-118.6-tuple-field-escape]** `&tuple.N` (tuple field by index) escape analysis chain-root tracking.
  Current: only named struct field chains are tracked. Tuple index access `&t.0` may not
  correctly promote the parent tuple. Verify and extend escape_analyze.rs if needed.
  Priority: M.

