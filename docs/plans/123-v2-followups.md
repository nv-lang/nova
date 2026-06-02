// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 123 V*.2 Followups — Umbrella

> **Создан 2026-06-02.** Worktree `nova-p123`. Branch
> `plan-123-v2-followups`. Bundle of V*.2+ followups identified at
> end of Plan 123 V*.1 closure.

---

## 1. Контекст

После закрытия Plan 123 V7.1 (IPA full integration) и V*.1 followups
(V3.1 / V4.1 / V5.1 / V6.1), в memory зафиксированы 9 followups V*.2+
(см. `[M-123-v*.2-*]`). V8 (cross-module IPA) — deferred indefinitely.

V*.2 split:
- **Foundation refactor:** V7.2 explicit IPA threading.
- **DX:** V6.3 configurable thresholds, V5.2 semantic tokens, V5.3
  pure quickfix.
- **Optimization expansion:** V3.2 pure-call non-literal args, V4.2
  chain prefix sharing.
- **Tooling:** V6.2 Plan 57 CPU bench integration.
- **Algorithm:** V7.3 SCC closure (Tarjan).

---

## 2. Скоуп

10 followups (last = push/status update). Each lands в отдельном
коммите за один прогон. Реализация идёт строго в порядке списка
(foundation refactor first, then expansion, then tooling, last DX).

| # | Followup | Тип | LOC≈ | Test fixtures |
|---|----------|-----|------|---------------|
| 0 | Baseline test pattern fix (`..Default::default()` spread) | infra | ~70 | n/a (lib tests) |
| 1 | V7.2 — explicit IpaCtx threading | refactor | ~150 | 2 new plan123_7_2 |
| 2 | V6.3 — configurable gate thresholds | config | ~80 | 2 new |
| 3 | V5.2 — semantic tokens | LSP | ~120 | LSP unit + 1 fixture |
| 4 | V5.3 — quickfix add #pure annotation | LSP | ~80 | 1 fixture |
| 5 | V3.2 — tuple/record literal args | pure-cache | ~150 | 4 new |
| 6 | V4.2 — chain prefix sharing | chain-cache | ~180 | 3 new |
| 7 | V6.2 — Plan 57 CPU bench integration | bench | ~100 | 1 bench fixture |
| 8 | V7.3 — SCC-based exact closure | algorithm | ~120 | 2 new |
| 9 | Push branch + final status update | git | trivial | n/a |

---

## 3. Не в скоупе

- V8 cross-module IPA — deferred indefinitely (substantial
  infrastructure, low ROI given Plan 123 V1-V7 коверы ~95% wins).
- Plan 124 [priv field] D-block changes — separate plan, не trogen.
- Plan 114 const_fn refactor — separate plan.

---

## 4. Acceptance (umbrella)

- **U1** All 9 followups landed в отдельных commit'ах
- **U2** field_cache lib tests 14/14 PASS unchanged (no regression)
- **U3** New runtime fixtures PASS (per-followup acceptance criteria)
- **U4** spec/decisions/08-runtime.md updated с D223/D219/D218/D217
  V*.2 amends
- **U5** 3 logs updated (project-creation.txt + simplifications.md +
  nova-private/discussion-log.md)
- **U6** Branch pushed для review (не self-merge'ится)

---

## 5. Per-followup sub-plan docs

- `123-baseline-test-pattern-fix.md` ✅ CLOSED
- `123.7.2-explicit-ipa-threading.md`
- `123.6.3-configurable-thresholds.md`
- `123.5.2-semantic-tokens.md`
- `123.5.3-pure-quickfix.md`
- `123.3.2-pure-literal-args-v2.md`
- `123.4.2-chain-prefix-sharing.md`
- `123.6.2-plan57-bench.md`
- `123.7.3-scc-closure.md`

---

## 6. Closure status

✅ CLOSED 2026-06-02. All 10 commits landed на plan-123-v2-followups.

### Final tally

| Followup | Commit | Acceptance | Tests |
|---|---|---|---|
| baseline-fix | 73010bf9 | B1-B4 ✅ | n/a |
| V7.2 | (later commit) | V7.2.1-V7.2.5 ✅ | 14 + 2 fixtures |
| V6.3 | (later commit) | V6.3.1-V6.3.4 ✅ | 14 unchanged |
| V5.2 | 0e43f907 | V5.2.1-V5.2.5 ✅ | 7 LSP |
| V5.3 | 518d78f8 | V5.3.1-V5.3.5 ✅ | 10 LSP |
| V3.2 | (later commit) | V3.2.1-V3.2.6 ✅ | +7 V3.2 unit |
| V4.2 | (later commit) | V4.2.1-V4.2.5 ✅ | +2 V4.2 + 1 fixture |
| V6.2 | ac69a5b7 | V6.2.1-V6.2.5 ✅ | +2 V6.2 unit |
| V7.3 | 80f35a4e | V7.3.1-V7.3.5 ✅ | +5 V7.3 unit |

### Aggregate
- 32 field_cache lib tests (14 baseline + 18 V*.2)
- 10 nova-lsp integration tests (3 baseline + 7 V*.2)
- 4 runtime fixtures (plan123_7_2 / 3_2 / 4_2)
- 9 spec amends (D217 ×5, D219 ×1, D223 ×2 — V7.1 §4 superseded)
- 9 sub-plan docs + 1 umbrella doc

### Branch
`plan-123-v2-followups` pushed к origin для review. NOT merged.
