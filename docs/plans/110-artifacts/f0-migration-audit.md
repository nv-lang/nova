// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 110 Ф.0.2 — Migration Audit

> Created 2026-05-31. Snapshot of cleanup-family usage in main @ 72f77f16733.

## 1. Nova fixtures touching `errdefer` / `okdefer` / `defer |result|`

**Total**: 42 fixtures, 90 occurrences.

### Cluster A — `nova_tests/plan100_4_*` (canonical D158/D160/D161/D162 tests)

These are **purpose-built** for the constructs being removed. Plan 110
removes the constructs → these fixtures become invalid.

**Migration strategy**: rewrite each as a `consume X = ... { ... }` fixture
(if testing semantics that maps to D188) or **archive** (if testing semantics
that no longer exists in D189/D190).

| Subdir | Files | Decision |
|---|---|---|
| `plan100_4_1/` | `errdefer_fail_ok_only_on_error.nv`, `errdefer_fail_ok_propagation.nv` | rewrite as `consume {}` with `on_exit` matching `Failure(_)` |
| `plan100_4_2/` | `errdefer_suspend_ok.nv`, `okdefer_suspend_ok.nv` | rewrite as `consume {}` with `suspend` in `on_exit` (D191) |
| `plan100_4_3/` | 10 fixtures (`okdefer_*`, `defer_with_result_*`, `errdefer_okdefer_exhaustive`, `mixed_defer_family_lifo`, neg-tests) | majority **archive** (constructs gone); 2 rewrite as `consume {}` |
| `plan100_4_4/` | 4 fixtures (multi-defer LIFO mixed) | rewrite — semantics maps to D188 R5 LIFO composition |
| `plan100_4_5/` | `neg_failable_errdefer_no_success_cover.nv` | **archive** (D162 rule simplified — exhaustive by construction) |

### Cluster B — `nova_tests/syntax/` (parser/grammar tests)

| File | Decision |
|---|---|
| `errdefer_basic.nv` | **archive** (parser will reject `errdefer` after Ф.5) |
| `errdefer_no_fire_on_normal.nv` | **archive** |
| `errdefer_rethrow.nv` | **archive** |
| `errdefer_throw.nv` | **archive** |
| `defer_on_interrupt.nv` | rewrite — `defer` plain still exists, `interrupt`-as-throw lives in D90 §7 amend |

Replacement parse-error fixtures: NEG-5.4 (D189-removed-okdefer / errdefer / defer-result).

### Cluster C — `nova_tests/expected_runtime/` (runtime panic + errdefer)

| File | Decision |
|---|---|
| `errdefer_panic_lifo.nv` | rewrite as `consume {}` with `on_exit(Panic)` matching |
| `errdefer_panic_mainflow.nv` | rewrite |
| `errdefer_panic_nested_mainflow.nv` | rewrite |
| `errdefer_on_panic.nv` | rewrite |
| `multi_expect_panic_errdefer.nv` | rewrite |
| `panic_no_errdefer_in_scope.nv` | rewrite (covers D188 R1 partial-construction) |
| `defer_throw_cascade_panic.nv` | retain (plain `defer` still exists) |

### Cluster D — `nova_tests/negative_capability/`

| File | Decision |
|---|---|
| `errdefer_keyword_reserved.nv` | replace with NEG-5.4 (D189-removed-errdefer) |
| `errdefer_return_rejected.nv` | replace with corresponding NEG for `consume {}` |

### Cluster E — `nova_tests/plan100_8/` (consume-analyze / LSP)

These are doc/LSP fixtures referencing `errdefer` in comments / hover-info:

| File | Decision |
|---|---|
| `bench_check_consume_within_budget.nv` | retain; update reference if needed |
| `consume_analyze_coverage_report.nv` | retain; update generator (Ф.10 LSP) |
| `consume_analyze_json_output.nv` | retain; update JSON format |
| `diagnostic_format_consistency.nv` | retain; update message format |
| `lsp_hover_consume_status.nv` | retain; show Consumable impl info instead |
| `lsp_hover_coverage_analysis.nv` | retain; update |
| `nova_doc_resource_lifecycle.nv` | retain; update doc render |

### Cluster F — `bench/plan100/`

| File | Decision |
|---|---|
| `defer_coverage_analysis.nv` | retain; update benchmark target |

## 2. Stdlib `.nv` files

**Single file** references `errdefer` / `okdefer` in comments only:

- `std/prelude/errors.nv:153` — doc-comment: «Composite error от **failable
  defer/errdefer/okdefer body** (D158).» → update to reference D193/D158
  amend.

