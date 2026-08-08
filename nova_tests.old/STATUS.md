<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# ⚠️ nova_tests/ — LEGACY / UNVERIFIED

**Это НЕ гейт корректности.** Корпус частично устарел (retired-API: `str.len` D249,
newtype-коэрция, plan104-128-дубли) и находится **под ревизию/миграцию** —
см. **[Plan 198](../docs/plans/198-nova-tests-triage.md)**.

- **Настоящие гейты корректности:** `spec_tests/conformance/` + module-тесты
  `std/**/*_test.nv`. Сюда новые тесты НЕ добавлять.
- **CI:** прогоны nova_tests разгейчены (non-blocking) 2026-07-11.
- **⛔ Load-bearing внутри (НЕ трогать до миграции Plan 198):**
  - `SOUNDNESS_REGRESSION`-файлы (`contracts/`, `plan140_4/`) — сторожат закрытые
    soundness-дыры, число под CI-ratchet.
  - byte-identity-baseline (эталон codegen-DCE-проверок).
  - уникальные репро, не покрытые conformance/module-тестами.
