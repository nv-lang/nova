# Backlog Followups

This file tracks deferred items, known limitations, and future improvement tickets
referenced from plan docs and simplifications.md.

---

## Plan 118.6 — Safe &x model

- **[M-118.6-tuple-field-escape]** `&tuple.N` (tuple field by index) escape analysis chain-root tracking.
  Current: only named struct field chains are tracked. Tuple index access `&t.0` may not
  correctly promote the parent tuple. Verify and extend escape_analyze.rs if needed.
  Priority: M.

