// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 58: Cross-toolchain MSVC verification

> **Создан 2026-05-16 EOD.** Закрывает acceptance item из Plan 55:
> "Cross-toolchain MSVC verify". В Plan 55 пропущен (все tests на Clang
> default), но для production cross-platform stability нужна matrix
> verification.

---

## Контекст

Nova bootstrap supports три C-compilers (Plan 09 closure):
- **Clang** — default (Plan 09 Ф.5: 10-15% perf improvement vs MSVC).
- **MSVC** — fallback на Windows если Clang недоступен.
- **GCC** — Linux/macOS default.

Plan 55 verification — **только Clang**. MSVC + GCC paths не
проверены после Plan 55 changes. Latent bugs возможны:
- MSVC strict mode (`/Wall`) может flag код который Clang accepts.
- Endianness / aliasing assumptions могут отличаться.
- GCC отдельный test matrix (Linux Docker smoke сейчас покрывает
  только Plan 44.x scope, не Plan 55 codegen changes).

---

## Scope

### Phase 1 — Windows MSVC verification

- **Setup:** ensure MSVC env-detection (`nova test --toolchain=msvc`)
  работает. Glance `compiler-codegen/src/toolchain.rs`.
- **Full `nova test`** на MSVC. Comparison с Clang baseline.
- **Expected delta:** 0 functional regressions, +5-15% wall-clock (MSVC
  slower).
- **Document any MSVC-only failures** в test_runner output (XFAIL
  marker for known MSVC-incompatible patterns).

### Phase 2 — Linux GCC verification

- **Docker setup:** existing Plan 44.4 has Linux Docker smoke (266
  tests). Расширить:
  - `nova test --toolchain=gcc` на full suite.
  - Diff vs Clang/Linux (если Clang on Linux также работает).
- **Cross-platform test markers:** для tests которые depend на OS-
  specific behavior (e.g. `target_os(windows)`), verify Plan 42.12
  `_linux.nv` peer-suffix работает correctly.

### Phase 3 — CI matrix

- **GitHub Actions matrix:**
  - `windows-latest` × {clang, msvc}
  - `ubuntu-latest` × {clang, gcc}
  - `macos-latest` × {clang}
- **Fail-fast: false** — collect all matrix results.
- **PR check:** must pass on все matrix cells.

### Phase 4 — Conformance documentation

- **`docs/toolchain-matrix.md`**:
  - Какие compilers supported (full / partial / unsupported).
  - Known MSVC-only / GCC-only issues + workarounds.
  - Performance baseline per toolchain.

---

## Acceptance criteria

- [ ] Phase 1 — `nova test --toolchain=msvc` full suite pass на Windows;
      delta vs Clang documented.
- [ ] Phase 2 — `nova test --toolchain=gcc` full suite pass на Linux;
      Docker workflow updated.
- [ ] Phase 3 — CI matrix gate (PR check на 3 OS × 2-3 toolchains).
- [ ] Phase 4 — `docs/toolchain-matrix.md` published.

---

## Estimate

| Phase | Effort | Зависимости |
|---|---|---|
| Phase 1 (MSVC Windows) | ~0.5 day | Plan 09 base |
| Phase 2 (GCC Linux) | ~0.5 day | Plan 44.4 base |
| Phase 3 (CI matrix) | ~0.5 day | GitHub Actions |
| Phase 4 (docs) | ~0.5 day | Phase 1+2 results |
| **Total** | **~2 dev-days** | depends на CI access |

---

## Risks

- **MSVC strict warnings** — `/Wall` может flag legitimate patterns.
  Mitigation: use `/W3` (default), не `/Wall`. Document any patterns
  rejected only by MSVC.
- **GCC pedantic mode** — undefined behavior detection может найти
  latent bugs в codegen output. Mitigation: enable `-pedantic-errors`
  только если хотим строгий test; default `-Wall -Wextra`.
- **Long CI wall-clock** — 3 OS × 3 toolchains × ~10min test suite =
  ~90 min. Mitigation: parallel matrix, optimize test runner.

---

## Что НЕ в Plan 58

- **WSL Linux tests** на Windows runner — overhead больше чем native
  Docker.
- **MinGW** — устаревший, не tested.
- **Apple Silicon (M1/M2)** verification — depends on macOS runner
  availability (отдельно).

---

## Связь

- **Plan 09** (Clang migration) — Plan 58 verifies что MSVC path
  не сломан Plan 09 changes (включая Plan 55 mono-pass changes).
- **Plan 44.4** (M:N runtime Stage 0) — Linux Docker smoke existing
  infrastructure, Plan 58 расширяет on full suite.
- **Plan 55** — closure: this plan unblocks "Cross-toolchain MSVC"
  unfilled acceptance.

---

## Priority

**P2** — production hardening, не блокер. Латентные bugs могут
сохраняться unwatched до hit'а реального user'а. Запустить как
side-investment.