**No active code uses errdefer/okdefer in std/** — bootstrap surface is clean.
This dramatically simplifies migration: stdlib resource types (File, Tx,
Mutex etc.) don't yet exist as `Consumable` impls; they need to be **written
from scratch** as part of Ф.4–Ф.5.

## 3. Resource-like types — candidates for `Consumable` impl

Catalogued by source plan; each becomes a `Consumable[E]` implementation:

| Type | Source plan | E |
|---|---|---|
| `Transaction` | (TBD — no std/db yet at main) | `DbError` |
| `File` | (TBD — no std/fs yet at main) | `IoError` |
| `BufReader` / `BufWriter` | (TBD) | `IoError` |
| `MutexGuard` | Plan 103.3 | `Never` (D194) |
| `RwLockGuard` (read+write) | Plan 103.3 | `Never` |
| `ReentrantGuard` | Plan 103.3 | `Never` |
| `SemaphorePermit` | Plan 103.4 | `Never` |
| `CancelScope` | Plan 49 | `Never` |
| `TcpStream` | Plan 83.12 | `IoError` |
| `TcpListener` | Plan 83.12 | `IoError` |
| `UdpSocket` | Plan 83.12 | `IoError` |
| `Channel` / `ChanReader` / `ChanWriter` | Plan 65 | `Never` |
| `JoinHandle` | Plan 83.4.2 (proposed) | `Never` |
| `Stream[T]` | Plan 84 (if exists) | `E` (generic) |

**Note**: `File` / `Transaction` / `BufReader` / `BufWriter` are not yet in
main as concrete std types. Their Plan 110 cleanup contracts will be defined
in Ф.4 alongside writing the types themselves OR (if those types are not
materially required for 0.1) — implementations deferred to follow-up plans
with `[M-110-stdlib-<type>]` markers.

## 4. Rust source — compiler/codegen footprint

122 occurrences across 9 files. Rough decomposition:

| File | Occ | Touchpoint |
|---|---|---|
| `lexer/mod.rs` + `lexer/token.rs` | 7 | tokens `errdefer` / `okdefer` / `defer\|r\|` — change to emit `E_KW_REMOVED_*` post-Ф.9 |
| `parser/mod.rs` | 4 | parsing rules — remove; add `consume X = expr { body }` rule (Ф.1) |
| `ast/mod.rs` | 3 | `Stmt::ErrDefer` / `Stmt::OkDefer` / `DeferWithResult` AST nodes — remove; add `Stmt::ConsumeScope` (Ф.1) |
| `types/mod.rs` | 51 | type-check rules — remove D160/D162 specifics; add D188 R1-R6 + D196 (Ф.1.5) |
| `codegen/emit_c.rs` | 46 | codegen — replace `nv_errdefer_*` emit with `nv_consume_scope_*` (Ф.2) |
| `interp/mod.rs` | 8 | interpreter — replace errdefer/okdefer with consume-scope eval |
| `doc/render_*.rs` | 3 | doc rendering — show Consumable impl + Plan 100.8 consume badge update |

**Refactor scope**: ~600-1000 lines of net Rust changes (most additions in
codegen + types). The 51 occurrences in `types/mod.rs` are mostly conditional
branches that will simplify (D162 rules reduce) — net SLOC decrease likely.

## 5. D90 §7 — interrupt + errdefer cases

`expected_runtime/defer_on_interrupt.nv` covers `interrupt` keyword with
`defer` (not `errdefer`). After D90 §7 amend, `interrupt v` in body delivers
as `Failure(CancelError { reason: ... })` to surrounding `consume {}.on_exit`
(if any) and to plain `defer` blocks (semantics-unchanged for plain `defer`).

Existing fixture continues to work; no rewrite needed. New fixtures added in
Ф.9 verifying:
- `interrupt v` propagates as `CancelError` through `consume {}` outcomes;
- `errdefer { ... }` form rejected (Ф.5 parser).

## 6. Auto-fix tool coverage estimate

3 canonical patterns (Plan 110 §D189):
1. `consume + errdefer + okdefer` → `consume {}` block.
2. Bare `errdefer { x }` → `mut done = false; defer { if !done { x } }; ...; done = true`.
3. `defer |result|` → `consume {}` or `with Cleanup = h`.

**Pattern 1** covers ~80% of `plan100_4_*` fixtures (the canonical
transaction-style pattern). **Pattern 2** covers `syntax/errdefer_*` and
some `expected_runtime/*` cases. **Pattern 3** covers `plan100_4_3/defer_*`
cluster.

Auto-fix coverage projected ≥ 95% of fixtures; remainder = NEG fixtures
that need to be replaced wholesale (parser rejects → new fixtures testing
parser-error).

## 7. Decision per Plan 110 §«Возможный split на sub-plans»

The audit finds:

- Spec drafts: 11 D-blocks already written (Ф.0.1 ✓);
- Stdlib migration scope: limited (no existing File/Tx/Buf types in main);
- Rust refactor: ~600-1000 lines, concentrated in types/codegen;
- Test fixture migration: 42 files, ~80% auto-fixable;
- New stdlib (File/Tx/Buf) work is OUT of scope — these types need to be
  designed before they can be `Consumable` (own sub-plan);
- Plan 103.7 prerequisite (`#realtime` attribute model) is **already landed**
  (Plan 113 closed 2026-05-30, see memory `project-plan113-status`);
- Plan 101 generic bounds — landed (already used in stdlib).

### Recommendation

Proceed with Plan 110 as a single plan (no split) because:
1. Bootstrap surface is small (no live stdlib resource types yet);
2. Core compiler work is concentrated in ~9 Rust files;
3. Plan 113 already provides `#realtime` attribute model for D198;
4. Auto-fix tool covers ≥ 95% of test fixtures.

**Followup plans** for items that don't fit core scope:
- `[M-110-stdlib-fs]` — `std/fs` with `File` Consumable impl;
- `[M-110-stdlib-db]` — `std/db` with `Transaction` Consumable impl;
- `[M-110-stdlib-bufio]` — `std/bufio` with `BufReader`/`BufWriter`;
- `[M-110-ffi-cleanup]` — Plan 100.5 FFI integration as Ф.12 (large enough
  to extract);
- `[M-110-stress-bench]` — Ф.11 benchmark suite if bench harness changes
  needed.

## 8. Cross-check with related plans (Ф.0.4)

| Plan | Status @ main | Impact |
|---|---|---|
| Plan 82 (Windows fiber arena) | ✅ closed | Ф.3.7 cross-platform cancel-shield validation ready |
| Plan 83.10/11 (cancel routing / centralized I/O) | partial (83.10 closed, 83.11 WIP) | minor — D188 cancel-shield uses existing per-fiber cancel-pending |
| Plan 100.5 (FFI integration) | spec only | Ф.12 likely extract to followup |
| Plan 100.6 (cross-module consume) | ✅ closed | D188 cross-module — works via existing manifest exports |
| Plan 100.7 (stdlib migration playbook) | ✅ closed | reuse playbook for stdlib Consumable impls |
| Plan 100.8 (consume-analyze + LSP) | ✅ closed | Ф.10 LSP quick-fix extends existing infra |
| Plan 101 (generic bounds) | ✅ closed | `[T Consumable[E]]` works through existing bounds |
| Plan 103.1 (memory ordering) | ✅ closed | D188 R6 release-acquire grounded |
| Plan 103.3 (Mutex family) | ✅ closed | Mutex/RwLock/ReentrantMutex ready for D194 impl |
| Plan 103.4 (Sem/Barrier/CDL/Cond) | ✅ closed | Semaphore ready for D194 impl |
| Plan 103.6 (realtime/blocking) | ✅ closed | D198 baseline |
| Plan 104.1 (LSP) | ✅ closed | Ф.10 integration ready |
| Plan 65 (Channels) | ✅ closed | Ф.5.2 Channel Consumable impl ready |
| Plan 83.12 (TCP/UDP) | ✅ closed | Ф.5.1 net stdlib Consumable impls ready |
| Plan 113 (`#realtime` attribute-only) | ✅ closed 2026-05-30 | D198 ready |
| Plan 114 (`ro`/`mut`/`consume` keywords) | ✅ merged | parser baseline syntax matches |

All hard prerequisites landed. Soft prerequisites (FFI, supervised drain
ownership) noted as Ф.12/Ф.5.3 deferral candidates.

---

## Conclusion

Ф.0 GATE result: **proceed with Plan 110 as single plan**, but defer:
- Ф.12 FFI integration → `[M-110-ffi-cleanup]` follow-up (if scope explodes).
- Ф.11 full benchmark suite → `[M-110-stress-bench]` if bench harness needs work.
- Stdlib types (File/Tx/Buf) not yet in main → `[M-110-stdlib-*]` follow-up
  per type if Ф.4 reveals they need new type design beyond Consumable impl.

Phase implementation continues with Ф.1 (Core parser + checker + protocol).
